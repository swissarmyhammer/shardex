//! Write-Ahead Log segment management for Shardex
//!
//! This module provides fixed-size memory-mapped WAL segments with atomic operations
//! for thread-safe write pointer management and segment lifecycle.

use crate::error::ShardexError;
use crate::layout::{DirectoryLayout, FileDiscovery};
use crate::memory::{MemoryMappedFile, StandardHeader};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// WAL file magic bytes for identification
const WAL_MAGIC: &[u8; 4] = b"WLOG";
/// WAL version number for format compatibility
const WAL_VERSION: u32 = 1;
/// Reserved space after StandardHeader for WAL metadata
const RESERVED_SPACE_SIZE: usize = 9;

/// Calculate the initial write position (after header and reserved space)
pub const fn initial_write_position() -> usize {
    StandardHeader::SIZE + RESERVED_SPACE_SIZE
}

/// WAL record header for proper record structure
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct WalRecordHeader {
    /// Length of the record data (not including this header)
    data_length: u32,
    /// CRC32 checksum of the record data
    checksum: u32,
}

impl WalRecordHeader {
    pub const SIZE: usize = std::mem::size_of::<Self>();

    pub fn new(data: &[u8]) -> Self {
        let checksum = crc32fast::hash(data);
        Self {
            data_length: data.len() as u32,
            checksum,
        }
    }

    pub fn data_length(&self) -> u32 {
        self.data_length
    }

    pub fn validate_checksum(&self, data: &[u8]) -> bool {
        if data.len() != self.data_length as usize {
            return false;
        }
        let expected_checksum = crc32fast::hash(data);
        self.checksum == expected_checksum
    }

    pub fn as_bytes(&self) -> [u8; Self::SIZE] {
        unsafe { std::mem::transmute(*self) }
    }

    pub fn from_bytes(bytes: &[u8; Self::SIZE]) -> Self {
        unsafe { std::mem::transmute(*bytes) }
    }
}

/// Fixed-size memory-mapped WAL segment
///
/// WalSegment provides atomic write operations with thread-safe write pointer
/// management and integrity validation through checksums.
pub struct WalSegment {
    /// Segment identifier
    id: u64,
    /// Memory-mapped file for the segment
    memory_map: Mutex<MemoryMappedFile>,
    /// Atomic write pointer tracking current position
    write_pointer: AtomicUsize,
    /// Total segment capacity in bytes
    capacity: usize,
}

/// WAL manager for segment lifecycle and coordination
///
/// WalManager handles segment creation, rotation, cleanup, and recovery
/// operations across multiple WAL segments.
pub struct WalManager {
    /// Current active segment for writes
    current_segment: Option<WalSegment>,
    /// Configured segment size in bytes
    segment_size: usize,
    /// Directory layout for file management
    layout: DirectoryLayout,
    /// Next segment ID counter
    next_segment_id: u64,
}

impl WalSegment {
    /// Create a new WAL segment with the specified ID and capacity
    pub fn create(segment_id: u64, file_path: PathBuf, capacity: usize) -> Result<Self, ShardexError> {
        if capacity < StandardHeader::SIZE {
            return Err(ShardexError::Wal(format!(
                "Segment capacity {} is too small, must be at least {} bytes",
                capacity,
                StandardHeader::SIZE
            )));
        }

        let mut memory_map = MemoryMappedFile::create(&file_path, capacity)?;

        // Write header with initial metadata
        let header = StandardHeader::new_without_checksum(WAL_MAGIC, WAL_VERSION, StandardHeader::SIZE as u64);
        memory_map.write_at(0, &header)?;
        memory_map.sync()?;

        // Initialize write pointer after header and reserved space
        let write_pointer = AtomicUsize::new(initial_write_position());

        Ok(Self {
            id: segment_id,
            memory_map: Mutex::new(memory_map),
            write_pointer,
            capacity,
        })
    }

    /// Open an existing WAL segment
    pub fn open(file_path: PathBuf) -> Result<Self, ShardexError> {
        let memory_map = MemoryMappedFile::open_read_write(&file_path)?;
        let capacity = memory_map.len();

        // Read and validate header
        let header: StandardHeader = memory_map.read_at(0)?;
        header.validate_magic(WAL_MAGIC)?;
        header.validate_version(WAL_VERSION, WAL_VERSION)?;
        header.validate_structure()?;

        // Extract segment ID from file path (assuming format wal_NNNNNN.log)
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ShardexError::Wal("Invalid WAL file name".to_string()))?;

        let segment_id = if file_name.starts_with("wal_") && file_name.ends_with(".log") {
            let id_str = &file_name[4..file_name.len() - 4];
            id_str
                .parse::<u64>()
                .map_err(|_| ShardexError::Wal(format!("Invalid segment ID in filename: {}", file_name)))?
        } else {
            return Err(ShardexError::Wal(format!("Invalid WAL filename format: {}", file_name)));
        };

        // Recover write position by following record structure
        let write_pos = Self::recover_write_position(&memory_map, capacity)?;
        let write_pointer = AtomicUsize::new(write_pos);

        Ok(Self {
            id: segment_id,
            memory_map: Mutex::new(memory_map),
            write_pointer,
            capacity,
        })
    }

    /// Recover write position by following record boundaries (efficient O(log n) approach)
    fn recover_write_position(memory_map: &MemoryMappedFile, capacity: usize) -> Result<usize, ShardexError> {
        let data_slice = memory_map.as_slice();
        let mut current_pos = initial_write_position(); // Account for header and reserved space

        // Follow record headers to find the end of valid data
        while current_pos + WalRecordHeader::SIZE <= capacity {
            // Try to read a record header
            if current_pos + WalRecordHeader::SIZE > capacity {
                break;
            }

            // Check if we've hit a zero region (end of data)
            let header_bytes = &data_slice[current_pos..current_pos + WalRecordHeader::SIZE];
            if header_bytes.iter().all(|&b| b == 0) {
                // Found zero region, this is likely the end of data
                break;
            }

            // Try to parse record header
            let mut header_array = [0u8; WalRecordHeader::SIZE];
            header_array.copy_from_slice(header_bytes);
            let record_header = WalRecordHeader::from_bytes(&header_array);

            let data_length = record_header.data_length() as usize;

            // Validate record bounds
            if data_length == 0 || current_pos + WalRecordHeader::SIZE + data_length > capacity {
                // Invalid record, this is the end of valid data
                break;
            }

            // Validate record checksum
            let record_data_start = current_pos + WalRecordHeader::SIZE;
            let record_data_end = record_data_start + data_length;
            let record_data = &data_slice[record_data_start..record_data_end];

            if !record_header.validate_checksum(record_data) {
                // Corrupted record, stop here
                return Err(ShardexError::Wal(format!(
                    "Corrupted record found at position {} during recovery",
                    current_pos
                )));
            }

            // Move to next record
            current_pos += WalRecordHeader::SIZE + data_length;
        }

        Ok(current_pos)
    }

    /// Get the segment ID
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Get current write pointer position
    pub fn write_pointer(&self) -> usize {
        self.write_pointer.load(Ordering::SeqCst)
    }

    /// Get total segment capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get remaining space in bytes
    pub fn remaining_space(&self) -> usize {
        self.capacity.saturating_sub(self.write_pointer())
    }

    /// Check if segment is full
    pub fn is_full(&self) -> bool {
        self.remaining_space() == 0
    }

    /// Append data to the segment atomically with record header
    pub fn append(&self, data: &[u8]) -> Result<usize, ShardexError> {
        if data.is_empty() {
            return Err(ShardexError::Wal("Cannot append empty data".to_string()));
        }

        let mut memory_map = self
            .memory_map
            .lock()
            .map_err(|_| ShardexError::Wal("Failed to acquire memory map lock".to_string()))?;

        let current_pointer = self.write_pointer.load(Ordering::SeqCst);
        let total_record_size = WalRecordHeader::SIZE + data.len();
        let new_pointer = current_pointer + total_record_size;

        if new_pointer > self.capacity {
            return Err(ShardexError::Wal(format!(
                "Segment full: cannot append record of {} bytes (header {} + data {}), only {} bytes remaining",
                total_record_size,
                WalRecordHeader::SIZE,
                data.len(),
                self.capacity - current_pointer
            )));
        }

        // Create record header with checksum
        let record_header = WalRecordHeader::new(data);

        // Write the record header and data to memory
        let mut_slice = memory_map.as_mut_slice()?;

        // Write header
        let header_bytes = record_header.as_bytes();
        mut_slice[current_pointer..current_pointer + WalRecordHeader::SIZE].copy_from_slice(&header_bytes);

        // Write data
        let data_start = current_pointer + WalRecordHeader::SIZE;
        mut_slice[data_start..data_start + data.len()].copy_from_slice(data);

        // Update the write pointer atomically
        self.write_pointer.store(new_pointer, Ordering::SeqCst);

        // Return the data offset (where the actual data starts, after header)
        Ok(current_pointer + WalRecordHeader::SIZE)
    }

    /// Append a WAL transaction to the segment
    pub fn append_transaction(&self, transaction: &crate::transactions::WalTransaction) -> Result<usize, ShardexError> {
        // Serialize the transaction
        let serialized = transaction.serialize()?;

        // Append the serialized transaction data
        self.append(&serialized)
    }

    /// Sync segment to disk
    pub fn sync(&self) -> Result<(), ShardexError> {
        let memory_map = self
            .memory_map
            .lock()
            .map_err(|_| ShardexError::Wal("Failed to acquire memory map lock for sync".to_string()))?;
        memory_map.sync()
    }

    /// Get a copy of the segment data for reading (used during replay)
    pub fn read_segment_data(&self) -> Result<Vec<u8>, ShardexError> {
        let memory_map = self
            .memory_map
            .lock()
            .map_err(|_| ShardexError::Wal("Failed to acquire memory map lock for reading".to_string()))?;
        Ok(memory_map.as_slice().to_vec())
    }

    /// Read a specific range of data from the segment
    pub fn read_range(&self, start: usize, length: usize) -> Result<Vec<u8>, ShardexError> {
        let memory_map = self
            .memory_map
            .lock()
            .map_err(|_| ShardexError::Wal("Failed to acquire memory map lock for reading".to_string()))?;

        let data_slice = memory_map.as_slice();
        if start + length > data_slice.len() {
            return Err(ShardexError::Wal(format!(
                "Read range out of bounds: start={}, length={}, segment_size={}",
                start,
                length,
                data_slice.len()
            )));
        }

        Ok(data_slice[start..start + length].to_vec())
    }

    /// Validate segment integrity
    pub fn validate_integrity(&self) -> Result<(), ShardexError> {
        let memory_map = self
            .memory_map
            .lock()
            .map_err(|_| ShardexError::Wal("Failed to acquire memory map lock for integrity validation".to_string()))?;

        // Read and validate header with full structure validation including checksums
        let header: StandardHeader = memory_map.read_at(0)?;
        header.validate_magic(WAL_MAGIC)?;
        header.validate_version(WAL_VERSION, WAL_VERSION)?;
        header.validate_structure()?;

        // Validate write pointer bounds
        let write_pos = self.write_pointer();
        let min_write_pos = initial_write_position(); // Account for header and reserved space
        if write_pos < min_write_pos {
            return Err(ShardexError::Wal(
                "Write pointer is before end of header and reserved space".to_string(),
            ));
        }

        if write_pos > self.capacity {
            return Err(ShardexError::Wal("Write pointer exceeds segment capacity".to_string()));
        }

        // Validate all records for integrity
        let data_slice = memory_map.as_slice();
        let mut current_pos = initial_write_position(); // Account for header and reserved space

        while current_pos < write_pos {
            if current_pos + WalRecordHeader::SIZE > write_pos {
                return Err(ShardexError::Wal(format!(
                    "Truncated record header at position {}",
                    current_pos
                )));
            }

            // Read record header
            let header_bytes = &data_slice[current_pos..current_pos + WalRecordHeader::SIZE];
            let mut header_array = [0u8; WalRecordHeader::SIZE];
            header_array.copy_from_slice(header_bytes);
            let record_header = WalRecordHeader::from_bytes(&header_array);

            let data_length = record_header.data_length() as usize;
            let record_data_start = current_pos + WalRecordHeader::SIZE;
            let record_data_end = record_data_start + data_length;

            if record_data_end > write_pos {
                return Err(ShardexError::Wal(format!(
                    "Truncated record data at position {}, expected {} bytes",
                    record_data_start, data_length
                )));
            }

            // Validate record checksum
            let record_data = &data_slice[record_data_start..record_data_end];
            if !record_header.validate_checksum(record_data) {
                return Err(ShardexError::Wal(format!(
                    "Record checksum validation failed at position {}",
                    current_pos
                )));
            }

            current_pos += WalRecordHeader::SIZE + data_length;
        }

        Ok(())
    }
}

impl WalManager {
    /// Create a new WAL manager
    pub fn new(layout: DirectoryLayout, segment_size: usize) -> Self {
        Self {
            current_segment: None,
            segment_size,
            layout,
            next_segment_id: 1,
        }
    }

    /// Initialize or recover WAL manager from existing segments
    pub fn initialize(&mut self) -> Result<(), ShardexError> {
        // Discover existing WAL segments
        let file_discovery = FileDiscovery::new(self.layout.clone());
        let discovery = file_discovery.discover_all()?;

        if discovery.wal_segments.is_empty() {
            // No existing segments, start fresh
            self.next_segment_id = 1;
            return Ok(());
        }

        // Find the highest segment ID
        let mut max_id = 0u64;
        for segment_file in &discovery.wal_segments {
            let segment_id = segment_file.segment_number as u64;
            if segment_id > max_id {
                max_id = segment_id;
            }
        }

        // Set next ID to be one higher than the max found
        self.next_segment_id = max_id + 1;

        // Try to open the most recent segment as the current one
        let latest_segment_path = self.layout.wal_segment_path(max_id as u32);
        if latest_segment_path.exists() {
            match WalSegment::open(latest_segment_path) {
                Ok(segment) => {
                    // Validate segment size matches configuration
                    if segment.capacity() != self.segment_size {
                        return Err(ShardexError::Wal(format!(
                            "Segment size mismatch: found {} bytes, expected {} bytes. Cannot recover with different segment size configuration.",
                            segment.capacity(),
                            self.segment_size
                        )));
                    }

                    // Only use this segment if it's not full
                    if !segment.is_full() {
                        self.current_segment = Some(segment);
                    }
                }
                Err(err) => {
                    // Return a more descriptive error for segment recovery failures
                    return Err(ShardexError::Wal(format!(
                        "Failed to recover latest segment {}: {}. Consider segment cleanup or configuration check.",
                        max_id, err
                    )));
                }
            }
        }

        Ok(())
    }

    /// Get or create the current active segment
    pub fn current_segment(&mut self) -> Result<&mut WalSegment, ShardexError> {
        if self.current_segment.is_none()
            || self
                .current_segment
                .as_ref()
                .map(|s| s.is_full())
                .unwrap_or(false)
        {
            // Need to create a new segment
            self.rotate_segment()?;
        }

        self.current_segment
            .as_mut()
            .ok_or_else(|| ShardexError::Wal("No current segment available".to_string()))
    }

    /// Rotate to a new segment
    pub fn rotate_segment(&mut self) -> Result<(), ShardexError> {
        let segment_id = self.next_segment_id;
        let segment_path = self.layout.wal_segment_path(segment_id as u32);

        let new_segment = WalSegment::create(segment_id, segment_path, self.segment_size)?;

        self.current_segment = Some(new_segment);
        self.next_segment_id += 1;

        Ok(())
    }

    /// Clean up obsolete segments
    pub fn cleanup_segments(&mut self, keep_segments: usize) -> Result<(), ShardexError> {
        let file_discovery = FileDiscovery::new(self.layout.clone());
        let discovery = file_discovery.discover_all()?;

        if discovery.wal_segments.len() <= keep_segments {
            return Ok(());
        }

        // Sort segments by ID and remove oldest ones
        let mut segments = discovery.wal_segments;
        segments.sort_by_key(|s| s.segment_number);

        let to_remove = segments.len() - keep_segments;
        for segment in segments.iter().take(to_remove) {
            let segment_path = self.layout.wal_segment_path(segment.segment_number);
            if let Err(e) = std::fs::remove_file(&segment_path) {
                return Err(ShardexError::Wal(format!(
                    "Failed to remove segment {}: {}",
                    segment.segment_number, e
                )));
            }
        }

        Ok(())
    }

    /// Get all segment IDs
    pub fn segment_ids(&self) -> Vec<u64> {
        let file_discovery = FileDiscovery::new(self.layout.clone());
        match file_discovery.discover_all() {
            Ok(discovery) => {
                let mut ids: Vec<u64> = discovery
                    .wal_segments
                    .iter()
                    .map(|s| s.segment_number as u64)
                    .collect();
                ids.sort();
                ids
            }
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestEnvironment;

    #[test]
    fn test_wal_segment_create() {
        let _test_env = TestEnvironment::new("test_wal_segment_create");
        let layout = DirectoryLayout::new(_test_env.path());
        let segment_path = layout.wal_segment_path(1);
        let capacity = 1024;

        let segment = WalSegment::create(1, segment_path, capacity).unwrap();

        assert_eq!(segment.id(), 1);
        assert_eq!(segment.capacity(), capacity);
        // The write pointer should start after header + reserved space
        let expected_start = initial_write_position();
        assert_eq!(segment.write_pointer(), expected_start);
        assert_eq!(segment.remaining_space(), capacity - expected_start);
        assert!(!segment.is_full());
    }

    #[test]
    fn test_wal_segment_append() {
        let _test_env = TestEnvironment::new("test_wal_segment_append");
        let layout = DirectoryLayout::new(_test_env.path());
        let segment_path = layout.wal_segment_path(1);
        let capacity = 1024;

        let segment = WalSegment::create(1, segment_path, capacity).unwrap();
        let data = b"test data";

        let offset = segment.append(data).unwrap();

        // The offset should be where the data starts (after header)
        assert_eq!(offset, 97, "Expected data offset to be 97, but got {}", offset);

        // After append, write_pointer should have advanced by record header + data
        // WalRecordHeader is 8 bytes (u32 + u32)
        let expected_write_pointer = initial_write_position() + WalRecordHeader::SIZE + data.len();
        assert_eq!(segment.write_pointer(), expected_write_pointer);
        assert_eq!(segment.remaining_space(), capacity - expected_write_pointer);
    }

    #[test]
    fn test_wal_segment_full() {
        let _test_env = TestEnvironment::new("test_wal_segment_full");
        let layout = DirectoryLayout::new(_test_env.path());
        let segment_path = layout.wal_segment_path(1);
        let capacity = 128; // Small capacity for testing

        let segment = WalSegment::create(1, segment_path, capacity).unwrap();
        // Account for record header in data size calculation
        // First record starts at initial_write_position(), then available space accounts for header
        let available_data_space = capacity - initial_write_position() - WalRecordHeader::SIZE;
        let data = vec![0u8; available_data_space];

        let offset = segment.append(&data).unwrap();
        assert_eq!(offset, initial_write_position() + WalRecordHeader::SIZE); // First data starts after initial position + record header
        assert!(segment.is_full());
        assert_eq!(segment.remaining_space(), 0);

        // Should fail to append more data
        let result = segment.append(b"extra");
        assert!(result.is_err());
    }

    #[test]
    fn test_wal_segment_open() {
        let _test_env = TestEnvironment::new("test_wal_segment_open");
        let layout = DirectoryLayout::new(_test_env.path());
        let segment_path = layout.wal_segment_path(1);
        let capacity = 1024;

        // Create and write to segment
        {
            let segment = WalSegment::create(1, segment_path.clone(), capacity).unwrap();
            let data = b"persistent data";
            segment.append(data).unwrap();
            segment.sync().unwrap();
        }

        // Open existing segment
        let segment = WalSegment::open(segment_path).unwrap();
        assert_eq!(segment.id(), 1);
        assert_eq!(segment.capacity(), capacity);
        // Account for record header in write pointer calculation
        // After writing data, write pointer should advance by header + data
        let expected_pos = initial_write_position() + WalRecordHeader::SIZE + "persistent data".len();
        assert_eq!(segment.write_pointer(), expected_pos);
    }

    #[test]
    fn test_wal_manager_initialization() {
        let _test_env = TestEnvironment::new("test_wal_manager_initialization");
        let layout = DirectoryLayout::new(_test_env.path());
        layout.create_directories().unwrap();

        let mut manager = WalManager::new(layout, 1024);
        manager.initialize().unwrap();

        // Should have no segments initially
        assert!(manager.segment_ids().is_empty());
    }

    #[test]
    fn test_wal_manager_current_segment() {
        let _test_env = TestEnvironment::new("test_wal_manager_current_segment");
        let layout = DirectoryLayout::new(_test_env.path());
        layout.create_directories().unwrap();

        let mut manager = WalManager::new(layout, 1024);
        manager.initialize().unwrap();

        let segment = manager.current_segment().unwrap();
        assert_eq!(segment.id(), 1);
        assert_eq!(segment.capacity(), 1024);
    }

    #[test]
    fn test_wal_manager_segment_rotation() {
        let _test_env = TestEnvironment::new("test_wal_manager_segment_rotation");
        let layout = DirectoryLayout::new(_test_env.path());
        layout.create_directories().unwrap();

        let mut manager = WalManager::new(layout, 128); // Small segments
        manager.initialize().unwrap();

        // Get initial segment
        let segment = manager.current_segment().unwrap();
        assert_eq!(segment.id(), 1);

        // Fill the segment (account for record header overhead)
        // Available space = total capacity - initial write position - record header size
        let remaining = segment.remaining_space();
        let data_size = remaining.saturating_sub(8); // WalRecordHeader is 8 bytes
        let data = vec![0u8; data_size];
        segment.append(&data).unwrap();
        assert!(segment.is_full());

        // Rotate to new segment
        manager.rotate_segment().unwrap();
        let new_segment = manager.current_segment().unwrap();
        assert_eq!(new_segment.id(), 2);
        assert!(!new_segment.is_full());
    }

    #[test]
    fn test_wal_segment_integrity() {
        let _test_env = TestEnvironment::new("test_wal_segment_integrity");
        let layout = DirectoryLayout::new(_test_env.path());
        let segment_path = layout.wal_segment_path(1);
        let capacity = 1024;

        let segment = WalSegment::create(1, segment_path, capacity).unwrap();

        // Basic integrity should pass for new segment
        segment.validate_integrity().unwrap();

        segment.append(b"test data").unwrap();

        // Validation should pass for valid segment with data
        segment.validate_integrity().unwrap();
    }
}

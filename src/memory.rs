//! Memory-mapped file abstractions for Shardex
//!
//! This module provides safe abstractions over memory-mapped files using memmap2,
//! with comprehensive error handling, data integrity validation, and file management
//! utilities. All operations are designed to be async-compatible and memory-safe.
//!
//! # Key Components
//!
//! - [`MemoryMappedFile`]: Core wrapper around memmap2 with safety abstractions
//! - [`StandardHeader`]: Comprehensive file header with metadata and version control
//! - File creation and resizing utilities with proper error handling
//!
//! # Usage Examples
//!
//! ## Creating and Writing to a Memory-Mapped File
//!
//! ```rust
//! use shardex::memory::{MemoryMappedFile, StandardHeader};
//! use tempfile::TempDir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let file_path = temp_dir.path().join("test.dat");
//!
//! // Create a new memory-mapped file with initial size
//! let mut mmf = MemoryMappedFile::create(&file_path, 1024)?;
//!
//! // Write a header with magic bytes and version
//! let header = StandardHeader::new(b"SHRD", 1, StandardHeader::SIZE as u64, &[42u8; 100]);
//! mmf.write_at(0, &header)?;
//!
//! // Write some data
//! let data: [u32; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
//! mmf.write_at(StandardHeader::SIZE, &data)?;
//!
//! // Sync to disk
//! mmf.sync()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Reading from a Memory-Mapped File
//!
//! ```rust
//! use shardex::memory::{MemoryMappedFile, StandardHeader};
//! use std::path::Path;
//!
//! # fn read_example(file_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
//! // Open existing file for reading
//! let mmf = MemoryMappedFile::open_read_only(file_path)?;
//!
//! // Read and validate header
//! let header: StandardHeader = mmf.read_at(0)?;
//! header.validate_magic(b"SHRD")?;
//! header.validate_checksum(&mmf.as_slice()[StandardHeader::SIZE..])?;
//!
//! // Read typed data
//! let data: [u32; 10] = mmf.read_at(StandardHeader::SIZE)?;
//! println!("First element: {}", data[0]);
//! # Ok(())
//! # }
//! ```
//!
//! ## File Resizing and Management
//!
//! ```rust
//! use shardex::memory::MemoryMappedFile;
//! use tempfile::TempDir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let file_path = temp_dir.path().join("growing.dat");
//!
//! let mut mmf = MemoryMappedFile::create(&file_path, 512)?;
//! assert_eq!(mmf.len(), 512);
//!
//! // Resize the file
//! mmf.resize(1024)?;
//! assert_eq!(mmf.len(), 1024);
//! # Ok(())
//! # }
//! ```

use crate::error::ShardexError;
use bytemuck::{Pod, Zeroable};
use memmap2::{Mmap, MmapMut, MmapOptions};
use std::fs::{File, OpenOptions};
use std::path::Path;

/// Memory-mapped file wrapper providing safe abstractions over memmap2
///
/// MemoryMappedFile provides type-safe operations on memory-mapped files with
/// comprehensive bounds checking, alignment validation, and error handling.
/// It supports both read-only and read-write access patterns.
#[derive(Debug)]
pub struct MemoryMappedFile {
    mmap: MmapVariant,
    file: Option<File>,
    len: usize,
}

#[derive(Debug)]
enum MmapVariant {
    ReadOnly(Mmap),
    ReadWrite(MmapMut),
}

/// Standardized file header for all Shardex file types
///
/// StandardHeader provides comprehensive file header management with version control,
/// metadata tracking, and integrity validation. All file types in Shardex use this
/// standardized header format for consistency and forward compatibility.
///
/// # Layout
/// ```text
/// Offset  | Size | Field         | Description
/// --------|------|---------------|------------------------------------------
/// 0       | 4    | magic         | File type identifier (e.g., "SHRD", "VECT")  
/// 4       | 4    | version       | Format version number
/// 8       | 4    | header_size   | Size of complete header in bytes
/// 12      | 4    | padding       | Alignment padding for data_offset
/// 16      | 8    | data_offset   | Offset to data section from file start
/// 24      | 4    | checksum      | Header + data checksum (CRC32)
/// 28      | 4    | padding       | Alignment padding for timestamps
/// 32      | 8    | created_at    | Creation timestamp (Unix epoch microseconds)
/// 40      | 8    | modified_at   | Last modification timestamp
/// 48      | 32   | reserved      | Reserved for future use (zeroed)
/// Total   | 80   |               | Complete header size (with alignment)
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct StandardHeader {
    /// Magic bytes for file type identification (4 bytes)
    pub magic: [u8; 4],
    /// Format version number
    pub version: u32,
    /// Size of complete header in bytes
    pub header_size: u32,
    /// Offset to data section from file start
    pub data_offset: u64,
    /// CRC32 checksum of header + data
    pub checksum: u32,
    /// Creation timestamp (Unix epoch microseconds)
    pub created_at: u64,
    /// Last modification timestamp (Unix epoch microseconds)
    pub modified_at: u64,
    /// Reserved for future use (must be zero)
    pub reserved: [u8; 32],
}

/// Legacy file header type alias for backward compatibility
pub type FileHeader = StandardHeader;

// SAFETY: StandardHeader contains only Pod types and has repr(C) layout
unsafe impl Pod for StandardHeader {}
// SAFETY: StandardHeader can be safely zero-initialized
unsafe impl Zeroable for StandardHeader {}

impl MemoryMappedFile {
    /// Create a new memory-mapped file with the specified size
    ///
    /// Creates parent directories if they don't exist and initializes
    /// the file with zeros. The file is opened in read-write mode.
    pub fn create<P: AsRef<Path>>(path: P, size: usize) -> Result<Self, ShardexError> {
        let path = path.as_ref();

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ShardexError::MemoryMapping(format!(
                    "Failed to create parent directories for {}: {}",
                    path.display(),
                    e
                ))
            })?;
        }

        // Create and size the file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(|e| ShardexError::MemoryMapping(format!("Failed to create file {}: {}", path.display(), e)))?;

        // Set file size
        file.set_len(size as u64).map_err(|e| {
            ShardexError::MemoryMapping(format!("Failed to set file size for {}: {}", path.display(), e))
        })?;

        // Create memory mapping
        let mmap = unsafe {
            MmapOptions::new().map_mut(&file).map_err(|e| {
                ShardexError::MemoryMapping(format!("Failed to create memory mapping for {}: {}", path.display(), e))
            })?
        };

        Ok(Self {
            mmap: MmapVariant::ReadWrite(mmap),
            file: Some(file),
            len: size,
        })
    }

    /// Open an existing file in read-only mode
    ///
    /// Maps the entire file contents into memory for reading.
    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Self, ShardexError> {
        let path = path.as_ref();

        let file = File::open(path)
            .map_err(|e| ShardexError::MemoryMapping(format!("Failed to open file {}: {}", path.display(), e)))?;

        let len = file
            .metadata()
            .map_err(|e| {
                ShardexError::MemoryMapping(format!("Failed to get file metadata for {}: {}", path.display(), e))
            })?
            .len() as usize;

        let mmap = unsafe {
            MmapOptions::new().map(&file).map_err(|e| {
                ShardexError::MemoryMapping(format!(
                    "Failed to create read-only memory mapping for {}: {}",
                    path.display(),
                    e
                ))
            })?
        };

        Ok(Self {
            mmap: MmapVariant::ReadOnly(mmap),
            file: Some(file),
            len,
        })
    }

    /// Open an existing file in read-write mode
    ///
    /// Maps the entire file contents into memory for reading and writing.
    pub fn open_read_write<P: AsRef<Path>>(path: P) -> Result<Self, ShardexError> {
        let path = path.as_ref();

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| {
                ShardexError::MemoryMapping(format!("Failed to open file {} for read-write: {}", path.display(), e))
            })?;

        let len = file
            .metadata()
            .map_err(|e| {
                ShardexError::MemoryMapping(format!("Failed to get file metadata for {}: {}", path.display(), e))
            })?
            .len() as usize;

        let mmap = unsafe {
            MmapOptions::new().map_mut(&file).map_err(|e| {
                ShardexError::MemoryMapping(format!(
                    "Failed to create read-write memory mapping for {}: {}",
                    path.display(),
                    e
                ))
            })?
        };

        Ok(Self {
            mmap: MmapVariant::ReadWrite(mmap),
            file: Some(file),
            len,
        })
    }

    /// Get the length of the mapped file
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if the mapped file is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get a reference to the mapped memory as a byte slice
    pub fn as_slice(&self) -> &[u8] {
        match &self.mmap {
            MmapVariant::ReadOnly(mmap) => mmap.as_ref(),
            MmapVariant::ReadWrite(mmap) => mmap.as_ref(),
        }
    }

    /// Get a mutable reference to the mapped memory as a byte slice
    ///
    /// Returns an error if the file was opened in read-only mode.
    pub fn as_mut_slice(&mut self) -> Result<&mut [u8], ShardexError> {
        match &mut self.mmap {
            MmapVariant::ReadOnly(_) => Err(ShardexError::MemoryMapping(
                "Cannot get mutable reference to read-only mapping".to_string(),
            )),
            MmapVariant::ReadWrite(mmap) => Ok(mmap.as_mut()),
        }
    }

    /// Read a Pod type from the specified offset
    ///
    /// Performs bounds checking and alignment verification before reading.
    pub fn read_at<T: Pod>(&self, offset: usize) -> Result<T, ShardexError> {
        let size = std::mem::size_of::<T>();

        if offset + size > self.len {
            return Err(ShardexError::MemoryMapping(format!(
                "Read at offset {} with size {} exceeds file length {}",
                offset, size, self.len
            )));
        }

        // Check alignment
        if offset % std::mem::align_of::<T>() != 0 {
            return Err(ShardexError::MemoryMapping(format!(
                "Offset {} is not properly aligned for type {} (requires {} byte alignment)",
                offset,
                std::any::type_name::<T>(),
                std::mem::align_of::<T>()
            )));
        }

        let bytes = &self.as_slice()[offset..offset + size];
        Ok(bytemuck::pod_read_unaligned(bytes))
    }

    /// Write a Pod type to the specified offset
    ///
    /// Performs bounds checking and alignment verification before writing.
    pub fn write_at<T: Pod>(&mut self, offset: usize, value: &T) -> Result<(), ShardexError> {
        let size = std::mem::size_of::<T>();

        if offset + size > self.len {
            return Err(ShardexError::MemoryMapping(format!(
                "Write at offset {} with size {} exceeds file length {}",
                offset, size, self.len
            )));
        }

        // Check alignment
        if offset % std::mem::align_of::<T>() != 0 {
            return Err(ShardexError::MemoryMapping(format!(
                "Offset {} is not properly aligned for type {} (requires {} byte alignment)",
                offset,
                std::any::type_name::<T>(),
                std::mem::align_of::<T>()
            )));
        }

        let bytes = bytemuck::bytes_of(value);
        let mut_slice = self.as_mut_slice()?;
        mut_slice[offset..offset + size].copy_from_slice(bytes);
        Ok(())
    }

    /// Read multiple Pod values from the specified offset
    ///
    /// Returns a slice view into the mapped memory without copying data.
    pub fn read_slice_at<T: Pod>(&self, offset: usize, count: usize) -> Result<&[T], ShardexError> {
        let size = std::mem::size_of::<T>() * count;

        if offset + size > self.len {
            return Err(ShardexError::MemoryMapping(format!(
                "Read slice at offset {} with size {} exceeds file length {}",
                offset, size, self.len
            )));
        }

        // Check alignment
        if offset % std::mem::align_of::<T>() != 0 {
            return Err(ShardexError::MemoryMapping(format!(
                "Offset {} is not properly aligned for type {} (requires {} byte alignment)",
                offset,
                std::any::type_name::<T>(),
                std::mem::align_of::<T>()
            )));
        }

        let bytes = &self.as_slice()[offset..offset + size];
        Ok(bytemuck::cast_slice(bytes))
    }

    /// Write multiple Pod values to the specified offset
    pub fn write_slice_at<T: Pod>(&mut self, offset: usize, values: &[T]) -> Result<(), ShardexError> {
        let size = std::mem::size_of_val(values);

        if offset + size > self.len {
            return Err(ShardexError::MemoryMapping(format!(
                "Write slice at offset {} with size {} exceeds file length {}",
                offset, size, self.len
            )));
        }

        // Check alignment
        if offset % std::mem::align_of::<T>() != 0 {
            return Err(ShardexError::MemoryMapping(format!(
                "Offset {} is not properly aligned for type {} (requires {} byte alignment)",
                offset,
                std::any::type_name::<T>(),
                std::mem::align_of::<T>()
            )));
        }

        let bytes = bytemuck::cast_slice(values);
        let mut_slice = self.as_mut_slice()?;
        mut_slice[offset..offset + size].copy_from_slice(bytes);
        Ok(())
    }

    /// Resize the memory-mapped file
    ///
    /// Creates a new mapping with the specified size. Data is preserved
    /// up to the minimum of the old and new sizes.
    pub fn resize(&mut self, new_size: usize) -> Result<(), ShardexError> {
        let file = self
            .file
            .as_ref()
            .ok_or_else(|| ShardexError::MemoryMapping("No file handle available for resize".to_string()))?;

        // Check if we're in read-write mode
        if matches!(self.mmap, MmapVariant::ReadOnly(_)) {
            return Err(ShardexError::MemoryMapping(
                "Cannot resize read-only memory mapping".to_string(),
            ));
        }

        // Drop the current mapping before resizing
        self.mmap = MmapVariant::ReadWrite(
            MmapMut::map_anon(0)
                .map_err(|e| ShardexError::MemoryMapping(format!("Failed to create temporary mapping: {}", e)))?,
        );

        // Resize the file
        file.set_len(new_size as u64)
            .map_err(|e| ShardexError::MemoryMapping(format!("Failed to resize file: {}", e)))?;

        // Create new mapping
        let new_mmap = unsafe {
            MmapOptions::new()
                .map_mut(file)
                .map_err(|e| ShardexError::MemoryMapping(format!("Failed to create new memory mapping: {}", e)))?
        };

        self.mmap = MmapVariant::ReadWrite(new_mmap);
        self.len = new_size;

        Ok(())
    }

    /// Synchronize mapped memory with the underlying file
    ///
    /// Forces any changes to be written to disk.
    pub fn sync(&self) -> Result<(), ShardexError> {
        match &self.mmap {
            MmapVariant::ReadOnly(_) => Ok(()), // Read-only mappings don't need syncing
            MmapVariant::ReadWrite(mmap) => {
                mmap.flush()
                    .map_err(|e| ShardexError::MemoryMapping(format!("Failed to sync memory mapping: {}", e)))?;
                Ok(())
            }
        }
    }

    /// Check if the mapping is read-only
    pub fn is_read_only(&self) -> bool {
        matches!(self.mmap, MmapVariant::ReadOnly(_))
    }
}

impl StandardHeader {
    /// Size of the StandardHeader structure in bytes
    pub const SIZE: usize = std::mem::size_of::<StandardHeader>();

    /// Current timestamp in microseconds since Unix epoch
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64
    }

    /// Create a new standard header with magic bytes, version, and data
    ///
    /// The header is created with current timestamps and checksum calculated
    /// from the provided data slice.
    pub fn new(magic: &[u8; 4], version: u32, data_offset: u64, data: &[u8]) -> Self {
        let now = Self::current_timestamp();
        let mut header = Self {
            magic: *magic,
            version,
            header_size: Self::SIZE as u32,
            data_offset,
            checksum: 0, // Will be calculated below
            created_at: now,
            modified_at: now,
            reserved: [0; 32],
        };

        header.update_checksum(data);
        header
    }

    /// Create a standard header without checksum validation
    ///
    /// Use this when the checksum will be calculated and set later.
    pub fn new_without_checksum(magic: &[u8; 4], version: u32, data_offset: u64) -> Self {
        let now = Self::current_timestamp();
        Self {
            magic: *magic,
            version,
            header_size: Self::SIZE as u32,
            data_offset,
            checksum: 0,
            created_at: now,
            modified_at: now,
            reserved: [0; 32],
        }
    }

    /// Create a header with explicit timestamps (for testing)
    pub fn new_with_timestamps(
        magic: &[u8; 4],
        version: u32,
        data_offset: u64,
        created_at: u64,
        modified_at: u64,
        data: &[u8],
    ) -> Self {
        let mut header = Self {
            magic: *magic,
            version,
            header_size: Self::SIZE as u32,
            data_offset,
            checksum: 0,
            created_at,
            modified_at,
            reserved: [0; 32],
        };

        header.update_checksum(data);
        header
    }

    /// Validate that the magic bytes match expected values
    pub fn validate_magic(&self, expected_magic: &[u8; 4]) -> Result<(), ShardexError> {
        if &self.magic != expected_magic {
            return Err(ShardexError::Corruption(format!(
                "Magic bytes mismatch: expected {:?}, found {:?}",
                expected_magic, self.magic
            )));
        }
        Ok(())
    }

    /// Validate version compatibility
    pub fn validate_version(&self, min_version: u32, max_version: u32) -> Result<(), ShardexError> {
        if self.version < min_version {
            return Err(ShardexError::Corruption(format!(
                "Version {} is too old (minimum supported: {})",
                self.version, min_version
            )));
        }

        if self.version > max_version {
            return Err(ShardexError::Corruption(format!(
                "Version {} is too new (maximum supported: {})",
                self.version, max_version
            )));
        }

        Ok(())
    }

    /// Check if version is exactly the expected version
    pub fn is_version(&self, expected_version: u32) -> bool {
        self.version == expected_version
    }

    /// Check if version is compatible (within range)
    pub fn is_compatible(&self, min_version: u32, max_version: u32) -> bool {
        self.version >= min_version && self.version <= max_version
    }

    /// Validate header structure integrity
    pub fn validate_structure(&self) -> Result<(), ShardexError> {
        // Check header size matches expected
        if self.header_size != Self::SIZE as u32 {
            return Err(ShardexError::Corruption(format!(
                "Invalid header size: expected {}, found {}",
                Self::SIZE,
                self.header_size
            )));
        }

        // Check data offset is reasonable (at least past header)
        if self.data_offset < Self::SIZE as u64 {
            return Err(ShardexError::Corruption(format!(
                "Invalid data offset: {} is less than header size {}",
                self.data_offset,
                Self::SIZE
            )));
        }

        // Check timestamps are reasonable (not in future, created <= modified)
        let now = Self::current_timestamp();
        if self.created_at > now {
            return Err(ShardexError::Corruption(format!(
                "Creation timestamp {} is in the future",
                self.created_at
            )));
        }

        if self.modified_at > now {
            return Err(ShardexError::Corruption(format!(
                "Modification timestamp {} is in the future",
                self.modified_at
            )));
        }

        if self.created_at > self.modified_at {
            return Err(ShardexError::Corruption(format!(
                "Creation timestamp {} is after modification timestamp {}",
                self.created_at, self.modified_at
            )));
        }

        // Check reserved bytes are zero
        if self.reserved != [0; 32] {
            return Err(ShardexError::Corruption("Reserved bytes are not zero".to_string()));
        }

        Ok(())
    }

    /// Validate the checksum against provided data
    pub fn validate_checksum(&self, data: &[u8]) -> Result<(), ShardexError> {
        let expected_checksum = Self::calculate_checksum_with_header(self, data);
        if self.checksum != expected_checksum {
            return Err(ShardexError::Corruption(format!(
                "Checksum mismatch: expected {}, found {}",
                expected_checksum, self.checksum
            )));
        }
        Ok(())
    }

    /// Perform complete header validation
    pub fn validate_complete(
        &self,
        expected_magic: &[u8; 4],
        min_version: u32,
        max_version: u32,
        data: &[u8],
    ) -> Result<(), ShardexError> {
        self.validate_magic(expected_magic)?;
        self.validate_version(min_version, max_version)?;
        self.validate_structure()?;
        self.validate_checksum(data)?;
        Ok(())
    }

    /// Update the header for modification (timestamp and checksum)
    pub fn update_for_modification(&mut self, data: &[u8]) {
        self.modified_at = Self::current_timestamp();
        self.update_checksum(data);
    }

    /// Update the checksum based on provided data
    pub fn update_checksum(&mut self, data: &[u8]) {
        self.checksum = Self::calculate_checksum_with_header(self, data);
    }

    /// Calculate CRC32 checksum including header metadata (excluding checksum field)
    fn calculate_checksum_with_header(header: &Self, data: &[u8]) -> u32 {
        // Create a normalized copy of the header for deterministic checksum calculation
        let mut header_copy = *header;
        header_copy.checksum = 0;
        // Zero out all potentially variable fields to ensure deterministic calculation
        header_copy.created_at = 0;
        header_copy.modified_at = 0;
        // Also normalize reserved field to all zeros
        header_copy.reserved = [0; 32];

        // Calculate checksum of header bytes (excluding checksum field)
        let header_bytes = bytemuck::bytes_of(&header_copy);
        let mut crc = 0xFFFFFFFF;

        // Hash header bytes
        crc = Self::crc32_update(crc, header_bytes);
        // Hash data bytes
        crc = Self::crc32_update(crc, data);

        crc ^ 0xFFFFFFFF
    }

    /// Core CRC32 hash implementation
    pub fn crc32_hash(data: &[u8]) -> u32 {
        let mut crc = 0xFFFFFFFF;
        crc = Self::crc32_update(crc, data);
        crc ^ 0xFFFFFFFF
    }

    /// Update CRC32 with additional data
    fn crc32_update(mut crc: u32, data: &[u8]) -> u32 {
        const CRC32_TABLE: [u32; 256] = generate_crc32_table();

        for &byte in data {
            let table_index = ((crc ^ u32::from(byte)) & 0xFF) as usize;
            crc = (crc >> 8) ^ CRC32_TABLE[table_index];
        }

        crc
    }
}

/// Generate CRC32 lookup table
const fn generate_crc32_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;

    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;

        while j < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
            j += 1;
        }

        table[i] = crc;
        i += 1;
    }

    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_create_memory_mapped_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.dat");

        let mmf = MemoryMappedFile::create(&file_path, 1024).unwrap();
        assert_eq!(mmf.len(), 1024);
        assert!(!mmf.is_empty());
        assert!(!mmf.is_read_only());
    }

    #[test]
    fn test_read_write_operations() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("rw_test.dat");

        let mut mmf = MemoryMappedFile::create(&file_path, 1024).unwrap();

        // Write a u64
        let value: u64 = 0x1234567890ABCDEF;
        mmf.write_at(0, &value).unwrap();

        // Read it back
        let read_value: u64 = mmf.read_at(0).unwrap();
        assert_eq!(value, read_value);
    }

    #[test]
    fn test_slice_operations() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("slice_test.dat");

        let mut mmf = MemoryMappedFile::create(&file_path, 1024).unwrap();

        // Write an array
        let values: [u32; 5] = [1, 2, 3, 4, 5];
        mmf.write_slice_at(0, &values).unwrap();

        // Read it back
        let read_values: &[u32] = mmf.read_slice_at(0, 5).unwrap();
        assert_eq!(&values[..], read_values);
    }

    #[test]
    fn test_bounds_checking() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("bounds_test.dat");

        let mut mmf = MemoryMappedFile::create(&file_path, 64).unwrap();

        // Try to read beyond bounds
        let result: Result<u64, _> = mmf.read_at(60);
        assert!(result.is_err());

        // Try to write beyond bounds
        let value: u64 = 42;
        let result = mmf.write_at(60, &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_alignment_checking() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("align_test.dat");

        let mut mmf = MemoryMappedFile::create(&file_path, 1024).unwrap();

        // Try to read u64 at unaligned offset
        let result: Result<u64, _> = mmf.read_at(1);
        assert!(result.is_err());

        // Try to write u64 at unaligned offset
        let value: u64 = 42;
        let result = mmf.write_at(1, &value);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_resize() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("resize_test.dat");

        let mut mmf = MemoryMappedFile::create(&file_path, 512).unwrap();
        assert_eq!(mmf.len(), 512);

        // Write some data
        let value: u64 = 0xDEADBEEF;
        mmf.write_at(0, &value).unwrap();

        // Resize
        mmf.resize(1024).unwrap();
        assert_eq!(mmf.len(), 1024);

        // Verify data is still there
        let read_value: u64 = mmf.read_at(0).unwrap();
        assert_eq!(value, read_value);
    }

    #[test]
    fn test_read_only_mapping() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path();

        // Write some data to the file first
        {
            let mut mmf = MemoryMappedFile::create(file_path, 64).unwrap();
            let value: u32 = 42;
            mmf.write_at(0, &value).unwrap();
            mmf.sync().unwrap();
        }

        // Open read-only
        let mmf = MemoryMappedFile::open_read_only(file_path).unwrap();
        assert!(mmf.is_read_only());

        // Should be able to read
        let value: u32 = mmf.read_at(0).unwrap();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_standard_header_creation() {
        let magic = b"TEST";
        let data = b"Hello, World!";
        let header = StandardHeader::new(magic, 1, StandardHeader::SIZE as u64, data);

        assert_eq!(header.magic, *magic);
        assert_eq!(header.version, 1);
        assert_eq!(header.header_size, StandardHeader::SIZE as u32);
        assert_eq!(header.data_offset, StandardHeader::SIZE as u64);
        assert_ne!(header.checksum, 0); // Should have calculated checksum
        assert!(header.created_at > 0);
        assert!(header.modified_at > 0);
        assert_eq!(header.reserved, [0; 32]);
    }

    #[test]
    fn test_standard_header_magic_validation() {
        let header = StandardHeader::new(b"TEST", 1, StandardHeader::SIZE as u64, b"data");

        // Valid magic
        assert!(header.validate_magic(b"TEST").is_ok());

        // Invalid magic
        assert!(header.validate_magic(b"FAIL").is_err());
    }

    #[test]
    fn test_standard_header_checksum_validation() {
        let data = b"Hello, World!";
        let header = StandardHeader::new(b"TEST", 1, StandardHeader::SIZE as u64, data);

        // Valid checksum
        assert!(header.validate_checksum(data).is_ok());

        // Invalid checksum
        assert!(header.validate_checksum(b"Different data").is_err());
    }

    #[test]
    fn test_standard_header_update_checksum() {
        let mut header = StandardHeader::new_without_checksum(b"TEST", 1, StandardHeader::SIZE as u64);
        assert_eq!(header.checksum, 0);

        let data = b"Some data";
        header.update_checksum(data);
        assert_ne!(header.checksum, 0);

        // Should validate correctly
        assert!(header.validate_checksum(data).is_ok());
    }

    #[test]
    fn test_standard_header_bytemuck() {
        let header = StandardHeader::new(b"TEST", 1, StandardHeader::SIZE as u64, b"data");

        // Should be able to convert to bytes
        let bytes = bytemuck::bytes_of(&header);
        assert_eq!(bytes.len(), StandardHeader::SIZE);

        // Should be able to convert back
        let header_restored = bytemuck::from_bytes::<StandardHeader>(bytes);
        assert_eq!(header.magic, header_restored.magic);
        assert_eq!(header.version, header_restored.version);
        assert_eq!(header.checksum, header_restored.checksum);
        assert_eq!(header.header_size, header_restored.header_size);
        assert_eq!(header.data_offset, header_restored.data_offset);
        assert_eq!(header.created_at, header_restored.created_at);
        assert_eq!(header.modified_at, header_restored.modified_at);
        assert_eq!(header.reserved, header_restored.reserved);
    }

    #[test]
    fn test_memory_mapped_file_with_header() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("header_test.dat");

        let test_data = vec![1u32, 2, 3, 4, 5];
        let data_bytes = bytemuck::cast_slice(&test_data);

        // Create file and write header + data
        {
            let mut mmf = MemoryMappedFile::create(&file_path, 1024).unwrap();

            let header = StandardHeader::new(b"SHRD", 1, StandardHeader::SIZE as u64, data_bytes);
            mmf.write_at(0, &header).unwrap();
            mmf.write_slice_at(StandardHeader::SIZE, &test_data)
                .unwrap();
            mmf.sync().unwrap();
        }

        // Read back and validate
        {
            let mmf = MemoryMappedFile::open_read_only(&file_path).unwrap();

            let header: StandardHeader = mmf.read_at(0).unwrap();
            header.validate_magic(b"SHRD").unwrap();

            let read_data: &[u32] = mmf
                .read_slice_at(StandardHeader::SIZE, test_data.len())
                .unwrap();
            header
                .validate_checksum(bytemuck::cast_slice(read_data))
                .unwrap();

            assert_eq!(read_data, &test_data[..]);
        }
    }

    #[test]
    fn test_parent_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let nested_path = temp_dir.path().join("nested").join("dirs").join("test.dat");

        // Should create parent directories automatically
        let mmf = MemoryMappedFile::create(&nested_path, 64).unwrap();
        assert_eq!(mmf.len(), 64);
        assert!(nested_path.exists());
    }

    #[test]
    fn test_zero_size_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.dat");

        let mmf = MemoryMappedFile::create(&file_path, 0).unwrap();
        assert_eq!(mmf.len(), 0);
        assert!(mmf.is_empty());
    }

    #[test]
    fn test_standard_header_version_validation() {
        let header = StandardHeader::new(b"TEST", 5, StandardHeader::SIZE as u64, b"data");

        // Valid version ranges
        assert!(header.validate_version(1, 10).is_ok());
        assert!(header.validate_version(5, 5).is_ok());

        // Version too old
        assert!(header.validate_version(6, 10).is_err());

        // Version too new
        assert!(header.validate_version(1, 4).is_err());

        // Test convenience methods
        assert!(header.is_version(5));
        assert!(!header.is_version(4));
        assert!(header.is_compatible(1, 10));
        assert!(!header.is_compatible(6, 10));
    }

    #[test]
    fn test_standard_header_structure_validation() {
        let mut header = StandardHeader::new(b"TEST", 1, StandardHeader::SIZE as u64, b"data");

        // Valid structure
        assert!(header.validate_structure().is_ok());

        // Invalid header size
        header.header_size = 50;
        assert!(header.validate_structure().is_err());
        header.header_size = StandardHeader::SIZE as u32;

        // Invalid data offset (too small)
        header.data_offset = 10;
        assert!(header.validate_structure().is_err());
        header.data_offset = StandardHeader::SIZE as u64;

        // Invalid timestamps (created after modified)
        header.created_at = 2000;
        header.modified_at = 1000;
        assert!(header.validate_structure().is_err());
        header.created_at = 1000;
        header.modified_at = 2000;

        // Invalid reserved bytes
        header.reserved[0] = 1;
        assert!(header.validate_structure().is_err());
        header.reserved[0] = 0;

        // Should be valid again
        assert!(header.validate_structure().is_ok());
    }

    #[test]
    fn test_standard_header_complete_validation() {
        let data = b"test data";
        let header = StandardHeader::new(b"TEST", 2, StandardHeader::SIZE as u64, data);

        // Complete validation should pass
        assert!(header.validate_complete(b"TEST", 1, 5, data).is_ok());

        // Should fail with wrong magic
        assert!(header.validate_complete(b"FAIL", 1, 5, data).is_err());

        // Should fail with version out of range
        assert!(header.validate_complete(b"TEST", 3, 5, data).is_err());
        assert!(header.validate_complete(b"TEST", 1, 1, data).is_err());

        // Should fail with wrong data
        assert!(header
            .validate_complete(b"TEST", 1, 5, b"wrong data")
            .is_err());
    }

    #[test]
    fn test_standard_header_update_for_modification() {
        let initial_data = b"initial data";
        let mut header =
            StandardHeader::new_with_timestamps(b"TEST", 1, StandardHeader::SIZE as u64, 1000, 1000, initial_data);

        // Simulate some time passing and data changing
        let new_data = b"modified data";
        header.update_for_modification(new_data);

        // Modified timestamp should be updated (greater than 1000)
        assert!(header.modified_at > 1000);
        // Created timestamp should remain the same
        assert_eq!(header.created_at, 1000);

        // Should validate with new data
        assert!(header.validate_checksum(new_data).is_ok());
        // Should not validate with old data
        assert!(header.validate_checksum(initial_data).is_err());
    }

    #[test]
    fn test_standard_header_with_timestamps() {
        let created = 1000;
        let modified = 2000;
        let data = b"test data";

        let header =
            StandardHeader::new_with_timestamps(b"TEST", 1, StandardHeader::SIZE as u64, created, modified, data);

        assert_eq!(header.created_at, created);
        assert_eq!(header.modified_at, modified);
        assert!(header.validate_checksum(data).is_ok());
    }

    #[test]
    fn test_standard_header_size_constants() {
        // Verify the header size is exactly what we expect (80 bytes with alignment)
        assert_eq!(StandardHeader::SIZE, 80);

        // Verify the structure layout
        let header = StandardHeader::new(b"TEST", 1, StandardHeader::SIZE as u64, b"data");
        let bytes = bytemuck::bytes_of(&header);
        assert_eq!(bytes.len(), 80);
    }

    #[test]
    fn test_file_header_compatibility() {
        // Test that FileHeader is an alias for StandardHeader
        let header: FileHeader = StandardHeader::new(b"TEST", 1, StandardHeader::SIZE as u64, b"data");
        assert_eq!(header.magic, *b"TEST");
        assert_eq!(header.version, 1);

        // Test that FileHeader methods work
        assert!(header.validate_magic(b"TEST").is_ok());
        assert!(header.validate_checksum(b"data").is_ok());
    }

    #[test]
    fn test_crc32_consistency() {
        let data1 = b"Hello, World!";
        let data2 = b"Hello, World!";
        let data3 = b"Different data";

        // Direct CRC32 should be consistent
        let checksum1 = StandardHeader::crc32_hash(data1);
        let checksum2 = StandardHeader::crc32_hash(data2);
        let checksum3 = StandardHeader::crc32_hash(data3);

        assert_eq!(checksum1, checksum2);
        assert_ne!(checksum1, checksum3);

        // Headers with identical header metadata and same data should have same checksum
        // Use explicit timestamps to ensure headers are identical
        let header1 = StandardHeader::new_with_timestamps(b"TEST", 1, StandardHeader::SIZE as u64, 1000, 1000, data1);
        let header2 = StandardHeader::new_with_timestamps(b"TEST", 1, StandardHeader::SIZE as u64, 1000, 1000, data2);
        assert_eq!(header1.checksum, header2.checksum);

        // Different data should produce different checksums
        let header3 = StandardHeader::new_with_timestamps(b"TEST", 1, StandardHeader::SIZE as u64, 1000, 1000, data3);
        assert_ne!(header1.checksum, header3.checksum);
    }

    #[test]
    fn test_sync_operations() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("sync_test.dat");

        let mut mmf = MemoryMappedFile::create(&file_path, 64).unwrap();
        let value: u64 = 0x123456789ABCDEF0;
        mmf.write_at(0, &value).unwrap();

        // Sync should not fail
        mmf.sync().unwrap();
    }
}

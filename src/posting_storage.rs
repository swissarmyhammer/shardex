//! Memory-mapped posting storage for efficient posting metadata management
//!
//! This module provides fixed-size memory-mapped storage for posting metadata with
//! efficient lookups, updates, and deletion tracking. It maintains parallel arrays
//! for document IDs, positions, lengths, and deleted flags, designed to work in
//! conjunction with VectorStorage for complete posting management.
//!
//! # Key Components
//!
//! - [`PostingStorage`]: Main posting metadata container with memory-mapped arrays
//! - [`PostingStorageHeader`]: Memory-mapped header with metadata and configuration
//! - Efficient lookup and iteration with deleted posting filtering
//!
//! # Usage Examples
//!
//! ## Creating and Using Posting Storage
//!
//! ```rust
//! use shardex::posting_storage::PostingStorage;
//! use shardex::identifiers::DocumentId;
//! use tempfile::TempDir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let storage_path = temp_dir.path().join("postings.dat");
//!
//! // Create storage with initial capacity of 1000 postings
//! let mut storage = PostingStorage::create(&storage_path, 1000)?;
//!
//! // Add some postings
//! let doc_id1 = DocumentId::new();
//! let doc_id2 = DocumentId::new();
//! let idx1 = storage.add_posting(doc_id1, 100, 50)?;
//! let idx2 = storage.add_posting(doc_id2, 200, 75)?;
//!
//! // Access postings (zero-copy)
//! let (retrieved_doc_id, start, length) = storage.get_posting(idx1)?;
//! assert_eq!(retrieved_doc_id, doc_id1);
//! assert_eq!(start, 100);
//! assert_eq!(length, 50);
//!
//! // Update a posting in place
//! storage.update_posting(idx1, doc_id1, 150, 60)?;
//!
//! // Remove a posting (tombstone marking)
//! storage.remove_posting(idx2)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Memory-Mapped Persistence
//!
//! ```rust
//! use shardex::posting_storage::PostingStorage;
//! use shardex::identifiers::DocumentId;
//! use std::path::Path;
//!
//! # fn persistence_example(storage_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
//! // Create and populate storage
//! {
//!     let mut storage = PostingStorage::create(storage_path, 500)?;
//!     let doc_id = DocumentId::new();
//!     storage.add_posting(doc_id, 100, 50)?;
//!     storage.sync()?;
//! }
//!
//! // Reopen existing storage
//! let storage = PostingStorage::open(storage_path)?;
//! assert_eq!(storage.capacity(), 500);
//! assert_eq!(storage.current_count(), 1);
//! # Ok(())
//! # }
//! ```

use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use crate::memory::{FileHeader, MemoryMappedFile};
use bytemuck::{Pod, Zeroable};
use std::path::Path;

/// Memory-mapped posting storage header containing metadata and configuration
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct PostingStorageHeader {
    /// File format header with magic bytes and checksum
    pub file_header: FileHeader,
    /// Maximum number of postings this storage can hold
    pub capacity: u32,
    /// Current number of postings stored (including deleted ones)
    pub current_count: u32,
    /// Number of active (non-deleted) postings
    pub active_count: u32,
    /// Offset to start of document IDs array in the file
    pub document_ids_offset: u64,
    /// Offset to start of starts array in the file
    pub starts_offset: u64,
    /// Offset to start of lengths array in the file
    pub lengths_offset: u64,
    /// Offset to start of deleted flags bitset in the file
    pub deleted_flags_offset: u64,
    /// Size of each document ID in bytes (16 bytes for u128)
    pub document_id_size: u32,
    /// Reserved bytes for future use
    pub reserved: [u8; 12],
}

/// High-performance memory-mapped posting storage
///
/// PostingStorage provides efficient storage and access for posting metadata with
/// zero-copy operations, proper alignment, and memory-mapped persistence for
/// large-scale posting datasets.
pub struct PostingStorage {
    /// Memory-mapped file for persistent storage
    mmap_file: MemoryMappedFile,
    /// Storage metadata and configuration
    header: PostingStorageHeader,
    /// Current capacity (cached from header for performance)
    capacity: usize,
    /// Flag indicating if storage is read-only
    read_only: bool,
}

// SAFETY: PostingStorageHeader contains only Pod types and has repr(C) layout
unsafe impl Pod for PostingStorageHeader {}
// SAFETY: PostingStorageHeader can be safely zero-initialized
unsafe impl Zeroable for PostingStorageHeader {}

/// Magic bytes for posting storage files
const POSTING_STORAGE_MAGIC: &[u8; 4] = b"PSTR";
/// Current version of the posting storage format
const POSTING_STORAGE_VERSION: u32 = 1;
/// Default alignment for data structures (currently unused but reserved for future optimizations)
#[allow(dead_code)]
const DEFAULT_ALIGNMENT: usize = 8;

impl PostingStorageHeader {
    /// Size of the header structure in bytes
    pub const SIZE: usize = std::mem::size_of::<PostingStorageHeader>();

    /// Create a new posting storage header
    pub fn new(capacity: usize) -> Result<Self, ShardexError> {
        if capacity == 0 {
            return Err(ShardexError::Config("Capacity cannot be zero".to_string()));
        }
        if capacity > u32::MAX as usize {
            return Err(ShardexError::Config(format!(
                "Capacity {} exceeds maximum {}",
                capacity,
                u32::MAX
            )));
        }

        let header_size = Self::SIZE;

        // Calculate offsets for each array section
        let document_ids_offset = header_size as u64;
        let document_ids_size = capacity * 16; // 16 bytes per DocumentId (u128)

        let starts_offset = document_ids_offset + document_ids_size as u64;
        let starts_size = capacity * 4; // 4 bytes per u32

        let lengths_offset = starts_offset + starts_size as u64;
        let lengths_size = capacity * 4; // 4 bytes per u32

        let deleted_flags_offset = lengths_offset + lengths_size as u64;
        // Deleted flags stored as bitset - 1 bit per posting, rounded up to bytes
        // let deleted_flags_size = (capacity + 7) / 8; // Bits to bytes

        Ok(Self {
            file_header: FileHeader::new_without_checksum(
                POSTING_STORAGE_MAGIC,
                POSTING_STORAGE_VERSION,
                FileHeader::SIZE as u64,
            ),
            capacity: capacity as u32,
            current_count: 0,
            active_count: 0,
            document_ids_offset,
            starts_offset,
            lengths_offset,
            deleted_flags_offset,
            document_id_size: 16,
            reserved: [0; 12],
        })
    }

    /// Validate the header magic bytes and version
    pub fn validate(&self) -> Result<(), ShardexError> {
        self.file_header.validate_magic(POSTING_STORAGE_MAGIC)?;

        if self.file_header.version != POSTING_STORAGE_VERSION {
            return Err(ShardexError::Corruption(format!(
                "Unsupported posting storage version: expected {}, found {}",
                POSTING_STORAGE_VERSION, self.file_header.version
            )));
        }

        // Validate field consistency
        if self.document_id_size != 16 {
            return Err(ShardexError::Corruption(format!(
                "Invalid document ID size: expected 16 bytes, found {}",
                self.document_id_size
            )));
        }

        if self.current_count > self.capacity {
            return Err(ShardexError::Corruption(format!(
                "Current count {} exceeds capacity {}",
                self.current_count, self.capacity
            )));
        }

        if self.active_count > self.current_count {
            return Err(ShardexError::Corruption(format!(
                "Active count {} exceeds current count {}",
                self.active_count, self.current_count
            )));
        }

        Ok(())
    }

    /// Update the checksum based on posting data
    pub fn update_checksum(&mut self, posting_data: &[u8]) {
        self.file_header.update_checksum(posting_data);
    }

    /// Calculate the total file size needed for this configuration
    pub fn calculate_file_size(&self) -> usize {
        let capacity = self.capacity as usize;
        let document_ids_size = capacity * 16; // 16 bytes per DocumentId
        let starts_size = capacity * 4; // 4 bytes per u32
        let lengths_size = capacity * 4; // 4 bytes per u32
        let deleted_flags_size = (capacity + 7) / 8; // Bits to bytes

        Self::SIZE + document_ids_size + starts_size + lengths_size + deleted_flags_size
    }
}

impl PostingStorage {
    /// Create a new posting storage file with the specified configuration
    ///
    /// # Arguments
    /// * `path` - Path where the storage file will be created
    /// * `capacity` - Initial capacity (number of postings)
    pub fn create<P: AsRef<Path>>(path: P, capacity: usize) -> Result<Self, ShardexError> {
        let path = path.as_ref();

        // Create and validate header
        let mut header = PostingStorageHeader::new(capacity)?;
        let total_size = header.calculate_file_size();

        // Create memory-mapped file
        let mut mmap_file = MemoryMappedFile::create(path, total_size)?;

        // Initialize all data areas with zeros
        let data_size = total_size - PostingStorageHeader::SIZE;
        let zero_data = vec![0u8; data_size];
        header.update_checksum(&zero_data);

        // Write header and initialize data areas
        mmap_file.write_at(0, &header)?;
        mmap_file.write_slice_at(PostingStorageHeader::SIZE, &zero_data)?;
        mmap_file.sync()?;

        Ok(Self {
            mmap_file,
            header,
            capacity,
            read_only: false,
        })
    }

    /// Open an existing posting storage file
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, ShardexError> {
        Self::open_with_mode(path, false)
    }

    /// Open an existing posting storage file in read-only mode
    pub fn open_read_only<P: AsRef<Path>>(path: P) -> Result<Self, ShardexError> {
        Self::open_with_mode(path, true)
    }

    /// Internal method to open storage with specified mode
    fn open_with_mode<P: AsRef<Path>>(path: P, read_only: bool) -> Result<Self, ShardexError> {
        let path = path.as_ref();

        let mmap_file = if read_only {
            MemoryMappedFile::open_read_only(path)?
        } else {
            MemoryMappedFile::open_read_write(path)?
        };

        // Read and validate header
        let header: PostingStorageHeader = mmap_file.read_at(0)?;
        header.validate()?;

        // Validate file size
        let expected_size = header.calculate_file_size();
        if mmap_file.len() < expected_size {
            return Err(ShardexError::Corruption(
                "File too small for declared posting capacity".to_string(),
            ));
        }

        // Validate checksum
        let data_start = PostingStorageHeader::SIZE;
        let data_size = expected_size - PostingStorageHeader::SIZE;
        let data = &mmap_file.as_slice()[data_start..data_start + data_size];
        header.file_header.validate_checksum(data)?;

        let capacity = header.capacity as usize;

        Ok(Self {
            mmap_file,
            header,
            capacity,
            read_only,
        })
    }

    /// Get the total capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the current count of postings (including deleted ones)
    pub fn current_count(&self) -> usize {
        self.header.current_count as usize
    }

    /// Get the count of active (non-deleted) postings
    pub fn active_count(&self) -> usize {
        self.header.active_count as usize
    }

    /// Check if the storage is read-only
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Check if the storage is at capacity
    pub fn is_full(&self) -> bool {
        self.current_count() >= self.capacity()
    }

    /// Get remaining capacity
    pub fn remaining_capacity(&self) -> usize {
        self.capacity().saturating_sub(self.current_count())
    }

    /// Add a posting to the storage
    ///
    /// Returns the index where the posting was stored.
    pub fn add_posting(
        &mut self,
        document_id: DocumentId,
        start: u32,
        length: u32,
    ) -> Result<usize, ShardexError> {
        if self.read_only {
            return Err(ShardexError::Config(
                "Cannot add posting to read-only storage".to_string(),
            ));
        }

        // Check capacity
        if self.is_full() {
            return Err(ShardexError::Config(
                "Posting storage is at capacity".to_string(),
            ));
        }

        let index = self.current_count();
        self.write_posting_at_index(index, document_id, start, length)?;
        self.set_deleted_flag(index, false)?;

        // Update counts
        self.header.current_count += 1;
        self.header.active_count += 1;
        self.update_header()?;

        Ok(index)
    }

    /// Get a posting by index
    ///
    /// Returns (document_id, start, length) tuple.
    pub fn get_posting(&self, index: usize) -> Result<(DocumentId, u32, u32), ShardexError> {
        if index >= self.current_count() {
            return Err(ShardexError::Config(format!(
                "Index {} out of bounds (current count: {})",
                index,
                self.current_count()
            )));
        }

        self.read_posting_at_index(index)
    }

    /// Update a posting at the specified index
    pub fn update_posting(
        &mut self,
        index: usize,
        document_id: DocumentId,
        start: u32,
        length: u32,
    ) -> Result<(), ShardexError> {
        if self.read_only {
            return Err(ShardexError::Config(
                "Cannot update posting in read-only storage".to_string(),
            ));
        }

        if index >= self.current_count() {
            return Err(ShardexError::Config(format!(
                "Index {} out of bounds (current count: {})",
                index,
                self.current_count()
            )));
        }

        self.write_posting_at_index(index, document_id, start, length)?;
        self.update_header()
    }

    /// Mark a posting as deleted (tombstone approach)
    ///
    /// The posting data remains in place but is marked as inactive.
    pub fn remove_posting(&mut self, index: usize) -> Result<(), ShardexError> {
        if self.read_only {
            return Err(ShardexError::Config(
                "Cannot remove posting from read-only storage".to_string(),
            ));
        }

        if index >= self.current_count() {
            return Err(ShardexError::Config(format!(
                "Index {} out of bounds (current count: {})",
                index,
                self.current_count()
            )));
        }

        self.set_deleted_flag(index, true)?;

        // Update active count
        if self.header.active_count > 0 {
            self.header.active_count -= 1;
        }

        self.update_header()
    }

    /// Check if a posting at the given index is deleted
    pub fn is_deleted(&self, index: usize) -> Result<bool, ShardexError> {
        if index >= self.current_count() {
            return Ok(false); // Non-existent postings are not deleted
        }

        self.get_deleted_flag(index)
    }

    /// Find all indices with the specified document ID
    pub fn find_by_document_id(&self, document_id: DocumentId) -> Result<Vec<usize>, ShardexError> {
        let mut indices = Vec::new();

        for i in 0..self.current_count() {
            if !self.is_deleted(i)? {
                let (doc_id, _, _) = self.get_posting(i)?;
                if doc_id == document_id {
                    indices.push(i);
                }
            }
        }

        Ok(indices)
    }

    /// Iterate over all active (non-deleted) postings
    pub fn iter_active(
        &self,
    ) -> impl Iterator<Item = Result<(usize, DocumentId, u32, u32), ShardexError>> + '_ {
        (0..self.current_count()).filter_map(move |index| {
            match self.is_deleted(index) {
                Ok(true) => None, // Skip deleted postings
                Ok(false) => match self.get_posting(index) {
                    Ok((doc_id, start, length)) => Some(Ok((index, doc_id, start, length))),
                    Err(e) => Some(Err(e)),
                },
                Err(e) => Some(Err(e)),
            }
        })
    }

    /// Synchronize the storage to disk
    pub fn sync(&mut self) -> Result<(), ShardexError> {
        if self.read_only {
            return Ok(()); // Read-only storage doesn't need syncing
        }

        self.update_header()?;
        self.mmap_file.sync()
    }

    /// Internal method to write a posting at a specific index
    fn write_posting_at_index(
        &mut self,
        index: usize,
        document_id: DocumentId,
        start: u32,
        length: u32,
    ) -> Result<(), ShardexError> {
        // Write document ID
        let doc_id_offset = self.header.document_ids_offset as usize + (index * 16);
        self.mmap_file.write_at(doc_id_offset, &document_id)?;

        // Write start position
        let start_offset = self.header.starts_offset as usize + (index * 4);
        self.mmap_file.write_at(start_offset, &start)?;

        // Write length
        let length_offset = self.header.lengths_offset as usize + (index * 4);
        self.mmap_file.write_at(length_offset, &length)?;

        Ok(())
    }

    /// Internal method to read a posting at a specific index
    fn read_posting_at_index(&self, index: usize) -> Result<(DocumentId, u32, u32), ShardexError> {
        // Read document ID
        let doc_id_offset = self.header.document_ids_offset as usize + (index * 16);
        let document_id: DocumentId = self.mmap_file.read_at(doc_id_offset)?;

        // Read start position
        let start_offset = self.header.starts_offset as usize + (index * 4);
        let start: u32 = self.mmap_file.read_at(start_offset)?;

        // Read length
        let length_offset = self.header.lengths_offset as usize + (index * 4);
        let length: u32 = self.mmap_file.read_at(length_offset)?;

        Ok((document_id, start, length))
    }

    /// Set the deleted flag for a posting at the specified index
    fn set_deleted_flag(&mut self, index: usize, deleted: bool) -> Result<(), ShardexError> {
        let byte_index = index / 8;
        let bit_index = index % 8;
        let byte_offset = self.header.deleted_flags_offset as usize + byte_index;

        // Read current byte
        let mut current_byte: u8 = self.mmap_file.read_at(byte_offset)?;

        // Update the specific bit
        if deleted {
            current_byte |= 1 << bit_index;
        } else {
            current_byte &= !(1 << bit_index);
        }

        // Write back the updated byte
        self.mmap_file.write_at(byte_offset, &current_byte)?;

        Ok(())
    }

    /// Get the deleted flag for a posting at the specified index
    fn get_deleted_flag(&self, index: usize) -> Result<bool, ShardexError> {
        let byte_index = index / 8;
        let bit_index = index % 8;
        let byte_offset = self.header.deleted_flags_offset as usize + byte_index;

        // Read the byte containing our bit
        let current_byte: u8 = self.mmap_file.read_at(byte_offset)?;

        // Check if the bit is set
        Ok((current_byte & (1 << bit_index)) != 0)
    }

    /// Validate the integrity of the posting storage
    ///
    /// Performs comprehensive validation including header consistency,
    /// data structure integrity, and cross-validation of metadata.
    pub fn validate_integrity(&self) -> Result<(), ShardexError> {
        // Validate header
        self.header.validate()?;

        // Validate file header checksum against data
        let data_start = PostingStorageHeader::SIZE;
        let data_size = self.header.calculate_file_size() - PostingStorageHeader::SIZE;

        if data_start + data_size > self.mmap_file.len() {
            return Err(ShardexError::Corruption(
                "File size is inconsistent with header metadata".to_string(),
            ));
        }

        let data = &self.mmap_file.as_slice()[data_start..data_start + data_size];
        self.header.file_header.validate_checksum(data)?;

        // Validate data structure consistency
        self.validate_data_consistency()?;

        Ok(())
    }

    /// Validate internal data consistency
    ///
    /// Checks that active/deleted counts match actual data,
    /// and that all data structures are consistent.
    fn validate_data_consistency(&self) -> Result<(), ShardexError> {
        let mut actual_active_count = 0u32;

        // Count actual active postings
        for i in 0..self.current_count() {
            if !self.is_deleted(i)? {
                actual_active_count += 1;
            }
        }

        // Check active count consistency
        if actual_active_count != self.header.active_count {
            return Err(ShardexError::Corruption(format!(
                "Active count mismatch: header claims {}, actual count is {}",
                self.header.active_count, actual_active_count
            )));
        }

        // Validate that all data within bounds makes sense
        for i in 0..self.current_count() {
            let (doc_id, start, length) = self.read_posting_at_index(i)?;

            // Basic sanity checks on the data
            if length > u32::MAX / 2 {
                return Err(ShardexError::Corruption(format!(
                    "Posting {} has unreasonable length: {}",
                    i, length
                )));
            }

            // Check for overflow in position + length
            if let Some(end_pos) = start.checked_add(length) {
                if end_pos < start {
                    return Err(ShardexError::Corruption(format!(
                        "Posting {} has invalid range: start={}, length={}",
                        i, start, length
                    )));
                }
            } else {
                return Err(ShardexError::Corruption(format!(
                    "Posting {} position overflow: start={}, length={}",
                    i, start, length
                )));
            }

            // Validate document ID is not all zeros (which would be invalid)
            if doc_id.raw() == 0 && !self.is_deleted(i)? {
                return Err(ShardexError::Corruption(format!(
                    "Active posting {} has invalid zero document ID",
                    i
                )));
            }
        }

        Ok(())
    }

    /// Get the underlying memory-mapped file for external integrity validation
    pub fn memory_mapped_file(&self) -> &MemoryMappedFile {
        &self.mmap_file
    }

    /// Update the header in the memory-mapped file
    fn update_header(&mut self) -> Result<(), ShardexError> {
        // Update checksum
        let data_start = PostingStorageHeader::SIZE;
        let data_size = self.header.calculate_file_size() - PostingStorageHeader::SIZE;
        let data = &self.mmap_file.as_slice()[data_start..data_start + data_size];

        self.header.file_header.update_checksum(data);

        // Write header to file
        self.mmap_file.write_at(0, &self.header)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_posting_storage_header_creation() {
        let header = PostingStorageHeader::new(1000).unwrap();

        assert_eq!(header.capacity, 1000);
        assert_eq!(header.current_count, 0);
        assert_eq!(header.active_count, 0);
        assert_eq!(header.document_id_size, 16);

        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_posting_storage_header_validation_errors() {
        // Zero capacity should fail
        assert!(PostingStorageHeader::new(0).is_err());

        // Capacity too large should fail
        assert!(PostingStorageHeader::new(u32::MAX as usize + 1).is_err());
    }

    #[test]
    fn test_create_posting_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        let storage = PostingStorage::create(&storage_path, 100).unwrap();

        assert_eq!(storage.capacity(), 100);
        assert_eq!(storage.current_count(), 0);
        assert_eq!(storage.active_count(), 0);
        assert!(!storage.is_read_only());
        assert!(!storage.is_full());
        assert_eq!(storage.remaining_capacity(), 100);
    }

    #[test]
    fn test_add_and_get_postings() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        let mut storage = PostingStorage::create(&storage_path, 10).unwrap();

        // Add some postings
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();

        let idx1 = storage.add_posting(doc_id1, 100, 50).unwrap();
        let idx2 = storage.add_posting(doc_id2, 200, 75).unwrap();

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(storage.current_count(), 2);
        assert_eq!(storage.active_count(), 2);

        // Retrieve postings
        let (retrieved_doc_id1, start1, length1) = storage.get_posting(idx1).unwrap();
        let (retrieved_doc_id2, start2, length2) = storage.get_posting(idx2).unwrap();

        assert_eq!(retrieved_doc_id1, doc_id1);
        assert_eq!(start1, 100);
        assert_eq!(length1, 50);

        assert_eq!(retrieved_doc_id2, doc_id2);
        assert_eq!(start2, 200);
        assert_eq!(length2, 75);
    }

    #[test]
    fn test_capacity_limits() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        let mut storage = PostingStorage::create(&storage_path, 2).unwrap();

        // Add postings up to capacity
        let doc_id = DocumentId::new();
        storage.add_posting(doc_id, 100, 50).unwrap();
        storage.add_posting(doc_id, 200, 75).unwrap();

        assert!(storage.is_full());
        assert_eq!(storage.remaining_capacity(), 0);

        // Adding beyond capacity should fail
        let result = storage.add_posting(doc_id, 300, 25);
        assert!(matches!(result, Err(ShardexError::Config(_))));
    }

    #[test]
    fn test_update_posting() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        let mut storage = PostingStorage::create(&storage_path, 10).unwrap();

        // Add a posting
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let idx = storage.add_posting(doc_id1, 100, 50).unwrap();

        // Update it
        storage.update_posting(idx, doc_id2, 200, 75).unwrap();

        // Verify update
        let (retrieved_doc_id, start, length) = storage.get_posting(idx).unwrap();
        assert_eq!(retrieved_doc_id, doc_id2);
        assert_eq!(start, 200);
        assert_eq!(length, 75);
    }

    #[test]
    fn test_remove_posting() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        let mut storage = PostingStorage::create(&storage_path, 10).unwrap();

        // Add postings
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let idx1 = storage.add_posting(doc_id1, 100, 50).unwrap();
        let idx2 = storage.add_posting(doc_id2, 200, 75).unwrap();

        assert_eq!(storage.active_count(), 2);

        // Remove one posting
        storage.remove_posting(idx1).unwrap();

        assert_eq!(storage.current_count(), 2); // Still 2 total
        assert_eq!(storage.active_count(), 1); // But only 1 active
        assert!(storage.is_deleted(idx1).unwrap());
        assert!(!storage.is_deleted(idx2).unwrap());

        // Posting 2 should still be accessible
        let (retrieved_doc_id2, start2, length2) = storage.get_posting(idx2).unwrap();
        assert_eq!(retrieved_doc_id2, doc_id2);
        assert_eq!(start2, 200);
        assert_eq!(length2, 75);
    }

    #[test]
    fn test_find_by_document_id() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        let mut storage = PostingStorage::create(&storage_path, 10).unwrap();

        // Add multiple postings for the same document
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();

        let idx1 = storage.add_posting(doc_id1, 100, 50).unwrap();
        let idx2 = storage.add_posting(doc_id2, 200, 75).unwrap();
        let idx3 = storage.add_posting(doc_id1, 300, 25).unwrap(); // Same doc as idx1

        // Find postings by document ID
        let indices1 = storage.find_by_document_id(doc_id1).unwrap();
        let indices2 = storage.find_by_document_id(doc_id2).unwrap();

        assert_eq!(indices1, vec![idx1, idx3]);
        assert_eq!(indices2, vec![idx2]);

        // Remove one posting and search again
        storage.remove_posting(idx3).unwrap();
        let indices1_after_removal = storage.find_by_document_id(doc_id1).unwrap();
        assert_eq!(indices1_after_removal, vec![idx1]); // Only active posting
    }

    #[test]
    fn test_iter_active() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        let mut storage = PostingStorage::create(&storage_path, 10).unwrap();

        // Add some postings
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let doc_id3 = DocumentId::new();

        let idx1 = storage.add_posting(doc_id1, 100, 50).unwrap();
        let idx2 = storage.add_posting(doc_id2, 200, 75).unwrap();
        let idx3 = storage.add_posting(doc_id3, 300, 25).unwrap();

        // Remove one posting
        storage.remove_posting(idx2).unwrap();

        // Iterate over active postings
        let active_postings: Result<Vec<_>, _> = storage.iter_active().collect();
        let active_postings = active_postings.unwrap();

        assert_eq!(active_postings.len(), 2);

        // Check that we get the right postings (idx1 and idx3)
        let indices: Vec<usize> = active_postings.iter().map(|(idx, _, _, _)| *idx).collect();
        assert!(indices.contains(&idx1));
        assert!(indices.contains(&idx3));
        assert!(!indices.contains(&idx2)); // Deleted posting should not appear
    }

    #[test]
    fn test_out_of_bounds_access() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        let storage = PostingStorage::create(&storage_path, 10).unwrap();

        // Try to access non-existent posting
        let result = storage.get_posting(0);
        assert!(matches!(result, Err(ShardexError::Config(_))));

        let result = storage.get_posting(5);
        assert!(matches!(result, Err(ShardexError::Config(_))));
    }

    #[test]
    fn test_persistence() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage_path = temp_file.path();

        let postings_to_add = vec![
            (DocumentId::new(), 100, 50),
            (DocumentId::new(), 200, 75),
            (DocumentId::new(), 300, 25),
        ];

        // Create and populate storage
        {
            let mut storage = PostingStorage::create(storage_path, 10).unwrap();

            for (doc_id, start, length) in &postings_to_add {
                storage.add_posting(*doc_id, *start, *length).unwrap();
            }
            storage.sync().unwrap();
        }

        // Reopen and verify
        {
            let storage = PostingStorage::open(storage_path).unwrap();

            assert_eq!(storage.capacity(), 10);
            assert_eq!(storage.current_count(), 3);
            assert_eq!(storage.active_count(), 3);

            for (i, (expected_doc_id, expected_start, expected_length)) in
                postings_to_add.iter().enumerate()
            {
                let (retrieved_doc_id, start, length) = storage.get_posting(i).unwrap();
                assert_eq!(retrieved_doc_id, *expected_doc_id);
                assert_eq!(start, *expected_start);
                assert_eq!(length, *expected_length);
            }
        }
    }

    #[test]
    fn test_read_only_mode() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage_path = temp_file.path();

        // Create storage with some data
        {
            let mut storage = PostingStorage::create(storage_path, 5).unwrap();
            let doc_id = DocumentId::new();
            storage.add_posting(doc_id, 100, 50).unwrap();
            storage.sync().unwrap();
        }

        // Open in read-only mode
        {
            let mut storage = PostingStorage::open_read_only(storage_path).unwrap();

            assert!(storage.is_read_only());
            assert_eq!(storage.current_count(), 1);

            // Should be able to read
            let (_retrieved_doc_id, start, length) = storage.get_posting(0).unwrap();
            assert_eq!(start, 100);
            assert_eq!(length, 50);

            // Should not be able to modify
            let new_doc_id = DocumentId::new();
            assert!(storage.add_posting(new_doc_id, 200, 75).is_err());
            assert!(storage.update_posting(0, new_doc_id, 200, 75).is_err());
            assert!(storage.remove_posting(0).is_err());
        }
    }

    #[test]
    fn test_header_bytemuck_compatibility() {
        let header = PostingStorageHeader::new(1000).unwrap();

        // Should be able to convert to bytes
        let bytes = bytemuck::bytes_of(&header);
        assert_eq!(bytes.len(), PostingStorageHeader::SIZE);

        // Should be able to convert back
        let header_restored = bytemuck::from_bytes::<PostingStorageHeader>(bytes);
        assert_eq!(header.capacity, header_restored.capacity);
        assert_eq!(header.current_count, header_restored.current_count);
        assert_eq!(header.active_count, header_restored.active_count);
    }

    #[test]
    fn test_header_validation() {
        let mut header = PostingStorageHeader::new(1000).unwrap();

        // Should validate correctly initially
        assert!(header.validate().is_ok());

        // Break magic bytes
        header.file_header.magic = *b"XXXX";
        assert!(header.validate().is_err());
        header.file_header.magic = *POSTING_STORAGE_MAGIC;

        // Break version
        header.file_header.version = 999;
        assert!(header.validate().is_err());
        header.file_header.version = POSTING_STORAGE_VERSION;

        // Break document ID size
        header.document_id_size = 8; // Should be 16
        assert!(header.validate().is_err());
        header.document_id_size = 16;

        // Break count consistency
        header.current_count = header.capacity + 1;
        assert!(header.validate().is_err());
        header.current_count = 0;

        header.active_count = header.current_count + 1;
        assert!(header.validate().is_err());
        header.active_count = 0;

        // Should validate correctly again
        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_deleted_flag_operations() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        let mut storage = PostingStorage::create(&storage_path, 100).unwrap();

        // Add multiple postings
        let doc_ids: Vec<DocumentId> = (0..10).map(|_| DocumentId::new()).collect();
        let mut indices = Vec::new();

        for doc_id in &doc_ids {
            let idx = storage.add_posting(*doc_id, 100, 50).unwrap();
            indices.push(idx);
        }

        // All should be active initially
        for idx in &indices {
            assert!(!storage.is_deleted(*idx).unwrap());
        }

        // Remove some postings
        storage.remove_posting(indices[1]).unwrap();
        storage.remove_posting(indices[5]).unwrap();
        storage.remove_posting(indices[8]).unwrap();

        // Check deleted flags
        for (i, idx) in indices.iter().enumerate() {
            let expected_deleted = i == 1 || i == 5 || i == 8;
            assert_eq!(storage.is_deleted(*idx).unwrap(), expected_deleted);
        }

        assert_eq!(storage.active_count(), 7); // 10 - 3 deleted = 7
    }

    #[test]
    fn test_file_size_calculation() {
        let header = PostingStorageHeader::new(1000).unwrap();
        let expected_size = PostingStorageHeader::SIZE +
                           (1000 * 16) + // document IDs
                           (1000 * 4) + // starts
                           (1000 * 4) + // lengths
                           1000_usize.div_ceil(8); // deleted flags bitset

        assert_eq!(header.calculate_file_size(), expected_size);
    }

    #[test]
    fn test_bit_manipulation() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("bits.dat");

        let mut storage = PostingStorage::create(&storage_path, 64).unwrap();

        // Add 64 postings (fills exactly 8 bytes of deleted flags)
        for i in 0..64 {
            let doc_id = DocumentId::new();
            storage.add_posting(doc_id, i as u32 * 100, 50).unwrap();
        }

        // Test various bit positions
        let test_indices = [0, 1, 7, 8, 15, 16, 31, 32, 63];

        for &idx in &test_indices {
            storage.remove_posting(idx).unwrap();
            assert!(storage.is_deleted(idx).unwrap());
        }

        // Verify non-deleted ones are still active
        for i in 0..64 {
            if !test_indices.contains(&i) {
                assert!(!storage.is_deleted(i).unwrap());
            }
        }
    }
}

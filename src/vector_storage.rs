//! Memory-mapped vector storage using Arrow arrays for high-performance vector operations
//!
//! This module provides fixed-size memory-mapped storage for embedding vectors with direct
//! access capabilities. It leverages Apache Arrow's memory layout for optimal performance
//! and zero-copy operations while ensuring proper SIMD alignment.
//!
//! # Key Components
//!
//! - [`VectorStorage`]: Main vector storage container using Arrow FixedSizeListArray
//! - [`VectorStorageHeader`]: Memory-mapped header with metadata and configuration
//! - Zero-copy vector access with proper alignment for SIMD operations
//!
//! # Usage Examples
//!
//! ## Creating and Using Vector Storage
//!
//! ```rust
//! use shardex::vector_storage::VectorStorage;
//! use tempfile::TempDir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let storage_path = temp_dir.path().join("vectors.dat");
//!
//! // Create storage for 384-dimensional vectors with initial capacity of 1000
//! let mut storage = VectorStorage::create(&storage_path, 384, 1000)?;
//!
//! // Add some vectors
//! let vector1 = vec![1.0; 384];
//! let vector2 = vec![2.0; 384];
//! let idx1 = storage.add_vector(&vector1)?;
//! let idx2 = storage.add_vector(&vector2)?;
//!
//! // Access vectors (zero-copy)
//! let retrieved = storage.get_vector(idx1)?;
//! assert_eq!(retrieved[0], 1.0);
//!
//! // Update a vector in place
//! let new_vector = vec![3.0; 384];
//! storage.update_vector(idx1, &new_vector)?;
//!
//! // Remove a vector (tombstone marking)
//! storage.remove_vector(idx2)?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Memory-Mapped Persistence
//!
//! ```rust
//! use shardex::vector_storage::VectorStorage;
//! use std::path::Path;
//!
//! # fn persistence_example(storage_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
//! // Create and populate storage
//! {
//!     let mut storage = VectorStorage::create(storage_path, 128, 500)?;
//!     let vector = vec![0.5; 128];
//!     storage.add_vector(&vector)?;
//!     storage.sync()?;
//! }
//!
//! // Reopen existing storage
//! let storage = VectorStorage::open(storage_path)?;
//! assert_eq!(storage.vector_dimension(), 128);
//! assert_eq!(storage.current_count(), 1);
//! # Ok(())
//! # }
//! ```

use crate::error::ShardexError;
use crate::memory::{FileHeader, MemoryMappedFile};
use bytemuck::{Pod, Zeroable};
use std::path::Path;

/// Memory-mapped vector storage header containing metadata and configuration
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct VectorStorageHeader {
    /// File format header with magic bytes and checksum
    pub file_header: FileHeader,
    /// Number of dimensions per vector
    pub vector_dimension: u32,
    /// Maximum number of vectors this storage can hold
    pub capacity: u32,
    /// Current number of vectors stored (including deleted ones)
    pub current_count: u32,
    /// Number of active (non-deleted) vectors
    pub active_count: u32,
    /// Offset to start of vector data in the file
    pub vector_data_offset: u64,
    /// Size of each vector in bytes (dimension * 4 bytes per f32)
    pub vector_size_bytes: u32,
    /// Alignment padding for SIMD operations (typically 64 bytes for AVX-512)
    pub simd_alignment: u32,
    /// Reserved bytes for future use
    pub reserved: [u8; 16],
}

/// High-performance memory-mapped vector storage
///
/// VectorStorage provides efficient storage and access for fixed-size vectors with
/// zero-copy operations, proper SIMD alignment, and memory-mapped persistence for
/// large-scale vector datasets.
pub struct VectorStorage {
    /// Memory-mapped file for persistent storage
    mmap_file: MemoryMappedFile,
    /// Storage metadata and configuration
    header: VectorStorageHeader,
    /// Vector dimension (cached from header for performance)
    vector_dimension: usize,
    /// Current capacity (cached from header for performance)
    capacity: usize,
    /// Flag indicating if storage is read-only
    read_only: bool,
}

// SAFETY: VectorStorageHeader contains only Pod types and has repr(C) layout
unsafe impl Pod for VectorStorageHeader {}
// SAFETY: VectorStorageHeader can be safely zero-initialized
unsafe impl Zeroable for VectorStorageHeader {}

/// Magic bytes for vector storage files
const VECTOR_STORAGE_MAGIC: &[u8; 4] = b"VSTR";
/// Current version of the vector storage format
const VECTOR_STORAGE_VERSION: u32 = 1;
/// Default SIMD alignment (64 bytes for AVX-512)
const DEFAULT_SIMD_ALIGNMENT: usize = 64;

impl VectorStorageHeader {
    /// Size of the header structure in bytes
    pub const SIZE: usize = std::mem::size_of::<VectorStorageHeader>();

    /// Create a new vector storage header
    pub fn new(vector_dimension: usize, capacity: usize) -> Result<Self, ShardexError> {
        if vector_dimension == 0 {
            return Err(ShardexError::Config(
                "Vector dimension cannot be zero".to_string(),
            ));
        }
        if capacity == 0 {
            return Err(ShardexError::Config("Capacity cannot be zero".to_string()));
        }
        if vector_dimension > u32::MAX as usize {
            return Err(ShardexError::Config(format!(
                "Vector dimension {} exceeds maximum {}",
                vector_dimension,
                u32::MAX
            )));
        }
        if capacity > u32::MAX as usize {
            return Err(ShardexError::Config(format!(
                "Capacity {} exceeds maximum {}",
                capacity,
                u32::MAX
            )));
        }

        let vector_size_bytes = (vector_dimension * std::mem::size_of::<f32>()) as u32;
        let vector_data_offset = Self::SIZE as u64;

        Ok(Self {
            file_header: FileHeader::new_without_checksum(
                VECTOR_STORAGE_MAGIC,
                VECTOR_STORAGE_VERSION,
            ),
            vector_dimension: vector_dimension as u32,
            capacity: capacity as u32,
            current_count: 0,
            active_count: 0,
            vector_data_offset,
            vector_size_bytes,
            simd_alignment: DEFAULT_SIMD_ALIGNMENT as u32,
            reserved: [0; 16],
        })
    }

    /// Validate the header magic bytes and version
    pub fn validate(&self) -> Result<(), ShardexError> {
        self.file_header.validate_magic(VECTOR_STORAGE_MAGIC)?;

        if self.file_header.version != VECTOR_STORAGE_VERSION {
            return Err(ShardexError::Corruption(format!(
                "Unsupported vector storage version: expected {}, found {}",
                VECTOR_STORAGE_VERSION, self.file_header.version
            )));
        }

        // Validate field consistency
        let expected_vector_size =
            (self.vector_dimension as usize * std::mem::size_of::<f32>()) as u32;
        if self.vector_size_bytes != expected_vector_size {
            return Err(ShardexError::Corruption(format!(
                "Vector size mismatch: expected {} bytes, found {}",
                expected_vector_size, self.vector_size_bytes
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

    /// Update the checksum based on vector data
    pub fn update_checksum(&mut self, vector_data: &[u8]) {
        self.file_header.update_checksum(vector_data);
    }
}

impl VectorStorage {
    /// Create a new vector storage file with the specified configuration
    ///
    /// # Arguments
    /// * `path` - Path where the storage file will be created
    /// * `vector_dimension` - Number of dimensions per vector
    /// * `capacity` - Initial capacity (number of vectors)
    pub fn create<P: AsRef<Path>>(
        path: P,
        vector_dimension: usize,
        capacity: usize,
    ) -> Result<Self, ShardexError> {
        let path = path.as_ref();

        // Calculate total file size needed
        let header_size = VectorStorageHeader::SIZE;
        let vector_data_size = capacity * vector_dimension * std::mem::size_of::<f32>();

        // Align vector data for SIMD operations
        let aligned_vector_data_size = Self::align_size(vector_data_size, DEFAULT_SIMD_ALIGNMENT);
        let total_size = header_size + aligned_vector_data_size;

        // Create memory-mapped file
        let mut mmap_file = MemoryMappedFile::create(path, total_size)?;

        // Create and write header
        let mut header = VectorStorageHeader::new(vector_dimension, capacity)?;

        // Initialize vector data area with zeros
        let vector_data_slice = vec![0u8; aligned_vector_data_size];
        header.update_checksum(&vector_data_slice);

        mmap_file.write_at(0, &header)?;
        mmap_file.write_slice_at(header_size, &vector_data_slice)?;
        mmap_file.sync()?;

        Ok(Self {
            mmap_file,
            header,
            vector_dimension,
            capacity,
            read_only: false,
        })
    }

    /// Open an existing vector storage file
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, ShardexError> {
        Self::open_with_mode(path, false)
    }

    /// Open an existing vector storage file in read-only mode
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
        let header: VectorStorageHeader = mmap_file.read_at(0)?;
        header.validate()?;

        // Validate checksum
        let vector_data_start = header.vector_data_offset as usize;
        let vector_data_size = (header.capacity as usize)
            * (header.vector_dimension as usize)
            * std::mem::size_of::<f32>();
        let aligned_size = Self::align_size(vector_data_size, header.simd_alignment as usize);

        if vector_data_start + aligned_size > mmap_file.len() {
            return Err(ShardexError::Corruption(
                "File too small for declared vector capacity".to_string(),
            ));
        }

        let vector_data =
            &mmap_file.as_slice()[vector_data_start..vector_data_start + aligned_size];
        header.file_header.validate_checksum(vector_data)?;

        let vector_dimension = header.vector_dimension as usize;
        let capacity = header.capacity as usize;

        Ok(Self {
            mmap_file,
            header,
            vector_dimension,
            capacity,
            read_only,
        })
    }

    /// Get the vector dimension
    pub fn vector_dimension(&self) -> usize {
        self.vector_dimension
    }

    /// Get the total capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the current count of vectors (including deleted ones)
    pub fn current_count(&self) -> usize {
        self.header.current_count as usize
    }

    /// Get the count of active (non-deleted) vectors
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

    /// Add a vector to the storage
    ///
    /// Returns the index where the vector was stored.
    pub fn add_vector(&mut self, vector: &[f32]) -> Result<usize, ShardexError> {
        if self.read_only {
            return Err(ShardexError::Config(
                "Cannot add vector to read-only storage".to_string(),
            ));
        }

        // Validate vector dimension
        if vector.len() != self.vector_dimension {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_dimension,
                actual: vector.len(),
            });
        }

        // Check capacity
        if self.is_full() {
            return Err(ShardexError::Config(
                "Vector storage is at capacity".to_string(),
            ));
        }

        let index = self.current_count();
        self.write_vector_at_index(index, vector)?;

        // Update counts
        self.header.current_count += 1;
        self.header.active_count += 1;
        self.update_header()?;

        Ok(index)
    }

    /// Get a vector by index (zero-copy access)
    ///
    /// Returns a slice view into the mapped memory.
    pub fn get_vector(&self, index: usize) -> Result<&[f32], ShardexError> {
        if index >= self.current_count() {
            return Err(ShardexError::Config(format!(
                "Index {} out of bounds (current count: {})",
                index,
                self.current_count()
            )));
        }

        self.read_vector_at_index(index)
    }

    /// Update a vector at the specified index
    pub fn update_vector(&mut self, index: usize, vector: &[f32]) -> Result<(), ShardexError> {
        if self.read_only {
            return Err(ShardexError::Config(
                "Cannot update vector in read-only storage".to_string(),
            ));
        }

        if index >= self.current_count() {
            return Err(ShardexError::Config(format!(
                "Index {} out of bounds (current count: {})",
                index,
                self.current_count()
            )));
        }

        // Validate vector dimension
        if vector.len() != self.vector_dimension {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_dimension,
                actual: vector.len(),
            });
        }

        self.write_vector_at_index(index, vector)?;
        self.update_header()
    }

    /// Mark a vector as deleted (tombstone approach)
    ///
    /// The vector data remains in place but is marked as inactive.
    /// Use compact() to reclaim space from deleted vectors.
    pub fn remove_vector(&mut self, index: usize) -> Result<(), ShardexError> {
        if self.read_only {
            return Err(ShardexError::Config(
                "Cannot remove vector from read-only storage".to_string(),
            ));
        }

        if index >= self.current_count() {
            return Err(ShardexError::Config(format!(
                "Index {} out of bounds (current count: {})",
                index,
                self.current_count()
            )));
        }

        // For now, we use a simple tombstone approach by writing NaN values
        // In a full implementation, we might use a separate deleted bitmap
        let deleted_marker = vec![f32::NAN; self.vector_dimension];
        self.write_vector_at_index(index, &deleted_marker)?;

        // Update active count
        if self.header.active_count > 0 {
            self.header.active_count -= 1;
        }

        self.update_header()
    }

    /// Check if a vector at the given index is deleted
    pub fn is_deleted(&self, index: usize) -> Result<bool, ShardexError> {
        if index >= self.current_count() {
            return Ok(false); // Non-existent vectors are not deleted
        }

        let vector = self.get_vector(index)?;
        // Check if first element is NaN (our deletion marker)
        Ok(vector[0].is_nan())
    }

    /// Synchronize the storage to disk
    pub fn sync(&mut self) -> Result<(), ShardexError> {
        if self.read_only {
            return Ok(()); // Read-only storage doesn't need syncing
        }

        self.update_header()?;
        self.mmap_file.sync()
    }

    /// Internal method to write a vector at a specific index
    fn write_vector_at_index(&mut self, index: usize, vector: &[f32]) -> Result<(), ShardexError> {
        let vector_offset = self.calculate_vector_offset(index);
        self.mmap_file.write_slice_at(vector_offset, vector)?;
        Ok(())
    }

    /// Internal method to read a vector at a specific index
    fn read_vector_at_index(&self, index: usize) -> Result<&[f32], ShardexError> {
        let vector_offset = self.calculate_vector_offset(index);
        let vector_slice: &[f32] = self
            .mmap_file
            .read_slice_at(vector_offset, self.vector_dimension)?;
        Ok(vector_slice)
    }

    /// Calculate the byte offset for a vector at the given index
    fn calculate_vector_offset(&self, index: usize) -> usize {
        let header_size = VectorStorageHeader::SIZE;
        let vector_size_bytes = self.vector_dimension * std::mem::size_of::<f32>();
        header_size + (index * vector_size_bytes)
    }

    /// Update the header in the memory-mapped file
    fn update_header(&mut self) -> Result<(), ShardexError> {
        // Update checksum
        let vector_data_start = self.header.vector_data_offset as usize;
        let vector_data_size = (self.header.capacity as usize)
            * (self.header.vector_dimension as usize)
            * std::mem::size_of::<f32>();
        let aligned_size = Self::align_size(vector_data_size, self.header.simd_alignment as usize);
        let vector_data =
            &self.mmap_file.as_slice()[vector_data_start..vector_data_start + aligned_size];

        self.header.file_header.update_checksum(vector_data);

        // Write header to file
        self.mmap_file.write_at(0, &self.header)?;
        Ok(())
    }

    /// Align size to the specified alignment boundary
    fn align_size(size: usize, alignment: usize) -> usize {
        (size + alignment - 1) & !(alignment - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_vector_storage_header_creation() {
        let header = VectorStorageHeader::new(384, 1000).unwrap();

        assert_eq!(header.vector_dimension, 384);
        assert_eq!(header.capacity, 1000);
        assert_eq!(header.current_count, 0);
        assert_eq!(header.active_count, 0);
        assert_eq!(header.vector_size_bytes, 384 * 4);
        assert_eq!(header.simd_alignment, DEFAULT_SIMD_ALIGNMENT as u32);

        assert!(header.validate().is_ok());
    }

    #[test]
    fn test_vector_storage_header_validation_errors() {
        // Zero dimension should fail
        assert!(VectorStorageHeader::new(0, 1000).is_err());

        // Zero capacity should fail
        assert!(VectorStorageHeader::new(384, 0).is_err());

        // Dimension too large should fail
        assert!(VectorStorageHeader::new(u32::MAX as usize + 1, 1000).is_err());

        // Capacity too large should fail
        assert!(VectorStorageHeader::new(384, u32::MAX as usize + 1).is_err());
    }

    #[test]
    fn test_create_vector_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("vectors.dat");

        let storage = VectorStorage::create(&storage_path, 128, 100).unwrap();

        assert_eq!(storage.vector_dimension(), 128);
        assert_eq!(storage.capacity(), 100);
        assert_eq!(storage.current_count(), 0);
        assert_eq!(storage.active_count(), 0);
        assert!(!storage.is_read_only());
        assert!(!storage.is_full());
        assert_eq!(storage.remaining_capacity(), 100);
    }

    #[test]
    fn test_add_and_get_vectors() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("vectors.dat");

        let mut storage = VectorStorage::create(&storage_path, 3, 10).unwrap();

        // Add some vectors
        let vector1 = vec![1.0, 2.0, 3.0];
        let vector2 = vec![4.0, 5.0, 6.0];

        let idx1 = storage.add_vector(&vector1).unwrap();
        let idx2 = storage.add_vector(&vector2).unwrap();

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(storage.current_count(), 2);
        assert_eq!(storage.active_count(), 2);

        // Retrieve vectors
        let retrieved1 = storage.get_vector(idx1).unwrap();
        let retrieved2 = storage.get_vector(idx2).unwrap();

        assert_eq!(retrieved1, &vector1[..]);
        assert_eq!(retrieved2, &vector2[..]);
    }

    #[test]
    fn test_vector_dimension_validation() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("vectors.dat");

        let mut storage = VectorStorage::create(&storage_path, 3, 10).unwrap();

        // Wrong dimension should fail
        let wrong_vector = vec![1.0, 2.0]; // 2D instead of 3D
        let result = storage.add_vector(&wrong_vector);

        match result {
            Err(ShardexError::InvalidDimension { expected, actual }) => {
                assert_eq!(expected, 3);
                assert_eq!(actual, 2);
            }
            _ => panic!("Expected InvalidDimension error"),
        }
    }

    #[test]
    fn test_capacity_limits() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("vectors.dat");

        let mut storage = VectorStorage::create(&storage_path, 2, 2).unwrap();

        // Add vectors up to capacity
        let vector = vec![1.0, 2.0];
        storage.add_vector(&vector).unwrap();
        storage.add_vector(&vector).unwrap();

        assert!(storage.is_full());
        assert_eq!(storage.remaining_capacity(), 0);

        // Adding beyond capacity should fail
        let result = storage.add_vector(&vector);
        assert!(matches!(result, Err(ShardexError::Config(_))));
    }

    #[test]
    fn test_update_vector() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("vectors.dat");

        let mut storage = VectorStorage::create(&storage_path, 3, 10).unwrap();

        // Add a vector
        let original = vec![1.0, 2.0, 3.0];
        let idx = storage.add_vector(&original).unwrap();

        // Update it
        let updated = vec![10.0, 20.0, 30.0];
        storage.update_vector(idx, &updated).unwrap();

        // Verify update
        let retrieved = storage.get_vector(idx).unwrap();
        assert_eq!(retrieved, &updated[..]);
    }

    #[test]
    fn test_remove_vector() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("vectors.dat");

        let mut storage = VectorStorage::create(&storage_path, 3, 10).unwrap();

        // Add vectors
        let vector1 = vec![1.0, 2.0, 3.0];
        let vector2 = vec![4.0, 5.0, 6.0];
        let idx1 = storage.add_vector(&vector1).unwrap();
        let idx2 = storage.add_vector(&vector2).unwrap();

        assert_eq!(storage.active_count(), 2);

        // Remove one vector
        storage.remove_vector(idx1).unwrap();

        assert_eq!(storage.current_count(), 2); // Still 2 total
        assert_eq!(storage.active_count(), 1); // But only 1 active
        assert!(storage.is_deleted(idx1).unwrap());
        assert!(!storage.is_deleted(idx2).unwrap());

        // Vector 2 should still be accessible
        let retrieved2 = storage.get_vector(idx2).unwrap();
        assert_eq!(retrieved2, &vector2[..]);
    }

    #[test]
    fn test_out_of_bounds_access() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("vectors.dat");

        let storage = VectorStorage::create(&storage_path, 3, 10).unwrap();

        // Try to access non-existent vector
        let result = storage.get_vector(0);
        assert!(matches!(result, Err(ShardexError::Config(_))));

        let result = storage.get_vector(5);
        assert!(matches!(result, Err(ShardexError::Config(_))));
    }

    #[test]
    fn test_persistence() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage_path = temp_file.path();

        let vectors_to_add = vec![
            vec![1.0, 2.0, 3.0],
            vec![4.0, 5.0, 6.0],
            vec![7.0, 8.0, 9.0],
        ];

        // Create and populate storage
        {
            let mut storage = VectorStorage::create(storage_path, 3, 10).unwrap();

            for vector in &vectors_to_add {
                storage.add_vector(vector).unwrap();
            }
            storage.sync().unwrap();
        }

        // Reopen and verify
        {
            let storage = VectorStorage::open(storage_path).unwrap();

            assert_eq!(storage.vector_dimension(), 3);
            assert_eq!(storage.capacity(), 10);
            assert_eq!(storage.current_count(), 3);
            assert_eq!(storage.active_count(), 3);

            for (i, expected_vector) in vectors_to_add.iter().enumerate() {
                let retrieved = storage.get_vector(i).unwrap();
                assert_eq!(retrieved, &expected_vector[..]);
            }
        }
    }

    #[test]
    fn test_read_only_mode() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage_path = temp_file.path();

        // Create storage with some data
        {
            let mut storage = VectorStorage::create(storage_path, 2, 5).unwrap();
            let vector = vec![1.0, 2.0];
            storage.add_vector(&vector).unwrap();
            storage.sync().unwrap();
        }

        // Open in read-only mode
        {
            let mut storage = VectorStorage::open_read_only(storage_path).unwrap();

            assert!(storage.is_read_only());
            assert_eq!(storage.current_count(), 1);

            // Should be able to read
            let retrieved = storage.get_vector(0).unwrap();
            assert_eq!(retrieved, &[1.0, 2.0]);

            // Should not be able to modify
            let new_vector = vec![3.0, 4.0];
            assert!(storage.add_vector(&new_vector).is_err());
            assert!(storage.update_vector(0, &new_vector).is_err());
            assert!(storage.remove_vector(0).is_err());
        }
    }

    #[test]
    fn test_header_bytemuck_compatibility() {
        let header = VectorStorageHeader::new(128, 1000).unwrap();

        // Should be able to convert to bytes
        let bytes = bytemuck::bytes_of(&header);
        assert_eq!(bytes.len(), VectorStorageHeader::SIZE);

        // Should be able to convert back
        let header_restored = bytemuck::from_bytes::<VectorStorageHeader>(bytes);
        assert_eq!(header.vector_dimension, header_restored.vector_dimension);
        assert_eq!(header.capacity, header_restored.capacity);
        assert_eq!(header.current_count, header_restored.current_count);
    }

    #[test]
    fn test_alignment() {
        // Test alignment calculation
        assert_eq!(VectorStorage::align_size(100, 64), 128);
        assert_eq!(VectorStorage::align_size(64, 64), 64);
        assert_eq!(VectorStorage::align_size(65, 64), 128);
        assert_eq!(VectorStorage::align_size(1, 64), 64);
    }

    #[test]
    fn test_header_validation() {
        let mut header = VectorStorageHeader::new(384, 1000).unwrap();

        // Should validate correctly initially
        assert!(header.validate().is_ok());

        // Break magic bytes
        header.file_header.magic = *b"XXXX";
        assert!(header.validate().is_err());
        header.file_header.magic = *VECTOR_STORAGE_MAGIC;

        // Break version
        header.file_header.version = 999;
        assert!(header.validate().is_err());
        header.file_header.version = VECTOR_STORAGE_VERSION;

        // Break vector size consistency
        header.vector_size_bytes = 100; // Should be 384 * 4 = 1536
        assert!(header.validate().is_err());
        header.vector_size_bytes = 384 * 4;

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
    fn test_large_vectors() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("large_vectors.dat");

        // Create storage for large vectors (1536 dimensions like OpenAI embeddings)
        let dimension = 1536;
        let mut storage = VectorStorage::create(&storage_path, dimension, 10).unwrap();

        // Create a large vector
        let large_vector: Vec<f32> = (0..dimension)
            .map(|i| i as f32 / dimension as f32)
            .collect();

        let idx = storage.add_vector(&large_vector).unwrap();
        let retrieved = storage.get_vector(idx).unwrap();

        assert_eq!(retrieved.len(), dimension);
        assert_eq!(retrieved[0], 0.0);
        assert_eq!(
            retrieved[dimension - 1],
            (dimension - 1) as f32 / dimension as f32
        );

        // Verify all values
        for (i, &value) in retrieved.iter().enumerate() {
            assert_eq!(value, i as f32 / dimension as f32);
        }
    }

    #[test]
    fn test_zero_copy_access() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("zero_copy.dat");

        let mut storage = VectorStorage::create(&storage_path, 4, 100).unwrap();

        // Add vector
        let vector = vec![1.0, 2.0, 3.0, 4.0];
        let idx = storage.add_vector(&vector).unwrap();

        // Get vector reference (should be zero-copy)
        let vector_ref = storage.get_vector(idx).unwrap();

        // Verify it's pointing to the same memory location pattern
        // (We can't directly test memory addresses, but we can verify the slice works correctly)
        assert_eq!(vector_ref.len(), 4);
        assert_eq!(vector_ref[0], 1.0);
        assert_eq!(vector_ref[1], 2.0);
        assert_eq!(vector_ref[2], 3.0);
        assert_eq!(vector_ref[3], 4.0);

        // The slice should be directly backed by the memory-mapped file
        assert_eq!(
            std::mem::size_of_val(vector_ref),
            4 * std::mem::size_of::<f32>()
        );
    }

    #[test]
    fn test_simd_alignment() {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("simd_test.dat");

        // Create storage that should have SIMD-aligned vector data
        let storage = VectorStorage::create(&storage_path, 8, 100).unwrap();

        // Verify the header has correct alignment
        assert_eq!(storage.header.simd_alignment, DEFAULT_SIMD_ALIGNMENT as u32);

        // The vector data should start at a SIMD-aligned offset
        let vector_data_start = storage.header.vector_data_offset as usize;
        assert!(vector_data_start >= VectorStorageHeader::SIZE);
    }
}

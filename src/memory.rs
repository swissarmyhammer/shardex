//! Memory-mapped file abstractions for Shardex
//!
//! This module provides safe abstractions over memory-mapped files using memmap2,
//! with comprehensive error handling, data integrity validation, and file management
//! utilities. All operations are designed to be async-compatible and memory-safe.
//!
//! # Key Components
//!
//! - [`MemoryMappedFile`]: Core wrapper around memmap2 with safety abstractions
//! - [`FileHeader`]: Structured header with magic bytes and checksum validation
//! - File creation and resizing utilities with proper error handling
//!
//! # Usage Examples
//!
//! ## Creating and Writing to a Memory-Mapped File
//!
//! ```rust
//! use shardex::memory::{MemoryMappedFile, FileHeader};
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
//! let header = FileHeader::new(b"SHRD", 1, &[42u8; 100]);
//! mmf.write_at(0, &header)?;
//!
//! // Write some data
//! let data: [u32; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
//! mmf.write_at(FileHeader::SIZE, &data)?;
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
//! use shardex::memory::{MemoryMappedFile, FileHeader};
//! use std::path::Path;
//!
//! # fn read_example(file_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
//! // Open existing file for reading
//! let mmf = MemoryMappedFile::open_read_only(file_path)?;
//!
//! // Read and validate header
//! let header: FileHeader = mmf.read_at(0)?;
//! header.validate_magic(b"SHRD")?;
//! header.validate_checksum(&mmf.as_slice()[FileHeader::SIZE..])?;
//!
//! // Read typed data
//! let data: [u32; 10] = mmf.read_at(FileHeader::SIZE)?;
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

/// File header structure with magic bytes and checksum validation
///
/// FileHeader provides a standardized way to identify file formats and
/// validate data integrity. It includes version information for compatibility
/// checking and CRC32 checksums for corruption detection.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FileHeader {
    /// Magic bytes for file format identification (4 bytes)
    pub magic: [u8; 4],
    /// File format version
    pub version: u32,
    /// CRC32 checksum of the data following the header
    pub checksum: u32,
    /// Reserved bytes for future use (align to 16 bytes total)
    pub reserved: [u8; 4],
}

// SAFETY: FileHeader contains only Pod types and has repr(C) layout
unsafe impl Pod for FileHeader {}
// SAFETY: FileHeader can be safely zero-initialized
unsafe impl Zeroable for FileHeader {}

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
            .map_err(|e| {
                ShardexError::MemoryMapping(format!(
                    "Failed to create file {}: {}",
                    path.display(),
                    e
                ))
            })?;

        // Set file size
        file.set_len(size as u64).map_err(|e| {
            ShardexError::MemoryMapping(format!(
                "Failed to set file size for {}: {}",
                path.display(),
                e
            ))
        })?;

        // Create memory mapping
        let mmap = unsafe {
            MmapOptions::new().map_mut(&file).map_err(|e| {
                ShardexError::MemoryMapping(format!(
                    "Failed to create memory mapping for {}: {}",
                    path.display(),
                    e
                ))
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

        let file = File::open(path).map_err(|e| {
            ShardexError::MemoryMapping(format!(
                "Failed to open file {}: {}",
                path.display(),
                e
            ))
        })?;

        let len = file.metadata().map_err(|e| {
            ShardexError::MemoryMapping(format!(
                "Failed to get file metadata for {}: {}",
                path.display(),
                e
            ))
        })?.len() as usize;

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
                ShardexError::MemoryMapping(format!(
                    "Failed to open file {} for read-write: {}",
                    path.display(),
                    e
                ))
            })?;

        let len = file.metadata().map_err(|e| {
            ShardexError::MemoryMapping(format!(
                "Failed to get file metadata for {}: {}",
                path.display(),
                e
            ))
        })?.len() as usize;

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
        let file = self.file.as_ref().ok_or_else(|| {
            ShardexError::MemoryMapping("No file handle available for resize".to_string())
        })?;

        // Check if we're in read-write mode
        if matches!(self.mmap, MmapVariant::ReadOnly(_)) {
            return Err(ShardexError::MemoryMapping(
                "Cannot resize read-only memory mapping".to_string(),
            ));
        }

        // Drop the current mapping before resizing
        self.mmap = MmapVariant::ReadWrite(MmapMut::map_anon(0).map_err(|e| {
            ShardexError::MemoryMapping(format!("Failed to create temporary mapping: {}", e))
        })?);

        // Resize the file
        file.set_len(new_size as u64).map_err(|e| {
            ShardexError::MemoryMapping(format!("Failed to resize file: {}", e))
        })?;

        // Create new mapping
        let new_mmap = unsafe {
            MmapOptions::new().map_mut(file).map_err(|e| {
                ShardexError::MemoryMapping(format!("Failed to create new memory mapping: {}", e))
            })?
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
                mmap.flush().map_err(|e| {
                    ShardexError::MemoryMapping(format!("Failed to sync memory mapping: {}", e))
                })?;
                Ok(())
            }
        }
    }

    /// Check if the mapping is read-only
    pub fn is_read_only(&self) -> bool {
        matches!(self.mmap, MmapVariant::ReadOnly(_))
    }
}

impl FileHeader {
    /// Size of the FileHeader structure in bytes
    pub const SIZE: usize = std::mem::size_of::<FileHeader>();

    /// Create a new file header with magic bytes and checksum
    ///
    /// The checksum is calculated from the provided data slice.
    pub fn new(magic: &[u8; 4], version: u32, data: &[u8]) -> Self {
        let checksum = Self::calculate_checksum(data);
        Self {
            magic: *magic,
            version,
            checksum,
            reserved: [0; 4],
        }
    }

    /// Create a file header without checksum validation
    ///
    /// Use this when the checksum will be calculated and set later.
    pub fn new_without_checksum(magic: &[u8; 4], version: u32) -> Self {
        Self {
            magic: *magic,
            version,
            checksum: 0,
            reserved: [0; 4],
        }
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

    /// Validate the checksum against provided data
    pub fn validate_checksum(&self, data: &[u8]) -> Result<(), ShardexError> {
        let expected_checksum = Self::calculate_checksum(data);
        if self.checksum != expected_checksum {
            return Err(ShardexError::Corruption(format!(
                "Checksum mismatch: expected {}, found {}",
                expected_checksum, self.checksum
            )));
        }
        Ok(())
    }

    /// Update the checksum based on provided data
    pub fn update_checksum(&mut self, data: &[u8]) {
        self.checksum = Self::calculate_checksum(data);
    }

    /// Calculate CRC32 checksum for data
    fn calculate_checksum(data: &[u8]) -> u32 {
        // Simple CRC32 implementation
        const CRC32_TABLE: [u32; 256] = generate_crc32_table();
        let mut crc = 0xFFFFFFFF;
        
        for &byte in data {
            let table_index = ((crc ^ u32::from(byte)) & 0xFF) as usize;
            crc = (crc >> 8) ^ CRC32_TABLE[table_index];
        }
        
        crc ^ 0xFFFFFFFF
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
    use tempfile::{TempDir, NamedTempFile};

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
    fn test_file_header_creation() {
        let magic = b"TEST";
        let data = b"Hello, World!";
        let header = FileHeader::new(magic, 1, data);
        
        assert_eq!(header.magic, *magic);
        assert_eq!(header.version, 1);
        assert_ne!(header.checksum, 0); // Should have calculated checksum
    }

    #[test]
    fn test_file_header_magic_validation() {
        let header = FileHeader::new(b"TEST", 1, b"data");
        
        // Valid magic
        assert!(header.validate_magic(b"TEST").is_ok());
        
        // Invalid magic
        assert!(header.validate_magic(b"FAIL").is_err());
    }

    #[test]
    fn test_file_header_checksum_validation() {
        let data = b"Hello, World!";
        let header = FileHeader::new(b"TEST", 1, data);
        
        // Valid checksum
        assert!(header.validate_checksum(data).is_ok());
        
        // Invalid checksum
        assert!(header.validate_checksum(b"Different data").is_err());
    }

    #[test]
    fn test_file_header_update_checksum() {
        let mut header = FileHeader::new_without_checksum(b"TEST", 1);
        assert_eq!(header.checksum, 0);
        
        let data = b"Some data";
        header.update_checksum(data);
        assert_ne!(header.checksum, 0);
        
        // Should validate correctly
        assert!(header.validate_checksum(data).is_ok());
    }

    #[test]
    fn test_file_header_bytemuck() {
        let header = FileHeader::new(b"TEST", 1, b"data");
        
        // Should be able to convert to bytes
        let bytes = bytemuck::bytes_of(&header);
        assert_eq!(bytes.len(), FileHeader::SIZE);
        
        // Should be able to convert back
        let header_restored = bytemuck::from_bytes::<FileHeader>(bytes);
        assert_eq!(header.magic, header_restored.magic);
        assert_eq!(header.version, header_restored.version);
        assert_eq!(header.checksum, header_restored.checksum);
    }

    #[test]
    fn test_crc32_consistency() {
        let data1 = b"Hello, World!";
        let data2 = b"Hello, World!";
        let data3 = b"Different data";
        
        let checksum1 = FileHeader::calculate_checksum(data1);
        let checksum2 = FileHeader::calculate_checksum(data2);
        let checksum3 = FileHeader::calculate_checksum(data3);
        
        assert_eq!(checksum1, checksum2);
        assert_ne!(checksum1, checksum3);
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
            
            let header = FileHeader::new(b"SHRD", 1, data_bytes);
            mmf.write_at(0, &header).unwrap();
            mmf.write_slice_at(FileHeader::SIZE, &test_data).unwrap();
            mmf.sync().unwrap();
        }
        
        // Read back and validate
        {
            let mmf = MemoryMappedFile::open_read_only(&file_path).unwrap();
            
            let header: FileHeader = mmf.read_at(0).unwrap();
            header.validate_magic(b"SHRD").unwrap();
            
            let read_data: &[u32] = mmf.read_slice_at(FileHeader::SIZE, test_data.len()).unwrap();
            header.validate_checksum(bytemuck::cast_slice(read_data)).unwrap();
            
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
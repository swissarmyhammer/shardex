//! Document text storage implementation for Shardex
//!
//! This module implements the core DocumentTextStorage component that manages
//! memory-mapped text storage files with append-only semantics and efficient lookup.
//!
//! The storage system uses two files:
//! - **text_index.dat**: Contains document metadata and text location information
//! - **text_data.dat**: Contains the actual UTF-8 text data with length prefixes
//!
//! # Architecture
//!
//! The document text storage follows Shardex's established patterns:
//! - Memory-mapped files for efficient access
//! - Append-only storage for consistency
//! - Backward search for finding latest document versions
//! - Comprehensive error handling and validation
//!
//! # File Formats
//!
//! ## Text Index File (text_index.dat)
//! ```text
//! [TextIndexHeader: 104 bytes]     // File header with metadata
//! [DocumentTextEntry: 32 bytes] *  // Document entries (append-only)
//! ```
//!
//! ## Text Data File (text_data.dat)
//! ```text
//! [TextDataHeader: 104 bytes]                           // File header
//! [length: u32][utf8_text_data][length: u32][data]...   // Text blocks with length prefixes
//! ```
//!
//! # Usage Example
//!
//! ```rust
//! use shardex::document_text_storage::DocumentTextStorage;
//! use shardex::identifiers::DocumentId;
//! use tempfile::TempDir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let max_size = 10 * 1024 * 1024; // 10MB
//!
//! // Create new document text storage
//! let mut storage = DocumentTextStorage::create(&temp_dir, max_size)?;
//!
//! // Store document text
//! let doc_id = DocumentId::new();
//! let text = "The quick brown fox jumps over the lazy dog.";
//! storage.store_text(doc_id, text)?;
//!
//! // Retrieve document text
//! let retrieved = storage.get_text(doc_id)?;
//! assert_eq!(text, retrieved);
//! # Ok(())
//! # }
//! ```

use crate::document_text_entry::{DocumentTextEntry, TextDataHeader, TextIndexHeader};
use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use crate::memory::MemoryMappedFile;
use std::path::Path;

/// Document text storage manager using memory-mapped files
///
/// DocumentTextStorage manages two memory-mapped files to provide efficient
/// text storage and retrieval with append-only semantics. It supports:
/// - Atomic text updates via memory mapping
/// - Backward search for latest document versions
/// - UTF-8 validation and size limit enforcement
/// - Comprehensive error handling and recovery
pub struct DocumentTextStorage {
    /// Memory-mapped text index file (text_index.dat)
    text_index_file: MemoryMappedFile,
    /// Memory-mapped text data file (text_data.dat)
    text_data_file: MemoryMappedFile,
    /// Cached index header for efficient access
    index_header: TextIndexHeader,
    /// Cached data header for efficient access
    data_header: TextDataHeader,
    /// Maximum size limit for individual documents
    max_document_size: usize,
}

impl DocumentTextStorage {
    /// Create new document text storage in the given directory
    ///
    /// Creates both index and data files with proper headers and initial size allocation.
    /// The directory will be created if it doesn't exist.
    ///
    /// # Arguments
    /// * `directory` - Directory path where storage files will be created
    /// * `max_document_size` - Maximum allowed size for individual documents in bytes
    ///
    /// # Returns
    /// * `Ok(DocumentTextStorage)` - Successfully created storage instance
    /// * `Err(ShardexError)` - File creation or initialization failed
    ///
    /// # Examples
    /// ```rust
    /// use shardex::document_text_storage::DocumentTextStorage;
    /// use tempfile::TempDir;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let temp_dir = TempDir::new()?;
    /// let storage = DocumentTextStorage::create(&temp_dir, 10_000_000)?; // 10MB limit
    /// # Ok(())
    /// # }
    /// ```
    pub fn create<P: AsRef<Path>>(
        directory: P,
        max_document_size: usize,
    ) -> Result<Self, ShardexError> {
        let directory = directory.as_ref();

        // Create directory if it doesn't exist
        std::fs::create_dir_all(directory).map_err(|e| {
            ShardexError::MemoryMapping(format!(
                "Failed to create directory {}: {}",
                directory.display(),
                e
            ))
        })?;

        // Define file paths
        let index_path = directory.join("text_index.dat");
        let data_path = directory.join("text_data.dat");

        // Create index file with initial size (header + space for some entries)
        let initial_index_size = TextIndexHeader::SIZE + (1000 * DocumentTextEntry::SIZE);
        let mut text_index_file = MemoryMappedFile::create(&index_path, initial_index_size)?;

        // Create data file with initial size (header + space for text)
        let initial_data_size = TextDataHeader::SIZE + 1024 * 1024; // 1MB initial
        let mut text_data_file = MemoryMappedFile::create(&data_path, initial_data_size)?;

        // Initialize index header
        let index_header = TextIndexHeader::new();
        text_index_file.write_at(0, &index_header)?;

        // Initialize data header
        let data_header = TextDataHeader::new();
        text_data_file.write_at(0, &data_header)?;

        // Sync headers to disk
        text_index_file.sync()?;
        text_data_file.sync()?;

        Ok(Self {
            text_index_file,
            text_data_file,
            index_header,
            data_header,
            max_document_size,
        })
    }

    /// Open existing document text storage
    ///
    /// Opens both index and data files and validates their headers. The files
    /// must already exist and have valid headers.
    ///
    /// # Arguments
    /// * `directory` - Directory path containing existing storage files
    ///
    /// # Returns
    /// * `Ok(DocumentTextStorage)` - Successfully opened storage instance
    /// * `Err(ShardexError)` - File opening or validation failed
    ///
    /// # Examples
    /// ```rust
    /// use shardex::document_text_storage::DocumentTextStorage;
    /// use std::path::Path;
    ///
    /// # fn open_example(dir_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    /// let storage = DocumentTextStorage::open(dir_path)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn open<P: AsRef<Path>>(directory: P) -> Result<Self, ShardexError> {
        let directory = directory.as_ref();

        // Define file paths
        let index_path = directory.join("text_index.dat");
        let data_path = directory.join("text_data.dat");

        // Open files in read-write mode
        let text_index_file = MemoryMappedFile::open_read_write(&index_path)?;
        let text_data_file = MemoryMappedFile::open_read_write(&data_path)?;

        // Read and validate index header
        let index_header: TextIndexHeader = text_index_file.read_at(0)?;
        index_header.validate()?;

        // Read and validate data header
        let data_header: TextDataHeader = text_data_file.read_at(0)?;
        data_header.validate()?;

        // Use default max document size (will be overridden by caller if needed)
        let max_document_size = 10 * 1024 * 1024; // 10MB default

        Ok(Self {
            text_index_file,
            text_data_file,
            index_header,
            data_header,
            max_document_size,
        })
    }

    /// Store document text and return offset information
    ///
    /// Appends the text to the data file and creates a corresponding index entry.
    /// The text is validated for UTF-8 encoding and size limits before storage.
    ///
    /// # Arguments
    /// * `document_id` - Unique identifier for the document
    /// * `text` - UTF-8 text content to store
    ///
    /// # Returns
    /// * `Ok(())` - Text successfully stored
    /// * `Err(ShardexError)` - Storage failed (size limit, UTF-8 validation, etc.)
    ///
    /// # Examples
    /// ```rust
    /// use shardex::document_text_storage::DocumentTextStorage;
    /// use shardex::identifiers::DocumentId;
    /// use tempfile::TempDir;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let temp_dir = TempDir::new()?;
    /// let mut storage = DocumentTextStorage::create(&temp_dir, 10_000_000)?;
    ///
    /// let doc_id = DocumentId::new();
    /// storage.store_text(doc_id, "Hello, world!")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn store_text(&mut self, document_id: DocumentId, text: &str) -> Result<(), ShardexError> {
        // Validate text size
        if text.len() > self.max_document_size {
            return Err(ShardexError::document_too_large(
                text.len(),
                self.max_document_size,
            ));
        }

        // Validate UTF-8 (str type guarantees this, but be explicit)
        if !text.is_char_boundary(text.len()) {
            return Err(ShardexError::text_corruption(
                "Text contains invalid UTF-8 sequences",
            ));
        }

        // Append text data to data file and get the offset where it was stored
        let text_offset = self.append_text_data(text)?;

        // Create index entry
        let entry = DocumentTextEntry::new(document_id, text_offset, text.len() as u64);
        entry.validate()?;

        // Append index entry
        self.append_index_entry(&entry)?;

        Ok(())
    }

    /// Retrieve full document text by document ID
    ///
    /// Searches backward through the index to find the latest entry for the document,
    /// then reads the corresponding text from the data file.
    ///
    /// # Arguments
    /// * `document_id` - Unique identifier for the document
    ///
    /// # Returns
    /// * `Ok(String)` - The document text
    /// * `Err(ShardexError)` - Document not found or read failed
    ///
    /// # Examples
    /// ```rust
    /// use shardex::document_text_storage::DocumentTextStorage;
    /// use shardex::identifiers::DocumentId;
    /// use tempfile::TempDir;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let temp_dir = TempDir::new()?;
    /// let mut storage = DocumentTextStorage::create(&temp_dir, 10_000_000)?;
    ///
    /// let doc_id = DocumentId::new();
    /// storage.store_text(doc_id, "Hello, world!")?;
    ///
    /// let retrieved = storage.get_text(doc_id)?;
    /// assert_eq!(retrieved, "Hello, world!");
    /// # Ok(())
    /// # }
    /// ```
    pub fn get_text(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        // Find the latest entry for this document
        let entry = self
            .find_latest_document_entry(document_id)?
            .ok_or_else(|| ShardexError::document_text_not_found(document_id.to_string()))?;

        // Read text from data file
        self.read_text_at_offset(entry.text_offset, entry.text_length)
    }

    /// Find latest document entry by searching backwards through index
    ///
    /// Searches from the end of the index file backward to find the most recent
    /// entry for the specified document. This ensures we get the latest version
    /// in append-only storage.
    ///
    /// # Arguments
    /// * `document_id` - Document ID to search for
    ///
    /// # Returns
    /// * `Ok(Some(DocumentTextEntry))` - Found the latest entry
    /// * `Ok(None)` - No entry found for this document
    /// * `Err(ShardexError)` - Search failed due to corruption or I/O error
    fn find_latest_document_entry(
        &self,
        document_id: DocumentId,
    ) -> Result<Option<DocumentTextEntry>, ShardexError> {
        let entry_count = self.index_header.entry_count;
        if entry_count == 0 {
            return Ok(None);
        }

        // Search backward through entries (most recent first)
        for i in (0..entry_count).rev() {
            let offset = self.index_header.offset_for_entry(i);
            let entry: DocumentTextEntry = self.text_index_file.read_at(offset as usize)?;

            // Validate the entry to detect corruption
            entry.validate().map_err(|e| {
                ShardexError::text_corruption(format!(
                    "Corrupted index entry at position {}: {}",
                    i, e
                ))
            })?;

            if entry.is_for_document(document_id) {
                return Ok(Some(entry));
            }
        }

        Ok(None)
    }

    /// Read text data at specific offset and length
    ///
    /// Reads UTF-8 text from the data file at the specified offset, validating
    /// the length prefix and ensuring the text is valid UTF-8.
    ///
    /// # Arguments
    /// * `offset` - Byte offset in the data file where text begins
    /// * `length` - Expected length of the text in bytes
    ///
    /// # Returns
    /// * `Ok(String)` - Successfully read and validated text
    /// * `Err(ShardexError)` - Read failed or text validation failed
    fn read_text_at_offset(&self, offset: u64, length: u64) -> Result<String, ShardexError> {
        // Validate offset is within file bounds
        if offset + 4 + length > self.text_data_file.len() as u64 {
            return Err(ShardexError::text_corruption(format!(
                "Text offset {} + length {} exceeds data file size {}",
                offset,
                length,
                self.text_data_file.len()
            )));
        }

        // Read length prefix (u32) - handle potential alignment issues
        let length_bytes = &self.text_data_file.as_slice()[offset as usize..(offset + 4) as usize];
        let stored_length = u32::from_le_bytes([
            length_bytes[0],
            length_bytes[1],
            length_bytes[2],
            length_bytes[3],
        ]);
        if stored_length as u64 != length {
            return Err(ShardexError::text_corruption(format!(
                "Length mismatch: expected {}, found {} at offset {}",
                length, stored_length, offset
            )));
        }

        // Read text data
        let text_slice =
            &self.text_data_file.as_slice()[(offset + 4) as usize..(offset + 4 + length) as usize];

        // Validate and convert to String
        String::from_utf8(text_slice.to_vec()).map_err(|e| {
            ShardexError::text_corruption(format!(
                "Invalid UTF-8 sequence at offset {}: {}",
                offset + 4,
                e
            ))
        })
    }

    /// Append text data to data file
    ///
    /// Writes text to the data file with a length prefix, updating the data header
    /// to track the new offset and size. Ensures file has sufficient space.
    ///
    /// # Arguments
    /// * `text` - UTF-8 text to append
    ///
    /// # Returns
    /// * `Ok(u64)` - Offset where the text was stored
    /// * `Err(ShardexError)` - Write failed or file resize needed
    fn append_text_data(&mut self, text: &str) -> Result<u64, ShardexError> {
        let text_bytes = text.as_bytes();
        let text_length = text_bytes.len() as u32;

        // Align the starting offset to 4 bytes
        let aligned_offset = (self.data_header.next_text_offset + 3) & !3; // Round up to nearest 4
        let alignment_padding = (aligned_offset - self.data_header.next_text_offset) as usize;

        // Calculate total size including alignment padding + length prefix + text data
        let total_size = alignment_padding + 4 + text_bytes.len();

        // Check if we need to resize the data file
        let required_size = self.data_header.next_text_offset + total_size as u64;
        if required_size > self.text_data_file.len() as u64 {
            let new_size = (required_size + 1024 * 1024).max(self.text_data_file.len() as u64 * 2);
            self.text_data_file.resize(new_size as usize)?;
        }

        let start_offset = self.data_header.next_text_offset;
        let mut_slice = self.text_data_file.as_mut_slice()?;

        // Write alignment padding if needed
        if alignment_padding > 0 {
            mut_slice[start_offset as usize..(start_offset + alignment_padding as u64) as usize]
                .fill(0);
        }

        // Write length prefix at aligned offset
        let length_bytes = text_length.to_le_bytes();
        mut_slice[aligned_offset as usize..(aligned_offset + 4) as usize]
            .copy_from_slice(&length_bytes);

        // Write text data
        let text_start = aligned_offset + 4;
        mut_slice[text_start as usize..(text_start as usize + text_bytes.len())]
            .copy_from_slice(text_bytes);

        // Update header
        self.data_header.next_text_offset += total_size as u64;
        self.data_header.total_text_size += text_bytes.len() as u64;

        // Write updated header to file
        self.text_data_file.write_at(0, &self.data_header)?;

        // Sync to ensure durability
        self.text_data_file.sync()?;

        Ok(aligned_offset)
    }

    /// Append index entry to index file
    ///
    /// Writes a new document text entry to the index file, updating the index header
    /// to track the entry count and next offset. Ensures file has sufficient space.
    ///
    /// # Arguments
    /// * `entry` - Document text entry to append
    ///
    /// # Returns
    /// * `Ok(())` - Entry successfully appended
    /// * `Err(ShardexError)` - Write failed or file resize needed
    fn append_index_entry(&mut self, entry: &DocumentTextEntry) -> Result<(), ShardexError> {
        // Check if we need to resize the index file
        let required_size = self.index_header.next_entry_offset + DocumentTextEntry::SIZE as u64;
        if required_size > self.text_index_file.len() as u64 {
            let new_size = (required_size + 32 * 1024).max(self.text_index_file.len() as u64 * 2);
            self.text_index_file.resize(new_size as usize)?;
        }

        // Write entry at next offset
        let offset = self.index_header.next_entry_offset;
        self.text_index_file.write_at(offset as usize, entry)?;

        // Update header
        self.index_header.add_entry();

        // Write updated header to file
        self.text_index_file.write_at(0, &self.index_header)?;

        // Sync to ensure durability
        self.text_index_file.sync()?;

        Ok(())
    }

    /// Get the current number of document entries in the index
    pub fn entry_count(&self) -> u32 {
        self.index_header.entry_count
    }

    /// Get the total size of text data stored
    pub fn total_text_size(&self) -> u64 {
        self.data_header.total_text_size
    }

    /// Get the maximum allowed document size
    pub fn max_document_size(&self) -> usize {
        self.max_document_size
    }

    /// Update the maximum document size limit
    pub fn set_max_document_size(&mut self, max_size: usize) {
        self.max_document_size = max_size;
    }

    /// Check if the storage is empty (no documents stored)
    pub fn is_empty(&self) -> bool {
        self.index_header.is_empty()
    }

    /// Get storage utilization ratio (stored text / total file size)
    pub fn utilization_ratio(&self) -> f64 {
        self.data_header.utilization_ratio()
    }

    /// Sync both index and data files to disk
    ///
    /// Ensures all pending changes are written to persistent storage.
    pub fn sync(&self) -> Result<(), ShardexError> {
        self.text_index_file.sync()?;
        self.text_data_file.sync()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_document_text_storage() {
        let temp_dir = TempDir::new().unwrap();
        let max_size = 1024 * 1024; // 1MB

        let storage = DocumentTextStorage::create(&temp_dir, max_size).unwrap();

        assert_eq!(storage.max_document_size(), max_size);
        assert_eq!(storage.entry_count(), 0);
        assert_eq!(storage.total_text_size(), 0);
        assert!(storage.is_empty());
    }

    #[test]
    fn test_store_and_retrieve_text() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        let doc_id = DocumentId::new();
        let text = "The quick brown fox jumps over the lazy dog.";

        // Store text
        storage.store_text(doc_id, text).unwrap();

        // Verify storage state
        assert_eq!(storage.entry_count(), 1);
        assert_eq!(storage.total_text_size(), text.len() as u64);
        assert!(!storage.is_empty());

        // Retrieve text
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(text, retrieved);
    }

    #[test]
    fn test_store_multiple_documents() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        let text1 = "First document text.";
        let text2 = "Second document with different content.";

        // Store both documents
        storage.store_text(doc1, text1).unwrap();
        storage.store_text(doc2, text2).unwrap();

        // Verify storage state
        assert_eq!(storage.entry_count(), 2);
        assert_eq!(
            storage.total_text_size(),
            (text1.len() + text2.len()) as u64
        );

        // Retrieve both documents
        assert_eq!(storage.get_text(doc1).unwrap(), text1);
        assert_eq!(storage.get_text(doc2).unwrap(), text2);
    }

    #[test]
    fn test_document_updates() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        let doc_id = DocumentId::new();
        let text1 = "Original text.";
        let text2 = "Updated text with more content.";

        // Store original text
        storage.store_text(doc_id, text1).unwrap();
        assert_eq!(storage.get_text(doc_id).unwrap(), text1);

        // Update with new text (creates new entry)
        storage.store_text(doc_id, text2).unwrap();

        // Should retrieve the latest (updated) text
        assert_eq!(storage.get_text(doc_id).unwrap(), text2);

        // Should have 2 entries (append-only)
        assert_eq!(storage.entry_count(), 2);
    }

    #[test]
    fn test_size_limit_enforcement() {
        let temp_dir = TempDir::new().unwrap();
        let small_limit = 100; // 100 bytes
        let mut storage = DocumentTextStorage::create(&temp_dir, small_limit).unwrap();

        let doc_id = DocumentId::new();
        let large_text = "A".repeat(200); // 200 bytes, exceeds limit

        // Should fail due to size limit
        let result = storage.store_text(doc_id, &large_text);
        assert!(result.is_err());

        match result.unwrap_err() {
            ShardexError::DocumentTooLarge { size, max_size } => {
                assert_eq!(size, 200);
                assert_eq!(max_size, small_limit);
            }
            e => panic!("Expected DocumentTooLarge error, got {:?}", e),
        }
    }

    #[test]
    fn test_unicode_text_storage() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        let doc_id = DocumentId::new();
        let unicode_text = "Hello 世界! 🌍 Español Français العربية русский";

        // Store unicode text
        storage.store_text(doc_id, unicode_text).unwrap();

        // Retrieve and verify
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(unicode_text, retrieved);
    }

    #[test]
    fn test_document_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        let nonexistent_doc = DocumentId::new();

        // Should return not found error
        let result = storage.get_text(nonexistent_doc);
        assert!(result.is_err());

        match result.unwrap_err() {
            ShardexError::DocumentTextNotFound { .. } => {
                // Expected error type
            }
            e => panic!("Expected DocumentTextNotFound error, got {:?}", e),
        }
    }

    #[test]
    fn test_open_existing_storage() {
        let temp_dir = TempDir::new().unwrap();
        let doc_id = DocumentId::new();
        let text = "Persistent text data.";

        // Create storage and add data
        {
            let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
            storage.store_text(doc_id, text).unwrap();
            storage.sync().unwrap();
        }

        // Re-open storage and verify data persists
        {
            let storage = DocumentTextStorage::open(&temp_dir).unwrap();
            assert_eq!(storage.entry_count(), 1);
            assert_eq!(storage.get_text(doc_id).unwrap(), text);
        }
    }

    #[test]
    fn test_empty_text_rejection() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        let doc_id = DocumentId::new();
        let empty_text = "";

        // Should fail - empty text not allowed
        let result = storage.store_text(doc_id, empty_text);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_growth() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        // Store many documents to trigger file growth
        let mut doc_ids = Vec::new();
        for i in 0..100 {
            let doc_id = DocumentId::new();
            let text = format!("Document {} with some content to fill space.", i);
            storage.store_text(doc_id, &text).unwrap();
            doc_ids.push((doc_id, text));
        }

        // Verify all documents are retrievable
        for (doc_id, expected_text) in doc_ids {
            let retrieved = storage.get_text(doc_id).unwrap();
            assert_eq!(retrieved, expected_text);
        }

        assert_eq!(storage.entry_count(), 100);
    }

    #[test]
    fn test_backward_search_finds_latest() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        let doc_id = DocumentId::new();

        // Store multiple versions of the same document
        let versions = vec![
            "Version 1",
            "Version 2 - updated",
            "Version 3 - final version",
        ];

        for version in &versions {
            storage.store_text(doc_id, version).unwrap();
        }

        // Should retrieve the latest version
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, "Version 3 - final version");
    }

    #[test]
    fn test_sync_operations() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        let doc_id = DocumentId::new();
        let text = "Text that needs to be synced.";

        storage.store_text(doc_id, text).unwrap();

        // Sync should not fail
        storage.sync().unwrap();
    }

    #[test]
    fn test_utilization_ratio() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        // Initially should have low utilization (header overhead)
        let initial_ratio = storage.utilization_ratio();
        assert!(initial_ratio >= 0.0 && initial_ratio <= 1.0);

        // Add some text
        let doc_id = DocumentId::new();
        let text = "Some text to change utilization.";
        storage.store_text(doc_id, text).unwrap();

        let final_ratio = storage.utilization_ratio();
        assert!(final_ratio > initial_ratio);
        assert!(final_ratio >= 0.0 && final_ratio <= 1.0);
    }

    #[test]
    fn test_max_document_size_update() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 100).unwrap();

        assert_eq!(storage.max_document_size(), 100);

        // Update limit
        storage.set_max_document_size(200);
        assert_eq!(storage.max_document_size(), 200);

        // Should now accept larger documents
        let doc_id = DocumentId::new();
        let text = "A".repeat(150); // 150 bytes
        storage.store_text(doc_id, &text).unwrap();
    }

    #[test]
    fn test_stress_file_growth_high_document_count() {
        let temp_dir = TempDir::new().unwrap();
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

        // Store many documents to stress test file growth
        let document_count = 1000;
        let mut doc_ids = Vec::with_capacity(document_count);

        for i in 0..document_count {
            let doc_id = DocumentId::new();
            let text = format!("Document #{} with substantial content to fill space and test memory mapping behavior under stress conditions. This text is designed to be large enough to trigger multiple file growth operations and test the robustness of the memory mapping system.", i);
            storage.store_text(doc_id, &text).unwrap();
            doc_ids.push((doc_id, text));
        }

        // Verify all documents are still retrievable after stress
        for (i, (doc_id, expected_text)) in doc_ids.iter().enumerate() {
            let retrieved = storage.get_text(*doc_id).unwrap();
            assert_eq!(
                retrieved, *expected_text,
                "Document {} content mismatch after stress test",
                i
            );
        }

        assert_eq!(storage.entry_count(), document_count as u32);

        // Verify utilization ratio is reasonable
        let utilization = storage.utilization_ratio();
        assert!(
            utilization > 0.0 && utilization <= 1.0,
            "Utilization ratio {} is out of bounds",
            utilization
        );
    }

    #[test]
    fn test_maximum_file_size_boundary_conditions() {
        let temp_dir = TempDir::new().unwrap();
        let max_doc_size = 1000; // Small limit for testing boundary conditions
        let mut storage = DocumentTextStorage::create(&temp_dir, max_doc_size).unwrap();

        let doc_id = DocumentId::new();

        // Test exactly at the limit
        let exact_limit_text = "A".repeat(max_doc_size);
        storage.store_text(doc_id, &exact_limit_text).unwrap();
        assert_eq!(storage.get_text(doc_id).unwrap(), exact_limit_text);

        // Test one byte over the limit - should fail
        let over_limit_doc_id = DocumentId::new();
        let over_limit_text = "A".repeat(max_doc_size + 1);
        let result = storage.store_text(over_limit_doc_id, &over_limit_text);
        assert!(result.is_err());

        match result.unwrap_err() {
            ShardexError::DocumentTooLarge { size, max_size } => {
                assert_eq!(size, max_doc_size + 1);
                assert_eq!(max_size, max_doc_size);
            }
            e => panic!("Expected DocumentTooLarge error, got {:?}", e),
        }

        // Test one byte under the limit
        let under_limit_doc_id = DocumentId::new();
        let under_limit_text = "A".repeat(max_doc_size - 1);
        storage
            .store_text(under_limit_doc_id, &under_limit_text)
            .unwrap();
        assert_eq!(
            storage.get_text(under_limit_doc_id).unwrap(),
            under_limit_text
        );

        // Test zero length (should fail based on existing empty_text_rejection test)
        let empty_doc_id = DocumentId::new();
        let empty_result = storage.store_text(empty_doc_id, "");
        assert!(empty_result.is_err());
    }
}

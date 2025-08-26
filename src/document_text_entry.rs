//! Document text storage data structures for Shardex
//!
//! This module provides the core data structures for storing and indexing document text
//! in Shardex. It supports memory-mapped access patterns for high performance text
//! storage and retrieval using the same patterns established in the Shardex codebase.
//!
//! The text storage system uses two files:
//! - **Text Index File** (`text_index.dat`): Contains document metadata and text location information
//! - **Text Data File** (`text_data.dat`): Contains the actual UTF-8 text data
//!
//! # File Format Specification
//!
//! ## Text Index File Layout
//! ```text
//! [TextIndexHeader: varies]         // File header with metadata
//! [DocumentTextEntry: 32 bytes] * N // Document entries (append-only)
//! ```
//!
//! ## Text Data File Layout  
//! ```text
//! [TextDataHeader: varies]          // File header with metadata
//! [text_length: u32][utf8_text_data][text_length: u32][utf8_text_data]...
//! ```
//!
//! # Usage Examples
//!
//! ## Creating Index Headers
//!
//! ```rust
//! use shardex::document_text_entry::{TextIndexHeader, TEXT_INDEX_MAGIC};
//! use shardex::memory::FileHeader;
//!
//! // Create a new text index header
//! let header = TextIndexHeader::new();
//!
//! // Validate the header
//! header.validate().expect("Header should be valid");
//!
//! // Check magic bytes
//! header.validate_magic().expect("Magic bytes should be correct");
//! ```
//!
//! ## Working with Document Entries
//!
//! ```rust
//! use shardex::document_text_entry::DocumentTextEntry;
//! use shardex::identifiers::DocumentId;
//!
//! let doc_id = DocumentId::new();
//! let entry = DocumentTextEntry {
//!     document_id: doc_id,
//!     text_offset: 1024,
//!     text_length: 256,
//! };
//!
//! // Validate entry
//! entry.validate().expect("Entry should be valid");
//!
//! println!("Document {} has {} bytes of text at offset {}",
//!          entry.document_id,
//!          entry.text_length,
//!          entry.text_offset);
//! ```
//!
//! ## Memory Mapping Compatibility
//!
//! All structures in this module are designed for direct memory mapping:
//!
//! ```rust
//! use shardex::document_text_entry::TextIndexHeader;
//! use bytemuck;
//!
//! let header = TextIndexHeader::new();
//!
//! // Can be safely converted to bytes
//! let bytes = bytemuck::bytes_of(&header);
//!
//! // Can be safely read from bytes
//! let restored: TextIndexHeader = bytemuck::pod_read_unaligned(bytes);
//! assert_eq!(header, restored);
//! ```

use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use crate::memory::FileHeader;
use bytemuck::{Pod, Zeroable};

/// Magic bytes for text index files (`text_index.dat`)
pub const TEXT_INDEX_MAGIC: &[u8; 4] = b"TIDX";

/// Magic bytes for text data files (`text_data.dat`)  
pub const TEXT_DATA_MAGIC: &[u8; 4] = b"TDAT";

/// Current version for text index file format
pub const TEXT_INDEX_VERSION: u32 = 1;

/// Current version for text data file format
pub const TEXT_DATA_VERSION: u32 = 1;

/// Header for text index file (`text_index.dat`)
///
/// The text index header contains metadata about the document text index,
/// including entry counts and offset tracking for append-only operations.
/// This header follows the same pattern as other Shardex storage files.
///
/// # Memory Layout
/// ```text
/// Offset | Size | Field             | Description
/// -------|------|-------------------|------------------------------------------
/// 0      | 80   | file_header       | StandardHeader with magic, version, etc.
/// 80     | 4    | entry_count       | Number of document entries in the file
/// 84     | 8    | next_entry_offset | Offset where the next entry will be written
/// 92     | 12   | _padding          | Reserved padding for alignment
/// Total  | 104  |                   | Complete header size (with alignment)
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct TextIndexHeader {
    /// File header with magic bytes, version, checksum, etc.
    pub file_header: FileHeader,
    /// Number of document entries currently in the file
    pub entry_count: u32,
    /// Offset where the next document entry will be written
    pub next_entry_offset: u64,
    /// Padding to ensure proper alignment (must be zero)
    pub _padding: [u8; 12],
}

/// Per-document entry in text index (append-only storage)
///
/// Each document text entry maps a document ID to its text location in the
/// corresponding text data file. Entries are append-only, so multiple entries
/// for the same document may exist (representing updates over time).
///
/// # Memory Layout
/// ```text
/// Offset | Size | Field       | Description
/// -------|------|-------------|---------------------------------------
/// 0      | 16   | document_id | ULID-based document identifier
/// 16     | 8    | text_offset | Byte offset in text data file
/// 24     | 8    | text_length | Length of text in bytes
/// Total  | 32   |             | Complete entry size (well-aligned)
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct DocumentTextEntry {
    /// Unique identifier for the document (16 bytes - ULID)
    pub document_id: DocumentId,
    /// Byte offset in the text data file where this document's text starts
    pub text_offset: u64,
    /// Length of the document text in bytes
    pub text_length: u64,
}

/// Header for text data file (`text_data.dat`)
///
/// The text data header tracks the total amount of text stored and provides
/// offset information for append operations. All text is stored as UTF-8.
///
/// # Memory Layout
/// ```text
/// Offset | Size | Field             | Description
/// -------|------|-------------------|------------------------------------------
/// 0      | 80   | file_header       | StandardHeader with magic, version, etc.
/// 80     | 8    | total_text_size   | Total bytes of text stored
/// 88     | 8    | next_text_offset  | Offset where next text will be written
/// 96     | 8    | _padding          | Reserved padding for alignment
/// Total  | 104  |                   | Complete header size (with alignment)
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct TextDataHeader {
    /// File header with magic bytes, version, checksum, etc.
    pub file_header: FileHeader,
    /// Total bytes of text currently stored in the file
    pub total_text_size: u64,
    /// Offset where the next text block will be written
    pub next_text_offset: u64,
    /// Padding to ensure proper alignment (must be zero)
    pub _padding: [u8; 8],
}

// SAFETY: TextIndexHeader contains only Pod types and has repr(C) layout
unsafe impl Pod for TextIndexHeader {}
// SAFETY: TextIndexHeader can be safely zero-initialized
unsafe impl Zeroable for TextIndexHeader {}

// SAFETY: DocumentTextEntry contains only Pod types and has repr(C) layout
unsafe impl Pod for DocumentTextEntry {}
// SAFETY: DocumentTextEntry can be safely zero-initialized
unsafe impl Zeroable for DocumentTextEntry {}

// SAFETY: TextDataHeader contains only Pod types and has repr(C) layout
unsafe impl Pod for TextDataHeader {}
// SAFETY: TextDataHeader can be safely zero-initialized
unsafe impl Zeroable for TextDataHeader {}

impl TextIndexHeader {
    /// Size of the TextIndexHeader structure in bytes
    pub const SIZE: usize = std::mem::size_of::<TextIndexHeader>();

    /// Create a new text index header with current timestamp and proper initialization
    ///
    /// The header is initialized with zero entries and the next entry offset
    /// set to immediately after the header.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use shardex::document_text_entry::TextIndexHeader;
    ///
    /// let header = TextIndexHeader::new();
    /// assert_eq!(header.entry_count, 0);
    /// assert_eq!(header.next_entry_offset, TextIndexHeader::SIZE as u64);
    /// ```
    pub fn new() -> Self {
        Self {
            file_header: FileHeader::new_without_checksum(TEXT_INDEX_MAGIC, TEXT_INDEX_VERSION, Self::SIZE as u64),
            entry_count: 0,
            next_entry_offset: Self::SIZE as u64,
            _padding: [0; 12],
        }
    }

    /// Create a header with explicit data for checksum calculation
    ///
    /// This method creates a header with the checksum calculated from the
    /// provided data slice, which should represent the entry data.
    pub fn new_with_data(data: &[u8]) -> Self {
        let mut header = Self::new();
        header.file_header = FileHeader::new(TEXT_INDEX_MAGIC, TEXT_INDEX_VERSION, Self::SIZE as u64, data);
        header
    }

    /// Validate the header structure and magic bytes
    ///
    /// Performs comprehensive validation including magic bytes, version,
    /// structural integrity, and padding verification.
    pub fn validate(&self) -> Result<(), ShardexError> {
        // Validate magic bytes
        self.validate_magic()?;

        // Validate version (currently only version 1 is supported)
        self.file_header
            .validate_version(TEXT_INDEX_VERSION, TEXT_INDEX_VERSION)?;

        // Validate structure
        self.file_header.validate_structure()?;

        // Check that next_entry_offset is reasonable (at least past header)
        if self.next_entry_offset < Self::SIZE as u64 {
            return Err(ShardexError::Corruption(format!(
                "Invalid next_entry_offset: {} is less than header size {}",
                self.next_entry_offset,
                Self::SIZE
            )));
        }

        // Check padding is zero
        if self._padding != [0; 12] {
            return Err(ShardexError::Corruption(
                "TextIndexHeader padding is not zero".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate magic bytes match expected TEXT_INDEX_MAGIC
    pub fn validate_magic(&self) -> Result<(), ShardexError> {
        self.file_header.validate_magic(TEXT_INDEX_MAGIC)
    }

    /// Validate checksum against provided entry data
    ///
    /// Uses enhanced checksum calculation that includes the entire header structure
    /// (normalized) plus the provided data, ensuring comprehensive validation.
    pub fn validate_checksum(&self, data: &[u8]) -> Result<(), ShardexError> {
        self.file_header
            .validate_checksum(data)
            .map_err(|e| ShardexError::Corruption(format!("TextIndexHeader checksum validation failed: {}", e)))
    }

    /// Update checksum based on entry data
    pub fn update_checksum(&mut self, data: &[u8]) {
        self.file_header.update_checksum(data);
    }

    /// Get the offset where the next document entry should be written
    pub fn next_entry_offset(&self) -> u64 {
        self.next_entry_offset
    }

    /// Calculate the offset for a specific entry index
    pub fn offset_for_entry(&self, entry_index: u32) -> u64 {
        Self::SIZE as u64 + (entry_index as u64 * DocumentTextEntry::SIZE as u64)
    }

    /// Add a new entry to the header tracking (updates counts and offsets)
    pub fn add_entry(&mut self) {
        self.entry_count += 1;
        self.next_entry_offset += DocumentTextEntry::SIZE as u64;
    }

    /// Check if the header indicates an empty index
    pub fn is_empty(&self) -> bool {
        self.entry_count == 0
    }

    /// Get the total size consumed by all entries
    pub fn total_entries_size(&self) -> u64 {
        self.entry_count as u64 * DocumentTextEntry::SIZE as u64
    }
}

impl DocumentTextEntry {
    /// Size of the DocumentTextEntry structure in bytes
    pub const SIZE: usize = std::mem::size_of::<DocumentTextEntry>();

    /// Create a new document text entry
    ///
    /// # Examples
    ///
    /// ```rust
    /// use shardex::document_text_entry::DocumentTextEntry;
    /// use shardex::identifiers::DocumentId;
    ///
    /// let doc_id = DocumentId::new();
    /// let entry = DocumentTextEntry::new(doc_id, 1024, 512);
    ///
    /// assert_eq!(entry.text_offset, 1024);
    /// assert_eq!(entry.text_length, 512);
    /// ```
    pub fn new(document_id: DocumentId, text_offset: u64, text_length: u64) -> Self {
        Self {
            document_id,
            text_offset,
            text_length,
        }
    }

    /// Validate the entry structure
    ///
    /// Ensures the entry has reasonable values for offset and length.
    pub fn validate(&self) -> Result<(), ShardexError> {
        // Check that text_length is non-zero (empty text should not be stored)
        if self.text_length == 0 {
            return Err(ShardexError::InvalidInput {
                field: "text_length".to_string(),
                reason: "Text length cannot be zero".to_string(),
                suggestion: "Store non-empty text or remove the document".to_string(),
            });
        }

        // Check for reasonable maximum text size (prevent corruption/attacks)
        const MAX_TEXT_SIZE: u64 = 100 * 1024 * 1024; // 100MB per document
        if self.text_length > MAX_TEXT_SIZE {
            return Err(ShardexError::InvalidInput {
                field: "text_length".to_string(),
                reason: format!(
                    "Text length {} exceeds maximum allowed size {}",
                    self.text_length, MAX_TEXT_SIZE
                ),
                suggestion: "Break large documents into smaller chunks".to_string(),
            });
        }

        // Check that offset + length doesn't overflow
        if let Some(end_offset) = self.text_offset.checked_add(self.text_length) {
            // Check for reasonable file size limit (prevent corruption)
            const MAX_FILE_SIZE: u64 = 10_u64.pow(12); // 1TB limit
            if end_offset > MAX_FILE_SIZE {
                return Err(ShardexError::InvalidInput {
                    field: "text_offset".to_string(),
                    reason: format!("Text end position {} exceeds file size limit", end_offset),
                    suggestion: "Use separate data files for very large datasets".to_string(),
                });
            }
        } else {
            return Err(ShardexError::InvalidInput {
                field: "text_offset".to_string(),
                reason: "Text offset + length overflows u64".to_string(),
                suggestion: "Use smaller offset or length values".to_string(),
            });
        }

        Ok(())
    }

    /// Check if this entry is for the specified document
    pub fn is_for_document(&self, document_id: DocumentId) -> bool {
        self.document_id == document_id
    }

    /// Get the end offset (offset + length) of this text entry
    pub fn end_offset(&self) -> Option<u64> {
        self.text_offset.checked_add(self.text_length)
    }

    /// Check if this entry overlaps with another entry's byte range
    pub fn overlaps_with(&self, other: &DocumentTextEntry) -> bool {
        let self_end = match self.end_offset() {
            Some(end) => end,
            None => return false, // If we can't calculate end, assume no overlap
        };

        let other_end = match other.end_offset() {
            Some(end) => end,
            None => return false,
        };

        // Check for overlap: entries overlap if one starts before the other ends
        self.text_offset < other_end && other.text_offset < self_end
    }
}

impl TextDataHeader {
    /// Size of the TextDataHeader structure in bytes
    pub const SIZE: usize = std::mem::size_of::<TextDataHeader>();

    /// Create a new text data header with current timestamp and proper initialization
    ///
    /// The header is initialized with zero text size and the next offset
    /// set to immediately after the header.
    pub fn new() -> Self {
        Self {
            file_header: FileHeader::new_without_checksum(TEXT_DATA_MAGIC, TEXT_DATA_VERSION, Self::SIZE as u64),
            total_text_size: 0,
            next_text_offset: Self::SIZE as u64,
            _padding: [0; 8],
        }
    }

    /// Create a header with explicit data for checksum calculation
    pub fn new_with_data(data: &[u8]) -> Self {
        let mut header = Self::new();
        header.file_header = FileHeader::new(TEXT_DATA_MAGIC, TEXT_DATA_VERSION, Self::SIZE as u64, data);
        header
    }

    /// Validate the header structure and magic bytes
    pub fn validate(&self) -> Result<(), ShardexError> {
        // Validate magic bytes
        self.validate_magic()?;

        // Validate version
        self.file_header
            .validate_version(TEXT_DATA_VERSION, TEXT_DATA_VERSION)?;

        // Validate structure
        self.file_header.validate_structure()?;

        // Check that next_text_offset is reasonable (at least past header)
        if self.next_text_offset < Self::SIZE as u64 {
            return Err(ShardexError::Corruption(format!(
                "Invalid next_text_offset: {} is less than header size {}",
                self.next_text_offset,
                Self::SIZE
            )));
        }

        // Check padding is zero
        if self._padding != [0; 8] {
            return Err(ShardexError::Corruption(
                "TextDataHeader padding is not zero".to_string(),
            ));
        }

        Ok(())
    }

    /// Validate magic bytes match expected TEXT_DATA_MAGIC
    pub fn validate_magic(&self) -> Result<(), ShardexError> {
        self.file_header.validate_magic(TEXT_DATA_MAGIC)
    }

    /// Validate checksum against provided text data
    pub fn validate_checksum(&self, data: &[u8]) -> Result<(), ShardexError> {
        self.file_header
            .validate_checksum(data)
            .map_err(|e| ShardexError::Corruption(format!("TextDataHeader checksum validation failed: {}", e)))
    }

    /// Update checksum based on text data
    pub fn update_checksum(&mut self, data: &[u8]) {
        self.file_header.update_checksum(data);
    }

    /// Get the offset where the next text block should be written
    pub fn next_text_offset(&self) -> u64 {
        self.next_text_offset
    }

    /// Add text to the header tracking (updates size and offset)
    pub fn add_text(&mut self, text_length: u64) {
        self.total_text_size += text_length;
        self.next_text_offset += text_length + 8; // +8 for length prefix and suffix
    }

    /// Check if the header indicates empty data
    pub fn is_empty(&self) -> bool {
        self.total_text_size == 0
    }

    /// Get utilization ratio (stored text / file size)
    pub fn utilization_ratio(&self) -> f64 {
        if self.next_text_offset <= Self::SIZE as u64 {
            0.0
        } else {
            self.total_text_size as f64 / (self.next_text_offset - Self::SIZE as u64) as f64
        }
    }
}

impl Default for TextIndexHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for TextDataHeader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem;

    #[test]
    fn test_text_index_header_creation() {
        let header = TextIndexHeader::new();

        assert_eq!(header.entry_count, 0);
        assert_eq!(header.next_entry_offset, TextIndexHeader::SIZE as u64);
        assert_eq!(header._padding, [0; 12]);
        assert_eq!(header.file_header.magic, *TEXT_INDEX_MAGIC);
        assert_eq!(header.file_header.version, TEXT_INDEX_VERSION);
    }

    #[test]
    fn test_text_data_header_creation() {
        let header = TextDataHeader::new();

        assert_eq!(header.total_text_size, 0);
        assert_eq!(header.next_text_offset, TextDataHeader::SIZE as u64);
        assert_eq!(header._padding, [0; 8]);
        assert_eq!(header.file_header.magic, *TEXT_DATA_MAGIC);
        assert_eq!(header.file_header.version, TEXT_DATA_VERSION);
    }

    #[test]
    fn test_document_text_entry_creation() {
        let doc_id = DocumentId::new();
        let entry = DocumentTextEntry::new(doc_id, 1024, 512);

        assert_eq!(entry.document_id, doc_id);
        assert_eq!(entry.text_offset, 1024);
        assert_eq!(entry.text_length, 512);
    }

    #[test]
    fn test_header_validation() {
        let index_header = TextIndexHeader::new();
        let data_header = TextDataHeader::new();

        assert!(index_header.validate().is_ok());
        assert!(data_header.validate().is_ok());

        assert!(index_header.validate_magic().is_ok());
        assert!(data_header.validate_magic().is_ok());
    }

    #[test]
    fn test_document_text_entry_validation() {
        let doc_id = DocumentId::new();

        // Valid entry
        let valid_entry = DocumentTextEntry::new(doc_id, 1024, 512);
        assert!(valid_entry.validate().is_ok());

        // Invalid entry - zero length
        let zero_length_entry = DocumentTextEntry::new(doc_id, 1024, 0);
        assert!(zero_length_entry.validate().is_err());

        // Invalid entry - too large
        let too_large_entry = DocumentTextEntry::new(doc_id, 1024, 200 * 1024 * 1024); // 200MB
        assert!(too_large_entry.validate().is_err());
    }

    #[test]
    fn test_entry_overlap_detection() {
        let doc_id = DocumentId::new();

        let entry1 = DocumentTextEntry::new(doc_id, 100, 50); // 100-150
        let entry2 = DocumentTextEntry::new(doc_id, 125, 50); // 125-175 (overlaps)
        let entry3 = DocumentTextEntry::new(doc_id, 200, 50); // 200-250 (no overlap)

        assert!(entry1.overlaps_with(&entry2));
        assert!(entry2.overlaps_with(&entry1));
        assert!(!entry1.overlaps_with(&entry3));
        assert!(!entry3.overlaps_with(&entry1));
    }

    #[test]
    fn test_header_add_operations() {
        let mut index_header = TextIndexHeader::new();
        let mut data_header = TextDataHeader::new();

        // Initially empty
        assert!(index_header.is_empty());
        assert!(data_header.is_empty());

        // Add entry to index
        index_header.add_entry();
        assert_eq!(index_header.entry_count, 1);
        assert_eq!(
            index_header.next_entry_offset,
            TextIndexHeader::SIZE as u64 + DocumentTextEntry::SIZE as u64
        );
        assert!(!index_header.is_empty());

        // Add text to data
        data_header.add_text(512);
        assert_eq!(data_header.total_text_size, 512);
        assert_eq!(data_header.next_text_offset, TextDataHeader::SIZE as u64 + 512 + 8);
        assert!(!data_header.is_empty());
    }

    #[test]
    fn test_bytemuck_compatibility() {
        let doc_id = DocumentId::new();
        let index_header = TextIndexHeader::new();
        let data_header = TextDataHeader::new();
        let entry = DocumentTextEntry::new(doc_id, 1024, 512);

        // Test Pod trait - should be able to cast to bytes
        let index_bytes: &[u8] = bytemuck::bytes_of(&index_header);
        let data_bytes: &[u8] = bytemuck::bytes_of(&data_header);
        let entry_bytes: &[u8] = bytemuck::bytes_of(&entry);

        assert_eq!(index_bytes.len(), TextIndexHeader::SIZE);
        assert_eq!(data_bytes.len(), TextDataHeader::SIZE);
        assert_eq!(entry_bytes.len(), DocumentTextEntry::SIZE);

        // Test round-trip
        let index_restored: TextIndexHeader = bytemuck::pod_read_unaligned(index_bytes);
        let data_restored: TextDataHeader = bytemuck::pod_read_unaligned(data_bytes);
        let entry_restored: DocumentTextEntry = bytemuck::pod_read_unaligned(entry_bytes);

        assert_eq!(index_header, index_restored);
        assert_eq!(data_header, data_restored);
        assert_eq!(entry, entry_restored);
    }

    #[test]
    fn test_zeroable_trait() {
        let zero_index: TextIndexHeader = bytemuck::Zeroable::zeroed();
        let zero_data: TextDataHeader = bytemuck::Zeroable::zeroed();
        let zero_entry: DocumentTextEntry = bytemuck::Zeroable::zeroed();

        assert_eq!(zero_index.entry_count, 0);
        assert_eq!(zero_index.next_entry_offset, 0);
        assert_eq!(zero_data.total_text_size, 0);
        assert_eq!(zero_data.next_text_offset, 0);
        assert_eq!(zero_entry.document_id.raw(), 0);
        assert_eq!(zero_entry.text_offset, 0);
        assert_eq!(zero_entry.text_length, 0);
    }

    #[test]
    fn test_memory_layout_sizes() {
        // Verify struct sizes are reasonable (actual size may vary due to alignment requirements)
        let index_size = mem::size_of::<TextIndexHeader>();
        let data_size = mem::size_of::<TextDataHeader>();
        let entry_size = mem::size_of::<DocumentTextEntry>();

        // Headers should be at least the size of FileHeader (80) plus our fields
        assert!(index_size >= 80 + 4 + 8); // FileHeader + entry_count + next_entry_offset
        assert!(data_size >= 80 + 8 + 8); // FileHeader + total_text_size + next_text_offset
        assert_eq!(entry_size, 32); // DocumentId (16) + text_offset (8) + text_length (8)

        // Verify alignment requirements
        assert!(mem::align_of::<TextIndexHeader>() >= 8);
        assert!(mem::align_of::<TextDataHeader>() >= 8);
        assert!(mem::align_of::<DocumentTextEntry>() >= 8);

        // Verify SIZE constants match actual sizes
        assert_eq!(TextIndexHeader::SIZE, index_size);
        assert_eq!(TextDataHeader::SIZE, data_size);
        assert_eq!(DocumentTextEntry::SIZE, entry_size);

        // Verify sizes are multiples of alignment (important for arrays)
        assert_eq!(TextIndexHeader::SIZE % mem::align_of::<TextIndexHeader>(), 0);
        assert_eq!(TextDataHeader::SIZE % mem::align_of::<TextDataHeader>(), 0);
        assert_eq!(DocumentTextEntry::SIZE % mem::align_of::<DocumentTextEntry>(), 0);
    }

    #[test]
    fn test_constants() {
        assert_eq!(TEXT_INDEX_MAGIC, b"TIDX");
        assert_eq!(TEXT_DATA_MAGIC, b"TDAT");
        assert_eq!(TEXT_INDEX_VERSION, 1);
        assert_eq!(TEXT_DATA_VERSION, 1);

        assert_eq!(TextIndexHeader::SIZE, mem::size_of::<TextIndexHeader>());
        assert_eq!(TextDataHeader::SIZE, mem::size_of::<TextDataHeader>());
        assert_eq!(DocumentTextEntry::SIZE, mem::size_of::<DocumentTextEntry>());
    }

    #[test]
    fn test_entry_helper_methods() {
        let doc_id = DocumentId::new();
        let entry = DocumentTextEntry::new(doc_id, 1000, 500);

        assert!(entry.is_for_document(doc_id));
        assert!(!entry.is_for_document(DocumentId::new()));
        assert_eq!(entry.end_offset(), Some(1500));

        // Test overflow protection
        let overflow_entry = DocumentTextEntry::new(doc_id, u64::MAX, 1);
        assert_eq!(overflow_entry.end_offset(), None);
    }

    #[test]
    fn test_header_offset_calculations() {
        let header = TextIndexHeader::new();

        assert_eq!(header.offset_for_entry(0), TextIndexHeader::SIZE as u64);
        assert_eq!(
            header.offset_for_entry(1),
            TextIndexHeader::SIZE as u64 + DocumentTextEntry::SIZE as u64
        );
        assert_eq!(
            header.offset_for_entry(10),
            TextIndexHeader::SIZE as u64 + 10 * DocumentTextEntry::SIZE as u64
        );

        assert_eq!(header.total_entries_size(), 0);

        let mut header_with_entries = header;
        header_with_entries.entry_count = 5;
        assert_eq!(
            header_with_entries.total_entries_size(),
            5 * DocumentTextEntry::SIZE as u64
        );
    }

    #[test]
    fn test_utilization_calculation() {
        let mut header = TextDataHeader::new();

        // Empty header has 0 utilization
        assert_eq!(header.utilization_ratio(), 0.0);

        // Add some text
        header.total_text_size = 1000;
        header.next_text_offset = TextDataHeader::SIZE as u64 + 1500; // includes overhead

        let expected_ratio = 1000.0 / 1500.0;
        assert!((header.utilization_ratio() - expected_ratio).abs() < 0.001);
    }

    #[test]
    fn test_default_implementations() {
        let default_index = TextIndexHeader::default();
        let default_data = TextDataHeader::default();
        let new_index = TextIndexHeader::new();
        let new_data = TextDataHeader::new();

        // Compare structure, not exact equality (due to timestamps)
        assert_eq!(default_index.entry_count, new_index.entry_count);
        assert_eq!(default_index.next_entry_offset, new_index.next_entry_offset);
        assert_eq!(default_index._padding, new_index._padding);
        assert_eq!(default_index.file_header.magic, new_index.file_header.magic);
        assert_eq!(default_index.file_header.version, new_index.file_header.version);

        assert_eq!(default_data.total_text_size, new_data.total_text_size);
        assert_eq!(default_data.next_text_offset, new_data.next_text_offset);
        assert_eq!(default_data._padding, new_data._padding);
        assert_eq!(default_data.file_header.magic, new_data.file_header.magic);
        assert_eq!(default_data.file_header.version, new_data.file_header.version);
    }
}

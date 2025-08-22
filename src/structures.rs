//! Core data structures for Shardex
//!
//! This module provides the fundamental data structures used throughout Shardex:
//! - Posting: Represents a document posting with its vector embedding
//! - SearchResult: Search result with similarity scoring
//! - IndexStats: Index statistics for monitoring and observability
//!
//! All structures are designed for memory mapping compatibility using bytemuck traits.

use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

/// A posting represents a document segment with its vector embedding
///
/// Postings are stored in shards and support memory-mapped access for high performance.
/// The vector field requires special handling for memory mapping since Vec<f32> is not
/// Pod-compatible. For memory mapping, vectors are stored separately and referenced by
/// pointer and length.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Posting {
    /// Unique identifier for the document this posting belongs to
    pub document_id: DocumentId,
    /// Starting byte position within the document
    pub start: u32,
    /// Length of the text segment in bytes
    pub length: u32,
    /// Vector embedding for this posting segment
    pub vector: Vec<f32>,
}

/// Search result containing a posting with similarity score
///
/// SearchResult extends Posting with a similarity score for ranking results.
/// Like Posting, it supports memory-mapped access patterns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// Unique identifier for the document this result belongs to
    pub document_id: DocumentId,
    /// Starting byte position within the document  
    pub start: u32,
    /// Length of the text segment in bytes
    pub length: u32,
    /// Vector embedding for this result segment
    pub vector: Vec<f32>,
    /// Similarity score (higher means more similar)
    pub similarity_score: f32,
}

/// Memory-mapped compatible posting header
///
/// This structure can be memory-mapped directly. The vector data is stored
/// separately and referenced by pointer and length fields.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct PostingHeader {
    /// Document identifier
    pub document_id: DocumentId,
    /// Starting byte position within the document
    pub start: u32,
    /// Length of the text segment in bytes  
    pub length: u32,
    /// Number of dimensions in the vector
    pub vector_len: u32,
    /// Offset to vector data within the shard
    pub vector_offset: u64,
}

/// Memory-mapped compatible search result header
///
/// Like PostingHeader but includes similarity score for search results.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct SearchResultHeader {
    /// Document identifier
    pub document_id: DocumentId,
    /// Starting byte position within the document
    pub start: u32,
    /// Length of the text segment in bytes
    pub length: u32,
    /// Number of dimensions in the vector
    pub vector_len: u32,
    /// Offset to vector data
    pub vector_offset: u64,
    /// Similarity score (higher means more similar)
    pub similarity_score: f32,
}

/// Index statistics for monitoring and observability
///
/// Provides comprehensive metrics about the index state and performance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexStats {
    /// Total number of shards in the index
    pub total_shards: usize,
    /// Total number of postings across all shards
    pub total_postings: usize,
    /// Number of pending operations in WAL
    pub pending_operations: usize,
    /// Estimated memory usage in bytes
    pub memory_usage: usize,
    /// Number of active (non-deleted) postings
    pub active_postings: usize,
    /// Number of deleted postings
    pub deleted_postings: usize,
    /// Average shard utilization (0.0 to 1.0)
    pub average_shard_utilization: f32,
    /// Vector dimension size
    pub vector_dimension: usize,
    /// Total disk space used in bytes
    pub disk_usage: usize,
}

// SAFETY: PostingHeader contains only Pod types and has repr(C) layout
unsafe impl Pod for PostingHeader {}
// SAFETY: PostingHeader can be safely zero-initialized
unsafe impl Zeroable for PostingHeader {}

// SAFETY: SearchResultHeader contains only Pod types and has repr(C) layout
unsafe impl Pod for SearchResultHeader {}
// SAFETY: SearchResultHeader can be safely zero-initialized
unsafe impl Zeroable for SearchResultHeader {}

impl Posting {
    /// Create a new posting with vector dimension validation
    pub fn new(
        document_id: DocumentId,
        start: u32,
        length: u32,
        vector: Vec<f32>,
        expected_dimension: usize,
    ) -> Result<Self, ShardexError> {
        if vector.len() != expected_dimension {
            return Err(ShardexError::InvalidDimension {
                expected: expected_dimension,
                actual: vector.len(),
            });
        }

        Ok(Self {
            document_id,
            start,
            length,
            vector,
        })
    }

    /// Get vector dimension
    pub fn vector_dimension(&self) -> usize {
        self.vector.len()
    }

    /// Create a memory-mapped header for this posting
    pub fn to_header(&self, vector_offset: u64) -> PostingHeader {
        PostingHeader {
            document_id: self.document_id,
            start: self.start,
            length: self.length,
            vector_len: self.vector.len() as u32,
            vector_offset,
        }
    }

    /// Validate that this posting has the expected vector dimension
    pub fn validate_dimension(&self, expected_dimension: usize) -> Result<(), ShardexError> {
        if self.vector.len() != expected_dimension {
            return Err(ShardexError::InvalidDimension {
                expected: expected_dimension,
                actual: self.vector.len(),
            });
        }
        Ok(())
    }
}

impl SearchResult {
    /// Create a new search result with vector dimension validation
    pub fn new(
        document_id: DocumentId,
        start: u32,
        length: u32,
        vector: Vec<f32>,
        similarity_score: f32,
        expected_dimension: usize,
    ) -> Result<Self, ShardexError> {
        if vector.len() != expected_dimension {
            return Err(ShardexError::InvalidDimension {
                expected: expected_dimension,
                actual: vector.len(),
            });
        }

        Ok(Self {
            document_id,
            start,
            length,
            vector,
            similarity_score,
        })
    }

    /// Create search result from posting and similarity score
    pub fn from_posting(posting: Posting, similarity_score: f32) -> Self {
        Self {
            document_id: posting.document_id,
            start: posting.start,
            length: posting.length,
            vector: posting.vector,
            similarity_score,
        }
    }

    /// Get vector dimension
    pub fn vector_dimension(&self) -> usize {
        self.vector.len()
    }

    /// Create a memory-mapped header for this search result
    pub fn to_header(&self, vector_offset: u64) -> SearchResultHeader {
        SearchResultHeader {
            document_id: self.document_id,
            start: self.start,
            length: self.length,
            vector_len: self.vector.len() as u32,
            vector_offset,
            similarity_score: self.similarity_score,
        }
    }

    /// Validate that this result has the expected vector dimension
    pub fn validate_dimension(&self, expected_dimension: usize) -> Result<(), ShardexError> {
        if self.vector.len() != expected_dimension {
            return Err(ShardexError::InvalidDimension {
                expected: expected_dimension,
                actual: self.vector.len(),
            });
        }
        Ok(())
    }
}

impl PostingHeader {
    /// Create a zero-initialized posting header
    pub fn new_zero() -> Self {
        Self::zeroed()
    }

    /// Check if this header represents a valid posting
    pub fn is_valid(&self) -> bool {
        self.vector_len > 0 && self.length > 0
    }
}

impl SearchResultHeader {
    /// Create a zero-initialized search result header
    pub fn new_zero() -> Self {
        Self::zeroed()
    }

    /// Check if this header represents a valid search result
    pub fn is_valid(&self) -> bool {
        self.vector_len > 0 && self.length > 0
    }
}

impl IndexStats {
    /// Create new index statistics
    pub fn new() -> Self {
        Self {
            total_shards: 0,
            total_postings: 0,
            pending_operations: 0,
            memory_usage: 0,
            active_postings: 0,
            deleted_postings: 0,
            average_shard_utilization: 0.0,
            vector_dimension: 0,
            disk_usage: 0,
        }
    }

    /// Calculate shard utilization percentage
    pub fn shard_utilization_percent(&self) -> f32 {
        self.average_shard_utilization * 100.0
    }

    /// Calculate deletion ratio (deleted / total)
    pub fn deletion_ratio(&self) -> f32 {
        if self.total_postings == 0 {
            0.0
        } else {
            self.deleted_postings as f32 / self.total_postings as f32
        }
    }

    /// Check if the index has pending operations
    pub fn has_pending_operations(&self) -> bool {
        self.pending_operations > 0
    }

    /// Get memory usage in human-readable format
    pub fn memory_usage_mb(&self) -> f32 {
        self.memory_usage as f32 / (1024.0 * 1024.0)
    }

    /// Get disk usage in human-readable format
    pub fn disk_usage_mb(&self) -> f32 {
        self.disk_usage as f32 / (1024.0 * 1024.0)
    }
}

impl Default for IndexStats {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for IndexStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IndexStats {{ shards: {}, postings: {} (active: {}, deleted: {}), \
             pending: {}, memory: {:.2}MB, disk: {:.2}MB, utilization: {:.1}% }}",
            self.total_shards,
            self.total_postings,
            self.active_postings,
            self.deleted_postings,
            self.pending_operations,
            self.memory_usage_mb(),
            self.disk_usage_mb(),
            self.shard_utilization_percent()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_posting_creation() {
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];
        let posting = Posting::new(doc_id, 0, 100, vector.clone(), 3).unwrap();

        assert_eq!(posting.document_id, doc_id);
        assert_eq!(posting.start, 0);
        assert_eq!(posting.length, 100);
        assert_eq!(posting.vector, vector);
        assert_eq!(posting.vector_dimension(), 3);
    }

    #[test]
    fn test_posting_dimension_validation() {
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];

        // Should fail with wrong expected dimension
        let result = Posting::new(doc_id, 0, 100, vector.clone(), 4);
        assert!(result.is_err());

        if let Err(ShardexError::InvalidDimension { expected, actual }) = result {
            assert_eq!(expected, 4);
            assert_eq!(actual, 3);
        } else {
            panic!("Expected InvalidDimension error");
        }
    }

    #[test]
    fn test_search_result_creation() {
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];
        let result = SearchResult::new(doc_id, 0, 100, vector.clone(), 0.85, 3).unwrap();

        assert_eq!(result.document_id, doc_id);
        assert_eq!(result.start, 0);
        assert_eq!(result.length, 100);
        assert_eq!(result.vector, vector);
        assert_eq!(result.similarity_score, 0.85);
        assert_eq!(result.vector_dimension(), 3);
    }

    #[test]
    fn test_search_result_from_posting() {
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];
        let posting = Posting::new(doc_id, 10, 50, vector.clone(), 3).unwrap();
        let search_result = SearchResult::from_posting(posting.clone(), 0.75);

        assert_eq!(search_result.document_id, posting.document_id);
        assert_eq!(search_result.start, posting.start);
        assert_eq!(search_result.length, posting.length);
        assert_eq!(search_result.vector, posting.vector);
        assert_eq!(search_result.similarity_score, 0.75);
    }

    #[test]
    fn test_posting_header_creation() {
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0, 4.0];
        let posting = Posting::new(doc_id, 20, 80, vector, 4).unwrap();
        let header = posting.to_header(1024);

        assert_eq!(header.document_id, doc_id);
        assert_eq!(header.start, 20);
        assert_eq!(header.length, 80);
        assert_eq!(header.vector_len, 4);
        assert_eq!(header.vector_offset, 1024);
        assert!(header.is_valid());
    }

    #[test]
    fn test_search_result_header_creation() {
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0, 4.0];
        let result = SearchResult::new(doc_id, 20, 80, vector, 0.92, 4).unwrap();
        let header = result.to_header(2048);

        assert_eq!(header.document_id, doc_id);
        assert_eq!(header.start, 20);
        assert_eq!(header.length, 80);
        assert_eq!(header.vector_len, 4);
        assert_eq!(header.vector_offset, 2048);
        assert_eq!(header.similarity_score, 0.92);
        assert!(header.is_valid());
    }

    #[test]
    fn test_posting_header_bytemuck() {
        let header = PostingHeader {
            document_id: DocumentId::new(),
            start: 100,
            length: 200,
            vector_len: 384,
            vector_offset: 4096,
        };

        // Test Pod trait - should be able to cast to bytes
        let bytes: &[u8] = bytemuck::bytes_of(&header);
        assert_eq!(bytes.len(), std::mem::size_of::<PostingHeader>());

        // Test round-trip
        let header_restored: PostingHeader = bytemuck::pod_read_unaligned(bytes);
        assert_eq!(header, header_restored);
    }

    #[test]
    fn test_search_result_header_bytemuck() {
        let header = SearchResultHeader {
            document_id: DocumentId::new(),
            start: 100,
            length: 200,
            vector_len: 384,
            vector_offset: 4096,
            similarity_score: 0.88,
        };

        // Test Pod trait - should be able to cast to bytes
        let bytes: &[u8] = bytemuck::bytes_of(&header);
        assert_eq!(bytes.len(), std::mem::size_of::<SearchResultHeader>());

        // Test round-trip
        let header_restored: SearchResultHeader = bytemuck::pod_read_unaligned(bytes);
        assert_eq!(header, header_restored);
    }

    #[test]
    fn test_index_stats_creation() {
        let stats = IndexStats::new();
        assert_eq!(stats.total_shards, 0);
        assert_eq!(stats.total_postings, 0);
        assert_eq!(stats.pending_operations, 0);
        assert_eq!(stats.memory_usage, 0);
        assert_eq!(stats.active_postings, 0);
        assert_eq!(stats.deleted_postings, 0);
        assert_eq!(stats.average_shard_utilization, 0.0);
        assert_eq!(stats.vector_dimension, 0);
        assert_eq!(stats.disk_usage, 0);
    }

    #[test]
    fn test_index_stats_calculations() {
        let mut stats = IndexStats::new();
        stats.total_postings = 1000;
        stats.active_postings = 800;
        stats.deleted_postings = 200;
        stats.average_shard_utilization = 0.75;
        stats.memory_usage = 2048 * 1024; // 2MB
        stats.disk_usage = 5 * 1024 * 1024; // 5MB

        assert_eq!(stats.shard_utilization_percent(), 75.0);
        assert_eq!(stats.deletion_ratio(), 0.2);
        assert!(!stats.has_pending_operations());
        assert_eq!(stats.memory_usage_mb(), 2.0);
        assert_eq!(stats.disk_usage_mb(), 5.0);
    }

    #[test]
    fn test_index_stats_display() {
        let mut stats = IndexStats::new();
        stats.total_shards = 5;
        stats.total_postings = 1000;
        stats.active_postings = 950;
        stats.deleted_postings = 50;
        stats.pending_operations = 10;
        stats.memory_usage = 1024 * 1024; // 1MB
        stats.disk_usage = 2 * 1024 * 1024; // 2MB
        stats.average_shard_utilization = 0.8;

        let display_str = format!("{}", stats);
        assert!(display_str.contains("shards: 5"));
        assert!(display_str.contains("postings: 1000"));
        assert!(display_str.contains("active: 950"));
        assert!(display_str.contains("deleted: 50"));
        assert!(display_str.contains("pending: 10"));
        assert!(display_str.contains("memory: 1.00MB"));
        assert!(display_str.contains("disk: 2.00MB"));
        assert!(display_str.contains("utilization: 80.0%"));
    }

    #[test]
    fn test_header_validity() {
        // Valid headers
        let valid_posting = PostingHeader {
            document_id: DocumentId::new(),
            start: 0,
            length: 100,
            vector_len: 384,
            vector_offset: 0,
        };
        assert!(valid_posting.is_valid());

        let valid_search_result = SearchResultHeader {
            document_id: DocumentId::new(),
            start: 0,
            length: 100,
            vector_len: 384,
            vector_offset: 0,
            similarity_score: 0.5,
        };
        assert!(valid_search_result.is_valid());

        // Invalid headers (zero length or vector_len)
        let invalid_posting = PostingHeader {
            document_id: DocumentId::new(),
            start: 0,
            length: 0, // Invalid
            vector_len: 384,
            vector_offset: 0,
        };
        assert!(!invalid_posting.is_valid());

        let invalid_search_result = SearchResultHeader {
            document_id: DocumentId::new(),
            start: 0,
            length: 100,
            vector_len: 0, // Invalid
            vector_offset: 0,
            similarity_score: 0.5,
        };
        assert!(!invalid_search_result.is_valid());
    }

    #[test]
    fn test_zero_initialized_headers() {
        let zero_posting = PostingHeader::new_zero();
        let zero_search_result = SearchResultHeader::new_zero();

        assert!(!zero_posting.is_valid()); // Should be invalid when zero
        assert!(!zero_search_result.is_valid()); // Should be invalid when zero

        // Test bytemuck zero initialization
        let zero_posting_bytemuck: PostingHeader = PostingHeader::zeroed();
        let zero_search_result_bytemuck: SearchResultHeader = SearchResultHeader::zeroed();

        assert_eq!(zero_posting_bytemuck.start, 0);
        assert_eq!(zero_posting_bytemuck.length, 0);
        assert_eq!(zero_posting_bytemuck.vector_len, 0);
        assert_eq!(zero_posting_bytemuck.vector_offset, 0);

        assert_eq!(zero_search_result_bytemuck.start, 0);
        assert_eq!(zero_search_result_bytemuck.length, 0);
        assert_eq!(zero_search_result_bytemuck.vector_len, 0);
        assert_eq!(zero_search_result_bytemuck.vector_offset, 0);
        assert_eq!(zero_search_result_bytemuck.similarity_score, 0.0);
    }

    #[test]
    fn test_memory_layout() {
        use std::mem;

        // Verify struct sizes are reasonable (with padding for alignment)
        // PostingHeader contains: DocumentId(16) + 3*u32(12) + u64(8) = 36 bytes minimum
        let posting_size = mem::size_of::<PostingHeader>();
        let search_result_size = mem::size_of::<SearchResultHeader>();

        // Should be at least the sum of field sizes
        assert!(
            posting_size >= 36,
            "PostingHeader size {} should be at least 36 bytes",
            posting_size
        );
        assert!(
            posting_size % mem::align_of::<PostingHeader>() == 0,
            "PostingHeader should be properly aligned"
        );

        // SearchResultHeader should be at least as big as PostingHeader (includes f32 similarity_score)
        assert!(
            search_result_size >= posting_size,
            "SearchResultHeader size {} should be at least as large as PostingHeader size {}",
            search_result_size,
            posting_size
        );
        assert!(
            search_result_size % mem::align_of::<SearchResultHeader>() == 0,
            "SearchResultHeader should be properly aligned"
        );

        // Verify alignment
        assert!(mem::align_of::<PostingHeader>() >= 8); // Should align to at least u64
        assert!(mem::align_of::<SearchResultHeader>() >= 8);
    }

    #[test]
    fn test_dimension_validation() {
        let doc_id = DocumentId::new();

        // Test posting dimension validation
        let vector = vec![1.0, 2.0, 3.0];
        let posting = Posting::new(doc_id, 0, 100, vector, 3).unwrap();

        assert!(posting.validate_dimension(3).is_ok());
        assert!(posting.validate_dimension(4).is_err());

        // Test search result dimension validation
        let vector = vec![1.0, 2.0, 3.0, 4.0];
        let search_result = SearchResult::new(doc_id, 0, 100, vector, 0.8, 4).unwrap();

        assert!(search_result.validate_dimension(4).is_ok());
        assert!(search_result.validate_dimension(3).is_err());
    }

    #[test]
    fn test_serialization() {
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];

        // Test Posting serialization
        let posting = Posting::new(doc_id, 10, 50, vector.clone(), 3).unwrap();
        let posting_json = serde_json::to_string(&posting).unwrap();
        let posting_restored: Posting = serde_json::from_str(&posting_json).unwrap();
        assert_eq!(posting, posting_restored);

        // Test SearchResult serialization
        let search_result = SearchResult::new(doc_id, 10, 50, vector, 0.9, 3).unwrap();
        let result_json = serde_json::to_string(&search_result).unwrap();
        let result_restored: SearchResult = serde_json::from_str(&result_json).unwrap();
        assert_eq!(search_result, result_restored);

        // Test IndexStats serialization
        let mut stats = IndexStats::new();
        stats.total_shards = 10;
        stats.total_postings = 1000;
        let stats_json = serde_json::to_string(&stats).unwrap();
        let stats_restored: IndexStats = serde_json::from_str(&stats_json).unwrap();
        assert_eq!(stats, stats_restored);
    }
}

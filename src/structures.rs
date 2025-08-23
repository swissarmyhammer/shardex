//! Core data structures for Shardex
//!
//! This module provides the fundamental data structures used throughout Shardex:
//! - [`Posting`]: Represents a document posting with its vector embedding
//! - [`SearchResult`]: Search result with similarity scoring  
//! - [`IndexStats`]: Index statistics for monitoring and observability
//!
//! All structures are designed for memory mapping compatibility using bytemuck traits.
//!
//! # Usage Examples
//!
//! ## Creating and Using Postings
//!
//! ```rust
//! use shardex::structures::Posting;
//! use shardex::identifiers::DocumentId;
//!
//! // Create a new posting with a 3-dimensional vector
//! let doc_id = DocumentId::new();
//! let vector = vec![0.1, 0.2, 0.3];
//!
//! let posting = Posting::new(doc_id, 100, 50, vector, 3)?;
//!
//! println!("Document: {}, Position: {}-{}",
//!          posting.document_id,
//!          posting.start,
//!          posting.start + posting.length);
//! # Ok::<(), shardex::error::ShardexError>(())
//! ```
//!
//! ## Working with Search Results
//!
//! ```rust
//! use shardex::structures::SearchResult;
//! use shardex::identifiers::DocumentId;
//!
//! // Create search results with similarity scores
//! let doc_id = DocumentId::new();
//! let vector = vec![0.5, 0.6, 0.7]; // Query embedding
//!
//! let result = SearchResult::new(doc_id, 200, 100, vector, 0.85, 3)?;
//!
//! // Results can be sorted by similarity score
//! let mut results = vec![result];
//! results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap());
//! # Ok::<(), shardex::error::ShardexError>(())
//! ```
//!
//! ## Memory Mapping with Headers
//!
//! For high-performance memory-mapped access, use the header structures:
//!
//! ```rust
//! use shardex::structures::{PostingHeader, SearchResultHeader};
//! use shardex::identifiers::DocumentId;
//!
//! // Headers are memory-mappable and reference vector data externally
//! let header = PostingHeader {
//!     document_id: DocumentId::new(),
//!     start: 100,
//!     length: 50,
//!     vector_offset: 1024,  // Offset in the mapped file
//!     vector_len: 128,      // Number of f32 elements
//! };
//!
//! // Headers can be safely cast from raw memory using bytemuck
//! let bytes = bytemuck::bytes_of(&header);
//! let restored = bytemuck::from_bytes::<PostingHeader>(bytes);
//! ```
//!
//! ## Index Statistics and Monitoring
//!
//! ```rust
//! use shardex::structures::IndexStats;
//!
//! let stats = IndexStats {
//!     total_shards: 5,
//!     total_postings: 50_000,
//!     pending_operations: 10,
//!     memory_usage: 26_214_400,     // ~25MB
//!     active_postings: 45_000,
//!     deleted_postings: 5_000,
//!     average_shard_utilization: 0.8,
//!     vector_dimension: 128,
//!     disk_usage: 52_428_800,       // ~50MB on disk
//! };
//!
//! // Display provides human-readable formatting
//! println!("{}", stats);
//! // Output shows comprehensive statistics
//! ```
//!
//! ## Performance Characteristics
//!
//! ### Runtime Structures (Posting, SearchResult)
//! - **Memory overhead**: `Vec<f32>` has dynamic allocation overhead (~24 bytes + vector data)
//! - **Access pattern**: Fast random access to vector elements
//! - **Use case**: Building indices, query processing, dynamic operations
//! - **Serialization**: JSON via serde, binary via bytemuck for headers
//!
//! ### Memory-Mapped Headers (PostingHeader, SearchResultHeader)  
//! - **Memory overhead**: Fixed 32 bytes per header (no heap allocations)
//! - **Access pattern**: Requires separate vector data lookup by offset
//! - **Use case**: Large-scale batch processing, persistent storage access
//! - **Performance**: Zero-copy deserialization, page cache friendly
//!
//! Choose runtime structures for dynamic operations and headers for bulk processing
//! of persistent data where memory efficiency is critical.

use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

/// A posting represents a document segment with its vector embedding
///
/// Postings are stored in shards and support memory-mapped access for high performance.
/// The vector field requires special handling for memory mapping since `Vec<f32>` is not
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
    /// 50th percentile search latency
    pub search_latency_p50: std::time::Duration,
    /// 95th percentile search latency
    pub search_latency_p95: std::time::Duration,
    /// 99th percentile search latency
    pub search_latency_p99: std::time::Duration,
    /// Write operations per second
    pub write_throughput: f64,
    /// Bloom filter hit rate (0.0 to 1.0)
    pub bloom_filter_hit_rate: f64,
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
            return Err(ShardexError::invalid_dimension_with_context(
                expected_dimension,
                self.vector.len(),
                "posting_vector",
            ));
        }
        Ok(())
    }
}

impl SearchResult {
    /// Create a new search result with vector dimension and similarity score validation
    pub fn new(
        document_id: DocumentId,
        start: u32,
        length: u32,
        vector: Vec<f32>,
        similarity_score: f32,
        expected_dimension: usize,
    ) -> Result<Self, ShardexError> {
        if vector.len() != expected_dimension {
            return Err(ShardexError::invalid_dimension_with_context(
                expected_dimension,
                vector.len(),
                "search_result",
            ));
        }

        if !(0.0..=1.0).contains(&similarity_score) || similarity_score.is_nan() {
            return Err(ShardexError::invalid_similarity_score_with_suggestion(
                similarity_score,
            ));
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
    pub fn from_posting(posting: Posting, similarity_score: f32) -> Result<Self, ShardexError> {
        if !(0.0..=1.0).contains(&similarity_score) || similarity_score.is_nan() {
            return Err(ShardexError::InvalidSimilarityScore {
                score: similarity_score,
            });
        }

        Ok(Self {
            document_id: posting.document_id,
            start: posting.start,
            length: posting.length,
            vector: posting.vector,
            similarity_score,
        })
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
            return Err(ShardexError::invalid_dimension_with_context(
                expected_dimension,
                self.vector.len(),
                "search_result",
            ));
        }
        Ok(())
    }

    /// Check if this result is highly similar (similarity >= 0.8)
    pub fn is_highly_similar(&self) -> bool {
        self.similarity_score >= 0.8
    }

    /// Check if this result is moderately similar (similarity >= 0.5)
    pub fn is_moderately_similar(&self) -> bool {
        self.similarity_score >= 0.5
    }

    /// Check if this result is weakly similar (similarity >= 0.3)
    pub fn is_weakly_similar(&self) -> bool {
        self.similarity_score >= 0.3
    }

    /// Check if this result has higher similarity than another result
    pub fn is_more_similar_than(&self, other: &SearchResult) -> bool {
        self.similarity_score > other.similarity_score
    }

    /// Check if this result has similarity within a given threshold of another result
    pub fn is_similar_to(&self, other: &SearchResult, threshold: f32) -> bool {
        (self.similarity_score - other.similarity_score).abs() <= threshold
    }

    /// Get the similarity tier for this result
    /// Returns "high" (>=0.8), "moderate" (>=0.5), "weak" (>=0.3), or "low" (<0.3)
    pub fn similarity_tier(&self) -> &'static str {
        if self.similarity_score >= 0.8 {
            "high"
        } else if self.similarity_score >= 0.5 {
            "moderate"
        } else if self.similarity_score >= 0.3 {
            "weak"
        } else {
            "low"
        }
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
            search_latency_p50: std::time::Duration::ZERO,
            search_latency_p95: std::time::Duration::ZERO,
            search_latency_p99: std::time::Duration::ZERO,
            write_throughput: 0.0,
            bloom_filter_hit_rate: 0.0,
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

    /// Get search latency P50 in milliseconds
    pub fn search_latency_p50_ms(&self) -> u64 {
        self.search_latency_p50.as_millis() as u64
    }

    /// Get search latency P95 in milliseconds
    pub fn search_latency_p95_ms(&self) -> u64 {
        self.search_latency_p95.as_millis() as u64
    }

    /// Get search latency P99 in milliseconds
    pub fn search_latency_p99_ms(&self) -> u64 {
        self.search_latency_p99.as_millis() as u64
    }

    /// Get write throughput operations per second
    pub fn write_ops_per_second(&self) -> f64 {
        self.write_throughput
    }

    /// Get bloom filter hit rate as percentage
    pub fn bloom_filter_hit_rate_percent(&self) -> f32 {
        (self.bloom_filter_hit_rate * 100.0) as f32
    }

    /// Check if performance metrics indicate healthy operation
    pub fn is_performance_healthy(&self) -> bool {
        // P95 latency should be under 1 second
        let latency_healthy = self.search_latency_p95 < std::time::Duration::from_millis(1000);

        // For empty indexes (no operations), consider bloom filter and throughput as healthy
        let bloom_healthy = self.bloom_filter_hit_rate == 0.0 || self.bloom_filter_hit_rate > 0.7;
        let throughput_healthy = self.write_throughput == 0.0 || self.write_throughput > 10.0;

        latency_healthy && bloom_healthy && throughput_healthy
    }

    /// Builder method to set total shards
    pub fn with_total_shards(mut self, total_shards: usize) -> Self {
        self.total_shards = total_shards;
        self
    }

    /// Builder method to set total postings
    pub fn with_total_postings(mut self, total_postings: usize) -> Self {
        self.total_postings = total_postings;
        self
    }

    /// Builder method to set pending operations
    pub fn with_pending_operations(mut self, pending_operations: usize) -> Self {
        self.pending_operations = pending_operations;
        self
    }

    /// Builder method to set memory usage
    pub fn with_memory_usage(mut self, memory_usage: usize) -> Self {
        self.memory_usage = memory_usage;
        self
    }

    /// Builder method to set active postings
    pub fn with_active_postings(mut self, active_postings: usize) -> Self {
        self.active_postings = active_postings;
        self
    }

    /// Builder method to set deleted postings
    pub fn with_deleted_postings(mut self, deleted_postings: usize) -> Self {
        self.deleted_postings = deleted_postings;
        self
    }

    /// Builder method to set average shard utilization
    pub fn with_average_shard_utilization(mut self, average_shard_utilization: f32) -> Self {
        self.average_shard_utilization = average_shard_utilization;
        self
    }

    /// Builder method to set vector dimension
    pub fn with_vector_dimension(mut self, vector_dimension: usize) -> Self {
        self.vector_dimension = vector_dimension;
        self
    }

    /// Builder method to set disk usage
    pub fn with_disk_usage(mut self, disk_usage: usize) -> Self {
        self.disk_usage = disk_usage;
        self
    }

    /// Builder method to set search latency percentiles
    pub fn with_search_latency_p50(mut self, latency: std::time::Duration) -> Self {
        self.search_latency_p50 = latency;
        self
    }

    /// Builder method to set search latency p95
    pub fn with_search_latency_p95(mut self, latency: std::time::Duration) -> Self {
        self.search_latency_p95 = latency;
        self
    }

    /// Builder method to set search latency p99
    pub fn with_search_latency_p99(mut self, latency: std::time::Duration) -> Self {
        self.search_latency_p99 = latency;
        self
    }

    /// Builder method to set write throughput
    pub fn with_write_throughput(mut self, throughput: f64) -> Self {
        self.write_throughput = throughput;
        self
    }

    /// Builder method to set bloom filter hit rate
    pub fn with_bloom_filter_hit_rate(mut self, hit_rate: f64) -> Self {
        self.bloom_filter_hit_rate = hit_rate;
        self
    }

    /// Convenience builder for creating test stats with common values
    pub fn for_test() -> Self {
        Self::new()
            .with_total_shards(5)
            .with_total_postings(1000)
            .with_active_postings(900)
            .with_deleted_postings(100)
            .with_vector_dimension(128)
            .with_memory_usage(1024 * 1024) // 1MB
            .with_disk_usage(2 * 1024 * 1024) // 2MB
            .with_average_shard_utilization(0.75)
            .with_pending_operations(10)
            .with_search_latency_p50(std::time::Duration::from_millis(50))
            .with_search_latency_p95(std::time::Duration::from_millis(150))
            .with_search_latency_p99(std::time::Duration::from_millis(300))
            .with_write_throughput(100.0)
            .with_bloom_filter_hit_rate(0.85)
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
             pending: {}, memory: {:.2}MB, disk: {:.2}MB, utilization: {:.1}%, \
             latency: p50={}ms/p95={}ms/p99={}ms, write_throughput: {:.1}ops/s, \
             bloom_hit_rate: {:.1}% }}",
            self.total_shards,
            self.total_postings,
            self.active_postings,
            self.deleted_postings,
            self.pending_operations,
            self.memory_usage_mb(),
            self.disk_usage_mb(),
            self.shard_utilization_percent(),
            self.search_latency_p50.as_millis(),
            self.search_latency_p95.as_millis(),
            self.search_latency_p99.as_millis(),
            self.write_throughput,
            self.bloom_filter_hit_rate * 100.0
        )
    }
}

/// Statistics and performance metrics for flush operations
///
/// FlushStats provides detailed timing information and metrics for flush operations,
/// enabling performance monitoring and optimization of the durability guarantees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlushStats {
    /// Time spent flushing WAL operations
    pub wal_flush_duration: std::time::Duration,
    /// Time spent applying operations to shards
    pub shard_apply_duration: std::time::Duration,
    /// Time spent synchronizing shard data to disk
    pub shard_sync_duration: std::time::Duration,
    /// Time spent on consistency validation
    pub validation_duration: std::time::Duration,
    /// Total flush operation duration
    pub total_duration: std::time::Duration,
    /// Number of shards synchronized to disk
    pub shards_synced: usize,
    /// Number of operations applied during flush
    pub operations_applied: usize,
    /// Number of bytes synchronized to disk
    pub bytes_synced: u64,
}

impl FlushStats {
    /// Create new flush stats with zero values
    pub fn new() -> Self {
        Self {
            wal_flush_duration: std::time::Duration::ZERO,
            shard_apply_duration: std::time::Duration::ZERO,
            shard_sync_duration: std::time::Duration::ZERO,
            validation_duration: std::time::Duration::ZERO,
            total_duration: std::time::Duration::ZERO,
            shards_synced: 0,
            operations_applied: 0,
            bytes_synced: 0,
        }
    }

    /// Get total flush duration in milliseconds
    pub fn total_duration_ms(&self) -> u64 {
        self.total_duration.as_millis() as u64
    }

    /// Get WAL flush duration in milliseconds
    pub fn wal_flush_duration_ms(&self) -> u64 {
        self.wal_flush_duration.as_millis() as u64
    }

    /// Get shard synchronization duration in milliseconds
    pub fn shard_sync_duration_ms(&self) -> u64 {
        self.shard_sync_duration.as_millis() as u64
    }

    /// Get validation duration in milliseconds
    pub fn validation_duration_ms(&self) -> u64 {
        self.validation_duration.as_millis() as u64
    }

    /// Calculate sync throughput in MB/s
    pub fn sync_throughput_mbps(&self) -> f64 {
        if self.shard_sync_duration.is_zero() {
            0.0
        } else {
            let mb_synced = self.bytes_synced as f64 / (1024.0 * 1024.0);
            let seconds = self.shard_sync_duration.as_secs_f64();
            mb_synced / seconds
        }
    }

    /// Calculate operations per second applied during flush
    pub fn operations_per_second(&self) -> f64 {
        if self.shard_apply_duration.is_zero() {
            0.0
        } else {
            let seconds = self.shard_apply_duration.as_secs_f64();
            self.operations_applied as f64 / seconds
        }
    }

    /// Check if this was a fast flush (< 100ms total)
    pub fn is_fast_flush(&self) -> bool {
        self.total_duration < std::time::Duration::from_millis(100)
    }

    /// Check if this was a slow flush (> 1000ms total)
    pub fn is_slow_flush(&self) -> bool {
        self.total_duration > std::time::Duration::from_millis(1000)
    }

    /// Get the most time-consuming phase of the flush
    pub fn slowest_phase(&self) -> &'static str {
        let durations = [
            ("wal_flush", self.wal_flush_duration),
            ("shard_apply", self.shard_apply_duration),
            ("shard_sync", self.shard_sync_duration),
            ("validation", self.validation_duration),
        ];

        durations
            .iter()
            .max_by_key(|(_, duration)| *duration)
            .map(|(name, _)| *name)
            .unwrap_or("unknown")
    }
}

impl Default for FlushStats {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for FlushStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FlushStats {{ total: {}ms, wal: {}ms, apply: {}ms, sync: {}ms, validation: {}ms, \
             shards: {}, ops: {}, throughput: {:.2}MB/s }}",
            self.total_duration_ms(),
            self.wal_flush_duration_ms(),
            self.shard_apply_duration.as_millis(),
            self.shard_sync_duration_ms(),
            self.validation_duration_ms(),
            self.shards_synced,
            self.operations_applied,
            self.sync_throughput_mbps()
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
        let search_result = SearchResult::from_posting(posting.clone(), 0.75).unwrap();

        assert_eq!(search_result.document_id, posting.document_id);
        assert_eq!(search_result.start, posting.start);
        assert_eq!(search_result.length, posting.length);
        assert_eq!(search_result.vector, posting.vector);
        assert_eq!(search_result.similarity_score, 0.75);
    }

    #[test]
    fn test_search_result_similarity_score_validation() {
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];

        // Valid similarity scores should work
        assert!(SearchResult::new(doc_id, 0, 10, vector.clone(), 0.0, 3).is_ok());
        assert!(SearchResult::new(doc_id, 0, 10, vector.clone(), 0.5, 3).is_ok());
        assert!(SearchResult::new(doc_id, 0, 10, vector.clone(), 1.0, 3).is_ok());

        // Invalid similarity scores should fail
        let result = SearchResult::new(doc_id, 0, 10, vector.clone(), -0.1, 3);
        assert!(
            matches!(result.unwrap_err(), ShardexError::InvalidInput { field, reason, .. } if field == "similarity_score" && reason.contains("Negative similarity scores"))
        );

        let result = SearchResult::new(doc_id, 0, 10, vector.clone(), 1.5, 3);
        assert!(
            matches!(result.unwrap_err(), ShardexError::InvalidInput { field, reason, .. } if field == "similarity_score" && reason.contains("Similarity score too large"))
        );

        let result = SearchResult::new(doc_id, 0, 10, vector.clone(), f32::NAN, 3);
        assert!(matches!(
            result.unwrap_err(),
            ShardexError::InvalidInput { field, reason, .. } if field == "similarity_score" && reason.contains("NaN values are not allowed")
        ));

        // Test from_posting validation
        let posting = Posting::new(doc_id, 0, 10, vector.clone(), 3).unwrap();
        assert!(SearchResult::from_posting(posting.clone(), 0.8).is_ok());

        let result = SearchResult::from_posting(posting.clone(), 2.0);
        assert!(
            matches!(result.unwrap_err(), ShardexError::InvalidSimilarityScore { score } if score == 2.0)
        );

        let result = SearchResult::from_posting(posting, -1.0);
        assert!(
            matches!(result.unwrap_err(), ShardexError::InvalidSimilarityScore { score } if score == -1.0)
        );
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
    fn test_search_result_similarity_convenience_methods() {
        let doc_id = DocumentId::new();
        let vector = vec![0.1, 0.2, 0.3];

        // Test similarity tier methods
        let high_result = SearchResult::new(doc_id, 0, 10, vector.clone(), 0.85, 3).unwrap();
        assert!(high_result.is_highly_similar());
        assert!(high_result.is_moderately_similar());
        assert!(high_result.is_weakly_similar());
        assert_eq!(high_result.similarity_tier(), "high");

        let moderate_result = SearchResult::new(doc_id, 10, 10, vector.clone(), 0.65, 3).unwrap();
        assert!(!moderate_result.is_highly_similar());
        assert!(moderate_result.is_moderately_similar());
        assert!(moderate_result.is_weakly_similar());
        assert_eq!(moderate_result.similarity_tier(), "moderate");

        let weak_result = SearchResult::new(doc_id, 20, 10, vector.clone(), 0.45, 3).unwrap();
        assert!(!weak_result.is_highly_similar());
        assert!(!weak_result.is_moderately_similar());
        assert!(weak_result.is_weakly_similar());
        assert_eq!(weak_result.similarity_tier(), "weak");

        let low_result = SearchResult::new(doc_id, 30, 10, vector, 0.15, 3).unwrap();
        assert!(!low_result.is_highly_similar());
        assert!(!low_result.is_moderately_similar());
        assert!(!low_result.is_weakly_similar());
        assert_eq!(low_result.similarity_tier(), "low");

        // Test comparison methods
        assert!(high_result.is_more_similar_than(&moderate_result));
        assert!(moderate_result.is_more_similar_than(&weak_result));
        assert!(weak_result.is_more_similar_than(&low_result));

        // Test similarity threshold
        assert!(high_result.is_similar_to(&high_result, 0.0)); // Same result
        assert!(!high_result.is_similar_to(&low_result, 0.1)); // Too different
        assert!(high_result.is_similar_to(&moderate_result, 0.25)); // Within threshold
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
    fn test_index_stats_builder_methods() {
        let stats = IndexStats::new()
            .with_total_shards(10)
            .with_total_postings(5000)
            .with_active_postings(4500)
            .with_deleted_postings(500)
            .with_vector_dimension(256)
            .with_memory_usage(5 * 1024 * 1024) // 5MB
            .with_disk_usage(10 * 1024 * 1024) // 10MB
            .with_average_shard_utilization(0.9)
            .with_pending_operations(25);

        assert_eq!(stats.total_shards, 10);
        assert_eq!(stats.total_postings, 5000);
        assert_eq!(stats.active_postings, 4500);
        assert_eq!(stats.deleted_postings, 500);
        assert_eq!(stats.vector_dimension, 256);
        assert_eq!(stats.memory_usage, 5 * 1024 * 1024);
        assert_eq!(stats.disk_usage, 10 * 1024 * 1024);
        assert_eq!(stats.average_shard_utilization, 0.9);
        assert_eq!(stats.pending_operations, 25);
    }

    #[test]
    fn test_index_stats_for_test_builder() {
        let stats = IndexStats::for_test();

        assert_eq!(stats.total_shards, 5);
        assert_eq!(stats.total_postings, 1000);
        assert_eq!(stats.active_postings, 900);
        assert_eq!(stats.deleted_postings, 100);
        assert_eq!(stats.vector_dimension, 128);
        assert_eq!(stats.memory_usage, 1024 * 1024);
        assert_eq!(stats.disk_usage, 2 * 1024 * 1024);
        assert_eq!(stats.average_shard_utilization, 0.75);
        assert_eq!(stats.pending_operations, 10);

        // Test that calculations work with the test data
        assert_eq!(stats.shard_utilization_percent(), 75.0);
        assert_eq!(stats.deletion_ratio(), 0.1);
        assert!(stats.has_pending_operations());
        assert_eq!(stats.memory_usage_mb(), 1.0);
        assert_eq!(stats.disk_usage_mb(), 2.0);
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

//! Result deduplication for search operations
//!
//! This module provides configurable duplicate elimination for search results and shard operations.
//! It supports different deduplication policies to balance performance with result uniqueness requirements.
//!
//! # Key Features
//!
//! - **Configurable Policies**: Different strategies for detecting duplicates
//! - **Memory Efficient**: Hash-based deduplication with controlled memory usage
//! - **Performance Optimized**: Fast hashing with `rustc-hash` for better performance
//! - **Statistics Tracking**: Detailed metrics on duplicate detection effectiveness
//!
//! # Usage Examples
//!
//! ## Basic Deduplication
//!
//! ```rust
//! use shardex::deduplication::{ResultDeduplicator, DeduplicationPolicy};
//! use shardex::structures::SearchResult;
//! use shardex::identifiers::DocumentId;
//!
//! # fn example() -> Result<(), shardex::error::ShardexError> {
//! let mut deduplicator = ResultDeduplicator::new(DeduplicationPolicy::ByDocumentAndPosition);
//!
//! let mut results = vec![
//!     SearchResult::new(DocumentId::new(), 0, 100, vec![0.1, 0.2], 0.9, 2)?,
//!     SearchResult::new(DocumentId::new(), 0, 100, vec![0.1, 0.2], 0.8, 2)?, // Duplicate position
//! ];
//!
//! let deduplicated = deduplicator.deduplicate(results);
//! assert_eq!(deduplicated.len(), 1); // One duplicate removed
//! # Ok(())
//! # }
//! ```
//!
//! ## Policy Comparison
//!
//! ```rust
//! use shardex::deduplication::{ResultDeduplicator, DeduplicationPolicy};
//!
//! // No deduplication - fastest performance
//! let none_dedup = ResultDeduplicator::new(DeduplicationPolicy::None);
//!
//! // Document-level deduplication - one result per document
//! let doc_dedup = ResultDeduplicator::new(DeduplicationPolicy::ByDocumentId);
//!
//! // Position-based deduplication - unique (document_id, start) pairs
//! let pos_dedup = ResultDeduplicator::new(DeduplicationPolicy::ByDocumentAndPosition);
//!
//! // Exact deduplication - considers all fields
//! let exact_dedup = ResultDeduplicator::new(DeduplicationPolicy::Exact);
//! ```

use crate::structures::SearchResult;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Deduplication policy for search results
///
/// Different policies provide trade-offs between performance and result uniqueness:
/// - `None`: Fastest, no deduplication
/// - `ByDocumentId`: Moderate performance, one result per document  
/// - `ByDocumentAndPosition`: Good balance, current default behavior
/// - `Exact`: Slowest, complete deduplication of identical results
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeduplicationPolicy {
    /// No deduplication - return all results
    /// 
    /// Use when performance is critical and duplicates are acceptable
    None,

    /// Deduplicate by document ID only
    /// 
    /// Only one result per document will be returned, typically the highest scoring
    ByDocumentId,

    /// Deduplicate by document ID and start position
    /// 
    /// This is the current default behavior - allows multiple results per document
    /// but prevents exact position duplicates
    ByDocumentAndPosition,

    /// Exact deduplication considering all fields
    /// 
    /// Most thorough but slowest - considers document_id, start, length, and vector similarity
    Exact,
}

impl Default for DeduplicationPolicy {
    fn default() -> Self {
        // Maintain backward compatibility with existing behavior
        Self::ByDocumentAndPosition
    }
}

impl DeduplicationPolicy {
    /// Get a human-readable description of this policy
    pub fn description(&self) -> &'static str {
        match self {
            Self::None => "No deduplication",
            Self::ByDocumentId => "Deduplicate by document ID",
            Self::ByDocumentAndPosition => "Deduplicate by document ID and position",
            Self::Exact => "Exact deduplication using all fields",
        }
    }

    /// Get the relative performance cost of this policy (1.0 = baseline)
    pub fn performance_cost(&self) -> f32 {
        match self {
            Self::None => 1.0,
            Self::ByDocumentId => 1.1,
            Self::ByDocumentAndPosition => 1.2,
            Self::Exact => 1.5,
        }
    }
}

/// Statistics about deduplication operations
#[derive(Debug, Clone)]
pub struct DeduplicationStats {
    /// Total number of results processed
    pub total_processed: usize,
    /// Number of duplicates found and removed
    pub duplicates_removed: usize,
    /// Number of unique results returned
    pub unique_results: usize,
    /// Deduplication efficiency (1.0 - duplicates/total)
    pub efficiency: f32,
}

impl Default for DeduplicationStats {
    fn default() -> Self {
        Self {
            total_processed: 0,
            duplicates_removed: 0,
            unique_results: 0,
            efficiency: 1.0, // Start with perfect efficiency
        }
    }
}

impl DeduplicationStats {
    /// Create new empty statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate efficiency ratio
    fn calculate_efficiency(&mut self) {
        if self.total_processed > 0 {
            self.efficiency = 1.0 - (self.duplicates_removed as f32 / self.total_processed as f32);
        } else {
            self.efficiency = 1.0;
        }
    }

    /// Record results of a deduplication operation
    pub fn record(&mut self, processed: usize, unique: usize) {
        self.total_processed += processed;
        self.unique_results += unique;
        self.duplicates_removed += processed - unique;
        self.calculate_efficiency();
    }
}

/// Result deduplicator with configurable policies
///
/// Provides efficient duplicate elimination for search results using various strategies.
/// Maintains internal state to track seen results and provides statistics.
pub struct ResultDeduplicator {
    policy: DeduplicationPolicy,
    seen_documents: FxHashSet<u128>,
    seen_postings: FxHashSet<(u128, u32)>,
    seen_exact: FxHashSet<u64>,
    stats: DeduplicationStats,
}

impl ResultDeduplicator {
    /// Create a new deduplicator with the specified policy
    pub fn new(policy: DeduplicationPolicy) -> Self {
        Self {
            policy,
            seen_documents: FxHashSet::default(),
            seen_postings: FxHashSet::default(),
            seen_exact: FxHashSet::default(),
            stats: DeduplicationStats::new(),
        }
    }

    /// Create a deduplicator with default policy (ByDocumentAndPosition)
    pub fn default() -> Self {
        Self::new(DeduplicationPolicy::default())
    }

    /// Get the current deduplication policy
    pub fn policy(&self) -> DeduplicationPolicy {
        self.policy
    }

    /// Get current deduplication statistics
    pub fn stats(&self) -> &DeduplicationStats {
        &self.stats
    }

    /// Reset internal state and statistics
    pub fn reset(&mut self) {
        self.seen_documents.clear();
        self.seen_postings.clear();
        self.seen_exact.clear();
        self.stats = DeduplicationStats::new();
    }

    /// Deduplicate a vector of search results according to the policy
    ///
    /// # Arguments
    /// * `results` - Vector of search results to deduplicate
    ///
    /// # Returns
    /// Vector of deduplicated results, typically sorted by similarity score
    pub fn deduplicate(&mut self, results: Vec<SearchResult>) -> Vec<SearchResult> {
        let original_count = results.len();
        
        let unique_results = match self.policy {
            DeduplicationPolicy::None => results,
            DeduplicationPolicy::ByDocumentId => self.deduplicate_by_document_id(results),
            DeduplicationPolicy::ByDocumentAndPosition => self.deduplicate_by_position(results),
            DeduplicationPolicy::Exact => self.deduplicate_exact(results),
        };

        let unique_count = unique_results.len();
        self.stats.record(original_count, unique_count);

        unique_results
    }

    /// Deduplicate keeping only one result per document (highest scoring)
    fn deduplicate_by_document_id(&mut self, results: Vec<SearchResult>) -> Vec<SearchResult> {
        let mut document_best: std::collections::HashMap<u128, SearchResult> = std::collections::HashMap::new();

        for result in results {
            let doc_id = result.document_id.raw();
            
            match document_best.get(&doc_id) {
                None => {
                    document_best.insert(doc_id, result);
                }
                Some(existing) if result.similarity_score > existing.similarity_score => {
                    document_best.insert(doc_id, result);
                }
                Some(_) => {
                    // Keep existing result, skip this one
                }
            }
        }

        let mut unique_results: Vec<SearchResult> = document_best.into_values().collect();
        
        // Sort by similarity score (descending)
        unique_results.sort_by(|a, b| {
            b.similarity_score
                .partial_cmp(&a.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        unique_results
    }

    /// Deduplicate by document ID and start position  
    fn deduplicate_by_position(&mut self, results: Vec<SearchResult>) -> Vec<SearchResult> {
        let mut unique_results = Vec::new();

        for result in results {
            let key = (result.document_id.raw(), result.start);
            
            if !self.seen_postings.contains(&key) {
                self.seen_postings.insert(key);
                unique_results.push(result);
            }
        }

        // Sort by similarity score (descending)
        unique_results.sort_by(|a, b| {
            b.similarity_score
                .partial_cmp(&a.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        unique_results
    }

    /// Exact deduplication using all fields
    fn deduplicate_exact(&mut self, results: Vec<SearchResult>) -> Vec<SearchResult> {
        let mut unique_results = Vec::new();

        for result in results {
            let hash = self.hash_search_result(&result);
            
            if !self.seen_exact.contains(&hash) {
                self.seen_exact.insert(hash);
                unique_results.push(result);
            }
        }

        // Sort by similarity score (descending)
        unique_results.sort_by(|a, b| {
            b.similarity_score
                .partial_cmp(&a.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        unique_results
    }

    /// Hash a search result for exact deduplication
    /// 
    /// Uses all fields except similarity_score for deduplication.
    /// Vector similarity is computed to handle floating point precision issues.
    fn hash_search_result(&self, result: &SearchResult) -> u64 {
        let mut hasher = DefaultHasher::new();
        
        // Hash core identifying fields
        result.document_id.raw().hash(&mut hasher);
        result.start.hash(&mut hasher);
        result.length.hash(&mut hasher);
        
        // Hash vector by quantizing to handle floating point precision
        // This prevents minor precision differences from being treated as different results
        for &val in &result.vector {
            // Quantize to 6 decimal places to handle floating point precision
            let quantized = (val * 1_000_000.0).round() as i64;
            quantized.hash(&mut hasher);
        }
        
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifiers::DocumentId;

    fn create_test_result(doc_id: DocumentId, start: u32, length: u32, vector: Vec<f32>, score: f32) -> SearchResult {
        let vector_len = vector.len();
        SearchResult::new(doc_id, start, length, vector, score, vector_len).unwrap()
    }

    #[test]
    fn test_deduplication_policy_default() {
        assert_eq!(DeduplicationPolicy::default(), DeduplicationPolicy::ByDocumentAndPosition);
    }

    #[test]
    fn test_deduplication_policy_descriptions() {
        assert_eq!(DeduplicationPolicy::None.description(), "No deduplication");
        assert_eq!(DeduplicationPolicy::ByDocumentId.description(), "Deduplicate by document ID");
        assert_eq!(DeduplicationPolicy::ByDocumentAndPosition.description(), "Deduplicate by document ID and position");
        assert_eq!(DeduplicationPolicy::Exact.description(), "Exact deduplication using all fields");
    }

    #[test]
    fn test_deduplication_policy_performance_costs() {
        assert_eq!(DeduplicationPolicy::None.performance_cost(), 1.0);
        assert!(DeduplicationPolicy::ByDocumentId.performance_cost() > 1.0);
        assert!(DeduplicationPolicy::ByDocumentAndPosition.performance_cost() > DeduplicationPolicy::ByDocumentId.performance_cost());
        assert!(DeduplicationPolicy::Exact.performance_cost() > DeduplicationPolicy::ByDocumentAndPosition.performance_cost());
    }

    #[test]
    fn test_deduplication_stats() {
        let mut stats = DeduplicationStats::new();
        assert_eq!(stats.efficiency, 1.0);
        
        // Record some duplicates
        stats.record(100, 80);
        assert_eq!(stats.total_processed, 100);
        assert_eq!(stats.unique_results, 80);
        assert_eq!(stats.duplicates_removed, 20);
        assert_eq!(stats.efficiency, 0.8);
    }

    #[test]
    fn test_result_deduplicator_creation() {
        let dedup = ResultDeduplicator::new(DeduplicationPolicy::ByDocumentId);
        assert_eq!(dedup.policy(), DeduplicationPolicy::ByDocumentId);
        assert_eq!(dedup.stats().total_processed, 0);
    }

    #[test]
    fn test_no_deduplication() {
        let mut deduplicator = ResultDeduplicator::new(DeduplicationPolicy::None);
        
        let doc_id = DocumentId::new();
        let results = vec![
            create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.9),
            create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.8), // Exact duplicate
        ];

        let deduplicated = deduplicator.deduplicate(results);
        assert_eq!(deduplicated.len(), 2); // No deduplication
        assert_eq!(deduplicator.stats().duplicates_removed, 0);
    }

    #[test]
    fn test_by_document_id_deduplication() {
        let mut deduplicator = ResultDeduplicator::new(DeduplicationPolicy::ByDocumentId);
        
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let results = vec![
            create_test_result(doc_id1, 0, 100, vec![0.1, 0.2], 0.9),
            create_test_result(doc_id1, 50, 100, vec![0.3, 0.4], 0.8), // Same document, different position
            create_test_result(doc_id2, 0, 100, vec![0.5, 0.6], 0.7),
        ];

        let deduplicated = deduplicator.deduplicate(results);
        assert_eq!(deduplicated.len(), 2); // One per document
        
        // Should keep the highest scoring result for doc_id1
        assert_eq!(deduplicated[0].document_id, doc_id1);
        assert_eq!(deduplicated[0].similarity_score, 0.9);
        assert_eq!(deduplicated[1].document_id, doc_id2);
    }

    #[test]
    fn test_by_position_deduplication() {
        let mut deduplicator = ResultDeduplicator::new(DeduplicationPolicy::ByDocumentAndPosition);
        
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let results = vec![
            create_test_result(doc_id1, 0, 100, vec![0.1, 0.2], 0.9),
            create_test_result(doc_id1, 0, 100, vec![0.1, 0.2], 0.8), // Same document and position
            create_test_result(doc_id1, 50, 100, vec![0.3, 0.4], 0.7), // Same document, different position
            create_test_result(doc_id2, 0, 100, vec![0.5, 0.6], 0.6),
        ];

        let deduplicated = deduplicator.deduplicate(results);
        assert_eq!(deduplicated.len(), 3); // One duplicate removed
        assert_eq!(deduplicator.stats().duplicates_removed, 1);
        
        // Results should be sorted by similarity score
        assert_eq!(deduplicated[0].similarity_score, 0.9);
        assert_eq!(deduplicated[1].similarity_score, 0.7);
        assert_eq!(deduplicated[2].similarity_score, 0.6);
    }

    #[test]
    fn test_exact_deduplication() {
        let mut deduplicator = ResultDeduplicator::new(DeduplicationPolicy::Exact);
        
        let doc_id = DocumentId::new();
        let results = vec![
            create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.9),
            create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.8), // Same everything except score
            create_test_result(doc_id, 0, 100, vec![0.1, 0.3], 0.8), // Different vector
            create_test_result(doc_id, 0, 200, vec![0.1, 0.2], 0.8), // Different length
        ];

        let deduplicated = deduplicator.deduplicate(results);
        assert_eq!(deduplicated.len(), 3); // One exact duplicate removed
        assert_eq!(deduplicator.stats().duplicates_removed, 1);
    }

    #[test]
    fn test_hash_search_result_consistency() {
        let deduplicator = ResultDeduplicator::new(DeduplicationPolicy::Exact);
        
        let doc_id = DocumentId::new();
        let result1 = create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.9);
        let result2 = create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.8); // Different score

        let hash1 = deduplicator.hash_search_result(&result1);
        let hash2 = deduplicator.hash_search_result(&result2);
        
        // Should be the same hash (score not included)
        assert_eq!(hash1, hash2);
        
        // Different vector should produce different hash
        let result3 = create_test_result(doc_id, 0, 100, vec![0.1, 0.3], 0.9);
        let hash3 = deduplicator.hash_search_result(&result3);
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_deduplicator_reset() {
        let mut deduplicator = ResultDeduplicator::new(DeduplicationPolicy::ByDocumentAndPosition);
        
        let doc_id = DocumentId::new();
        let results = vec![
            create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.9),
            create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.8),
        ];

        let _deduplicated = deduplicator.deduplicate(results);
        assert_eq!(deduplicator.stats().total_processed, 2);
        
        deduplicator.reset();
        assert_eq!(deduplicator.stats().total_processed, 0);
        assert!(deduplicator.seen_postings.is_empty());
    }

    #[test]
    fn test_empty_results() {
        let mut deduplicator = ResultDeduplicator::new(DeduplicationPolicy::ByDocumentAndPosition);
        let results = vec![];
        
        let deduplicated = deduplicator.deduplicate(results);
        assert!(deduplicated.is_empty());
        assert_eq!(deduplicator.stats().efficiency, 1.0);
    }

    #[test]
    fn test_single_result() {
        let mut deduplicator = ResultDeduplicator::new(DeduplicationPolicy::Exact);
        let doc_id = DocumentId::new();
        let results = vec![create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.9)];
        
        let deduplicated = deduplicator.deduplicate(results);
        assert_eq!(deduplicated.len(), 1);
        assert_eq!(deduplicator.stats().efficiency, 1.0);
        assert_eq!(deduplicator.stats().duplicates_removed, 0);
    }

    #[test]
    fn test_all_duplicates() {
        let mut deduplicator = ResultDeduplicator::new(DeduplicationPolicy::ByDocumentAndPosition);
        
        let doc_id = DocumentId::new();
        let results = vec![
            create_test_result(doc_id, 0, 100, vec![0.1, 0.2], 0.9),
            create_test_result(doc_id, 0, 100, vec![0.3, 0.4], 0.8),
            create_test_result(doc_id, 0, 100, vec![0.5, 0.6], 0.7),
        ];

        let deduplicated = deduplicator.deduplicate(results);
        assert_eq!(deduplicated.len(), 1); // Only one unique position
        assert_eq!(deduplicator.stats().duplicates_removed, 2);
        assert!((deduplicator.stats().efficiency - (1.0 / 3.0)).abs() < 0.0001);
    }

    #[test]
    fn test_vector_quantization_in_hashing() {
        let deduplicator = ResultDeduplicator::new(DeduplicationPolicy::Exact);
        
        let doc_id = DocumentId::new();
        // These vectors are very close but not exactly equal due to floating point precision
        let result1 = create_test_result(doc_id, 0, 100, vec![0.1, 0.2000001], 0.9);
        let result2 = create_test_result(doc_id, 0, 100, vec![0.1, 0.2000002], 0.8);

        let hash1 = deduplicator.hash_search_result(&result1);
        let hash2 = deduplicator.hash_search_result(&result2);
        
        // Should be the same hash due to quantization (difference < 1e-6)
        assert_eq!(hash1, hash2);
        
        // But larger differences should still produce different hashes
        let result3 = create_test_result(doc_id, 0, 100, vec![0.1, 0.3], 0.9);
        let hash3 = deduplicator.hash_search_result(&result3);
        assert_ne!(hash1, hash3);
    }
}
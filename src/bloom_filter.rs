//! Bloom Filter Implementation for Document IDs
//!
//! This module provides a memory-mapped bloom filter implementation optimized for
//! efficient document ID lookups across shards. The bloom filter uses multiple
//! hash functions to achieve configurable false positive rates while maintaining
//! high performance for insertion and membership testing.
//!
//! # Usage Examples
//!
//! ## Basic Usage
//!
//! ```rust
//! use shardex::bloom_filter::BloomFilter;
//! use shardex::identifiers::DocumentId;
//!
//! // Create a bloom filter for 10,000 documents with 1% false positive rate
//! let mut filter = BloomFilter::new(10_000, 0.01)?;
//!
//! let doc_id = DocumentId::new();
//! filter.insert(doc_id);
//!
//! assert!(filter.contains(doc_id));
//! # Ok::<(), shardex::error::ShardexError>(())
//! ```
//!
//! ## Builder Pattern Configuration
//!
//! ```rust
//! use shardex::bloom_filter::BloomFilterBuilder;
//!
//! let mut filter = BloomFilterBuilder::new()
//!     .capacity(50_000)
//!     .false_positive_rate(0.005)
//!     .build()?;
//! # Ok::<(), shardex::error::ShardexError>(())
//! ```
//!
//! ## Merging Bloom Filters
//!
//! ```rust
//! use shardex::bloom_filter::BloomFilter;
//! use shardex::identifiers::DocumentId;
//!
//! let mut filter1 = BloomFilter::new(1000, 0.01)?;
//! let mut filter2 = BloomFilter::new(1000, 0.01)?;
//!
//! let doc1 = DocumentId::new();
//! let doc2 = DocumentId::new();
//!
//! filter1.insert(doc1);
//! filter2.insert(doc2);
//!
//! filter1.merge(&filter2)?;
//!
//! assert!(filter1.contains(doc1));
//! assert!(filter1.contains(doc2));
//! # Ok::<(), shardex::error::ShardexError>(())
//! ```
//!
//! ## Memory Mapping Support
//!
//! ```rust
//! use shardex::bloom_filter::{BloomFilter, BloomFilterHeader};
//! use bytemuck;
//!
//! let filter = BloomFilter::new(1000, 0.01)?;
//! let header = filter.to_header(0); // bit_array_offset parameter
//!
//! // Headers can be memory-mapped directly
//! let header_bytes = bytemuck::bytes_of(&header);
//! let restored_header = bytemuck::from_bytes::<BloomFilterHeader>(header_bytes);
//! # Ok::<(), shardex::error::ShardexError>(())
//! ```
//!
//! # Performance Characteristics
//!
//! - **Insertion**: O(k) where k is the number of hash functions (typically 3-5)
//! - **Lookup**: O(k) with very fast bit array access
//! - **Memory Usage**: Configurable based on capacity and false positive rate
//! - **Hash Functions**: Optimized for ULID document IDs with good distribution
//!
//! # False Positive Rates
//!
//! The bloom filter provides configurable false positive rates:
//! - 0.1% (0.001): High memory usage, very few false positives
//! - 1% (0.01): Balanced memory and accuracy (recommended)
//! - 5% (0.05): Lower memory usage, more false positives
//!
//! False negatives are impossible - if `contains()` returns false, the document
//! ID is definitely not in the filter.

use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// A memory-mapped bloom filter for efficient document ID lookups
///
/// The BloomFilter uses multiple hash functions to achieve configurable false
/// positive rates while maintaining high performance. It's designed to be
/// memory-mappable using bytemuck traits for persistence and zero-copy access.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BloomFilter {
    /// Bit array storing the bloom filter bits
    /// Using Vec<u64> for efficient bit operations and memory mapping
    bit_array: Vec<u64>,
    /// Number of hash functions used
    hash_functions: usize,
    /// Expected number of elements
    capacity: usize,
    /// Actual number of insertions performed
    inserted_count: usize,
    /// Configured false positive rate
    false_positive_rate: f64,
    /// Number of bits in the bit array
    bit_array_size: usize,
}

/// Memory-mapped compatible bloom filter header
///
/// This structure can be memory-mapped directly. The bit array data is stored
/// separately and referenced by offset and size fields.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct BloomFilterHeader {
    /// Number of hash functions used
    pub hash_functions: u32,
    /// Expected number of elements
    pub capacity: u32,
    /// Actual number of insertions performed
    pub inserted_count: u32,
    /// False positive rate (multiplied by 1,000,000 for integer storage)
    pub false_positive_rate_micros: u32,
    /// Number of bits in the bit array
    pub bit_array_size: u32,
    /// Size of bit array data in bytes
    pub bit_array_bytes: u32,
    /// Offset to bit array data within the mapped file
    pub bit_array_offset: u64,
}

/// Builder for configuring bloom filters
#[derive(Debug, Clone)]
pub struct BloomFilterBuilder {
    capacity: usize,
    false_positive_rate: f64,
}

/// Statistics for bloom filter performance monitoring
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BloomFilterStats {
    /// Number of hash functions
    pub hash_functions: usize,
    /// Expected capacity
    pub capacity: usize,
    /// Actual insertions performed
    pub inserted_count: usize,
    /// Configured false positive rate
    pub false_positive_rate: f64,
    /// Current load factor (inserted_count / capacity)
    pub load_factor: f64,
    /// Expected false positive rate at current load
    pub actual_false_positive_rate: f64,
    /// Memory usage in bytes
    pub memory_usage: usize,
    /// Number of bits set in the filter
    pub bits_set: usize,
    /// Bit array utilization (bits_set / total_bits)
    pub bit_utilization: f64,
}

// SAFETY: BloomFilterHeader contains only Pod types and has repr(C) layout
unsafe impl Pod for BloomFilterHeader {}
// SAFETY: BloomFilterHeader can be safely zero-initialized
unsafe impl Zeroable for BloomFilterHeader {}

impl BloomFilter {
    /// Create a new bloom filter with specified capacity and false positive rate
    ///
    /// # Arguments
    /// * `capacity` - Expected number of elements to insert
    /// * `false_positive_rate` - Desired false positive rate (0.0 to 1.0)
    ///
    /// # Returns
    /// A configured BloomFilter ready for use
    ///
    /// # Errors
    /// Returns ShardexError::Config if parameters are invalid
    pub fn new(capacity: usize, false_positive_rate: f64) -> Result<Self, ShardexError> {
        if capacity == 0 {
            return Err(ShardexError::Config(
                "Bloom filter capacity must be greater than 0".to_string(),
            ));
        }

        if false_positive_rate <= 0.0 || false_positive_rate >= 1.0 {
            return Err(ShardexError::Config(
                "False positive rate must be between 0.0 and 1.0 (exclusive)".to_string(),
            ));
        }

        let (bit_array_size, hash_functions) =
            Self::calculate_parameters(capacity, false_positive_rate);
        let bit_array = vec![0u64; bit_array_size.div_ceil(64)]; // Round up to u64 boundaries

        Ok(Self {
            bit_array,
            hash_functions,
            capacity,
            inserted_count: 0,
            false_positive_rate,
            bit_array_size,
        })
    }

    /// Calculate optimal bit array size and hash function count
    ///
    /// Uses standard bloom filter formulas:
    /// - m = -n * ln(p) / (ln(2)^2) where m=bits, n=capacity, p=false_positive_rate
    /// - k = (m/n) * ln(2) where k=hash_functions
    fn calculate_parameters(capacity: usize, false_positive_rate: f64) -> (usize, usize) {
        let capacity_f = capacity as f64;
        let ln2 = std::f64::consts::LN_2;

        // Calculate optimal bit array size
        let bit_array_size = (-capacity_f * false_positive_rate.ln() / (ln2 * ln2)).ceil() as usize;

        // Calculate optimal number of hash functions
        let hash_functions = ((bit_array_size as f64 / capacity_f) * ln2).ceil() as usize;

        // Ensure we have at least 1 hash function and no more than 10 (practical limit)
        let hash_functions = hash_functions.clamp(1, 10);

        (bit_array_size, hash_functions)
    }

    /// Insert a document ID into the bloom filter
    ///
    /// Sets multiple bits in the bit array based on hash function outputs.
    /// This operation cannot fail and will never produce false negatives.
    pub fn insert(&mut self, document_id: DocumentId) {
        let hashes = self.hash_document_id(document_id);

        for hash_value in hashes {
            let bit_index = (hash_value as usize) % self.bit_array_size;
            let array_index = bit_index / 64;
            let bit_position = bit_index % 64;

            self.bit_array[array_index] |= 1u64 << bit_position;
        }

        self.inserted_count += 1;
    }

    /// Test if a document ID might be in the bloom filter
    ///
    /// Returns true if the document ID might be present (with possible false positives).
    /// Returns false if the document ID is definitely not present (no false negatives).
    pub fn contains(&self, document_id: DocumentId) -> bool {
        let hashes = self.hash_document_id(document_id);

        for hash_value in hashes {
            let bit_index = (hash_value as usize) % self.bit_array_size;
            let array_index = bit_index / 64;
            let bit_position = bit_index % 64;

            if (self.bit_array[array_index] & (1u64 << bit_position)) == 0 {
                return false; // Definitely not present
            }
        }

        true // Might be present
    }

    /// Merge another bloom filter into this one
    ///
    /// Both filters must have the same configuration (bit array size and hash functions).
    /// The merge operation uses bitwise OR to combine the bit arrays.
    ///
    /// # Arguments
    /// * `other` - The bloom filter to merge into this one
    ///
    /// # Returns
    /// Ok(()) on success, or ShardexError if filters are incompatible
    pub fn merge(&mut self, other: &BloomFilter) -> Result<(), ShardexError> {
        if self.bit_array_size != other.bit_array_size {
            return Err(ShardexError::Config(format!(
                "Cannot merge bloom filters with different bit array sizes: {} vs {}",
                self.bit_array_size, other.bit_array_size
            )));
        }

        if self.hash_functions != other.hash_functions {
            return Err(ShardexError::Config(format!(
                "Cannot merge bloom filters with different hash function counts: {} vs {}",
                self.hash_functions, other.hash_functions
            )));
        }

        // Merge bit arrays using bitwise OR
        for (i, other_bits) in other.bit_array.iter().enumerate() {
            self.bit_array[i] |= other_bits;
        }

        // Update insertion count (upper bound estimate)
        self.inserted_count += other.inserted_count;

        Ok(())
    }

    /// Clear all bits in the bloom filter
    ///
    /// Resets the filter to empty state for reuse.
    pub fn clear(&mut self) {
        for bits in &mut self.bit_array {
            *bits = 0;
        }
        self.inserted_count = 0;
    }

    /// Get bloom filter statistics for monitoring
    pub fn stats(&self) -> BloomFilterStats {
        let load_factor = if self.capacity > 0 {
            self.inserted_count as f64 / self.capacity as f64
        } else {
            0.0
        };

        // Calculate actual false positive rate based on current load
        let actual_false_positive_rate = if self.inserted_count > 0 {
            let k = self.hash_functions as f64;
            let m = self.bit_array_size as f64;
            let n = self.inserted_count as f64;
            (1.0 - (-k * n / m).exp()).powf(k)
        } else {
            0.0
        };

        let bits_set = self.count_set_bits();
        let bit_utilization = if self.bit_array_size > 0 {
            bits_set as f64 / self.bit_array_size as f64
        } else {
            0.0
        };

        BloomFilterStats {
            hash_functions: self.hash_functions,
            capacity: self.capacity,
            inserted_count: self.inserted_count,
            false_positive_rate: self.false_positive_rate,
            load_factor,
            actual_false_positive_rate,
            memory_usage: self.bit_array.len() * 8, // Vec<u64> memory usage
            bits_set,
            bit_utilization,
        }
    }

    /// Create a memory-mapped header for this bloom filter
    pub fn to_header(&self, bit_array_offset: u64) -> BloomFilterHeader {
        BloomFilterHeader {
            hash_functions: self.hash_functions as u32,
            capacity: self.capacity as u32,
            inserted_count: self.inserted_count as u32,
            false_positive_rate_micros: (self.false_positive_rate * 1_000_000.0) as u32,
            bit_array_size: self.bit_array_size as u32,
            bit_array_bytes: (self.bit_array.len() * 8) as u32,
            bit_array_offset,
        }
    }

    /// Check if the bloom filter is at capacity
    pub fn is_at_capacity(&self) -> bool {
        self.inserted_count >= self.capacity
    }

    /// Check if the bloom filter is overloaded (beyond recommended capacity)
    pub fn is_overloaded(&self) -> bool {
        self.inserted_count > self.capacity
    }

    /// Get the current load factor
    pub fn load_factor(&self) -> f64 {
        if self.capacity > 0 {
            self.inserted_count as f64 / self.capacity as f64
        } else {
            0.0
        }
    }

    /// Get the number of insertions
    pub fn inserted_count(&self) -> usize {
        self.inserted_count
    }

    /// Get the configured capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the false positive rate
    pub fn false_positive_rate(&self) -> f64 {
        self.false_positive_rate
    }

    /// Get the number of hash functions
    pub fn hash_functions(&self) -> usize {
        self.hash_functions
    }

    /// Get the bit array size
    pub fn bit_array_size(&self) -> usize {
        self.bit_array_size
    }

    /// Generate hash values for a document ID using multiple hash functions
    fn hash_document_id(&self, document_id: DocumentId) -> Vec<u64> {
        let mut hashes = Vec::with_capacity(self.hash_functions);
        let bytes = document_id.to_bytes();

        // Use different hash seeds to create independent hash functions
        for i in 0..self.hash_functions {
            let mut hasher = DefaultHasher::new();

            // Hash the document ID bytes
            bytes.hash(&mut hasher);

            // Add the hash function index as seed to create different hash values
            i.hash(&mut hasher);

            hashes.push(hasher.finish());
        }

        hashes
    }

    /// Count the number of set bits in the bit array
    fn count_set_bits(&self) -> usize {
        self.bit_array
            .iter()
            .map(|&bits| bits.count_ones() as usize)
            .sum()
    }
}

impl BloomFilterBuilder {
    /// Create a new bloom filter builder
    pub fn new() -> Self {
        Self {
            capacity: 1000,
            false_positive_rate: 0.01,
        }
    }

    /// Set the expected capacity
    pub fn capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    /// Set the false positive rate
    pub fn false_positive_rate(mut self, rate: f64) -> Self {
        self.false_positive_rate = rate;
        self
    }

    /// Build the bloom filter with configured parameters
    pub fn build(self) -> Result<BloomFilter, ShardexError> {
        BloomFilter::new(self.capacity, self.false_positive_rate)
    }
}

impl Default for BloomFilterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl BloomFilterHeader {
    /// Create a zero-initialized bloom filter header
    pub fn new_zero() -> Self {
        Self::zeroed()
    }

    /// Check if this header represents a valid bloom filter
    pub fn is_valid(&self) -> bool {
        self.hash_functions > 0
            && self.capacity > 0
            && self.bit_array_size > 0
            && self.bit_array_bytes > 0
    }

    /// Get the false positive rate from the stored micros value
    pub fn false_positive_rate(&self) -> f64 {
        self.false_positive_rate_micros as f64 / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_bloom_filter_creation() {
        let filter = BloomFilter::new(1000, 0.01).unwrap();

        assert_eq!(filter.capacity(), 1000);
        assert_eq!(filter.inserted_count(), 0);
        assert_eq!(filter.false_positive_rate(), 0.01);
        assert!(filter.hash_functions() >= 1);
        assert!(filter.bit_array_size() > 0);
    }

    #[test]
    fn test_invalid_parameters() {
        // Zero capacity
        let result = BloomFilter::new(0, 0.01);
        assert!(result.is_err());
        assert!(matches!(result, Err(ShardexError::Config(_))));

        // Invalid false positive rate
        let result = BloomFilter::new(1000, 0.0);
        assert!(result.is_err());

        let result = BloomFilter::new(1000, 1.0);
        assert!(result.is_err());

        let result = BloomFilter::new(1000, 1.5);
        assert!(result.is_err());
    }

    #[test]
    fn test_parameter_calculation() {
        let (bits, hash_funcs) = BloomFilter::calculate_parameters(1000, 0.01);

        // For 1000 elements with 1% false positive rate, should need roughly 9600 bits
        assert!(
            bits >= 9000 && bits <= 10000,
            "Expected ~9600 bits, got {}",
            bits
        );

        // Should use multiple hash functions for good distribution
        assert!(
            hash_funcs >= 3 && hash_funcs <= 10,
            "Expected 3-10 hash functions, got {}",
            hash_funcs
        );
    }

    #[test]
    fn test_insert_and_contains() {
        let mut filter = BloomFilter::new(100, 0.01).unwrap();
        let doc_id = DocumentId::new();

        // Should not contain before insertion
        assert!(!filter.contains(doc_id));

        // Insert and test
        filter.insert(doc_id);
        assert!(filter.contains(doc_id));
        assert_eq!(filter.inserted_count(), 1);
    }

    #[test]
    fn test_no_false_negatives() {
        let mut filter = BloomFilter::new(1000, 0.05).unwrap();
        let mut inserted_ids = Vec::new();

        // Insert many document IDs
        for _ in 0..500 {
            let doc_id = DocumentId::new();
            inserted_ids.push(doc_id);
            filter.insert(doc_id);
        }

        // All inserted IDs must be found (no false negatives)
        for doc_id in inserted_ids {
            assert!(
                filter.contains(doc_id),
                "False negative detected for document ID"
            );
        }
    }

    #[test]
    fn test_false_positive_rate() {
        let mut filter = BloomFilter::new(1000, 0.05).unwrap();
        let mut inserted_ids = HashSet::new();

        // Insert exactly the capacity
        for _ in 0..1000 {
            let doc_id = DocumentId::new();
            inserted_ids.insert(doc_id);
            filter.insert(doc_id);
        }

        // Test with many non-inserted IDs to measure false positive rate
        let test_count = 10000;
        let mut false_positives = 0;

        for _ in 0..test_count {
            let test_id = DocumentId::new();
            if !inserted_ids.contains(&test_id) && filter.contains(test_id) {
                false_positives += 1;
            }
        }

        let actual_fp_rate = false_positives as f64 / test_count as f64;

        // Should be within reasonable bounds of configured rate (0.05)
        // Allow up to 10% false positive rate due to test randomness
        assert!(
            actual_fp_rate <= 0.10,
            "False positive rate too high: {}",
            actual_fp_rate
        );
    }

    #[test]
    fn test_merge_compatible_filters() {
        let mut filter1 = BloomFilter::new(100, 0.01).unwrap();
        let mut filter2 = BloomFilter::new(100, 0.01).unwrap();

        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();

        filter1.insert(doc1);
        filter2.insert(doc2);

        // Merge filter2 into filter1
        filter1.merge(&filter2).unwrap();

        // Both documents should be present
        assert!(filter1.contains(doc1));
        assert!(filter1.contains(doc2));

        // Insertion count should be sum
        assert_eq!(filter1.inserted_count(), 2);
    }

    #[test]
    fn test_merge_incompatible_filters() {
        let mut filter1 = BloomFilter::new(100, 0.01).unwrap();
        let filter2 = BloomFilter::new(200, 0.01).unwrap(); // Different capacity

        let result = filter1.merge(&filter2);
        assert!(result.is_err());
        assert!(matches!(result, Err(ShardexError::Config(_))));
    }

    #[test]
    fn test_clear() {
        let mut filter = BloomFilter::new(100, 0.01).unwrap();
        let doc_id = DocumentId::new();

        filter.insert(doc_id);
        assert!(filter.contains(doc_id));
        assert_eq!(filter.inserted_count(), 1);

        filter.clear();
        assert!(!filter.contains(doc_id));
        assert_eq!(filter.inserted_count(), 0);
    }

    #[test]
    fn test_load_factor_tracking() {
        let mut filter = BloomFilter::new(100, 0.01).unwrap();

        assert_eq!(filter.load_factor(), 0.0);
        assert!(!filter.is_at_capacity());
        assert!(!filter.is_overloaded());

        // Insert half capacity
        for _ in 0..50 {
            filter.insert(DocumentId::new());
        }

        assert_eq!(filter.load_factor(), 0.5);
        assert!(!filter.is_at_capacity());

        // Insert to full capacity
        for _ in 0..50 {
            filter.insert(DocumentId::new());
        }

        assert_eq!(filter.load_factor(), 1.0);
        assert!(filter.is_at_capacity());

        // Insert beyond capacity
        filter.insert(DocumentId::new());
        assert!(filter.load_factor() > 1.0);
        assert!(filter.is_overloaded());
    }

    #[test]
    fn test_statistics() {
        let mut filter = BloomFilter::new(1000, 0.01).unwrap();

        // Insert some documents
        for _ in 0..500 {
            filter.insert(DocumentId::new());
        }

        let stats = filter.stats();

        assert_eq!(stats.capacity, 1000);
        assert_eq!(stats.inserted_count, 500);
        assert_eq!(stats.load_factor, 0.5);
        assert_eq!(stats.false_positive_rate, 0.01);
        assert!(stats.actual_false_positive_rate > 0.0);
        assert!(stats.memory_usage > 0);
        assert!(stats.bits_set > 0);
        assert!(stats.bit_utilization > 0.0 && stats.bit_utilization <= 1.0);
    }

    #[test]
    fn test_builder_pattern() {
        let filter = BloomFilterBuilder::new()
            .capacity(5000)
            .false_positive_rate(0.005)
            .build()
            .unwrap();

        assert_eq!(filter.capacity(), 5000);
        assert_eq!(filter.false_positive_rate(), 0.005);
    }

    #[test]
    fn test_header_creation() {
        let filter = BloomFilter::new(1000, 0.01).unwrap();
        let header = filter.to_header(2048);

        assert_eq!(header.capacity, 1000);
        assert_eq!(header.hash_functions, filter.hash_functions() as u32);
        assert_eq!(header.bit_array_offset, 2048);
        assert!(header.is_valid());
        assert_eq!(header.false_positive_rate(), 0.01);
    }

    #[test]
    fn test_header_bytemuck() {
        let header = BloomFilterHeader {
            hash_functions: 5,
            capacity: 1000,
            inserted_count: 500,
            false_positive_rate_micros: 10_000, // 0.01 * 1,000,000
            bit_array_size: 9600,
            bit_array_bytes: 1200,
            bit_array_offset: 4096,
        };

        // Test Pod trait - should be able to cast to bytes
        let bytes: &[u8] = bytemuck::bytes_of(&header);
        assert!(bytes.len() > 0);

        // Test round-trip
        let header_restored: BloomFilterHeader = bytemuck::pod_read_unaligned(bytes);
        assert_eq!(header, header_restored);
    }

    #[test]
    fn test_zero_initialized_header() {
        let zero_header = BloomFilterHeader::new_zero();
        assert!(!zero_header.is_valid()); // Should be invalid when zero
        assert_eq!(zero_header.false_positive_rate(), 0.0);

        let zero_header_bytemuck: BloomFilterHeader = BloomFilterHeader::zeroed();
        assert_eq!(zero_header_bytemuck.hash_functions, 0);
        assert_eq!(zero_header_bytemuck.capacity, 0);
        assert_eq!(zero_header_bytemuck.bit_array_size, 0);
    }

    #[test]
    fn test_hash_function_independence() {
        let filter = BloomFilter::new(100, 0.01).unwrap();
        let doc_id = DocumentId::new();

        let hashes = filter.hash_document_id(doc_id);

        // Should generate the expected number of hash values
        assert_eq!(hashes.len(), filter.hash_functions());

        // Hash values should be different (very high probability)
        for i in 0..hashes.len() {
            for j in (i + 1)..hashes.len() {
                assert_ne!(
                    hashes[i], hashes[j],
                    "Hash functions should produce different values"
                );
            }
        }
    }

    #[test]
    fn test_consistent_hashing() {
        let filter1 = BloomFilter::new(100, 0.01).unwrap();
        let filter2 = BloomFilter::new(100, 0.01).unwrap();
        let doc_id = DocumentId::new();

        let hashes1 = filter1.hash_document_id(doc_id);
        let hashes2 = filter2.hash_document_id(doc_id);

        // Same document ID should produce same hash values across filters
        assert_eq!(hashes1, hashes2);
    }

    #[test]
    fn test_bit_operations() {
        let mut filter = BloomFilter::new(100, 0.01).unwrap();
        let initial_bits_set = filter.count_set_bits();

        assert_eq!(initial_bits_set, 0);

        filter.insert(DocumentId::new());
        let after_insert = filter.count_set_bits();

        assert!(after_insert > 0);
        assert!(after_insert <= filter.hash_functions()); // At most k bits set per insertion
    }

    #[test]
    fn test_serialization() {
        let mut filter = BloomFilter::new(1000, 0.01).unwrap();

        // Insert some data to make it interesting
        for _ in 0..100 {
            filter.insert(DocumentId::new());
        }

        // Test JSON serialization
        let json = serde_json::to_string(&filter).unwrap();
        let filter_restored: BloomFilter = serde_json::from_str(&json).unwrap();

        assert_eq!(filter, filter_restored);

        // Test that functionality is preserved
        let doc_id = DocumentId::new();
        filter.insert(doc_id);
        filter_restored.contains(doc_id); // Should not panic
    }

    #[test]
    fn test_stats_serialization() {
        let mut filter = BloomFilter::new(1000, 0.01).unwrap();

        for _ in 0..100 {
            filter.insert(DocumentId::new());
        }

        let stats = filter.stats();
        let json = serde_json::to_string(&stats).unwrap();
        let stats_restored: BloomFilterStats = serde_json::from_str(&json).unwrap();

        assert_eq!(stats, stats_restored);
    }

    #[test]
    fn test_edge_cases() {
        // Very small capacity
        let mut small_filter = BloomFilter::new(1, 0.01).unwrap();
        let doc_id = DocumentId::new();

        small_filter.insert(doc_id);
        assert!(small_filter.contains(doc_id));

        // Very high false positive rate (but valid)
        let mut high_fp_filter = BloomFilter::new(100, 0.99).unwrap();
        high_fp_filter.insert(doc_id);
        assert!(high_fp_filter.contains(doc_id));

        // Very low false positive rate
        let mut low_fp_filter = BloomFilter::new(100, 0.001).unwrap();
        low_fp_filter.insert(doc_id);
        assert!(low_fp_filter.contains(doc_id));
    }

    #[test]
    fn test_memory_layout() {
        use std::mem;

        // Verify header size is reasonable
        let header_size = mem::size_of::<BloomFilterHeader>();
        assert!(header_size >= 32); // Should have multiple u32/u64 fields
        assert!(header_size % mem::align_of::<BloomFilterHeader>() == 0);

        // Verify alignment
        assert!(mem::align_of::<BloomFilterHeader>() >= 8); // Should align to at least u64
    }
}

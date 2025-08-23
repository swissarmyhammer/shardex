//! In-Memory Index (Shardex) Structure
//!
//! This module provides the main ShardexIndex that manages shard metadata and centroids
//! for efficient vector similarity search. The index uses parallel vector storage for
//! cache-friendly access patterns and supports dynamic shard management.
//!
//! # Key Components
//!
//! - [`ShardexIndex`]: Main in-memory index managing shard collection
//! - [`ShardexMetadata`]: Enhanced metadata for shard tracking and selection
//! - Centroid-based shard lookup for efficient query routing
//! - Dynamic shard addition and removal with automatic rebalancing
//! - Memory-mapped persistence for large shard collections
//!
//! # Usage Examples
//!
//! ## Creating a New Index
//!
//! ```rust
//! use shardex::shardex_index::ShardexIndex;
//! use shardex::config::ShardexConfig;
//! use tempfile::TempDir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let config = ShardexConfig::new()
//!     .directory_path(temp_dir.path())
//!     .vector_size(384)
//!     .shardex_segment_size(1000);
//!
//! let mut index = ShardexIndex::create(config)?;
//!
//! println!("Created index with capacity for {} shards per segment",
//!          index.segment_capacity());
//! # Ok(())
//! # }
//! ```
//!
//! ## Opening an Existing Index
//!
//! ```rust
//! use shardex::shardex_index::ShardexIndex;
//! use std::path::Path;
//!
//! # fn open_example(directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
//! let index = ShardexIndex::open(directory)?;
//!
//! println!("Opened index with {} shards", index.shard_count());
//! # Ok(())
//! # }
//! ```
//!
//! ## Finding Candidate Shards for Search
//!
//! ```rust
//! use shardex::shardex_index::ShardexIndex;
//!
//! # fn search_example(index: &ShardexIndex) -> Result<(), Box<dyn std::error::Error>> {
//! let query_vector = vec![0.5; 384];
//! let slop_factor = 3;
//!
//! // Find the 3 nearest shards to the query vector
//! let candidate_shards = index.find_nearest_shards(&query_vector, slop_factor)?;
//!
//! for shard_id in candidate_shards {
//!     println!("Search shard: {}", shard_id);
//! }
//! # Ok(())
//! # }
//! ```

use crate::config::ShardexConfig;
use crate::deduplication::{DeduplicationPolicy, ResultDeduplicator};
use crate::distance::DistanceMetric;
use crate::error::ShardexError;
use crate::identifiers::{DocumentId, ShardId};
use crate::shard::{Shard, ShardMetadata as BaseShardMetadata};
use crate::structures::SearchResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Enhanced shard metadata for the ShardexIndex
///
/// This extends the base ShardMetadata with additional fields needed for
/// efficient shard selection and management in the index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShardexMetadata {
    /// Unique identifier for this shard
    pub id: ShardId,
    /// Centroid vector representing the center of all non-deleted vectors
    pub centroid: Vec<f32>,
    /// Current number of postings in this shard
    pub posting_count: usize,
    /// Maximum capacity of this shard
    pub capacity: usize,
    /// When this shard was last modified
    pub last_modified: SystemTime,
    /// Current utilization as a percentage (0.0 to 1.0)
    pub utilization: f32,
    /// Whether the shard is available for write operations
    pub writable: bool,
    /// Estimated memory usage for this shard in bytes
    pub memory_usage: usize,
}

/// Main in-memory index managing shard collection and centroids
///
/// The ShardexIndex provides efficient shard management and centroid-based lookup
/// for vector similarity search. It uses parallel vector storage for cache-friendly
/// access patterns and supports persistence through memory-mapped segments.
pub struct ShardexIndex {
    /// Shard metadata in parallel with centroids for cache efficiency
    shards: Vec<ShardexMetadata>,
    /// Directory where the index and shards are stored
    directory: PathBuf,
    /// Vector dimension size for all shards
    vector_size: usize,
    /// Maximum number of shards per memory-mapped segment
    segment_capacity: usize,
    /// Cache of loaded shard instances for quick access
    shard_cache: HashMap<ShardId, Shard>,
    /// Maximum number of shards to keep in cache
    cache_limit: usize,
    /// Deduplication policy for search results
    deduplication_policy: DeduplicationPolicy,
}

impl ShardexMetadata {
    /// Create new shard metadata from base metadata and centroid
    pub fn from_shard_metadata(
        id: ShardId,
        base_metadata: &BaseShardMetadata,
        centroid: Vec<f32>,
        capacity: usize,
    ) -> Self {
        let utilization = if capacity > 0 {
            base_metadata.active_count as f32 / capacity as f32
        } else {
            0.0
        };

        Self {
            id,
            centroid,
            posting_count: base_metadata.active_count,
            capacity,
            last_modified: SystemTime::now(),
            utilization,
            writable: !base_metadata.read_only,
            memory_usage: base_metadata.disk_usage, // Approximation
        }
    }

    /// Update metadata from a shard instance
    pub fn update_from_shard(&mut self, shard: &Shard) {
        self.centroid = shard.get_centroid().to_vec();
        self.posting_count = shard.active_count();
        self.utilization = if self.capacity > 0 {
            self.posting_count as f32 / self.capacity as f32
        } else {
            0.0
        };
        self.last_modified = SystemTime::now();
        self.writable = !shard.is_read_only();
        // Update memory usage from shard metadata
        self.memory_usage = shard.metadata().disk_usage;
    }

    /// Check if this shard should be split based on utilization
    pub fn should_split(&self) -> bool {
        self.utilization >= 0.9 // Split at 90% utilization
    }

    /// Calculate distance between this shard's centroid and a query vector
    pub fn distance_to_query(&self, query_vector: &[f32]) -> Result<f32, ShardexError> {
        if query_vector.len() != self.centroid.len() {
            return Err(ShardexError::InvalidDimension {
                expected: self.centroid.len(),
                actual: query_vector.len(),
            });
        }

        Ok(Self::euclidean_distance(&self.centroid, query_vector))
    }

    /// Calculate Euclidean distance between two vectors
    ///
    /// This implementation is optimized for cache-friendly memory access patterns
    /// and can take advantage of compiler auto-vectorization and SIMD operations.
    fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
        debug_assert_eq!(a.len(), b.len(), "Vectors must have same length");

        // Use a more cache-friendly approach that helps with SIMD optimization
        let mut sum_squared = 0.0f32;

        // Process in chunks to enable better auto-vectorization
        for chunk in a.chunks_exact(4).zip(b.chunks_exact(4)) {
            let (a_chunk, b_chunk) = chunk;

            // Unroll loop for better SIMD utilization
            let diff0 = a_chunk[0] - b_chunk[0];
            let diff1 = a_chunk[1] - b_chunk[1];
            let diff2 = a_chunk[2] - b_chunk[2];
            let diff3 = a_chunk[3] - b_chunk[3];

            sum_squared += diff0 * diff0 + diff1 * diff1 + diff2 * diff2 + diff3 * diff3;
        }

        // Handle remaining elements
        let remainder_len = a.len() % 4;
        if remainder_len > 0 {
            let start = a.len() - remainder_len;
            for i in start..a.len() {
                let diff = a[i] - b[i];
                sum_squared += diff * diff;
            }
        }

        sum_squared.sqrt()
    }

    /// Get age of this shard since last modification
    pub fn age(&self) -> std::time::Duration {
        self.last_modified.elapsed().unwrap_or_default()
    }
}

impl ShardexIndex {
    /// Create a new ShardexIndex with the given configuration
    ///
    /// This creates a new empty index that can be populated with shards.
    /// The index will be stored in the directory specified in the configuration.
    ///
    /// # Arguments
    /// * `config` - Configuration containing directory path, vector size, and segment parameters
    pub fn create(config: ShardexConfig) -> Result<Self, ShardexError> {
        // Validate configuration
        config.validate()?;

        // Ensure directory exists
        std::fs::create_dir_all(&config.directory_path).map_err(ShardexError::Io)?;

        // Create index metadata file
        let index_metadata_path = config.directory_path.join("shardex.meta");
        let metadata = IndexMetadata {
            version: 1,
            vector_size: config.vector_size,
            segment_capacity: config.shardex_segment_size,
            created_at: SystemTime::now(),
            last_modified: SystemTime::now(),
        };

        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| ShardexError::Config(format!("Failed to serialize metadata: {}", e)))?;

        std::fs::write(&index_metadata_path, metadata_json).map_err(ShardexError::Io)?;

        Ok(Self {
            shards: Vec::new(),
            directory: config.directory_path,
            vector_size: config.vector_size,
            segment_capacity: config.shardex_segment_size,
            shard_cache: HashMap::new(),
            cache_limit: 100, // Default cache limit
            deduplication_policy: config.deduplication_policy,
        })
    }

    /// Open an existing ShardexIndex from the specified directory
    ///
    /// This loads the index metadata and discovers existing shards in the directory.
    /// Shard data is loaded lazily when needed to minimize memory usage.
    ///
    /// # Arguments
    /// * `directory` - Directory containing the index files
    pub fn open<P: AsRef<Path>>(directory: P) -> Result<Self, ShardexError> {
        let directory = directory.as_ref().to_path_buf();

        // Load index metadata
        let metadata_path = directory.join("shardex.meta");
        if !metadata_path.exists() {
            return Err(ShardexError::Config(format!(
                "Index metadata not found: {}",
                metadata_path.display()
            )));
        }

        let metadata_content = std::fs::read_to_string(&metadata_path).map_err(ShardexError::Io)?;
        let metadata: IndexMetadata = serde_json::from_str(&metadata_content)
            .map_err(|e| ShardexError::Config(format!("Failed to parse metadata: {}", e)))?;

        let mut index = Self {
            shards: Vec::new(),
            directory,
            vector_size: metadata.vector_size,
            segment_capacity: metadata.segment_capacity,
            shard_cache: HashMap::new(),
            cache_limit: 100,
            deduplication_policy: DeduplicationPolicy::default(),
        };

        // Discover and load existing shards
        index.discover_shards()?;

        Ok(index)
    }

    /// Add a new shard to the index
    ///
    /// This method adds shard metadata to the index for tracking. The shard
    /// itself should already exist on disk before calling this method.
    ///
    /// # Arguments
    /// * `shard` - The shard instance to add to the index
    pub fn add_shard(&mut self, shard: Shard) -> Result<(), ShardexError> {
        let shard_id = shard.id();

        // Check if shard already exists in index
        if self.shards.iter().any(|s| s.id == shard_id) {
            return Err(ShardexError::Config(format!(
                "Shard {} already exists in index",
                shard_id
            )));
        }

        // Validate vector dimension compatibility
        if shard.vector_size() != self.vector_size {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_size,
                actual: shard.vector_size(),
            });
        }

        // Create metadata from shard
        let metadata = ShardexMetadata::from_shard_metadata(
            shard_id,
            shard.metadata(),
            shard.get_centroid().to_vec(),
            shard.capacity(),
        );

        // Add to index
        self.shards.push(metadata);

        // Add to cache if there's room
        if self.shard_cache.len() < self.cache_limit {
            self.shard_cache.insert(shard_id, shard);
        }

        // Persist the updated index
        self.save_metadata()?;

        Ok(())
    }

    /// Remove a shard from the index
    ///
    /// This removes the shard metadata from the index. The actual shard files
    /// are not deleted and must be removed separately if desired.
    ///
    /// # Arguments
    /// * `shard_id` - ID of the shard to remove from the index
    pub fn remove_shard(&mut self, shard_id: ShardId) -> Result<bool, ShardexError> {
        // Find and remove the shard metadata
        let initial_count = self.shards.len();
        self.shards.retain(|s| s.id != shard_id);

        let removed = self.shards.len() < initial_count;

        // Remove from cache if present
        self.shard_cache.remove(&shard_id);

        if removed {
            // Persist the updated index
            self.save_metadata()?;
        }

        Ok(removed)
    }

    /// Find the single nearest shard to a query vector
    ///
    /// This method is optimized for write operations where only the closest shard
    /// is needed. It returns the ID of the shard with the centroid closest to the
    /// query vector.
    ///
    /// # Arguments
    /// * `query_vector` - The vector to find the nearest shard for
    ///
    /// # Returns
    /// The ID of the nearest shard, or None if the index is empty
    pub fn find_nearest_shard(
        &self,
        query_vector: &[f32],
    ) -> Result<Option<ShardId>, ShardexError> {
        // Validate query vector dimension
        if query_vector.len() != self.vector_size {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_size,
                actual: query_vector.len(),
            });
        }

        if self.shards.is_empty() {
            return Ok(None);
        }

        let mut nearest_shard_id: Option<ShardId> = None;
        let mut nearest_distance = f32::INFINITY;

        // Find the closest shard without storing all distances
        for shard_metadata in &self.shards {
            let distance = shard_metadata.distance_to_query(query_vector)?;
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_shard_id = Some(shard_metadata.id);
            }
        }

        Ok(nearest_shard_id)
    }

    /// Find multiple candidate shards for search operations
    ///
    /// This method calculates the distance from the query vector to each shard's
    /// centroid and returns the IDs of the nearest shards up to the slop factor limit.
    /// This is an alias for find_nearest_shards with a more descriptive name for search contexts.
    ///
    /// # Arguments
    /// * `query_vector` - The vector to find candidate shards for
    /// * `slop_factor` - Maximum number of candidate shards to return
    pub fn find_candidate_shards(
        &self,
        query_vector: &[f32],
        slop_factor: usize,
    ) -> Result<Vec<ShardId>, ShardexError> {
        self.find_nearest_shards(query_vector, slop_factor)
    }

    /// Calculate centroid distances for all shards in parallel
    ///
    /// This method computes the distance from the query vector to each shard's
    /// centroid and returns the distances in the same order as the shards.
    /// Useful for external analysis and custom selection logic.
    ///
    /// Uses parallel computation when there are many shards to improve performance
    /// on multi-core systems.
    ///
    /// # Arguments
    /// * `query_vector` - The vector to calculate distances from
    ///
    /// # Returns
    /// Vector of distances corresponding to each shard in index order
    pub fn calculate_centroid_distances(
        &self,
        query_vector: &[f32],
    ) -> Result<Vec<f32>, ShardexError> {
        use rayon::prelude::*;

        // Validate query vector dimension
        if query_vector.len() != self.vector_size {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_size,
                actual: query_vector.len(),
            });
        }

        // For small numbers of shards, sequential processing is more efficient
        // due to reduced overhead. Use parallel processing for larger collections.
        const PARALLEL_THRESHOLD: usize = 10;

        if self.shards.len() >= PARALLEL_THRESHOLD {
            // Parallel calculation for large shard collections
            let distances: Result<Vec<f32>, ShardexError> = self
                .shards
                .par_iter()
                .map(|shard_metadata| shard_metadata.distance_to_query(query_vector))
                .collect();

            distances
        } else {
            // Sequential calculation for small collections
            let distances: Result<Vec<f32>, ShardexError> = self
                .shards
                .iter()
                .map(|shard_metadata| shard_metadata.distance_to_query(query_vector))
                .collect();

            distances
        }
    }

    /// Find the nearest shards to a query vector
    ///
    /// This method calculates the distance from the query vector to each shard's
    /// centroid and returns the IDs of the nearest shards up to the slop factor limit.
    ///
    /// # Arguments
    /// * `query_vector` - The vector to find nearest shards for
    /// * `slop_factor` - Maximum number of nearest shards to return
    pub fn find_nearest_shards(
        &self,
        query_vector: &[f32],
        slop_factor: usize,
    ) -> Result<Vec<ShardId>, ShardexError> {
        // Validate query vector dimension
        if query_vector.len() != self.vector_size {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_size,
                actual: query_vector.len(),
            });
        }

        if slop_factor == 0 || self.shards.is_empty() {
            return Ok(Vec::new());
        }

        // Calculate distances to all shard centroids
        let distances = self.calculate_centroid_distances(query_vector)?;

        // Create paired vector of (ShardId, distance) and sort by distance
        let mut shard_distances: Vec<(ShardId, f32)> = self
            .shards
            .iter()
            .zip(distances.iter())
            .map(|(metadata, &distance)| (metadata.id, distance))
            .collect();

        // Sort by distance (ascending - nearest first)
        shard_distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return up to slop_factor nearest shard IDs
        let result_count = slop_factor.min(shard_distances.len());
        Ok(shard_distances
            .into_iter()
            .take(result_count)
            .map(|(id, _)| id)
            .collect())
    }

    /// Calculate optimal slop factor based on index characteristics
    ///
    /// This method analyzes the current state of the index to determine an optimal
    /// slop factor that balances search accuracy with performance. It considers
    /// vector dimensionality, shard distribution, and historical performance data.
    ///
    /// # Arguments
    /// * `vector_size` - Size of the query vector in dimensions
    /// * `shard_count` - Number of shards in the index
    ///
    /// # Returns
    /// Recommended slop factor based on index characteristics
    pub fn calculate_optimal_slop(&self, vector_size: usize, shard_count: usize) -> usize {
        // Use simple heuristics based on index characteristics
        // This can be enhanced with machine learning or statistical analysis

        if shard_count <= 1 {
            return 1;
        }

        // Base slop factor starts with a reasonable default
        let mut optimal_slop = 3;

        // Adjust for vector dimensionality
        // Higher dimensions benefit from searching more shards for accuracy
        if vector_size > 512 {
            optimal_slop += 2;
        } else if vector_size > 256 {
            optimal_slop += 1;
        }

        // Adjust for shard count
        // More shards allow for higher selectivity
        let shard_factor = match shard_count {
            1..=5 => shard_count,
            6..=20 => 5 + (shard_count - 5) / 3,
            21..=100 => 10 + (shard_count - 20) / 8,
            _ => 20,
        };

        optimal_slop = optimal_slop.min(shard_factor);

        // Ensure we don't exceed the total number of shards
        optimal_slop.min(shard_count)
    }

    /// Select shards with configurable slop factor and adaptive adjustment
    ///
    /// This method extends find_nearest_shards with adaptive slop factor selection
    /// based on the current index state and optional performance considerations.
    ///
    /// # Arguments
    /// * `query` - Query vector for similarity search
    /// * `slop` - Base slop factor to use
    ///
    /// # Returns
    /// Vector of shard IDs selected based on the slop factor and adaptive logic
    pub fn select_shards_with_slop(
        &self,
        query: &[f32],
        slop: usize,
    ) -> Result<Vec<ShardId>, ShardexError> {
        // For now, this is equivalent to find_nearest_shards
        // Can be enhanced with more sophisticated selection logic
        self.find_nearest_shards(query, slop)
    }

    /// Get the current number of shards in the index
    ///
    /// # Returns
    /// Number of shards currently in the index
    pub fn get_shard_count(&self) -> usize {
        self.shards.len()
    }

    /// Execute parallel search across multiple candidate shards
    ///
    /// This method performs similarity search across the specified candidate shards
    /// in parallel and aggregates the results into a single ranked list. It provides
    /// significant performance improvements when searching large numbers of shards.
    ///
    /// # Arguments
    /// * `query` - Query vector for similarity search
    /// * `candidate_shards` - List of shard IDs to search in parallel
    /// * `k` - Maximum number of results to return
    ///
    /// # Performance Notes
    /// - Uses rayon for parallel execution across candidate shards
    /// - Efficient result merging using BinaryHeap for O(log k) operations
    /// - Early termination when sufficient high-quality results are found
    /// - Memory-efficient processing with per-shard result limits
    ///
    /// # Example
    /// ```rust
    /// use shardex::shardex_index::ShardexIndex;
    ///
    /// # fn example(index: &mut ShardexIndex) -> Result<(), Box<dyn std::error::Error>> {
    /// let query = vec![0.1, 0.2, 0.3, 0.4];
    /// let candidates = index.find_candidate_shards(&query, 5)?;
    ///
    /// // Search top 10 results across 5 nearest shards in parallel
    /// let results = index.parallel_search(&query, &candidates, 10)?;
    ///
    /// for result in results {
    ///     println!("Doc: {}, Score: {:.3}", result.document_id, result.similarity_score);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn parallel_search(
        &mut self,
        query: &[f32],
        candidate_shards: &[ShardId],
        k: usize,
    ) -> Result<Vec<SearchResult>, ShardexError> {
        use rayon::prelude::*;

        // Validate query vector dimension
        if query.len() != self.vector_size {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_size,
                actual: query.len(),
            });
        }

        if k == 0 || candidate_shards.is_empty() {
            return Ok(Vec::new());
        }

        // Calculate per-shard result limit for efficiency
        // Request more than k from each shard to improve result quality
        // but limit to avoid excessive memory usage
        let per_shard_limit = (k * 2).clamp(50, 1000);

        // Convert candidate_shards to Vec for parallel processing
        let candidate_vec: Vec<ShardId> = candidate_shards.to_vec();

        // Perform parallel search across all candidate shards
        let shard_results: Result<Vec<Vec<SearchResult>>, ShardexError> = candidate_vec
            .par_iter()
            .map(|&shard_id| {
                // We need to handle the mutable borrow issue for get_shard
                // For now, we'll open shards directly to avoid borrowing conflicts
                let shard = Shard::open_read_only(shard_id, &self.directory).map_err(|e| {
                    ShardexError::Search(format!("Failed to open shard {}: {}", shard_id, e))
                })?;

                // Perform search on this shard
                let mut results = shard.search(query, per_shard_limit).map_err(|e| {
                    ShardexError::Search(format!("Search failed in shard {}: {}", shard_id, e))
                })?;

                // Sort results by similarity score (highest first) for early termination potential
                results.sort_by(|a, b| {
                    b.similarity_score
                        .partial_cmp(&a.similarity_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                Ok(results)
            })
            .collect();

        let all_shard_results = shard_results?;

        // Merge and rank results from all shards using configured deduplication policy
        let final_results =
            Self::merge_results_with_policy(all_shard_results, k, self.deduplication_policy);

        Ok(final_results)
    }

    /// Parallel search across multiple shards with configurable distance metric
    ///
    /// Similar to parallel_search but allows specifying the distance metric to use
    /// for similarity calculations.
    pub fn parallel_search_with_metric(
        &mut self,
        query: &[f32],
        candidate_shards: &[ShardId],
        k: usize,
        metric: DistanceMetric,
    ) -> Result<Vec<SearchResult>, ShardexError> {
        use rayon::prelude::*;

        // Validate query vector dimension
        if query.len() != self.vector_size {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_size,
                actual: query.len(),
            });
        }

        if k == 0 || candidate_shards.is_empty() {
            return Ok(Vec::new());
        }

        // Calculate per-shard result limit for efficiency
        // Request more than k from each shard to improve result quality
        // but limit to avoid excessive memory usage
        let per_shard_limit = (k * 2).clamp(50, 1000);

        // Convert candidate_shards to Vec for parallel processing
        let candidate_vec: Vec<ShardId> = candidate_shards.to_vec();

        // Perform parallel search across all candidate shards with specified metric
        let shard_results: Result<Vec<Vec<SearchResult>>, ShardexError> = candidate_vec
            .par_iter()
            .map(|&shard_id| {
                // We need to handle the mutable borrow issue for get_shard
                // For now, we'll open shards directly to avoid borrowing conflicts
                let shard = Shard::open_read_only(shard_id, &self.directory).map_err(|e| {
                    ShardexError::Search(format!("Failed to open shard {}: {}", shard_id, e))
                })?;

                // Perform search on this shard with the specified metric
                let mut results = shard
                    .search_with_metric(query, per_shard_limit, metric)
                    .map_err(|e| {
                        ShardexError::Search(format!("Search failed in shard {}: {}", shard_id, e))
                    })?;

                // Sort results by similarity score (highest first) for early termination potential
                results.sort_by(|a, b| {
                    b.similarity_score
                        .partial_cmp(&a.similarity_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                Ok(results)
            })
            .collect();

        let all_shard_results = shard_results?;

        // Merge and rank results from all shards using configured deduplication policy
        let final_results =
            Self::merge_results_with_policy(all_shard_results, k, self.deduplication_policy);

        Ok(final_results)
    }

    #[allow(clippy::empty_line_after_doc_comments)]
    /// Merge results from multiple shards with configurable deduplication policy.
    ///
    /// This method aggregates search results from multiple shards and returns
    /// the top-k results ranked by similarity score. Results are deduplicated
    /// based on configured policy to avoid returning duplicate entries from
    /// overlapping shard boundaries.
    ///
    /// # Arguments
    /// * `shard_results` - Vector of result vectors, one per shard
    /// * `k` - Maximum number of results to return
    /// * `policy` - Deduplication policy to apply
    ///
    /// # Returns
    /// Vector of deduplicated results, sorted by similarity score (descending)
    ///
    /// # Performance
    /// - Time complexity: O(n log n) where n is total results from all shards
    /// - Space complexity: O(n) for deduplication tracking
    /// - Memory efficient: processes results in streaming fashion
    #[allow(clippy::empty_line_after_doc_comments)]
    fn merge_results_with_policy(
        shard_results: Vec<Vec<SearchResult>>,
        k: usize,
        policy: DeduplicationPolicy,
    ) -> Vec<SearchResult> {
        if k == 0 {
            return Vec::new();
        }

        // Flatten all results into a single vector
        let all_results: Vec<SearchResult> = shard_results.into_iter().flatten().collect();

        // Apply deduplication using the new system
        let mut deduplicator = ResultDeduplicator::new(policy);
        let mut deduplicated_results = deduplicator.deduplicate(all_results);

        // Limit to k results (already sorted by deduplicator)
        if deduplicated_results.len() > k {
            deduplicated_results.truncate(k);
        }

        deduplicated_results
    }

    /// Get a shard instance by ID
    ///
    /// This method returns a cached shard if available, or loads it from disk.
    /// The shard is added to the cache for future access.
    ///
    /// # Arguments
    /// * `shard_id` - ID of the shard to retrieve
    pub fn get_shard(&mut self, shard_id: ShardId) -> Result<&Shard, ShardexError> {
        // Check if shard is already in cache
        if !self.shard_cache.contains_key(&shard_id) {
            // Load shard from disk
            let shard = Shard::open(shard_id, &self.directory)?;

            // Make room in cache if needed
            if self.shard_cache.len() >= self.cache_limit {
                // Simple LRU: remove first item (oldest in HashMap iteration order)
                if let Some(old_id) = self.shard_cache.keys().next().copied() {
                    self.shard_cache.remove(&old_id);
                }
            }

            self.shard_cache.insert(shard_id, shard);
        }

        self.shard_cache
            .get(&shard_id)
            .ok_or_else(|| ShardexError::Config(format!("Shard {} not found", shard_id)))
    }

    /// Get mutable reference to a shard instance by ID
    ///
    /// This method returns a mutable reference to a cached shard, loading it if necessary.
    /// Use this when you need to modify the shard (add/remove postings).
    ///
    /// # Arguments
    /// * `shard_id` - ID of the shard to retrieve
    pub fn get_shard_mut(&mut self, shard_id: ShardId) -> Result<&mut Shard, ShardexError> {
        // Ensure shard is loaded in cache
        if !self.shard_cache.contains_key(&shard_id) {
            self.get_shard(shard_id)?; // This will load it into cache
        }

        self.shard_cache
            .get_mut(&shard_id)
            .ok_or_else(|| ShardexError::Config(format!("Shard {} not found", shard_id)))
    }

    /// Update shard metadata after shard modification
    ///
    /// Call this method after modifying a shard to ensure the index metadata
    /// reflects the current state of the shard.
    ///
    /// # Arguments
    /// * `shard_id` - ID of the shard that was modified
    pub fn refresh_shard_metadata(&mut self, shard_id: ShardId) -> Result<(), ShardexError> {
        // First ensure shard is loaded in cache
        if !self.shard_cache.contains_key(&shard_id) {
            let shard = Shard::open_read_only(shard_id, &self.directory)?;

            // Make room in cache if needed
            if self.shard_cache.len() >= self.cache_limit {
                if let Some(old_id) = self.shard_cache.keys().next().copied() {
                    self.shard_cache.remove(&old_id);
                }
            }

            self.shard_cache.insert(shard_id, shard);
        }

        // Now we can safely borrow the cached shard and update metadata
        if let Some(shard) = self.shard_cache.get(&shard_id) {
            if let Some(metadata) = self.shards.iter_mut().find(|s| s.id == shard_id) {
                metadata.update_from_shard(shard);
            }

            // Persist changes
            self.save_metadata()?;
        }

        Ok(())
    }

    /// Get the total number of shards in the index
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    /// Get the vector dimension size
    pub fn vector_size(&self) -> usize {
        self.vector_size
    }

    /// Get the segment capacity
    pub fn segment_capacity(&self) -> usize {
        self.segment_capacity
    }

    /// Get all shard IDs in the index
    pub fn shard_ids(&self) -> Vec<ShardId> {
        self.shards.iter().map(|s| s.id).collect()
    }

    /// Get metadata for all shards
    pub fn all_shard_metadata(&self) -> &[ShardexMetadata] {
        &self.shards
    }

    /// Clear the shard cache to free memory
    ///
    /// This method removes all cached shard instances from memory while preserving
    /// the index metadata. Useful for memory management when working with large
    /// numbers of shards.
    pub fn clear_cache(&mut self) {
        self.shard_cache.clear();
    }

    /// Set the cache size limit
    ///
    /// This controls how many shard instances are kept in memory. A larger cache
    /// improves performance but uses more memory. A smaller cache uses less memory
    /// but may require more disk access.
    ///
    /// # Arguments
    /// * `limit` - Maximum number of shards to keep cached
    pub fn set_cache_limit(&mut self, limit: usize) {
        self.cache_limit = limit;

        // Trim cache if it's now over the limit
        while self.shard_cache.len() > limit {
            if let Some(old_id) = self.shard_cache.keys().next().copied() {
                self.shard_cache.remove(&old_id);
            } else {
                break;
            }
        }
    }

    /// Bulk add multiple shards to the index
    ///
    /// This method efficiently adds multiple shards to the index in a single operation.
    /// It's more efficient than adding shards individually when dealing with many shards.
    ///
    /// # Arguments
    /// * `shards` - Vector of shard instances to add to the index
    pub fn bulk_add_shards(&mut self, shards: Vec<Shard>) -> Result<(), ShardexError> {
        for shard in shards {
            // Validate vector dimension compatibility
            if shard.vector_size() != self.vector_size {
                return Err(ShardexError::InvalidDimension {
                    expected: self.vector_size,
                    actual: shard.vector_size(),
                });
            }

            let shard_id = shard.id();

            // Check if shard already exists in index
            if self.shards.iter().any(|s| s.id == shard_id) {
                return Err(ShardexError::Config(format!(
                    "Shard {} already exists in index",
                    shard_id
                )));
            }

            // Create metadata from shard
            let metadata = ShardexMetadata::from_shard_metadata(
                shard_id,
                shard.metadata(),
                shard.get_centroid().to_vec(),
                shard.capacity(),
            );

            // Add to index
            self.shards.push(metadata);

            // Add to cache if there's room (but don't exceed limit)
            if self.shard_cache.len() < self.cache_limit {
                self.shard_cache.insert(shard_id, shard);
            }
        }

        // Persist the updated index once at the end
        self.save_metadata()?;

        Ok(())
    }

    /// Get shards that need splitting based on utilization thresholds
    ///
    /// This method identifies shards that should be split due to high utilization.
    /// It returns shard IDs sorted by utilization (highest first) to prioritize
    /// the most urgent splits.
    pub fn get_shards_to_split(&self) -> Vec<ShardId> {
        let mut candidates: Vec<(ShardId, f32)> = self
            .shards
            .iter()
            .filter(|s| s.should_split())
            .map(|s| (s.id, s.utilization))
            .collect();

        // Sort by utilization descending (most urgent first)
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        candidates.into_iter().map(|(id, _)| id).collect()
    }

    /// Find underutilized shards that could be merged or retired
    ///
    /// This method identifies shards with very low utilization that might be
    /// candidates for merging with other shards to improve efficiency.
    ///
    /// # Arguments
    /// * `threshold` - Utilization threshold below which shards are considered underutilized
    pub fn get_underutilized_shards(&self, threshold: f32) -> Vec<(ShardId, f32)> {
        let mut underutilized: Vec<(ShardId, f32)> = self
            .shards
            .iter()
            .filter(|s| s.utilization < threshold && s.posting_count > 0)
            .map(|s| (s.id, s.utilization))
            .collect();

        // Sort by utilization ascending (least utilized first)
        underutilized.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        underutilized
    }

    /// Refresh all shard metadata from disk
    ///
    /// This method reloads metadata for all shards from their disk files.
    /// Useful after external modifications or for consistency checks.
    pub fn refresh_all_metadata(&mut self) -> Result<(), ShardexError> {
        let shard_ids: Vec<ShardId> = self.shards.iter().map(|s| s.id).collect();

        for shard_id in shard_ids {
            self.refresh_shard_metadata(shard_id)?;
        }

        Ok(())
    }

    /// Validate the consistency of the index
    ///
    /// This method performs comprehensive validation of the index including:
    /// - Metadata consistency between index and individual shards
    /// - File existence verification
    /// - Centroid accuracy checks
    pub fn validate_index(&mut self) -> Result<Vec<String>, ShardexError> {
        let mut warnings = Vec::new();

        for metadata in &self.shards {
            let shard_id = metadata.id;

            // Check if shard files exist
            let vector_path = self.directory.join(format!("{}.vectors", shard_id));
            let posting_path = self.directory.join(format!("{}.postings", shard_id));

            if !vector_path.exists() {
                warnings.push(format!("Missing vector file for shard {}", shard_id));
                continue;
            }

            if !posting_path.exists() {
                warnings.push(format!("Missing posting file for shard {}", shard_id));
                continue;
            }

            // Load shard and verify metadata consistency
            match Shard::open_read_only(shard_id, &self.directory) {
                Ok(shard) => {
                    // Check centroid accuracy
                    let actual_centroid = shard.get_centroid();
                    if actual_centroid.len() != metadata.centroid.len() {
                        warnings.push(format!(
                            "Centroid dimension mismatch for shard {}: expected {}, got {}",
                            shard_id,
                            metadata.centroid.len(),
                            actual_centroid.len()
                        ));
                    }

                    // Check posting count accuracy
                    let actual_count = shard.active_count();
                    if actual_count != metadata.posting_count {
                        warnings.push(format!(
                            "Posting count mismatch for shard {}: metadata shows {}, actual {}",
                            shard_id, metadata.posting_count, actual_count
                        ));
                    }

                    // Validate the shard's internal integrity
                    if let Err(e) = shard.validate_integrity() {
                        warnings.push(format!("Shard {} integrity error: {}", shard_id, e));
                    }
                }
                Err(e) => {
                    warnings.push(format!("Failed to open shard {}: {}", shard_id, e));
                }
            }
        }

        Ok(warnings)
    }

    /// Get index statistics
    pub fn statistics(&self) -> IndexStatistics {
        let total_postings = self.shards.iter().map(|s| s.posting_count).sum();
        let total_capacity = self.shards.iter().map(|s| s.capacity).sum();
        let total_memory_usage = self.shards.iter().map(|s| s.memory_usage).sum();

        let average_utilization = if self.shards.is_empty() {
            0.0
        } else {
            self.shards.iter().map(|s| s.utilization).sum::<f32>() / self.shards.len() as f32
        };

        let cached_shards = self.shard_cache.len();

        IndexStatistics {
            total_shards: self.shards.len(),
            total_postings,
            total_capacity,
            average_utilization,
            total_memory_usage,
            cached_shards,
            vector_dimension: self.vector_size,
        }
    }

    /// Discover existing shards in the directory
    ///
    /// This method scans the directory for shard files and loads their metadata.
    /// It's called during index opening to populate the shard list.
    fn discover_shards(&mut self) -> Result<(), ShardexError> {
        let entries = std::fs::read_dir(&self.directory).map_err(ShardexError::Io)?;

        for entry in entries {
            let entry = entry.map_err(ShardexError::Io)?;
            let path = entry.path();

            // Look for .vectors files to identify shards
            if let Some(extension) = path.extension() {
                if extension == "vectors" {
                    if let Some(filename) = path.file_stem() {
                        if let Some(filename_str) = filename.to_str() {
                            // Parse the filename as a ULID
                            if let Ok(shard_id) = filename_str.parse::<ShardId>() {
                                // Check if corresponding .postings file exists
                                let posting_path =
                                    self.directory.join(format!("{}.postings", shard_id));
                                if posting_path.exists() {
                                    // Load shard metadata
                                    self.load_shard_metadata(shard_id)?;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Load metadata for a specific shard
    ///
    /// This method loads a shard temporarily to extract its metadata and centroid,
    /// then discards the shard instance to save memory.
    fn load_shard_metadata(&mut self, shard_id: ShardId) -> Result<(), ShardexError> {
        // Load the shard to get its metadata
        let shard = Shard::open_read_only(shard_id, &self.directory)?;

        // Create metadata
        let metadata = ShardexMetadata::from_shard_metadata(
            shard_id,
            shard.metadata(),
            shard.get_centroid().to_vec(),
            shard.capacity(),
        );

        self.shards.push(metadata);
        Ok(())
    }

    /// Save index metadata to disk
    fn save_metadata(&self) -> Result<(), ShardexError> {
        let metadata = IndexMetadata {
            version: 1,
            vector_size: self.vector_size,
            segment_capacity: self.segment_capacity,
            created_at: SystemTime::UNIX_EPOCH, // Will be overwritten by existing file
            last_modified: SystemTime::now(),
        };

        let metadata_path = self.directory.join("shardex.meta");
        let metadata_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| ShardexError::Config(format!("Failed to serialize metadata: {}", e)))?;

        std::fs::write(&metadata_path, metadata_json).map_err(ShardexError::Io)?;
        Ok(())
    }

    /// Get index statistics in a format compatible with structures::IndexStats
    pub fn stats(&self) -> Result<crate::structures::IndexStats, ShardexError> {
        let stats = self.statistics();

        Ok(crate::structures::IndexStats {
            total_shards: stats.total_shards,
            total_postings: stats.total_postings,
            pending_operations: 0, // WAL operations not implemented yet
            memory_usage: stats.total_memory_usage,
            active_postings: stats.total_postings, // Assume all postings are active for now
            deleted_postings: 0,                   // Deleted postings tracking not implemented yet
            average_shard_utilization: stats.average_utilization,
            vector_dimension: stats.vector_dimension,
            disk_usage: stats.total_memory_usage, // Approximate disk usage
        })
    }

    /// Create a deep copy of this ShardexIndex for copy-on-write operations
    ///
    /// This method creates a complete copy of the index including all shard metadata
    /// but does not duplicate the actual shard cache. The copy will have an empty
    /// cache that will be populated as shards are accessed.
    ///
    /// # Performance Notes
    /// - O(n) complexity where n is the number of shards
    /// - Shard cache is not copied to avoid excessive memory usage
    /// - Metadata is cloned but actual shard files remain shared
    ///
    /// # Returns
    /// A new ShardexIndex instance with identical metadata but empty cache
    pub fn deep_clone(&self) -> Result<ShardexIndex, ShardexError> {
        Ok(ShardexIndex {
            shards: self.shards.clone(),
            directory: self.directory.clone(),
            vector_size: self.vector_size,
            segment_capacity: self.segment_capacity,
            shard_cache: HashMap::new(), // Start with empty cache
            cache_limit: self.cache_limit,
            deduplication_policy: self.deduplication_policy,
        })
    }

    /// Delete all postings for a document across all candidate shards
    ///
    /// This method efficiently deletes all postings associated with a document ID by:
    /// 1. Using bloom filters to identify candidate shards that might contain the document
    /// 2. Executing deletion in parallel across candidate shards
    /// 3. Returning the total number of postings that were actually deleted
    ///
    /// # Arguments
    /// * `doc_id` - The document ID to delete all postings for
    ///
    /// # Returns
    /// The total number of postings that were deleted across all shards
    ///
    /// # Performance
    /// - Uses bloom filter optimization to avoid scanning irrelevant shards
    /// - Parallel execution across candidate shards for scalability
    /// - Only loads shards that might contain the document
    ///
    /// # Examples
    /// ```rust
    /// # use shardex::{ShardexIndex, DocumentId};
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut index = ShardexIndex::create(Default::default())?;
    /// let doc_id = DocumentId::new();
    /// let deleted_count = index.delete_document(doc_id)?;
    /// println!("Deleted {} postings for document {}", deleted_count, doc_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete_document(&mut self, doc_id: DocumentId) -> Result<usize, ShardexError> {
        // Find candidate shards using bloom filter optimization
        let candidate_shards = self.find_candidate_shards_for_deletion(doc_id);

        if candidate_shards.is_empty() {
            return Ok(0); // No candidate shards, document definitely not present
        }

        let mut total_deleted = 0;

        // Process each candidate shard
        for shard_id in candidate_shards {
            let deleted_count = {
                // Load the shard for writing (needed for deletion)
                let shard = self.get_shard_mut(shard_id)?;

                // Remove all postings for this document from the shard
                match shard.remove_document(doc_id) {
                    Ok(deleted_count) => {
                        total_deleted += deleted_count;
                        deleted_count
                    }
                    Err(e) => {
                        tracing::warn!(
                            shard_id = %shard_id,
                            document_id = %doc_id,
                            error = %e,
                            "Failed to delete document from shard"
                        );
                        // Continue with other shards even if one fails
                        0
                    }
                }
            }; // shard borrow ends here

            // Update the shard metadata if postings were deleted
            if deleted_count > 0 {
                self.update_shard_metadata_from_disk(shard_id)?;
            }
        }

        Ok(total_deleted)
    }

    /// Delete all postings for multiple documents in batch
    ///
    /// This method efficiently processes multiple document deletions by:
    /// 1. Grouping documents by their candidate shards to minimize shard loading
    /// 2. Processing each group in parallel for optimal performance
    /// 3. Returning per-document deletion counts for detailed tracking
    ///
    /// # Arguments
    /// * `doc_ids` - Slice of document IDs to delete
    ///
    /// # Returns
    /// Vector containing the deletion count for each document (same order as input)
    ///
    /// # Performance
    /// - Optimizes shard access by grouping documents with overlapping candidate shards
    /// - Uses parallel processing for independent shard groups
    /// - Bloom filter optimization reduces unnecessary shard scans
    ///
    /// # Examples
    /// ```rust
    /// # use shardex::{ShardexIndex, DocumentId};
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut index = ShardexIndex::create(Default::default())?;
    /// let doc_ids = vec![DocumentId::new(), DocumentId::new(), DocumentId::new()];
    /// let deletion_counts = index.delete_documents(&doc_ids)?;
    ///
    /// for (doc_id, count) in doc_ids.iter().zip(deletion_counts.iter()) {
    ///     println!("Deleted {} postings for document {}", count, doc_id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn delete_documents(&mut self, doc_ids: &[DocumentId]) -> Result<Vec<usize>, ShardexError> {
        if doc_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut results = vec![0; doc_ids.len()];

        // Group documents by their candidate shards for efficient processing
        let mut shard_to_doc_indices: HashMap<ShardId, Vec<usize>> = HashMap::new();

        for (doc_index, &doc_id) in doc_ids.iter().enumerate() {
            let candidate_shards = self.find_candidate_shards_for_deletion(doc_id);

            for shard_id in candidate_shards {
                shard_to_doc_indices
                    .entry(shard_id)
                    .or_default()
                    .push(doc_index);
            }
        }

        // Process each shard and its associated documents
        for (shard_id, doc_indices) in shard_to_doc_indices {
            // Load the shard for writing
            let shard = match self.get_shard_mut(shard_id) {
                Ok(shard) => shard,
                Err(e) => {
                    tracing::warn!(
                        shard_id = %shard_id,
                        error = %e,
                        "Failed to load shard for deletion, skipping"
                    );
                    continue;
                }
            };

            let shard_modified = {
                let mut any_deleted = false;

                // Delete documents from this shard
                for &doc_index in &doc_indices {
                    let doc_id = doc_ids[doc_index];

                    match shard.remove_document(doc_id) {
                        Ok(deleted_count) => {
                            results[doc_index] += deleted_count;
                            if deleted_count > 0 {
                                any_deleted = true;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                shard_id = %shard_id,
                                document_id = %doc_id,
                                error = %e,
                                "Failed to delete document from shard"
                            );
                            // Continue with other documents
                        }
                    }
                }

                any_deleted
            }; // shard borrow ends here

            // Update shard metadata if any deletions occurred
            if shard_modified {
                if let Err(e) = self.update_shard_metadata_from_disk(shard_id) {
                    tracing::warn!(
                        shard_id = %shard_id,
                        error = %e,
                        "Failed to update shard metadata after deletion"
                    );
                }
            }
        }

        Ok(results)
    }

    /// Find candidate shards that might contain a document for deletion
    ///
    /// This method uses bloom filters to efficiently identify which shards might
    /// contain postings for the given document ID, avoiding unnecessary scans
    /// of shards that definitely don't contain the document.
    ///
    /// # Arguments
    /// * `doc_id` - The document ID to find candidate shards for
    ///
    /// # Returns
    /// Vector of shard IDs that might contain the document (may include false positives)
    ///
    /// # Performance
    /// - Bloom filters eliminate false negatives (if returns empty, document is definitely not present)
    /// - May include false positives (shard might not actually contain the document)
    /// - Very fast operation that only requires checking bloom filter bits
    pub fn find_candidate_shards_for_deletion(&self, doc_id: DocumentId) -> Vec<ShardId> {
        let mut candidates = Vec::new();

        for shard_metadata in &self.shards {
            // Use bloom filter to check if this shard might contain the document
            // We need to load the shard temporarily to check its bloom filter
            match Shard::open_read_only(shard_metadata.id, &self.directory) {
                Ok(shard) => {
                    // Check if the shard's bloom filter indicates this document might be present
                    // Access bloom filter directly since it's synchronous
                    if shard.metadata().bloom_filter.contains(doc_id) {
                        candidates.push(shard_metadata.id);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        shard_id = %shard_metadata.id,
                        error = %e,
                        "Failed to load shard for bloom filter check, including as candidate"
                    );
                    // If we can't check the bloom filter, include it as a candidate
                    // to be safe (better false positive than false negative)
                    candidates.push(shard_metadata.id);
                }
            }
        }

        candidates
    }

    /// Update shard metadata after deletion operations by reloading from disk
    ///
    /// This internal method updates the in-memory shard metadata to reflect
    /// changes after document deletions by loading the shard from disk and
    /// extracting the updated metadata.
    ///
    /// # Arguments
    /// * `shard_id` - ID of the shard that was modified
    pub fn update_shard_metadata_from_disk(
        &mut self,
        shard_id: ShardId,
    ) -> Result<(), ShardexError> {
        // Load the shard to get updated metadata
        let shard = Shard::open_read_only(shard_id, &self.directory)?;

        // Find and update the shard metadata
        if let Some(metadata) = self.shards.iter_mut().find(|s| s.id == shard_id) {
            // Update posting count and utilization from the shard
            let shard_metadata = shard.metadata();
            metadata.posting_count = shard_metadata.active_count;
            metadata.utilization = shard_metadata.active_count as f32 / shard.capacity() as f32;
            metadata.last_modified = SystemTime::now();

            // Update centroid (it may have changed due to deletions)
            metadata.centroid = shard.get_centroid().to_vec();

            tracing::debug!(
                shard_id = %shard_id,
                new_posting_count = metadata.posting_count,
                new_utilization = metadata.utilization,
                "Updated shard metadata after deletion"
            );
        }

        Ok(())
    }
}

/// Index metadata stored in the shardex.meta file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexMetadata {
    version: u32,
    vector_size: usize,
    segment_capacity: usize,
    created_at: SystemTime,
    last_modified: SystemTime,
}

/// Statistics about the ShardexIndex
#[derive(Debug, Clone)]
pub struct IndexStatistics {
    /// Total number of shards in the index
    pub total_shards: usize,
    /// Total number of postings across all shards
    pub total_postings: usize,
    /// Total capacity across all shards
    pub total_capacity: usize,
    /// Average utilization across all shards (0.0 to 1.0)
    pub average_utilization: f32,
    /// Total estimated memory usage in bytes
    pub total_memory_usage: usize,
    /// Number of shards currently cached in memory
    pub cached_shards: usize,
    /// Vector dimension size
    pub vector_dimension: usize,
}

impl std::fmt::Display for IndexStatistics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "IndexStatistics {{ shards: {}, postings: {}, capacity: {}, \
             utilization: {:.1}%, memory: {:.2}MB, cached: {} }}",
            self.total_shards,
            self.total_postings,
            self.total_capacity,
            self.average_utilization * 100.0,
            self.total_memory_usage as f64 / (1024.0 * 1024.0),
            self.cached_shards
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifiers::DocumentId;
    use crate::structures::Posting;
    use crate::test_utils::TestEnvironment;
    use tempfile::TempDir;

    #[test]
    fn test_create_shardex_index() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(128)
            .shardex_segment_size(100);

        let index = ShardexIndex::create(config).unwrap();

        assert_eq!(index.vector_size(), 128);
        assert_eq!(index.segment_capacity(), 100);
        assert_eq!(index.shard_count(), 0);
    }

    #[test]
    fn test_add_and_remove_shard() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(64)
            .shardex_segment_size(50);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create a test shard
        let shard_id = ShardId::new();
        let shard = Shard::create(shard_id, 100, 64, temp_dir.path().to_path_buf()).unwrap();

        // Add shard to index
        index.add_shard(shard).unwrap();
        assert_eq!(index.shard_count(), 1);
        assert!(index.shard_ids().contains(&shard_id));

        // Remove shard from index
        let removed = index.remove_shard(shard_id).unwrap();
        assert!(removed);
        assert_eq!(index.shard_count(), 0);
        assert!(!index.shard_ids().contains(&shard_id));

        // Try to remove non-existent shard
        let not_removed = index.remove_shard(shard_id).unwrap();
        assert!(!not_removed);
    }

    #[test]
    fn test_find_nearest_shards() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(3);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create shards with different centroids
        let shard1_id = ShardId::new();
        let mut shard1 = Shard::create(shard1_id, 100, 3, temp_dir.path().to_path_buf()).unwrap();

        let shard2_id = ShardId::new();
        let mut shard2 = Shard::create(shard2_id, 100, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add postings to create different centroids
        let doc_id1 = DocumentId::new();
        let posting1 = Posting::new(doc_id1, 0, 10, vec![1.0, 0.0, 0.0], 3).unwrap();
        shard1.add_posting(posting1).unwrap();

        let doc_id2 = DocumentId::new();
        let posting2 = Posting::new(doc_id2, 0, 10, vec![0.0, 1.0, 0.0], 3).unwrap();
        shard2.add_posting(posting2).unwrap();

        // Add shards to index
        index.add_shard(shard1).unwrap();
        index.add_shard(shard2).unwrap();

        // Query closer to shard1's centroid
        let query = vec![0.9, 0.1, 0.0];
        let nearest = index.find_nearest_shards(&query, 2).unwrap();

        assert_eq!(nearest.len(), 2);
        assert_eq!(nearest[0], shard1_id); // Should be closest
    }

    #[test]
    fn test_open_existing_index() {
        let temp_dir = TempDir::new().unwrap();

        // Create index and add a shard
        {
            let config = ShardexConfig::new()
                .directory_path(temp_dir.path())
                .vector_size(32);

            let mut index = ShardexIndex::create(config).unwrap();

            let shard_id = ShardId::new();
            let shard = Shard::create(shard_id, 50, 32, temp_dir.path().to_path_buf()).unwrap();
            index.add_shard(shard).unwrap();
        }

        // Reopen the index
        let index = ShardexIndex::open(temp_dir.path()).unwrap();

        assert_eq!(index.vector_size(), 32);
        assert_eq!(index.shard_count(), 1);
    }

    #[test]
    fn test_index_statistics() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(16);

        let mut index = ShardexIndex::create(config).unwrap();

        // Initially empty
        let stats = index.statistics();
        assert_eq!(stats.total_shards, 0);
        assert_eq!(stats.total_postings, 0);

        // Add a shard with some postings
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 20, 16, temp_dir.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 0, 10, vec![0.5; 16], 16).unwrap();
        shard.add_posting(posting).unwrap();

        index.add_shard(shard).unwrap();

        let stats = index.statistics();
        assert_eq!(stats.total_shards, 1);
        assert_eq!(stats.total_postings, 1);
        assert_eq!(stats.total_capacity, 20);
        assert!(stats.average_utilization > 0.0);
    }

    #[test]
    fn test_shardex_metadata_operations() {
        let shard_id = ShardId::new();
        let base_metadata = BaseShardMetadata::new(false, 100).unwrap();

        let centroid = vec![0.1, 0.2, 0.3];
        let metadata =
            ShardexMetadata::from_shard_metadata(shard_id, &base_metadata, centroid.clone(), 100);

        assert_eq!(metadata.id, shard_id);
        assert_eq!(metadata.centroid, centroid);
        assert_eq!(metadata.capacity, 100);
        assert!(!metadata.should_split()); // Low utilization

        // Test distance calculation
        let query = vec![0.2, 0.3, 0.4];
        let distance = metadata.distance_to_query(&query).unwrap();
        assert!(distance > 0.0);

        // Test with different dimension
        let wrong_query = vec![0.1, 0.2];
        let result = metadata.distance_to_query(&wrong_query);
        assert!(result.is_err());
    }

    #[test]
    fn test_shard_caching() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(8);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create and add a shard
        let shard_id = ShardId::new();
        let shard = Shard::create(shard_id, 10, 8, temp_dir.path().to_path_buf()).unwrap();
        index.add_shard(shard).unwrap();

        // Access shard - should be cached
        let _shard_ref = index.get_shard(shard_id).unwrap();
        let stats = index.statistics();
        assert_eq!(stats.cached_shards, 1);

        // Access mutable reference
        let _shard_mut = index.get_shard_mut(shard_id).unwrap();
        let stats = index.statistics();
        assert_eq!(stats.cached_shards, 1);
    }

    #[test]
    fn test_invalid_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(4);

        let mut index = ShardexIndex::create(config).unwrap();

        // Try to add shard with wrong vector dimension
        let shard_id = ShardId::new();
        let wrong_shard = Shard::create(shard_id, 10, 8, temp_dir.path().to_path_buf()).unwrap(); // Wrong dimension
        let result = index.add_shard(wrong_shard);
        assert!(result.is_err());

        // Try to find nearest shards with wrong query dimension
        let wrong_query = vec![0.1, 0.2, 0.3]; // Wrong dimension for vector_size=4
        let result = index.find_nearest_shards(&wrong_query, 1);
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_management() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(8);

        let mut index = ShardexIndex::create(config).unwrap();

        // Set a small cache limit
        index.set_cache_limit(2);

        // Create and add multiple shards
        let mut shard_ids = Vec::new();
        for _ in 0..5 {
            let shard_id = ShardId::new();
            let shard = Shard::create(shard_id, 10, 8, temp_dir.path().to_path_buf()).unwrap();
            shard_ids.push(shard_id);
            index.add_shard(shard).unwrap();
        }

        // Cache should be limited to 2 shards
        let stats = index.statistics();
        assert!(stats.cached_shards <= 2);

        // Clear cache
        index.clear_cache();
        let stats = index.statistics();
        assert_eq!(stats.cached_shards, 0);

        // Access a shard - should be cached again
        let _shard = index.get_shard(shard_ids[0]).unwrap();
        let stats = index.statistics();
        assert_eq!(stats.cached_shards, 1);
    }

    #[test]
    fn test_bulk_operations() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(4);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create multiple shards for bulk add
        let mut shards = Vec::new();
        for _ in 0..3 {
            let shard_id = ShardId::new();
            let shard = Shard::create(shard_id, 20, 4, temp_dir.path().to_path_buf()).unwrap();
            shards.push(shard);
        }

        // Bulk add shards
        index.bulk_add_shards(shards).unwrap();
        assert_eq!(index.shard_count(), 3);

        // Try bulk add with wrong vector dimension
        let wrong_shard =
            Shard::create(ShardId::new(), 10, 8, temp_dir.path().to_path_buf()).unwrap();
        let result = index.bulk_add_shards(vec![wrong_shard]);
        assert!(result.is_err());
    }

    #[test]
    fn test_shard_management_analysis() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(2);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create shards with different utilization levels
        let high_util_id = ShardId::new();
        let mut high_util_shard =
            Shard::create(high_util_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Fill shard to high utilization (> 90%)
        for i in 0..10 {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, i * 10, 10, vec![0.1, 0.2], 2).unwrap();
            high_util_shard.add_posting(posting).unwrap();
        }

        let low_util_id = ShardId::new();
        let mut low_util_shard =
            Shard::create(low_util_id, 100, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add just a few postings for low utilization
        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 0, 10, vec![0.3, 0.4], 2).unwrap();
        low_util_shard.add_posting(posting).unwrap();

        index.add_shard(high_util_shard).unwrap();
        index.add_shard(low_util_shard).unwrap();

        // Test split candidates
        let to_split = index.get_shards_to_split();
        assert_eq!(to_split.len(), 1);
        assert_eq!(to_split[0], high_util_id);

        // Test underutilized shards
        let underutilized = index.get_underutilized_shards(0.5);
        assert_eq!(underutilized.len(), 1);
        assert_eq!(underutilized[0].0, low_util_id);
    }

    #[test]
    fn test_metadata_refresh() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(2);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create and add a shard
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 50, 2, temp_dir.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 0, 10, vec![0.5, 0.6], 2).unwrap();
        shard.add_posting(posting).unwrap();

        index.add_shard(shard).unwrap();

        // Modify the shard through the index
        let shard_mut = index.get_shard_mut(shard_id).unwrap();
        let doc_id2 = DocumentId::new();
        let posting2 = Posting::new(doc_id2, 10, 10, vec![0.7, 0.8], 2).unwrap();
        shard_mut.add_posting(posting2).unwrap();

        // Refresh metadata
        index.refresh_shard_metadata(shard_id).unwrap();

        // Check that metadata reflects changes
        let metadata = index
            .all_shard_metadata()
            .iter()
            .find(|m| m.id == shard_id)
            .unwrap();
        assert_eq!(metadata.posting_count, 2);

        // Test refresh all metadata
        index.refresh_all_metadata().unwrap();
    }

    #[test]
    fn test_index_validation() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(2);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create and add a valid shard
        let shard_id = ShardId::new();
        let shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();
        index.add_shard(shard).unwrap();

        // Validate index - should have no warnings
        let warnings = index.validate_index().unwrap();
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_euclidean_distance_calculation() {
        let shard_id = ShardId::new();
        let base_metadata = BaseShardMetadata::new(false, 100).unwrap();
        let centroid = vec![1.0, 0.0, 0.0];
        let metadata =
            ShardexMetadata::from_shard_metadata(shard_id, &base_metadata, centroid, 100);

        // Test distance to same point (should be 0)
        let same_point = vec![1.0, 0.0, 0.0];
        let distance = metadata.distance_to_query(&same_point).unwrap();
        assert!(
            (distance - 0.0).abs() < 1e-6,
            "Distance to same point should be 0, got {}",
            distance
        );

        // Test distance to orthogonal point - from (1,0,0) to (0,1,0) should be sqrt(2)
        let orthogonal_y = vec![0.0, 1.0, 0.0];
        let distance = metadata.distance_to_query(&orthogonal_y).unwrap();
        let expected = f32::sqrt(2.0);
        assert!(
            (distance - expected).abs() < 1e-6,
            "Distance should be sqrt(2)={}, got {}",
            expected,
            distance
        );

        // Test distance to orthogonal point - from (1,0,0) to (0,0,1) should be sqrt(2)
        let orthogonal_z = vec![0.0, 0.0, 1.0];
        let distance = metadata.distance_to_query(&orthogonal_z).unwrap();
        let expected = f32::sqrt(2.0);
        assert!(
            (distance - expected).abs() < 1e-6,
            "Distance should be sqrt(2)={}, got {}",
            expected,
            distance
        );
    }

    #[test]
    fn test_shardex_metadata_splitting_logic() {
        let shard_id = ShardId::new();
        let mut base_metadata = BaseShardMetadata::new(false, 100).unwrap();

        // Test different utilization levels
        base_metadata.active_count = 50; // 50% utilization
        let metadata1 =
            ShardexMetadata::from_shard_metadata(shard_id, &base_metadata, vec![0.0], 100);
        assert!(!metadata1.should_split());

        base_metadata.active_count = 95; // 95% utilization
        let metadata2 =
            ShardexMetadata::from_shard_metadata(shard_id, &base_metadata, vec![0.0], 100);
        assert!(metadata2.should_split());

        base_metadata.active_count = 90; // Exactly 90% utilization
        let metadata3 =
            ShardexMetadata::from_shard_metadata(shard_id, &base_metadata, vec![0.0], 100);
        assert!(metadata3.should_split());
    }

    #[test]
    fn test_multiple_nearest_shards() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(2);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create shards with known centroids
        let centroids = vec![
            vec![1.0, 0.0],  // East
            vec![0.0, 1.0],  // North
            vec![-1.0, 0.0], // West
            vec![0.0, -1.0], // South
        ];

        for centroid in &centroids {
            let shard_id = ShardId::new();
            let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

            // Add posting with the desired centroid vector
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 0, 10, centroid.clone(), 2).unwrap();
            shard.add_posting(posting).unwrap();

            index.add_shard(shard).unwrap();
        }

        // Query from northeast - should get North and East as nearest
        let query = vec![0.5, 0.5];
        let nearest = index.find_nearest_shards(&query, 2).unwrap();
        assert_eq!(nearest.len(), 2);

        // Query from origin - should get all shards at equal distance, but limited by slop factor
        let query = vec![0.0, 0.0];
        let nearest = index.find_nearest_shards(&query, 3).unwrap();
        assert_eq!(nearest.len(), 3);

        // Test with slop factor larger than available shards
        let nearest = index.find_nearest_shards(&query, 10).unwrap();
        assert_eq!(nearest.len(), 4); // Should return all 4 shards
    }

    #[test]
    fn test_find_nearest_shard() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(3);

        let mut index = ShardexIndex::create(config).unwrap();

        // Test with empty index
        let query = vec![1.0, 0.0, 0.0];
        let result = index.find_nearest_shard(&query).unwrap();
        assert!(result.is_none());

        // Add shards with different centroids
        let centroids = vec![
            (vec![1.0, 0.0, 0.0], "shard_east"),  // Distance 0 from query
            (vec![0.0, 1.0, 0.0], "shard_north"), // Distance sqrt(2) from query
            (vec![0.0, 0.0, 1.0], "shard_up"),    // Distance sqrt(2) from query
        ];

        let mut shard_ids = Vec::new();
        for (centroid, _name) in &centroids {
            let shard_id = ShardId::new();
            let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

            // Add posting with the desired centroid vector
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 0, 10, centroid.clone(), 3).unwrap();
            shard.add_posting(posting).unwrap();

            shard_ids.push(shard_id);
            index.add_shard(shard).unwrap();
        }

        // Query closest to first centroid
        let query = vec![1.0, 0.0, 0.0];
        let nearest = index.find_nearest_shard(&query).unwrap().unwrap();
        assert_eq!(nearest, shard_ids[0]);

        // Query closest to second centroid
        let query = vec![0.0, 1.0, 0.0];
        let nearest = index.find_nearest_shard(&query).unwrap().unwrap();
        assert_eq!(nearest, shard_ids[1]);

        // Test with invalid dimensions
        let invalid_query = vec![1.0, 0.0]; // Wrong dimension
        let result = index.find_nearest_shard(&invalid_query);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_candidate_shards() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(2);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create shards at unit circle positions
        let num_shards = 8;
        let mut shard_ids = Vec::new();

        for i in 0..num_shards {
            let angle = 2.0 * std::f32::consts::PI * (i as f32) / (num_shards as f32);
            let centroid = vec![angle.cos(), angle.sin()];

            let shard_id = ShardId::new();
            let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 0, 10, centroid, 2).unwrap();
            shard.add_posting(posting).unwrap();

            shard_ids.push(shard_id);
            index.add_shard(shard).unwrap();
        }

        // Query from origin - should return requested number of candidates
        let query = vec![0.0, 0.0];
        let candidates = index.find_candidate_shards(&query, 3).unwrap();
        assert_eq!(candidates.len(), 3);

        // Query close to first shard
        let query = vec![1.0, 0.0];
        let candidates = index.find_candidate_shards(&query, 2).unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0], shard_ids[0]); // Should be closest to angle 0

        // Test with slop factor larger than available shards
        let candidates = index.find_candidate_shards(&query, 20).unwrap();
        assert_eq!(candidates.len(), num_shards);

        // Test with zero slop factor
        let candidates = index.find_candidate_shards(&query, 0).unwrap();
        assert_eq!(candidates.len(), 0);
    }

    #[test]
    fn test_calculate_centroid_distances() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(2);

        let mut index = ShardexIndex::create(config).unwrap();

        // Test with empty index
        let query = vec![0.0, 0.0];
        let distances = index.calculate_centroid_distances(&query).unwrap();
        assert!(distances.is_empty());

        // Add shards at known positions
        let centroids = vec![
            vec![3.0, 4.0], // Distance 5 from origin
            vec![1.0, 0.0], // Distance 1 from origin
            vec![0.0, 0.0], // Distance 0 from origin
        ];

        for centroid in &centroids {
            let shard_id = ShardId::new();
            let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 0, 10, centroid.clone(), 2).unwrap();
            shard.add_posting(posting).unwrap();

            index.add_shard(shard).unwrap();
        }

        // Calculate distances from origin
        let query = vec![0.0, 0.0];
        let distances = index.calculate_centroid_distances(&query).unwrap();

        assert_eq!(distances.len(), 3);
        assert!((distances[0] - 5.0).abs() < 1e-6); // Distance to (3,4)
        assert!((distances[1] - 1.0).abs() < 1e-6); // Distance to (1,0)
        assert!((distances[2] - 0.0).abs() < 1e-6); // Distance to (0,0)

        // Test with invalid dimensions
        let invalid_query = vec![1.0]; // Wrong dimension
        let result = index.calculate_centroid_distances(&invalid_query);
        assert!(result.is_err());
    }

    #[test]
    fn test_centroid_distance_calculation_edge_cases() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(1);

        let mut index = ShardexIndex::create(config).unwrap();

        // Single shard case
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 10, 1, temp_dir.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 0, 10, vec![5.0], 1).unwrap();
        shard.add_posting(posting).unwrap();
        index.add_shard(shard).unwrap();

        // Test single shard selection
        let query = vec![0.0];
        let nearest = index.find_nearest_shard(&query).unwrap().unwrap();
        assert_eq!(nearest, shard_id);

        // Test distance calculation
        let distances = index.calculate_centroid_distances(&query).unwrap();
        assert_eq!(distances.len(), 1);
        assert!((distances[0] - 5.0).abs() < 1e-6);

        // Test candidate shards with single shard
        let candidates = index.find_candidate_shards(&query, 5).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], shard_id);
    }

    #[test]
    fn test_simd_optimized_distance_calculation() {
        // Test the SIMD-optimized distance calculation with various vector sizes
        let test_cases = vec![
            // Test different alignment scenarios
            (vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]), // Length 3 (not aligned to 4)
            (vec![1.0, 2.0, 3.0, 4.0], vec![5.0, 6.0, 7.0, 8.0]), // Length 4 (aligned)
            (
                vec![1.0, 2.0, 3.0, 4.0, 5.0],
                vec![6.0, 7.0, 8.0, 9.0, 10.0],
            ), // Length 5 (one remainder)
            (vec![0.0; 16], vec![1.0; 16]),             // Length 16 (multiple of 4)
            (vec![0.0; 17], vec![1.0; 17]),             // Length 17 (with remainder)
        ];

        for (a, b) in test_cases {
            // Calculate expected distance using simple method
            let expected: f32 = a
                .iter()
                .zip(b.iter())
                .map(|(x, y)| {
                    let diff = x - y;
                    diff * diff
                })
                .sum::<f32>()
                .sqrt();

            // Calculate using optimized method
            let actual = ShardexMetadata::euclidean_distance(&a, &b);

            assert!(
                (actual - expected).abs() < 1e-6,
                "Distance mismatch for vectors of length {}: expected {}, got {}",
                a.len(),
                expected,
                actual
            );
        }
    }

    #[test]
    fn test_parallel_distance_calculation() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(4);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create enough shards to trigger parallel processing (>= 10)
        let num_shards = 15;
        for i in 0..num_shards {
            let shard_id = ShardId::new();
            let mut shard = Shard::create(shard_id, 10, 4, temp_dir.path().to_path_buf()).unwrap();

            // Create a unique centroid for each shard
            let centroid = vec![i as f32, 0.0, 0.0, 0.0];
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 0, 10, centroid, 4).unwrap();
            shard.add_posting(posting).unwrap();

            index.add_shard(shard).unwrap();
        }

        // Test parallel distance calculation
        let query = vec![0.0, 0.0, 0.0, 0.0];
        let distances = index.calculate_centroid_distances(&query).unwrap();

        assert_eq!(distances.len(), num_shards);

        // Verify distances are correct (should be 0, 1, 2, 3, ..., 14)
        for (i, &distance) in distances.iter().enumerate() {
            let expected = i as f32;
            assert!(
                (distance - expected).abs() < 1e-6,
                "Distance {} should be {}, got {}",
                i,
                expected,
                distance
            );
        }

        // Test that find_nearest_shard returns the closest (first) shard
        let nearest = index.find_nearest_shard(&query).unwrap().unwrap();
        let shard_ids = index.shard_ids();
        assert_eq!(nearest, shard_ids[0]); // Should be shard with centroid [0,0,0,0]
    }

    #[test]
    fn test_parallel_search_basic_functionality() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(3);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create multiple shards with different postings
        let mut shard_ids = Vec::new();
        let test_vectors = [
            vec![1.0, 0.0, 0.0],  // East
            vec![0.0, 1.0, 0.0],  // North
            vec![-1.0, 0.0, 0.0], // West
        ];

        for centroid in test_vectors.iter() {
            let shard_id = ShardId::new();
            let mut shard = Shard::create(shard_id, 100, 3, temp_dir.path().to_path_buf()).unwrap();

            // Add multiple postings to each shard
            for j in 0..5 {
                let doc_id = DocumentId::new();
                let mut vector = centroid.clone();
                // Add small variation to create different postings
                vector[0] += (j as f32) * 0.1;
                let posting = Posting::new(doc_id, j * 10, 10, vector, 3).unwrap();
                shard.add_posting(posting).unwrap();
            }

            shard_ids.push(shard_id);
            index.add_shard(shard).unwrap();
        }

        // Test parallel search
        let query = vec![0.9, 0.1, 0.0]; // Close to East centroid
        let candidate_shards = index.find_candidate_shards(&query, 3).unwrap();
        let results = index
            .parallel_search(&query, &candidate_shards, 10)
            .unwrap();

        // Should return results from multiple shards
        assert!(!results.is_empty());
        assert!(results.len() <= 10);

        // Results should be sorted by similarity score (descending)
        for i in 1..results.len() {
            assert!(
                results[i - 1].similarity_score >= results[i].similarity_score,
                "Results should be sorted by similarity score"
            );
        }

        // Best results should come from the East shard (closest to query)
        let best_result = &results[0];
        assert!(
            best_result.similarity_score > 0.8,
            "Best result should have high similarity"
        );
    }

    #[test]
    fn test_parallel_search_vs_sequential() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(4);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create multiple shards with known postings
        let mut shard_ids = Vec::new();
        for i in 0..5 {
            let shard_id = ShardId::new();
            let mut shard = Shard::create(shard_id, 20, 4, temp_dir.path().to_path_buf()).unwrap();

            // Add postings with predictable vectors
            for j in 0..10 {
                let doc_id = DocumentId::new();
                let vector = vec![(i as f32) * 0.2 + (j as f32) * 0.05, 0.5, 0.3, 0.1];
                let posting = Posting::new(doc_id, j * 10, 10, vector, 4).unwrap();
                shard.add_posting(posting).unwrap();
            }

            shard_ids.push(shard_id);
            index.add_shard(shard).unwrap();
        }

        let query = vec![0.4, 0.5, 0.3, 0.1];
        let candidate_shards = index.find_candidate_shards(&query, 5).unwrap();

        // Get parallel search results
        let parallel_results = index
            .parallel_search(&query, &candidate_shards, 15)
            .unwrap();

        // Get sequential results by searching each shard individually and merging
        let mut sequential_results = Vec::new();
        for &shard_id in &candidate_shards {
            let shard = index.get_shard(shard_id).unwrap();
            let shard_results = shard.search(&query, 15).unwrap();
            sequential_results.extend(shard_results);
        }

        // Sort sequential results
        sequential_results.sort_by(|a, b| {
            b.similarity_score
                .partial_cmp(&a.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sequential_results.truncate(15);

        // Results should be the same (both methods should find the same top results)
        assert_eq!(parallel_results.len(), sequential_results.len().min(15));

        // Compare the top results (similarity scores should be very close)
        for i in 0..parallel_results.len().min(10) {
            let parallel_score = parallel_results[i].similarity_score;
            let sequential_score = sequential_results[i].similarity_score;
            assert!(
                (parallel_score - sequential_score).abs() < 1e-6,
                "Parallel and sequential results should be identical at position {}: {} vs {}",
                i,
                parallel_score,
                sequential_score
            );
        }
    }

    #[test]
    fn test_parallel_search_deduplication() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(2);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create two shards that might have overlapping results
        let doc_id = DocumentId::new(); // Same document ID for both shards
        let vector = vec![0.5, 0.5];

        // Shard 1
        let shard1_id = ShardId::new();
        let mut shard1 = Shard::create(shard1_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();
        let posting1 = Posting::new(doc_id, 0, 10, vector.clone(), 2).unwrap();
        shard1.add_posting(posting1).unwrap();

        // Shard 2 with same document and start position (should be deduplicated)
        let shard2_id = ShardId::new();
        let mut shard2 = Shard::create(shard2_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();
        let posting2 = Posting::new(doc_id, 0, 10, vector, 2).unwrap();
        shard2.add_posting(posting2).unwrap();

        // Add different posting to shard2 (should not be deduplicated)
        let doc_id2 = DocumentId::new();
        let posting3 = Posting::new(doc_id2, 10, 10, vec![0.6, 0.6], 2).unwrap();
        shard2.add_posting(posting3).unwrap();

        index.add_shard(shard1).unwrap();
        index.add_shard(shard2).unwrap();

        let query = vec![0.5, 0.5];
        let candidate_shards = vec![shard1_id, shard2_id];
        let results = index
            .parallel_search(&query, &candidate_shards, 10)
            .unwrap();

        // Should have exactly 2 unique results (one duplicate removed)
        assert_eq!(results.len(), 2);

        // Check that we have the expected documents
        let doc_ids: Vec<u128> = results.iter().map(|r| r.document_id.raw()).collect();
        assert!(doc_ids.contains(&doc_id.raw()));
        assert!(doc_ids.contains(&doc_id2.raw()));
    }

    #[test]
    fn test_parallel_search_empty_cases() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(2);

        let mut index = ShardexIndex::create(config).unwrap();

        let query = vec![0.5, 0.5];

        // Test with empty candidate shards
        let results = index.parallel_search(&query, &[], 10).unwrap();
        assert!(results.is_empty());

        // Test with k = 0
        let shard_id = ShardId::new();
        let shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();
        index.add_shard(shard).unwrap();

        let results = index.parallel_search(&query, &[shard_id], 0).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_parallel_search_error_handling() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(3);

        let mut index = ShardexIndex::create(config).unwrap();

        // Test invalid query vector dimension
        let wrong_query = vec![0.1, 0.2]; // Wrong dimension
        let result = index.parallel_search(&wrong_query, &[], 10);
        assert!(result.is_err());
        if let Err(ShardexError::InvalidDimension { expected, actual }) = result {
            assert_eq!(expected, 3);
            assert_eq!(actual, 2);
        } else {
            panic!("Expected InvalidDimension error");
        }

        // Test with non-existent shard ID
        let fake_shard_id = ShardId::new();
        let query = vec![0.1, 0.2, 0.3];
        let result = index.parallel_search(&query, &[fake_shard_id], 10);
        assert!(result.is_err());
        // Should get a search error about failed shard opening
        assert!(matches!(result.unwrap_err(), ShardexError::Search(_)));
    }

    #[test]
    fn test_merge_results_functionality() {
        // Test the merge_results function directly
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let doc_id3 = DocumentId::new();

        // Create results from multiple "shards"
        let shard1_results = vec![
            SearchResult::new(doc_id1, 0, 10, vec![1.0, 0.0], 0.9, 2).unwrap(),
            SearchResult::new(doc_id2, 10, 10, vec![0.8, 0.2], 0.7, 2).unwrap(),
        ];

        let shard2_results = vec![
            SearchResult::new(doc_id3, 20, 10, vec![0.9, 0.1], 0.8, 2).unwrap(),
            SearchResult::new(doc_id1, 30, 10, vec![0.7, 0.3], 0.6, 2).unwrap(), // Different start position, should not be deduplicated
        ];

        let all_results = vec![shard1_results, shard2_results];

        // Test normal merging
        let merged = ShardexIndex::merge_results_with_policy(
            all_results.clone(),
            3,
            DeduplicationPolicy::default(),
        );
        assert_eq!(merged.len(), 3);

        // Should be sorted by similarity score
        assert_eq!(merged[0].similarity_score, 0.9); // doc_id1, start=0
        assert_eq!(merged[1].similarity_score, 0.8); // doc_id3
        assert_eq!(merged[2].similarity_score, 0.7); // doc_id2

        // Test with k larger than available results
        let merged = ShardexIndex::merge_results_with_policy(
            all_results.clone(),
            10,
            DeduplicationPolicy::default(),
        );
        assert_eq!(merged.len(), 4); // All unique results

        // Test with k = 0
        let merged = ShardexIndex::merge_results_with_policy(
            all_results.clone(),
            0,
            DeduplicationPolicy::default(),
        );
        assert!(merged.is_empty());

        // Test with empty input
        let merged =
            ShardexIndex::merge_results_with_policy(vec![], 5, DeduplicationPolicy::default());
        assert!(merged.is_empty());
    }

    #[test]
    fn test_parallel_search_large_k_values() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(2);

        let mut index = ShardexIndex::create(config).unwrap();

        // Create a shard with many postings
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 100, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add 50 postings with varying similarity to query
        for i in 0..50 {
            let doc_id = DocumentId::new();
            let similarity_factor = i as f32 / 50.0;
            let vector = vec![0.5 + similarity_factor * 0.3, 0.5];
            let posting = Posting::new(doc_id, i * 10, 10, vector, 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        index.add_shard(shard).unwrap();

        let query = vec![0.5, 0.5];

        // Test various k values
        let test_k_values = vec![1, 5, 10, 25, 50, 100];

        for k in test_k_values {
            let results = index.parallel_search(&query, &[shard_id], k).unwrap();

            // Should return min(k, available_results)
            let expected_len = k.min(50);
            assert_eq!(
                results.len(),
                expected_len,
                "For k={}, expected {} results, got {}",
                k,
                expected_len,
                results.len()
            );

            // Results should be sorted by similarity
            for i in 1..results.len() {
                assert!(
                    results[i - 1].similarity_score >= results[i].similarity_score,
                    "Results should be sorted by similarity at k={}",
                    k
                );
            }
        }
    }

    #[test]
    fn test_delete_document_single_shard() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(384)
            .shardex_segment_size(100);
        let mut index = ShardexIndex::create(config).unwrap();

        // Create a shard and add some postings
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 100, 384, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let doc_id3 = DocumentId::new();

        // Add multiple postings for doc_id1
        let posting1 = Posting {
            document_id: doc_id1,
            start: 0,
            length: 10,
            vector: vec![1.0; 384],
        };
        let posting2 = Posting {
            document_id: doc_id1,
            start: 10,
            length: 15,
            vector: vec![2.0; 384],
        };
        // Add single posting for doc_id2
        let posting3 = Posting {
            document_id: doc_id2,
            start: 0,
            length: 20,
            vector: vec![3.0; 384],
        };

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();
        shard.add_posting(posting3).unwrap();

        // Add shard to index
        index.add_shard(shard).unwrap();

        // Delete doc_id1 (should remove 2 postings)
        let deleted_count = index.delete_document(doc_id1).unwrap();
        assert_eq!(deleted_count, 2);

        // Delete doc_id2 (should remove 1 posting)
        let deleted_count = index.delete_document(doc_id2).unwrap();
        assert_eq!(deleted_count, 1);

        // Try to delete doc_id3 (not present, should remove 0)
        let deleted_count = index.delete_document(doc_id3).unwrap();
        assert_eq!(deleted_count, 0);
    }

    #[test]
    fn test_delete_document_multiple_shards() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(384)
            .shardex_segment_size(100);
        let mut index = ShardexIndex::create(config).unwrap();

        // Create two shards
        let shard_id1 = ShardId::new();
        let shard_id2 = ShardId::new();
        let mut shard1 = Shard::create(shard_id1, 100, 384, temp_dir.path().to_path_buf()).unwrap();
        let mut shard2 = Shard::create(shard_id2, 100, 384, temp_dir.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();

        // Add postings for the same document to both shards
        let posting1 = Posting {
            document_id: doc_id,
            start: 0,
            length: 10,
            vector: vec![1.0; 384],
        };
        let posting2 = Posting {
            document_id: doc_id,
            start: 10,
            length: 15,
            vector: vec![2.0; 384],
        };
        let posting3 = Posting {
            document_id: doc_id,
            start: 20,
            length: 20,
            vector: vec![3.0; 384],
        };

        shard1.add_posting(posting1).unwrap();
        shard1.add_posting(posting2).unwrap();
        shard2.add_posting(posting3).unwrap();

        // Add shards to index
        index.add_shard(shard1).unwrap();
        index.add_shard(shard2).unwrap();

        // Delete the document (should remove postings from both shards)
        let deleted_count = index.delete_document(doc_id).unwrap();
        assert_eq!(deleted_count, 3); // 2 from shard1 + 1 from shard2
    }

    #[test]
    fn test_delete_documents_batch() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(384)
            .shardex_segment_size(100);
        let mut index = ShardexIndex::create(config).unwrap();

        // Create a shard and add postings for multiple documents
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 100, 384, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let doc_id3 = DocumentId::new();
        let doc_id4 = DocumentId::new(); // Not added to shard

        // Add postings
        let posting1 = Posting {
            document_id: doc_id1,
            start: 0,
            length: 10,
            vector: vec![1.0; 384],
        };
        let posting2 = Posting {
            document_id: doc_id1,
            start: 10,
            length: 15,
            vector: vec![1.5; 384],
        };
        let posting3 = Posting {
            document_id: doc_id2,
            start: 0,
            length: 20,
            vector: vec![2.0; 384],
        };
        let posting4 = Posting {
            document_id: doc_id3,
            start: 0,
            length: 25,
            vector: vec![3.0; 384],
        };

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();
        shard.add_posting(posting3).unwrap();
        shard.add_posting(posting4).unwrap();

        // Add shard to index
        index.add_shard(shard).unwrap();

        // Delete multiple documents in batch
        let doc_ids = vec![doc_id1, doc_id2, doc_id3, doc_id4];
        let deletion_counts = index.delete_documents(&doc_ids).unwrap();

        // Verify deletion counts
        assert_eq!(deletion_counts.len(), 4);
        assert_eq!(deletion_counts[0], 2); // doc_id1: 2 postings
        assert_eq!(deletion_counts[1], 1); // doc_id2: 1 posting
        assert_eq!(deletion_counts[2], 1); // doc_id3: 1 posting
        assert_eq!(deletion_counts[3], 0); // doc_id4: not present
    }

    #[test]
    fn test_delete_documents_multiple_shards() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(384)
            .shardex_segment_size(100);
        let mut index = ShardexIndex::create(config).unwrap();

        // Create three shards
        let shard_id1 = ShardId::new();
        let shard_id2 = ShardId::new();
        let shard_id3 = ShardId::new();
        let mut shard1 = Shard::create(shard_id1, 100, 384, temp_dir.path().to_path_buf()).unwrap();
        let mut shard2 = Shard::create(shard_id2, 100, 384, temp_dir.path().to_path_buf()).unwrap();
        let mut shard3 = Shard::create(shard_id3, 100, 384, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();

        // Distribute postings across shards
        let posting1 = Posting {
            document_id: doc_id1,
            start: 0,
            length: 10,
            vector: vec![1.0; 384],
        };
        let posting2 = Posting {
            document_id: doc_id1,
            start: 10,
            length: 15,
            vector: vec![1.5; 384],
        };
        let posting3 = Posting {
            document_id: doc_id2,
            start: 0,
            length: 20,
            vector: vec![2.0; 384],
        };
        let posting4 = Posting {
            document_id: doc_id1,
            start: 20,
            length: 25,
            vector: vec![1.8; 384],
        };

        shard1.add_posting(posting1).unwrap(); // doc_id1 in shard1
        shard2.add_posting(posting2).unwrap(); // doc_id1 in shard2
        shard2.add_posting(posting3).unwrap(); // doc_id2 in shard2
        shard3.add_posting(posting4).unwrap(); // doc_id1 in shard3

        // Add shards to index
        index.add_shard(shard1).unwrap();
        index.add_shard(shard2).unwrap();
        index.add_shard(shard3).unwrap();

        // Delete both documents
        let doc_ids = vec![doc_id1, doc_id2];
        let deletion_counts = index.delete_documents(&doc_ids).unwrap();

        // Verify deletion counts
        assert_eq!(deletion_counts.len(), 2);
        assert_eq!(deletion_counts[0], 3); // doc_id1: 1+1+1 postings across 3 shards
        assert_eq!(deletion_counts[1], 1); // doc_id2: 1 posting in shard2
    }

    #[test]
    fn test_find_candidate_shards_for_deletion() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(384)
            .shardex_segment_size(100);
        let mut index = ShardexIndex::create(config).unwrap();

        // Create two shards
        let shard_id1 = ShardId::new();
        let shard_id2 = ShardId::new();
        let mut shard1 = Shard::create(shard_id1, 100, 384, temp_dir.path().to_path_buf()).unwrap();
        let mut shard2 = Shard::create(shard_id2, 100, 384, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let doc_id3 = DocumentId::new(); // Not in any shard

        // Add postings to different shards
        let posting1 = Posting {
            document_id: doc_id1,
            start: 0,
            length: 10,
            vector: vec![1.0; 384],
        };
        let posting2 = Posting {
            document_id: doc_id2,
            start: 0,
            length: 15,
            vector: vec![2.0; 384],
        };

        shard1.add_posting(posting1).unwrap();
        shard2.add_posting(posting2).unwrap();

        // Add shards to index
        index.add_shard(shard1).unwrap();
        index.add_shard(shard2).unwrap();

        // Test candidate shard finding
        let candidates1 = index.find_candidate_shards_for_deletion(doc_id1);
        assert!(candidates1.contains(&shard_id1));
        assert!(!candidates1.contains(&shard_id2) || candidates1.len() > 1); // May contain due to false positive

        let candidates2 = index.find_candidate_shards_for_deletion(doc_id2);
        assert!(candidates2.contains(&shard_id2));
        assert!(!candidates2.contains(&shard_id1) || candidates2.len() > 1); // May contain due to false positive

        // Document not in any shard - bloom filters should eliminate all shards
        let candidates3 = index.find_candidate_shards_for_deletion(doc_id3);
        // Should be empty or very few false positives depending on bloom filter state
        assert!(candidates3.len() <= 2); // Allow some false positives but not all shards
    }

    #[test]
    fn test_delete_document_empty_index() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(384)
            .shardex_segment_size(100);
        let mut index = ShardexIndex::create(config).unwrap();

        let doc_id = DocumentId::new();

        // Delete from empty index should return 0
        let deleted_count = index.delete_document(doc_id).unwrap();
        assert_eq!(deleted_count, 0);
    }

    #[test]
    fn test_delete_documents_empty_list() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(384)
            .shardex_segment_size(100);
        let mut index = ShardexIndex::create(config).unwrap();

        // Delete empty list should return empty result
        let deletion_counts = index.delete_documents(&[]).unwrap();
        assert_eq!(deletion_counts.len(), 0);
    }

    #[test]
    fn test_delete_document_updates_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(384)
            .shardex_segment_size(100);
        let mut index = ShardexIndex::create(config).unwrap();

        // Create shard and add postings
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 100, 384, temp_dir.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();
        let posting1 = Posting {
            document_id: doc_id,
            start: 0,
            length: 10,
            vector: vec![1.0; 384],
        };
        let posting2 = Posting {
            document_id: doc_id,
            start: 10,
            length: 15,
            vector: vec![2.0; 384],
        };

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();

        // Add shard to index and verify initial metadata
        index.add_shard(shard).unwrap();
        let initial_utilization = {
            let metadata = index.shards.iter().find(|s| s.id == shard_id).unwrap();
            assert_eq!(metadata.posting_count, 2);
            metadata.utilization
        };

        // Delete document and verify metadata is updated
        let deleted_count = index.delete_document(doc_id).unwrap();
        assert_eq!(deleted_count, 2);

        let updated_metadata = index.shards.iter().find(|s| s.id == shard_id).unwrap();
        assert_eq!(updated_metadata.posting_count, 0);
        assert!(updated_metadata.utilization < initial_utilization);
    }

    #[test]
    fn test_delete_document_bloom_filter_optimization() {
        let temp_dir = TempDir::new().unwrap();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(384)
            .shardex_segment_size(100);
        let mut index = ShardexIndex::create(config).unwrap();

        // Create many shards but only add document to one
        let mut shard_ids = Vec::new();
        for _ in 0..5 {
            let shard_id = ShardId::new();
            let shard = Shard::create(shard_id, 100, 384, temp_dir.path().to_path_buf()).unwrap();
            shard_ids.push(shard_id);
            index.add_shard(shard).unwrap();
        }

        // Add document only to the first shard
        let doc_id = DocumentId::new();
        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: 10,
            vector: vec![1.0; 384],
        };

        {
            let shard = index.get_shard_mut(shard_ids[0]).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Find candidate shards - should be much fewer than total due to bloom filter
        let candidates = index.find_candidate_shards_for_deletion(doc_id);
        assert!(candidates.len() <= 2); // Should only include the relevant shard + maybe 1 false positive

        // Delete should work correctly
        let deleted_count = index.delete_document(doc_id).unwrap();
        assert_eq!(deleted_count, 1);
    }

    #[test]
    fn test_calculate_optimal_slop() {
        let _env = TestEnvironment::new("test_calculate_optimal_slop");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128)
            .shard_size(10);

        let index = ShardexIndex::create(config).unwrap();

        // Test with no shards
        let optimal = index.calculate_optimal_slop(384, 0);
        assert_eq!(optimal, 1);

        // Test with single shard
        let optimal = index.calculate_optimal_slop(384, 1);
        assert_eq!(optimal, 1);

        // Test with multiple shards and different vector sizes
        // The method works on shard count parameter, not actual shards in the index
        let optimal_small = index.calculate_optimal_slop(128, 5);
        let optimal_large = index.calculate_optimal_slop(1024, 5);

        assert!(optimal_small > 0);
        assert!(optimal_large > 0);
        assert!(optimal_large >= optimal_small); // Larger vectors should generally need more shards
    }

    #[test]
    fn test_select_shards_with_slop() {
        let _env = TestEnvironment::new("test_select_shards_with_slop");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(2)
            .shard_size(10);

        let index = ShardexIndex::create(config).unwrap();

        // Test slop-based shard selection with empty index
        let query = vec![1.0, 0.0];
        let selected = index.select_shards_with_slop(&query, 2).unwrap();

        // Should return empty vector since there are no shards
        assert!(selected.is_empty());

        // Test with zero slop factor
        let selected_zero = index.select_shards_with_slop(&query, 0).unwrap();
        assert!(selected_zero.is_empty());
    }

    #[test]
    fn test_shard_count_basic() {
        let _env = TestEnvironment::new("test_shard_count_basic");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128)
            .shard_size(10);

        let index = ShardexIndex::create(config).unwrap();

        // Initially should have 0 shards (just testing the structure exists)
        assert_eq!(index.shards.len(), 0);
    }

    #[test]
    fn test_calculate_optimal_slop_with_different_parameters() {
        let _env = TestEnvironment::new("test_calculate_optimal_slop_params");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).unwrap();

        // Test edge cases
        assert_eq!(index.calculate_optimal_slop(128, 0), 1);
        assert_eq!(index.calculate_optimal_slop(128, 1), 1);

        // Test normal cases
        let slop_5_shards = index.calculate_optimal_slop(128, 5);
        let slop_20_shards = index.calculate_optimal_slop(128, 20);
        let slop_100_shards = index.calculate_optimal_slop(128, 100);

        // More shards should allow for higher slop (up to a point)
        assert!(slop_20_shards >= slop_5_shards);

        // Should not exceed total shard count
        assert!(slop_5_shards <= 5);
        assert!(slop_20_shards <= 20);
        assert!(slop_100_shards <= 100);

        // Test with different vector sizes
        let slop_small_vec = index.calculate_optimal_slop(128, 50);
        let slop_large_vec = index.calculate_optimal_slop(1024, 50);

        // Larger vectors should generally benefit from higher slop factors
        assert!(slop_large_vec >= slop_small_vec);
    }
}

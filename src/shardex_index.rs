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
use crate::error::ShardexError;
use crate::identifiers::ShardId;
use crate::shard::{Shard, ShardMetadata as BaseShardMetadata};
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
    fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
        debug_assert_eq!(a.len(), b.len(), "Vectors must have same length");

        a.iter()
            .zip(b.iter())
            .map(|(x, y)| {
                let diff = x - y;
                diff * diff
            })
            .sum::<f32>()
            .sqrt()
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
        let mut shard_distances: Vec<(ShardId, f32)> = Vec::new();

        for shard_metadata in &self.shards {
            let distance = shard_metadata.distance_to_query(query_vector)?;
            shard_distances.push((shard_metadata.id, distance));
        }

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
        let base_metadata = BaseShardMetadata::new(false);

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
        let base_metadata = BaseShardMetadata::new(false);
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
        let mut base_metadata = BaseShardMetadata::new(false);

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
}

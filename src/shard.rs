//! Individual shard creation and management
//!
//! This module provides the core Shard functionality that combines vector and posting
//! storage for complete shard management. Shards are the fundamental storage units
//! in Shardex, each containing both embedding vectors and posting metadata.
//!
//! # Key Components
//!
//! - [`Shard`]: Main shard container combining vector and posting storage
//! - [`ShardMetadata`]: Metadata tracking for shard statistics and health
//! - Atomic creation and loading with integrity validation
//!
//! # Usage Examples
//!
//! ## Creating a New Shard
//!
//! ```rust
//! use shardex::shard::Shard;
//! use shardex::identifiers::ShardId;
//! use tempfile::TempDir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let shard_id = ShardId::new();
//!
//! // Create shard with 384-dimensional vectors and capacity of 1000
//! let shard = Shard::create(
//!     shard_id,
//!     1000,        // capacity
//!     384,         // vector_size
//!     temp_dir.path().to_path_buf()
//! )?;
//!
//! assert_eq!(shard.id(), shard_id);
//! assert_eq!(shard.capacity(), 1000);
//! assert_eq!(shard.vector_size(), 384);
//! # Ok(())
//! # }
//! ```
//!
//! ## Opening an Existing Shard
//!
//! ```rust
//! use shardex::shard::Shard;
//! use shardex::identifiers::ShardId;
//! use std::path::Path;
//!
//! # fn open_example(shard_id: ShardId, directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
//! // Open existing shard from directory
//! let shard = Shard::open(shard_id, directory)?;
//!
//! println!("Opened shard with {} active postings", shard.active_count());
//! # Ok(())
//! # }
//! ```
//!
//! ## Working with Postings
//!
//! ```rust
//! use shardex::shard::Shard;
//! use shardex::structures::Posting;
//! use shardex::identifiers::{ShardId, DocumentId};
//! use tempfile::TempDir;
//!
//! # fn posting_example() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let mut shard = Shard::create(
//!     ShardId::new(),
//!     100,
//!     128,
//!     temp_dir.path().to_path_buf()
//! )?;
//!
//! // Create a posting with vector
//! let document_id = DocumentId::new();
//! let vector = vec![0.5; 128];
//! let posting = Posting::new(document_id, 100, 50, vector, 128)?;
//!
//! // Add posting to shard
//! let index = shard.add_posting(posting)?;
//!
//! // Retrieve posting
//! let retrieved = shard.get_posting(index)?;
//! assert_eq!(retrieved.document_id, document_id);
//! # Ok(())
//! # }
//! ```

use crate::bloom_filter::BloomFilter;
use crate::distance::DistanceMetric;
use crate::error::ShardexError;
use crate::identifiers::{DocumentId, ShardId};
use crate::posting_storage::PostingStorage;
use crate::structures::{Posting, SearchResult};
use crate::vector_storage::VectorStorage;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Metadata for shard tracking and monitoring
#[derive(Debug, Clone, PartialEq)]
pub struct ShardMetadata {
    /// When this shard was created
    pub created_at: SystemTime,
    /// Current total number of postings (including deleted)
    pub current_count: usize,
    /// Number of active (non-deleted) postings
    pub active_count: usize,
    /// Total disk space used by this shard in bytes
    pub disk_usage: usize,
    /// Whether the shard is read-only
    pub read_only: bool,
    /// Bloom filter for efficient document ID lookups
    pub bloom_filter: BloomFilter,
}

/// Individual shard combining vector and posting storage
///
/// A Shard represents a complete storage unit in Shardex, containing both
/// vector embeddings and posting metadata. Shards provide atomic operations
/// and maintain consistency between the two storage components.
pub struct Shard {
    /// Unique identifier for this shard
    id: ShardId,
    /// Vector storage component
    vector_storage: VectorStorage,
    /// Posting storage component
    posting_storage: PostingStorage,
    /// Maximum number of postings this shard can hold
    capacity: usize,
    /// Number of dimensions per vector
    vector_size: usize,
    /// Directory containing the shard files
    directory: PathBuf,
    /// Metadata for monitoring and statistics
    metadata: ShardMetadata,
    /// Centroid vector representing the center of all non-deleted vectors
    centroid: Vec<f32>,
    /// Count of active vectors used for incremental centroid updates
    active_vector_count: usize,
}

impl ShardMetadata {
    /// Create new shard metadata with current timestamp
    pub fn new(read_only: bool, capacity: usize) -> Result<Self, ShardexError> {
        // Create bloom filter with 1% false positive rate
        let bloom_filter = BloomFilter::new(capacity, 0.01)?;

        Ok(Self {
            created_at: SystemTime::now(),
            current_count: 0,
            active_count: 0,
            disk_usage: 0,
            read_only,
            bloom_filter,
        })
    }

    /// Update metadata from storage components
    pub fn update_from_storages(
        &mut self,
        vector_storage: &VectorStorage,
        posting_storage: &PostingStorage,
    ) {
        self.current_count = vector_storage
            .current_count()
            .max(posting_storage.current_count());
        self.active_count = vector_storage
            .active_count()
            .min(posting_storage.active_count());

        // Estimate disk usage (this is a rough estimate)
        let vector_file_size = std::mem::size_of::<f32>()
            * vector_storage.capacity()
            * vector_storage.vector_dimension();
        let posting_file_size = posting_storage.capacity() * (16 + 4 + 4 + 1); // rough estimate
        self.disk_usage = vector_file_size + posting_file_size;
    }

    /// Calculate utilization as a percentage
    pub fn utilization(&self, capacity: usize) -> f32 {
        if capacity == 0 {
            0.0
        } else {
            self.active_count as f32 / capacity as f32
        }
    }

    /// Get age of this shard since creation
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed().unwrap_or_default()
    }
}

impl Shard {
    /// Create a new shard with the specified configuration
    ///
    /// This method atomically creates both vector and posting storage files.
    /// If either creation fails, no files are left behind.
    ///
    /// # Arguments
    /// * `id` - Unique identifier for the shard
    /// * `capacity` - Maximum number of postings this shard can hold
    /// * `vector_size` - Number of dimensions per vector
    /// * `directory` - Directory where shard files will be created
    pub fn create(
        id: ShardId,
        capacity: usize,
        vector_size: usize,
        directory: PathBuf,
    ) -> Result<Self, ShardexError> {
        // Validate parameters
        if capacity == 0 {
            return Err(ShardexError::Config(
                "Shard capacity cannot be zero".to_string(),
            ));
        }
        if vector_size == 0 {
            return Err(ShardexError::Config(
                "Vector size cannot be zero".to_string(),
            ));
        }

        // Ensure directory exists
        std::fs::create_dir_all(&directory).map_err(ShardexError::Io)?;

        // Calculate file paths
        let vector_path = directory.join(format!("{}.vectors", id));
        let posting_path = directory.join(format!("{}.postings", id));

        // Use temporary file names during creation for atomicity
        let temp_vector_path = directory.join(format!("{}.vectors.tmp", id));
        let temp_posting_path = directory.join(format!("{}.postings.tmp", id));

        // Create storages with temporary names
        let _vector_storage = VectorStorage::create(&temp_vector_path, vector_size, capacity)
            .map_err(|e| ShardexError::Shard(format!("Failed to create vector storage: {}", e)))?;

        let _posting_storage =
            PostingStorage::create(&temp_posting_path, capacity).map_err(|e| {
                // Clean up vector storage if posting creation fails
                let _ = std::fs::remove_file(&temp_vector_path);
                ShardexError::Shard(format!("Failed to create posting storage: {}", e))
            })?;

        // Atomically rename temporary files to final names
        std::fs::rename(&temp_vector_path, &vector_path).map_err(|e| {
            // Clean up on failure
            let _ = std::fs::remove_file(&temp_vector_path);
            let _ = std::fs::remove_file(&temp_posting_path);
            ShardexError::Io(e)
        })?;

        std::fs::rename(&temp_posting_path, &posting_path).map_err(|e| {
            // Clean up on failure
            let _ = std::fs::remove_file(&vector_path);
            let _ = std::fs::remove_file(&temp_posting_path);
            ShardexError::Io(e)
        })?;

        // Reopen storages with final file names
        let vector_storage = VectorStorage::open(&vector_path)
            .map_err(|e| ShardexError::Shard(format!("Failed to reopen vector storage: {}", e)))?;

        let posting_storage = PostingStorage::open(&posting_path)
            .map_err(|e| ShardexError::Shard(format!("Failed to reopen posting storage: {}", e)))?;

        // Create metadata
        let mut metadata = ShardMetadata::new(false, capacity)?;
        metadata.update_from_storages(&vector_storage, &posting_storage);

        // Initialize centroid as zero vector
        let centroid = vec![0.0; vector_size];

        Ok(Self {
            id,
            vector_storage,
            posting_storage,
            capacity,
            vector_size,
            directory,
            metadata,
            centroid,
            active_vector_count: 0,
        })
    }

    /// Open an existing shard from the specified directory
    ///
    /// # Arguments
    /// * `id` - Unique identifier for the shard to open
    /// * `directory` - Directory containing the shard files
    pub fn open(id: ShardId, directory: &Path) -> Result<Self, ShardexError> {
        let vector_path = directory.join(format!("{}.vectors", id));
        let posting_path = directory.join(format!("{}.postings", id));

        // Check that both files exist
        if !vector_path.exists() {
            return Err(ShardexError::Shard(format!(
                "Vector storage file not found: {}",
                vector_path.display()
            )));
        }
        if !posting_path.exists() {
            return Err(ShardexError::Shard(format!(
                "Posting storage file not found: {}",
                posting_path.display()
            )));
        }

        // Open storage components
        let vector_storage = VectorStorage::open(&vector_path)
            .map_err(|e| ShardexError::Shard(format!("Failed to open vector storage: {}", e)))?;

        let posting_storage = PostingStorage::open(&posting_path)
            .map_err(|e| ShardexError::Shard(format!("Failed to open posting storage: {}", e)))?;

        // Validate consistency between storages
        let capacity = vector_storage.capacity();
        let vector_size = vector_storage.vector_dimension();

        if posting_storage.capacity() != capacity {
            return Err(ShardexError::Corruption(format!(
                "Capacity mismatch: vector storage has {}, posting storage has {}",
                capacity,
                posting_storage.capacity()
            )));
        }

        // Create metadata
        let mut metadata = ShardMetadata::new(
            vector_storage.is_read_only() || posting_storage.is_read_only(),
            capacity,
        )?;
        metadata.update_from_storages(&vector_storage, &posting_storage);

        // Initialize centroid and calculate from existing data
        let centroid = vec![0.0; vector_size];
        let active_vector_count = vector_storage.active_count();

        let mut shard = Self {
            id,
            vector_storage,
            posting_storage,
            capacity,
            vector_size,
            directory: directory.to_path_buf(),
            metadata,
            centroid,
            active_vector_count,
        };

        // Calculate centroid from existing vectors only if there are vectors
        if active_vector_count > 0 {
            shard.recalculate_centroid();
        }

        // Populate bloom filter from existing postings
        shard.populate_bloom_filter()?;

        Ok(shard)
    }

    /// Open an existing shard in read-only mode
    pub fn open_read_only(id: ShardId, directory: &Path) -> Result<Self, ShardexError> {
        let vector_path = directory.join(format!("{}.vectors", id));
        let posting_path = directory.join(format!("{}.postings", id));

        // Check that both files exist
        if !vector_path.exists() {
            return Err(ShardexError::Shard(format!(
                "Vector storage file not found: {}",
                vector_path.display()
            )));
        }
        if !posting_path.exists() {
            return Err(ShardexError::Shard(format!(
                "Posting storage file not found: {}",
                posting_path.display()
            )));
        }

        // Open storage components in read-only mode
        let vector_storage = VectorStorage::open_read_only(&vector_path)
            .map_err(|e| ShardexError::Shard(format!("Failed to open vector storage: {}", e)))?;

        let posting_storage = PostingStorage::open_read_only(&posting_path)
            .map_err(|e| ShardexError::Shard(format!("Failed to open posting storage: {}", e)))?;

        // Validate consistency between storages
        let capacity = vector_storage.capacity();
        let vector_size = vector_storage.vector_dimension();

        if posting_storage.capacity() != capacity {
            return Err(ShardexError::Corruption(format!(
                "Capacity mismatch: vector storage has {}, posting storage has {}",
                capacity,
                posting_storage.capacity()
            )));
        }

        // Create metadata
        let mut metadata = ShardMetadata::new(true, capacity)?;
        metadata.update_from_storages(&vector_storage, &posting_storage);

        // Initialize centroid and calculate from existing data
        let centroid = vec![0.0; vector_size];
        let active_vector_count = vector_storage.active_count();

        let mut shard = Self {
            id,
            vector_storage,
            posting_storage,
            capacity,
            vector_size,
            directory: directory.to_path_buf(),
            metadata,
            centroid,
            active_vector_count,
        };

        // Calculate centroid from existing vectors only if there are vectors
        if active_vector_count > 0 {
            shard.recalculate_centroid();
        }

        // Populate bloom filter from existing postings
        shard.populate_bloom_filter()?;

        Ok(shard)
    }

    /// Get the shard ID
    pub fn id(&self) -> ShardId {
        self.id
    }

    /// Get the capacity of this shard
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get the vector dimension size
    pub fn vector_size(&self) -> usize {
        self.vector_size
    }

    /// Get the directory containing this shard's files
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Get the current count of postings (including deleted ones)
    pub fn current_count(&self) -> usize {
        self.metadata.current_count
    }

    /// Get the count of active (non-deleted) postings
    pub fn active_count(&self) -> usize {
        // Count unique postings using append-only semantics, excluding deleted ones
        self.iter_unique_postings_backward()
            .filter(|result| {
                match result {
                    Ok((index, _)) => !self.is_deleted(*index).unwrap_or(true),
                    Err(_) => false, // Skip errors in count
                }
            })
            .count()
    }

    /// Check if the shard is read-only
    pub fn is_read_only(&self) -> bool {
        self.metadata.read_only
    }

    /// Check if the shard is at capacity
    pub fn is_full(&self) -> bool {
        self.current_count() >= self.capacity()
    }

    /// Get remaining capacity
    pub fn remaining_capacity(&self) -> usize {
        self.capacity().saturating_sub(self.current_count())
    }

    /// Get available capacity (alias for remaining_capacity for API consistency)
    pub fn available_capacity(&self) -> usize {
        self.remaining_capacity()
    }

    /// Get shard metadata
    pub fn metadata(&self) -> &ShardMetadata {
        &self.metadata
    }

    /// Check if a document might exist in this shard using bloom filter
    ///
    /// Returns true if the document might be in this shard (with possible false positives).
    /// Returns false if the document is definitely not in this shard (no false negatives).
    /// Use this method to skip expensive shard scans when the document is not present.
    pub async fn contains_document(&self, doc_id: DocumentId) -> bool {
        self.metadata.bloom_filter.contains(doc_id)
    }

    /// Add a posting to the shard
    ///
    /// This method adds both the vector and posting metadata atomically.
    /// If either operation fails, the shard remains in a consistent state.
    pub fn add_posting(&mut self, posting: Posting) -> Result<usize, ShardexError> {
        if self.is_read_only() {
            return Err(ShardexError::Config(
                "Cannot add posting to read-only shard".to_string(),
            ));
        }

        // Validate vector dimension
        if posting.vector.len() != self.vector_size {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_size,
                actual: posting.vector.len(),
            });
        }

        // Check capacity
        if self.is_full() {
            return Err(ShardexError::Shard("Shard is at capacity".to_string()));
        }

        // Add to vector storage first
        let vector_index = self
            .vector_storage
            .add_vector(&posting.vector)
            .map_err(|e| ShardexError::Shard(format!("Failed to add vector: {}", e)))?;

        // Add to posting storage
        let posting_index = self
            .posting_storage
            .add_posting(posting.document_id, posting.start, posting.length)
            .map_err(|e| {
                // If posting storage fails, we need to remove the vector to maintain consistency
                // This is a bit tricky since VectorStorage doesn't have a direct remove by index method
                // For now, we'll mark it as deleted
                let _ = self.vector_storage.remove_vector(vector_index);
                ShardexError::Shard(format!("Failed to add posting: {}", e))
            })?;

        // Ensure indices match (they should since we add sequentially)
        if vector_index != posting_index {
            // This is a serious consistency error - try to clean up
            let _ = self.vector_storage.remove_vector(vector_index);
            let _ = self.posting_storage.remove_posting(posting_index);
            return Err(ShardexError::Corruption(format!(
                "Index mismatch: vector index {} != posting index {}",
                vector_index, posting_index
            )));
        }

        // Update bloom filter with the document ID
        self.metadata.bloom_filter.insert(posting.document_id);

        // Update centroid with the new vector
        self.update_centroid_add(&posting.vector);

        // Update metadata
        self.metadata
            .update_from_storages(&self.vector_storage, &self.posting_storage);

        Ok(vector_index)
    }

    /// Get a posting by index
    pub fn get_posting(&self, index: usize) -> Result<Posting, ShardexError> {
        if index >= self.current_count() {
            return Err(ShardexError::Config(format!(
                "Index {} out of bounds (current count: {})",
                index,
                self.current_count()
            )));
        }

        // Get vector
        let vector = self
            .vector_storage
            .get_vector(index)
            .map_err(|e| ShardexError::Shard(format!("Failed to get vector: {}", e)))?;

        // Get posting metadata
        let (document_id, start, length) = self
            .posting_storage
            .get_posting(index)
            .map_err(|e| ShardexError::Shard(format!("Failed to get posting: {}", e)))?;

        // Create posting with owned vector data
        let posting = Posting::new(
            document_id,
            start,
            length,
            vector.to_vec(),
            self.vector_size,
        )?;

        Ok(posting)
    }

    /// Iterate over all postings in reverse order (most recent first)
    /// This supports append-only semantics where newer postings supersede older ones
    pub fn iter_postings_backward(
        &self,
    ) -> impl Iterator<Item = Result<(usize, Posting), ShardexError>> + '_ {
        (0..self.current_count())
            .rev()
            .map(move |index| match self.get_posting(index) {
                Ok(posting) => Ok((index, posting)),
                Err(e) => Err(e),
            })
    }

    /// Iterate over unique postings in append-only style
    /// Returns only the most recent posting for each (document_id, start, length) combination
    /// by reading backwards and skipping duplicates
    pub fn iter_unique_postings_backward(
        &self,
    ) -> impl Iterator<Item = Result<(usize, Posting), ShardexError>> + '_ {
        use std::collections::HashSet;

        let mut seen = HashSet::new();
        self.iter_postings_backward().filter_map(move |result| {
            match result {
                Ok((index, posting)) => {
                    let key = (posting.document_id, posting.start, posting.length);
                    if seen.insert(key) {
                        Some(Ok((index, posting)))
                    } else {
                        None // Skip duplicate posting
                    }
                }
                Err(e) => Some(Err(e)),
            }
        })
    }

    /// Check if a posting at the given index is deleted
    pub fn is_deleted(&self, index: usize) -> Result<bool, ShardexError> {
        if index >= self.current_count() {
            return Ok(false);
        }

        // Check both storages for consistency
        let vector_deleted = self
            .vector_storage
            .is_deleted(index)
            .map_err(|e| ShardexError::Shard(format!("Failed to check vector deletion: {}", e)))?;

        let posting_deleted = self
            .posting_storage
            .is_deleted(index)
            .map_err(|e| ShardexError::Shard(format!("Failed to check posting deletion: {}", e)))?;

        // Both should have the same deletion state
        if vector_deleted != posting_deleted {
            return Err(ShardexError::Corruption(format!(
                "Deletion state mismatch at index {}: vector={}, posting={}",
                index, vector_deleted, posting_deleted
            )));
        }

        Ok(vector_deleted)
    }

    /// Remove a posting by index
    pub fn remove_posting(&mut self, index: usize) -> Result<(), ShardexError> {
        if self.is_read_only() {
            return Err(ShardexError::Config(
                "Cannot remove posting from read-only shard".to_string(),
            ));
        }

        if index >= self.current_count() {
            return Err(ShardexError::Config(format!(
                "Index {} out of bounds (current count: {})",
                index,
                self.current_count()
            )));
        }

        // Get the vector before removing it for centroid update
        let vector = self.vector_storage.get_vector(index).map_err(|e| {
            ShardexError::Shard(format!("Failed to get vector for centroid update: {}", e))
        })?;
        let vector_copy = vector.to_vec(); // Copy to owned vector

        // Remove from both storages
        self.vector_storage
            .remove_vector(index)
            .map_err(|e| ShardexError::Shard(format!("Failed to remove vector: {}", e)))?;

        self.posting_storage
            .remove_posting(index)
            .map_err(|e| ShardexError::Shard(format!("Failed to remove posting: {}", e)))?;

        // Update centroid after successful removal
        self.update_centroid_remove(&vector_copy);

        // Update metadata
        self.metadata
            .update_from_storages(&self.vector_storage, &self.posting_storage);

        Ok(())
    }

    /// Remove all postings for a specific document ID
    ///
    /// This method iterates through all postings and removes any that match
    /// the given document ID. Returns the number of postings that were removed.
    pub fn remove_document(&mut self, doc_id: DocumentId) -> Result<usize, ShardexError> {
        if self.is_read_only() {
            return Err(ShardexError::Config(
                "Cannot remove documents from read-only shard".to_string(),
            ));
        }

        // Check bloom filter first - if document is definitely not in shard, skip expensive scan
        if !self.metadata.bloom_filter.contains(doc_id) {
            return Ok(0); // No documents removed - document definitely not in shard
        }

        let mut removed_vectors = Vec::new();

        // First pass: collect vectors to remove for centroid update
        for index in 0..self.current_count() {
            // Skip already deleted postings
            if self.is_deleted(index)? {
                continue;
            }

            // Get the document ID for this posting
            let (posting_doc_id, _, _) = self
                .posting_storage
                .get_posting(index)
                .map_err(|e| ShardexError::Shard(format!("Failed to get posting: {}", e)))?;

            // If this posting matches the document ID, collect its vector
            if posting_doc_id == doc_id {
                let vector = self
                    .vector_storage
                    .get_vector(index)
                    .map_err(|e| ShardexError::Shard(format!("Failed to get vector: {}", e)))?;
                removed_vectors.push(vector.to_vec());
            }
        }

        let mut removed_count = 0;

        // Second pass: actually remove the postings
        // We iterate backwards to avoid index shifting issues
        for index in (0..self.current_count()).rev() {
            // Skip already deleted postings
            if self.is_deleted(index)? {
                continue;
            }

            // Get the document ID for this posting
            let (posting_doc_id, _, _) = self
                .posting_storage
                .get_posting(index)
                .map_err(|e| ShardexError::Shard(format!("Failed to get posting: {}", e)))?;

            // If this posting matches the document ID, remove it (but don't update centroid yet)
            if posting_doc_id == doc_id {
                // Remove from both storages directly without centroid update
                self.vector_storage
                    .remove_vector(index)
                    .map_err(|e| ShardexError::Shard(format!("Failed to remove vector: {}", e)))?;

                self.posting_storage
                    .remove_posting(index)
                    .map_err(|e| ShardexError::Shard(format!("Failed to remove posting: {}", e)))?;

                removed_count += 1;
            }
        }

        // Update centroid for all removed vectors
        for vector in &removed_vectors {
            self.update_centroid_remove(vector);
        }

        // Update metadata
        self.metadata
            .update_from_storages(&self.vector_storage, &self.posting_storage);

        Ok(removed_count)
    }

    /// Search for the K nearest neighbors to the query vector within this shard
    ///
    /// This method uses append-only semantics, reading postings backward to find
    /// the most recent version of each unique posting (document_id, start, length).
    /// Calculates dot product similarity scores and returns the top K results
    /// sorted by similarity score in descending order.
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>, ShardexError> {
        // Validate query vector dimension
        if query.len() != self.vector_size {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_size,
                actual: query.len(),
            });
        }

        if k == 0 {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();

        // Use append-only iteration to get only the most recent version of each posting
        for result in self.iter_unique_postings_backward() {
            let (index, posting) = result?;

            // Get the vector for this posting
            let vector = self
                .vector_storage
                .get_vector(index)
                .map_err(|e| ShardexError::Shard(format!("Failed to get vector: {}", e)))?;

            // Calculate cosine similarity
            let similarity_score = Self::calculate_cosine_similarity(query, vector);

            // Create search result
            let search_result = SearchResult::from_posting(posting, similarity_score)?;

            results.push(search_result);
        }

        // Sort by similarity score in descending order
        results.sort_by(|a, b| {
            b.similarity_score
                .partial_cmp(&a.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Return top K results
        results.truncate(k);
        Ok(results)
    }

    /// Calculate cosine similarity between two vectors
    ///
    /// Cosine similarity returns a value between -1.0 and 1.0, where:
    /// - 1.0 means the vectors point in the same direction (most similar)
    /// - 0.0 means the vectors are orthogonal (no similarity)
    /// - -1.0 means the vectors point in opposite directions (least similar)
    ///
    /// We normalize it to 0.0-1.0 range for SearchResult compatibility by
    /// applying the formula: (cosine + 1.0) / 2.0
    fn calculate_cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        debug_assert_eq!(a.len(), b.len(), "Vectors must have same length");

        // Check for invalid values in input vectors
        if a.iter().any(|x| !x.is_finite()) || b.iter().any(|x| !x.is_finite()) {
            return 0.5; // Neutral similarity for invalid vectors
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        // Handle zero vectors or invalid norms to avoid division by zero
        if norm_a == 0.0 || norm_b == 0.0 || !norm_a.is_finite() || !norm_b.is_finite() {
            return 0.5; // Neutral similarity for zero vectors (maps to 0.0 cosine)
        }

        let cosine = dot_product / (norm_a * norm_b);

        // Check if cosine is valid before clamping
        if !cosine.is_finite() {
            return 0.5; // Neutral similarity for invalid cosine
        }

        // Clamp cosine to valid range to handle floating point precision issues
        let cosine = cosine.clamp(-1.0, 1.0);

        // Normalize to 0.0-1.0 range: (cosine + 1.0) / 2.0
        // This maps -1.0 -> 0.0, 0.0 -> 0.5, 1.0 -> 1.0
        (cosine + 1.0) / 2.0
    }

    /// Search for the K nearest neighbors using a specific distance metric
    ///
    /// This method is similar to the basic search method but allows specifying
    /// which distance metric to use for similarity calculation.
    pub fn search_with_metric(
        &self,
        query: &[f32],
        k: usize,
        metric: DistanceMetric,
    ) -> Result<Vec<SearchResult>, ShardexError> {
        // Validate query vector dimension
        if query.len() != self.vector_size {
            return Err(ShardexError::InvalidDimension {
                expected: self.vector_size,
                actual: query.len(),
            });
        }

        if k == 0 {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();

        // Use append-only iteration to get only the most recent version of each posting
        for result in self.iter_unique_postings_backward() {
            let (index, posting) = result?;

            // Get the vector for this posting
            let vector = self
                .vector_storage
                .get_vector(index)
                .map_err(|e| ShardexError::Shard(format!("Failed to get vector: {}", e)))?;

            // Calculate similarity using specified metric
            let similarity_score = metric.similarity(query, vector)?;

            // Create search result
            let search_result = SearchResult::from_posting(posting, similarity_score)?;

            results.push(search_result);
        }

        // Sort by similarity score in descending order
        results.sort_by(|a, b| {
            b.similarity_score
                .partial_cmp(&a.similarity_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Return top K results
        results.truncate(k);
        Ok(results)
    }

    /// Synchronize the shard to disk
    pub fn sync(&mut self) -> Result<(), ShardexError> {
        if self.is_read_only() {
            return Ok(());
        }

        self.vector_storage
            .sync()
            .map_err(|e| ShardexError::Shard(format!("Failed to sync vector storage: {}", e)))?;

        self.posting_storage
            .sync()
            .map_err(|e| ShardexError::Shard(format!("Failed to sync posting storage: {}", e)))?;

        // Update metadata after sync
        self.metadata
            .update_from_storages(&self.vector_storage, &self.posting_storage);

        Ok(())
    }

    /// Validate the integrity of the shard
    ///
    /// This performs comprehensive validation including:
    /// - Individual storage integrity
    /// - Cross-storage consistency
    /// - Metadata accuracy
    pub fn validate_integrity(&self) -> Result<(), ShardexError> {
        // Validate individual storages
        self.vector_storage
            .validate_integrity()
            .map_err(|e| ShardexError::Shard(format!("Vector storage integrity failed: {}", e)))?;

        self.posting_storage
            .validate_integrity()
            .map_err(|e| ShardexError::Shard(format!("Posting storage integrity failed: {}", e)))?;

        // Validate cross-storage consistency
        let vector_count = self.vector_storage.current_count();
        let posting_count = self.posting_storage.current_count();

        if vector_count != posting_count {
            return Err(ShardexError::Corruption(format!(
                "Count mismatch: vector storage has {}, posting storage has {}",
                vector_count, posting_count
            )));
        }

        let vector_active = self.vector_storage.active_count();
        let posting_active = self.posting_storage.active_count();

        if vector_active != posting_active {
            return Err(ShardexError::Corruption(format!(
                "Active count mismatch: vector storage has {}, posting storage has {}",
                vector_active, posting_active
            )));
        }

        // Validate each posting individually for consistency
        for i in 0..vector_count {
            let vector_deleted = self.vector_storage.is_deleted(i).map_err(|e| {
                ShardexError::Shard(format!("Failed to check vector deletion at {}: {}", i, e))
            })?;

            let posting_deleted = self.posting_storage.is_deleted(i).map_err(|e| {
                ShardexError::Shard(format!("Failed to check posting deletion at {}: {}", i, e))
            })?;

            if vector_deleted != posting_deleted {
                return Err(ShardexError::Corruption(format!(
                    "Deletion state mismatch at index {}: vector={}, posting={}",
                    i, vector_deleted, posting_deleted
                )));
            }

            // For active postings, validate they can be read correctly
            if !vector_deleted {
                let _vector = self.vector_storage.get_vector(i).map_err(|e| {
                    ShardexError::Corruption(format!("Cannot read vector at index {}: {}", i, e))
                })?;

                let _posting = self.posting_storage.get_posting(i).map_err(|e| {
                    ShardexError::Corruption(format!("Cannot read posting at index {}: {}", i, e))
                })?;
            }
        }

        // Validate centroid consistency
        let expected_centroid = self.calculate_centroid();

        // Check centroid dimension
        if self.centroid.len() != self.vector_size {
            return Err(ShardexError::Corruption(format!(
                "Centroid dimension mismatch: expected {}, got {}",
                self.vector_size,
                self.centroid.len()
            )));
        }

        // Check if active vector count matches actual count
        let actual_active_count = vector_active;
        if self.active_vector_count != actual_active_count {
            return Err(ShardexError::Corruption(format!(
                "Active vector count mismatch: stored {}, actual {}",
                self.active_vector_count, actual_active_count
            )));
        }

        // Check centroid accuracy (with floating-point tolerance)
        const CENTROID_TOLERANCE: f32 = 1e-5;
        for (i, (&stored, &expected)) in self
            .centroid
            .iter()
            .zip(expected_centroid.iter())
            .enumerate()
        {
            let diff = (stored - expected).abs();
            if diff > CENTROID_TOLERANCE {
                return Err(ShardexError::Corruption(format!(
                    "Centroid component {} mismatch: stored {}, calculated {} (diff: {})",
                    i, stored, expected, diff
                )));
            }
        }

        Ok(())
    }

    /// Calculate centroid from all unique postings using append-only semantics
    ///
    /// This method performs a fresh calculation of the centroid by scanning unique
    /// postings in reverse order to get the most recent version of each posting.
    /// The centroid is the mean position of all active vectors.
    ///
    /// Returns a vector representing the centroid, or a zero vector if no active vectors exist.
    pub fn calculate_centroid(&self) -> Vec<f32> {
        let mut centroid = vec![0.0; self.vector_size];
        let mut active_count = 0;

        // Sum all unique vectors using append-only iteration
        for (index, _posting) in self.iter_unique_postings_backward().flatten() {
            if let Ok(vector) = self.vector_storage.get_vector(index) {
                for (j, &value) in vector.iter().enumerate() {
                    centroid[j] += value;
                }
                active_count += 1;
            }
        }

        // Calculate mean (centroid)
        if active_count > 0 {
            let count_f32 = active_count as f32;
            for value in &mut centroid {
                *value /= count_f32;
            }
        }

        centroid
    }

    /// Get the current centroid (read-only access)
    ///
    /// Returns a slice reference to the current centroid vector.
    /// The centroid represents the mean position of all non-deleted vectors in this shard.
    pub fn get_centroid(&self) -> &[f32] {
        &self.centroid
    }

    /// Incrementally update centroid when adding a new vector
    ///
    /// Uses the incremental mean formula to update the centroid efficiently:
    /// `new_centroid = old_centroid + (new_vector - old_centroid) / new_count`
    ///
    /// This is more efficient than recalculating the entire centroid.
    ///
    /// # Arguments
    /// * `vector` - The vector being added to the shard
    pub fn update_centroid_add(&mut self, vector: &[f32]) {
        debug_assert_eq!(vector.len(), self.vector_size, "Vector dimension mismatch");

        self.active_vector_count += 1;

        if self.active_vector_count == 1 {
            // First vector becomes the centroid
            self.centroid.copy_from_slice(vector);
        } else {
            let count_f32 = self.active_vector_count as f32;

            // Incremental mean update: new_centroid = old_centroid + (new_vector - old_centroid) / count
            for (i, &vector_val) in vector.iter().enumerate() {
                self.centroid[i] += (vector_val - self.centroid[i]) / count_f32;
            }
        }
    }

    /// Incrementally update centroid when removing a vector
    ///
    /// Uses the incremental mean formula to update the centroid efficiently when a vector is removed:
    /// `new_centroid = (old_centroid * old_count - removed_vector) / new_count`
    ///
    /// # Arguments
    /// * `vector` - The vector being removed from the shard
    pub fn update_centroid_remove(&mut self, vector: &[f32]) {
        debug_assert_eq!(vector.len(), self.vector_size, "Vector dimension mismatch");

        if self.active_vector_count == 0 {
            return; // No vectors to remove
        }

        if self.active_vector_count == 1 {
            // Removing the last vector - reset to zero centroid
            for value in &mut self.centroid {
                *value = 0.0;
            }
            self.active_vector_count = 0;
        } else {
            let old_count_f32 = self.active_vector_count as f32;
            self.active_vector_count -= 1;
            let new_count_f32 = self.active_vector_count as f32;

            // Remove vector from centroid: new_centroid = (old_centroid * old_count - removed_vector) / new_count
            for (i, &vector_val) in vector.iter().enumerate() {
                self.centroid[i] = (self.centroid[i] * old_count_f32 - vector_val) / new_count_f32;
            }
        }
    }

    /// Recalculate centroid from scratch for accuracy
    ///
    /// This method performs a full recalculation of the centroid by scanning all vectors.
    /// Use this method periodically to correct any drift from incremental updates due to
    /// floating-point precision errors. Uses append-only semantics to count unique postings.
    pub fn recalculate_centroid(&mut self) {
        self.centroid = self.calculate_centroid();

        // Update active vector count to match actual unique posting count
        let mut count = 0;
        for result in self.iter_unique_postings_backward() {
            if result.is_ok() {
                count += 1;
            }
        }
        self.active_vector_count = count;
    }

    /// Populate bloom filter from existing postings using append-only semantics
    ///
    /// This method scans unique postings (most recent version of each) and adds
    /// their document IDs to the bloom filter. This is used when opening existing
    /// shards to rebuild the bloom filter state.
    fn populate_bloom_filter(&mut self) -> Result<(), ShardexError> {
        // Clear the bloom filter first
        self.metadata.bloom_filter.clear();

        // Collect unique postings first to avoid borrowing conflicts
        let unique_postings: Result<Vec<_>, _> = self.iter_unique_postings_backward().collect();
        let unique_postings = unique_postings?;

        // Add document IDs to bloom filter
        for (_index, posting) in unique_postings {
            self.metadata.bloom_filter.insert(posting.document_id);
        }

        Ok(())
    }

    /// Check if the shard should be split based on capacity utilization
    ///
    /// Returns true when the shard is at 90% capacity or greater.
    /// This threshold ensures splits happen before the shard becomes completely full.
    pub fn should_split(&self) -> bool {
        if self.capacity == 0 {
            return false;
        }

        // Split when at 90% capacity
        let split_threshold = (self.capacity as f64 * 0.9) as usize;
        self.current_count() >= split_threshold
    }

    /// Split the shard into two balanced sub-shards using k-means clustering
    ///
    /// This method creates two new shards with approximately half the capacity each,
    /// and distributes the postings using k-means clustering (k=2) for balanced splits.
    ///
    /// # Returns
    /// A tuple of two new shards: (shard_a, shard_b)
    ///
    /// # Errors
    /// Returns an error if:
    /// - The shard is read-only
    /// - The shard has insufficient data to split (< 2 active postings)
    /// - Clustering fails
    /// - Shard creation fails
    pub async fn split(&self) -> Result<(Shard, Shard), ShardexError> {
        if self.is_read_only() {
            return Err(ShardexError::Config(
                "Cannot split read-only shard".to_string(),
            ));
        }

        if self.active_count() < 2 {
            return Err(ShardexError::Config(
                "Cannot split shard with less than 2 active postings".to_string(),
            ));
        }

        // Collect unique postings using append-only semantics
        let unique_postings: Result<Vec<(usize, Posting)>, ShardexError> =
            self.iter_unique_postings_backward().collect();
        let unique_postings = unique_postings?;

        // Use k-means clustering to determine how to split the vectors
        let indices: Vec<usize> = unique_postings.iter().map(|(index, _)| *index).collect();
        let (cluster_a_indices, cluster_b_indices) = self.cluster_unique_vectors(&indices)?;

        // Create two new shards with appropriate capacity
        let unique_count = unique_postings.len();
        let has_duplicates = self.current_count() > unique_count;

        let new_capacity = if has_duplicates {
            // For append-only scenarios, we need extra buffer for clustering imbalance
            let min_capacity_needed = (unique_count + 1) / 2 + 5;
            std::cmp::max(self.capacity / 2, min_capacity_needed)
        } else {
            // For regular scenarios, just split capacity in half (minimum 5)
            std::cmp::max(self.capacity / 2, 5)
        };

        let shard_a_id = ShardId::new();
        let shard_b_id = ShardId::new();

        let mut shard_a = Shard::create(
            shard_a_id,
            new_capacity,
            self.vector_size,
            self.directory.clone(),
        )?;

        let mut shard_b = Shard::create(
            shard_b_id,
            new_capacity,
            self.vector_size,
            self.directory.clone(),
        )?;

        // Transfer postings to appropriate shards based on clustering
        for &index in &cluster_a_indices {
            // Find the posting for this index
            if let Some((_, posting)) = unique_postings.iter().find(|(i, _)| *i == index) {
                shard_a.add_posting(posting.clone())?;
            }
        }

        for &index in &cluster_b_indices {
            // Find the posting for this index
            if let Some((_, posting)) = unique_postings.iter().find(|(i, _)| *i == index) {
                shard_b.add_posting(posting.clone())?;
            }
        }

        Ok((shard_a, shard_b))
    }

    /// Cluster vectors into two groups using k-means algorithm (k=2)
    ///
    /// This internal method performs k-means clustering to determine the best way
    /// to split the shard's vectors into two balanced groups.
    ///
    /// # Returns
    /// A tuple of two vectors containing the indices of postings for each cluster:
    /// (cluster_a_indices, cluster_b_indices)
    #[allow(dead_code)]
    fn cluster_vectors(&self) -> Result<(Vec<usize>, Vec<usize>), ShardexError> {
        let active_indices: Vec<usize> = (0..self.current_count())
            .filter(|&i| !self.is_deleted(i).unwrap_or(true))
            .collect();

        if active_indices.len() < 2 {
            return Err(ShardexError::Config(
                "Need at least 2 active vectors for clustering".to_string(),
            ));
        }

        // For very small sets, just split in half
        if active_indices.len() <= 4 {
            let mid = active_indices.len() / 2;
            let cluster_a = active_indices[..mid].to_vec();
            let cluster_b = active_indices[mid..].to_vec();
            return Ok((cluster_a, cluster_b));
        }

        // Initialize centroids using the two most distant vectors (furthest pair)
        let (centroid_a_idx, centroid_b_idx) = self.find_furthest_pair(&active_indices)?;

        // Check if all vectors are identical (pathological case for k-means)
        let first_vector = self
            .vector_storage
            .get_vector(active_indices[0])
            .map_err(|e| ShardexError::Shard(format!("Failed to get first vector: {}", e)))?;
        let mut all_identical = true;

        for &idx in &active_indices[1..] {
            let vector = self.vector_storage.get_vector(idx).map_err(|e| {
                ShardexError::Shard(format!("Failed to get vector for identity check: {}", e))
            })?;
            if Self::euclidean_distance(first_vector, vector) > 1e-6 {
                all_identical = false;
                break;
            }
        }

        // If all vectors are identical, split evenly rather than using k-means
        if all_identical {
            let mid = active_indices.len() / 2;
            let cluster_a = active_indices[..mid].to_vec();
            let cluster_b = active_indices[mid..].to_vec();
            return Ok((cluster_a, cluster_b));
        }

        let centroid_a_vector = self
            .vector_storage
            .get_vector(centroid_a_idx)
            .map_err(|e| ShardexError::Shard(format!("Failed to get centroid A vector: {}", e)))?;
        let centroid_b_vector = self
            .vector_storage
            .get_vector(centroid_b_idx)
            .map_err(|e| ShardexError::Shard(format!("Failed to get centroid B vector: {}", e)))?;

        let mut centroid_a = centroid_a_vector.to_vec();
        let mut centroid_b = centroid_b_vector.to_vec();

        let mut cluster_a_indices = Vec::new();
        let mut cluster_b_indices = Vec::new();
        let max_iterations = 10;

        // K-means iterations
        for iteration in 0..max_iterations {
            cluster_a_indices.clear();
            cluster_b_indices.clear();

            // Assign each vector to the nearest centroid
            for &index in &active_indices {
                let vector = self.vector_storage.get_vector(index).map_err(|e| {
                    ShardexError::Shard(format!("Failed to get vector for clustering: {}", e))
                })?;

                let dist_a = Self::euclidean_distance(vector, &centroid_a);
                let dist_b = Self::euclidean_distance(vector, &centroid_b);

                if dist_a <= dist_b {
                    cluster_a_indices.push(index);
                } else {
                    cluster_b_indices.push(index);
                }
            }

            // Ensure both clusters have at least one vector
            if cluster_a_indices.is_empty() {
                cluster_a_indices.push(cluster_b_indices.pop().unwrap());
            } else if cluster_b_indices.is_empty() {
                cluster_b_indices.push(cluster_a_indices.pop().unwrap());
            }

            // Update centroids
            let new_centroid_a = self.calculate_cluster_centroid(&cluster_a_indices)?;
            let new_centroid_b = self.calculate_cluster_centroid(&cluster_b_indices)?;

            // Check for convergence
            let centroid_a_change = Self::euclidean_distance(&centroid_a, &new_centroid_a);
            let centroid_b_change = Self::euclidean_distance(&centroid_b, &new_centroid_b);

            centroid_a = new_centroid_a;
            centroid_b = new_centroid_b;

            // Converged if centroids don't move much
            if centroid_a_change < 1e-6 && centroid_b_change < 1e-6 {
                break;
            }

            // For debugging: prevent infinite loops
            if iteration == max_iterations - 1 {
                tracing::warn!("K-means clustering reached max iterations without convergence");
            }
        }

        Ok((cluster_a_indices, cluster_b_indices))
    }

    /// Cluster vectors using provided indices (for append-only semantics)
    ///
    /// This method performs k-means clustering on a provided set of indices
    /// representing unique postings from append-only iteration.
    fn cluster_unique_vectors(
        &self,
        indices: &[usize],
    ) -> Result<(Vec<usize>, Vec<usize>), ShardexError> {
        if indices.len() < 2 {
            return Err(ShardexError::Config(
                "Need at least 2 vectors for clustering".to_string(),
            ));
        }

        // For very small sets, just split in half
        if indices.len() <= 4 {
            let mid = indices.len() / 2;
            let cluster_a = indices[..mid].to_vec();
            let cluster_b = indices[mid..].to_vec();
            return Ok((cluster_a, cluster_b));
        }

        // Initialize centroids using the two most distant vectors (furthest pair)
        let (centroid_a_idx, centroid_b_idx) = self.find_furthest_pair(indices)?;

        // Check if all vectors are identical (pathological case for k-means)
        let first_vector = self
            .vector_storage
            .get_vector(indices[0])
            .map_err(|e| ShardexError::Shard(format!("Failed to get first vector: {}", e)))?;
        let mut all_identical = true;

        for &idx in &indices[1..] {
            let vector = self.vector_storage.get_vector(idx).map_err(|e| {
                ShardexError::Shard(format!("Failed to get vector for identity check: {}", e))
            })?;
            if Self::euclidean_distance(first_vector, vector) > 1e-6 {
                all_identical = false;
                break;
            }
        }

        // If all vectors are identical, split evenly rather than using k-means
        if all_identical {
            let mid = indices.len() / 2;
            let cluster_a = indices[..mid].to_vec();
            let cluster_b = indices[mid..].to_vec();
            return Ok((cluster_a, cluster_b));
        }

        // Get initial centroid vectors
        let centroid_a = self
            .vector_storage
            .get_vector(centroid_a_idx)
            .map_err(|e| ShardexError::Shard(format!("Failed to get centroid A vector: {}", e)))?
            .to_vec();
        let centroid_b = self
            .vector_storage
            .get_vector(centroid_b_idx)
            .map_err(|e| ShardexError::Shard(format!("Failed to get centroid B vector: {}", e)))?
            .to_vec();

        let mut cluster_a_indices = Vec::new();
        let mut cluster_b_indices = Vec::new();

        // K-means clustering
        let max_iterations = 10;
        for iteration in 0..max_iterations {
            cluster_a_indices.clear();
            cluster_b_indices.clear();

            // Assign each vector to the nearest centroid
            for &idx in indices {
                let vector = self.vector_storage.get_vector(idx).map_err(|e| {
                    ShardexError::Shard(format!("Failed to get vector during clustering: {}", e))
                })?;

                let dist_a = Self::euclidean_distance(&centroid_a, vector);
                let dist_b = Self::euclidean_distance(&centroid_b, vector);

                if dist_a <= dist_b {
                    cluster_a_indices.push(idx);
                } else {
                    cluster_b_indices.push(idx);
                }
            }

            // If one cluster is empty, split evenly
            if cluster_a_indices.is_empty() || cluster_b_indices.is_empty() {
                let mid = indices.len() / 2;
                return Ok((indices[..mid].to_vec(), indices[mid..].to_vec()));
            }

            // Check for convergence (no changes in cluster assignments)
            if iteration > 0 {
                // For simplicity, assume convergence after a few iterations
                break;
            }
        }

        Ok((cluster_a_indices, cluster_b_indices))
    }

    /// Find the pair of vectors with maximum distance (furthest pair)
    ///
    /// This is used to initialize k-means centroids with a good starting point.
    fn find_furthest_pair(&self, indices: &[usize]) -> Result<(usize, usize), ShardexError> {
        if indices.len() < 2 {
            return Err(ShardexError::Config(
                "Need at least 2 vectors to find furthest pair".to_string(),
            ));
        }

        let mut max_distance = 0.0;
        let mut furthest_pair = (indices[0], indices[1]);

        for i in 0..indices.len() {
            for j in i + 1..indices.len() {
                let vector_i = self.vector_storage.get_vector(indices[i]).map_err(|e| {
                    ShardexError::Shard(format!("Failed to get vector for furthest pair: {}", e))
                })?;
                let vector_j = self.vector_storage.get_vector(indices[j]).map_err(|e| {
                    ShardexError::Shard(format!("Failed to get vector for furthest pair: {}", e))
                })?;

                let distance = Self::euclidean_distance(vector_i, vector_j);
                if distance > max_distance {
                    max_distance = distance;
                    furthest_pair = (indices[i], indices[j]);
                }
            }
        }

        Ok(furthest_pair)
    }

    /// Calculate the centroid (mean) of a cluster of vectors
    #[allow(dead_code)]
    fn calculate_cluster_centroid(&self, indices: &[usize]) -> Result<Vec<f32>, ShardexError> {
        if indices.is_empty() {
            return Ok(vec![0.0; self.vector_size]);
        }

        let mut centroid = vec![0.0; self.vector_size];

        for &index in indices {
            let vector = self.vector_storage.get_vector(index).map_err(|e| {
                ShardexError::Shard(format!(
                    "Failed to get vector for centroid calculation: {}",
                    e
                ))
            })?;

            for (i, &value) in vector.iter().enumerate() {
                centroid[i] += value;
            }
        }

        // Calculate mean
        let count = indices.len() as f32;
        for value in &mut centroid {
            *value /= count;
        }

        Ok(centroid)
    }

    /// Calculate Euclidean distance between two vectors
    fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
        debug_assert_eq!(a.len(), b.len(), "Vectors must have same length");

        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f32>()
            .sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifiers::DocumentId;
    use tempfile::TempDir;

    #[test]
    fn test_shard_creation() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let shard = Shard::create(shard_id, 100, 128, temp_dir.path().to_path_buf()).unwrap();

        assert_eq!(shard.id(), shard_id);
        assert_eq!(shard.capacity(), 100);
        assert_eq!(shard.vector_size(), 128);
        assert_eq!(shard.current_count(), 0);
        assert_eq!(shard.active_count(), 0);
        assert!(!shard.is_read_only());
        assert!(!shard.is_full());
        assert_eq!(shard.remaining_capacity(), 100);

        // Verify files exist
        let vector_path = temp_dir.path().join(format!("{}.vectors", shard_id));
        let posting_path = temp_dir.path().join(format!("{}.postings", shard_id));
        assert!(vector_path.exists());
        assert!(posting_path.exists());
    }

    #[test]
    fn test_shard_creation_validation() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        // Zero capacity should fail
        let result = Shard::create(shard_id, 0, 128, temp_dir.path().to_path_buf());
        assert!(result.is_err());

        // Zero vector size should fail
        let result = Shard::create(shard_id, 100, 0, temp_dir.path().to_path_buf());
        assert!(result.is_err());
    }

    #[test]
    fn test_shard_add_and_get_posting() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Create a posting
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];
        let posting = Posting::new(doc_id, 100, 50, vector.clone(), 3).unwrap();

        // Add posting
        let index = shard.add_posting(posting.clone()).unwrap();
        assert_eq!(index, 0);
        assert_eq!(shard.current_count(), 1);
        assert_eq!(shard.active_count(), 1);

        // Get posting back
        let retrieved = shard.get_posting(index).unwrap();
        assert_eq!(retrieved.document_id, posting.document_id);
        assert_eq!(retrieved.start, posting.start);
        assert_eq!(retrieved.length, posting.length);
        assert_eq!(retrieved.vector, posting.vector);
    }

    #[test]
    fn test_shard_vector_dimension_validation() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Wrong dimension should fail
        let doc_id = DocumentId::new();
        let wrong_vector = vec![1.0, 2.0]; // 2D instead of 3D
        let posting = Posting::new(doc_id, 100, 50, wrong_vector, 2).unwrap();

        let result = shard.add_posting(posting);
        match result {
            Err(ShardexError::InvalidDimension { expected, actual }) => {
                assert_eq!(expected, 3);
                assert_eq!(actual, 2);
            }
            _ => panic!("Expected InvalidDimension error"),
        }
    }

    #[test]
    fn test_shard_capacity_limits() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(
            shard_id,
            2, // Small capacity
            2,
            temp_dir.path().to_path_buf(),
        )
        .unwrap();

        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0];

        // Add postings up to capacity
        let posting1 = Posting::new(doc_id, 100, 50, vector.clone(), 2).unwrap();
        shard.add_posting(posting1).unwrap();

        let posting2 = Posting::new(doc_id, 200, 75, vector.clone(), 2).unwrap();
        shard.add_posting(posting2).unwrap();

        assert!(shard.is_full());
        assert_eq!(shard.remaining_capacity(), 0);

        // Adding beyond capacity should fail
        let posting3 = Posting::new(doc_id, 300, 25, vector, 2).unwrap();
        let result = shard.add_posting(posting3);
        assert!(matches!(result, Err(ShardexError::Shard(_))));
    }

    #[test]
    fn test_shard_remove_posting() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add postings
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];

        let posting1 = Posting::new(doc_id1, 100, 50, vector.clone(), 3).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 75, vector, 3).unwrap();

        let idx1 = shard.add_posting(posting1).unwrap();
        let idx2 = shard.add_posting(posting2).unwrap();

        assert_eq!(shard.active_count(), 2);

        // Remove one posting
        shard.remove_posting(idx1).unwrap();

        assert_eq!(shard.current_count(), 2); // Still 2 total
        assert_eq!(shard.active_count(), 1); // But only 1 active
        assert!(shard.is_deleted(idx1).unwrap());
        assert!(!shard.is_deleted(idx2).unwrap());

        // Second posting should still be accessible
        let retrieved = shard.get_posting(idx2).unwrap();
        assert_eq!(retrieved.document_id, doc_id2);
    }

    #[test]
    fn test_shard_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let directory = temp_dir.path().to_path_buf();

        let postings_to_add = vec![
            (DocumentId::new(), 100, 50, vec![1.0, 2.0, 3.0]),
            (DocumentId::new(), 200, 75, vec![4.0, 5.0, 6.0]),
            (DocumentId::new(), 300, 25, vec![7.0, 8.0, 9.0]),
        ];

        // Create and populate shard
        {
            let mut shard = Shard::create(shard_id, 10, 3, directory.clone()).unwrap();

            for (doc_id, start, length, vector) in &postings_to_add {
                let posting = Posting::new(*doc_id, *start, *length, vector.clone(), 3).unwrap();
                shard.add_posting(posting).unwrap();
            }

            shard.sync().unwrap();
        }

        // Reopen and verify
        {
            let shard = Shard::open(shard_id, &directory).unwrap();

            assert_eq!(shard.id(), shard_id);
            assert_eq!(shard.capacity(), 10);
            assert_eq!(shard.vector_size(), 3);
            assert_eq!(shard.current_count(), 3);
            assert_eq!(shard.active_count(), 3);

            for (i, (expected_doc_id, expected_start, expected_length, expected_vector)) in
                postings_to_add.iter().enumerate()
            {
                let retrieved = shard.get_posting(i).unwrap();
                assert_eq!(retrieved.document_id, *expected_doc_id);
                assert_eq!(retrieved.start, *expected_start);
                assert_eq!(retrieved.length, *expected_length);
                assert_eq!(retrieved.vector, *expected_vector);
            }
        }
    }

    #[test]
    fn test_shard_read_only_mode() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let directory = temp_dir.path().to_path_buf();

        // Create shard with some data
        {
            let mut shard = Shard::create(shard_id, 5, 2, directory.clone()).unwrap();
            let doc_id = DocumentId::new();
            let vector = vec![1.0, 2.0];
            let posting = Posting::new(doc_id, 100, 50, vector, 2).unwrap();
            shard.add_posting(posting).unwrap();
            shard.sync().unwrap();
        }

        // Open in read-only mode
        {
            let mut shard = Shard::open_read_only(shard_id, &directory).unwrap();

            assert!(shard.is_read_only());
            assert_eq!(shard.current_count(), 1);

            // Should be able to read
            let retrieved = shard.get_posting(0).unwrap();
            assert_eq!(retrieved.start, 100);
            assert_eq!(retrieved.length, 50);

            // Should not be able to modify
            let new_doc_id = DocumentId::new();
            let new_vector = vec![3.0, 4.0];
            let new_posting = Posting::new(new_doc_id, 200, 75, new_vector, 2).unwrap();

            assert!(shard.add_posting(new_posting).is_err());
            assert!(shard.remove_posting(0).is_err());
        }
    }

    #[test]
    fn test_shard_integrity_validation() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add some postings
        for i in 0..5 {
            let doc_id = DocumentId::new();
            let vector = vec![i as f32, (i + 1) as f32, (i + 2) as f32];
            let posting = Posting::new(doc_id, i * 100, 50, vector, 3).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Remove some postings
        shard.remove_posting(1).unwrap();
        shard.remove_posting(3).unwrap();

        // Validate integrity
        shard.validate_integrity().unwrap();

        // Check that counts are consistent
        assert_eq!(shard.current_count(), 5);
        assert_eq!(shard.active_count(), 3);
    }

    #[test]
    fn test_shard_open_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        // Try to open non-existent shard
        let result = Shard::open(shard_id, temp_dir.path());
        assert!(matches!(result, Err(ShardexError::Shard(_))));
    }

    #[test]
    fn test_shard_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        let metadata = shard.metadata();
        assert_eq!(metadata.current_count, 0);
        assert_eq!(metadata.active_count, 0);
        assert!(!metadata.read_only);
        assert!(metadata.age().as_secs() < 2); // Should be very recent

        // Add a posting and check metadata update
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];
        let posting = Posting::new(doc_id, 100, 50, vector, 3).unwrap();
        shard.add_posting(posting).unwrap();

        let metadata = shard.metadata();
        assert_eq!(metadata.current_count, 1);
        assert_eq!(metadata.active_count, 1);
        assert!(metadata.utilization(shard.capacity()) > 0.0);
    }

    #[test]
    fn test_shard_out_of_bounds() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Try to access non-existent posting
        let result = shard.get_posting(0);
        assert!(matches!(result, Err(ShardexError::Config(_))));

        let result = shard.get_posting(5);
        assert!(matches!(result, Err(ShardexError::Config(_))));
    }

    #[test]
    fn test_shard_available_capacity() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 5, 2, temp_dir.path().to_path_buf()).unwrap();

        // Initially should have full capacity available
        assert_eq!(shard.available_capacity(), 5);
        assert_eq!(shard.remaining_capacity(), 5);

        // Add a posting
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0];
        let posting = Posting::new(doc_id, 100, 50, vector, 2).unwrap();
        shard.add_posting(posting).unwrap();

        // Available capacity should decrease
        assert_eq!(shard.available_capacity(), 4);
        assert_eq!(shard.remaining_capacity(), 4);

        // Add more postings
        for i in 1..5 {
            let doc_id = DocumentId::new();
            let vector = vec![i as f32, (i + 1) as f32];
            let posting = Posting::new(doc_id, i * 100, 50, vector, 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Should be full now
        assert_eq!(shard.available_capacity(), 0);
        assert!(shard.is_full());
    }

    #[test]
    fn test_shard_remove_document_single() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();

        // Add postings for different documents
        let posting1 = Posting::new(doc_id1, 100, 50, vec![1.0, 2.0, 3.0], 3).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 75, vec![4.0, 5.0, 6.0], 3).unwrap();
        let posting3 = Posting::new(doc_id1, 300, 25, vec![7.0, 8.0, 9.0], 3).unwrap(); // Same doc as posting1

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();
        shard.add_posting(posting3).unwrap();

        assert_eq!(shard.active_count(), 3);

        // Remove all postings for doc_id1
        let removed_count = shard.remove_document(doc_id1).unwrap();
        assert_eq!(removed_count, 2); // Should remove posting1 and posting3

        assert_eq!(shard.current_count(), 3); // Still 3 total
        assert_eq!(shard.active_count(), 1); // Only 1 active (posting2)

        // Verify the right posting remains active
        assert!(!shard.is_deleted(1).unwrap()); // posting2 at index 1 should be active
        assert!(shard.is_deleted(0).unwrap()); // posting1 at index 0 should be deleted
        assert!(shard.is_deleted(2).unwrap()); // posting3 at index 2 should be deleted
    }

    #[test]
    fn test_shard_remove_document_multiple() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();

        // Add multiple postings for the same document
        for i in 0..5 {
            let vector = vec![i as f32, (i + 1) as f32];
            let posting = Posting::new(doc_id, i * 100, 50, vector, 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        assert_eq!(shard.active_count(), 5);

        // Remove all postings for the document
        let removed_count = shard.remove_document(doc_id).unwrap();
        assert_eq!(removed_count, 5);

        assert_eq!(shard.current_count(), 5); // Still 5 total
        assert_eq!(shard.active_count(), 0); // None active
    }

    #[test]
    fn test_shard_remove_document_nonexistent() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new(); // This one won't be added

        // Add a posting
        let posting = Posting::new(doc_id1, 100, 50, vec![1.0, 2.0, 3.0], 3).unwrap();
        shard.add_posting(posting).unwrap();

        // Try to remove a document that doesn't exist
        let removed_count = shard.remove_document(doc_id2).unwrap();
        assert_eq!(removed_count, 0);

        // Original posting should still be there
        assert_eq!(shard.active_count(), 1);
        assert!(!shard.is_deleted(0).unwrap());
    }

    #[test]
    fn test_shard_remove_document_read_only() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let directory = temp_dir.path().to_path_buf();

        // Create shard with data
        {
            let mut shard = Shard::create(shard_id, 5, 2, directory.clone()).unwrap();
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 100, 50, vec![1.0, 2.0], 2).unwrap();
            shard.add_posting(posting).unwrap();
            shard.sync().unwrap();
        }

        // Open in read-only mode
        {
            let mut shard = Shard::open_read_only(shard_id, &directory).unwrap();
            let doc_id = DocumentId::new();

            // Should fail to remove from read-only shard
            let result = shard.remove_document(doc_id);
            assert!(matches!(result, Err(ShardexError::Config(_))));
        }
    }

    #[test]
    fn test_shard_search_basic() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add postings with known vectors
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let doc_id3 = DocumentId::new();

        let posting1 = Posting::new(doc_id1, 100, 50, vec![1.0, 0.0, 0.0], 3).unwrap(); // Unit vector along x-axis
        let posting2 = Posting::new(doc_id2, 200, 75, vec![0.0, 1.0, 0.0], 3).unwrap(); // Unit vector along y-axis
        let posting3 = Posting::new(doc_id3, 300, 25, vec![0.0, 0.0, 1.0], 3).unwrap(); // Unit vector along z-axis

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();
        shard.add_posting(posting3).unwrap();

        // Query with x-axis vector - should match posting1 best
        let query = vec![1.0, 0.0, 0.0];
        let results = shard.search(&query, 3).unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].document_id, doc_id1); // Best match first
        assert_eq!(results[0].similarity_score, 1.0); // Perfect match

        // The other two should have similarity score of 0.5 (orthogonal vectors in normalized range)
        assert_eq!(results[1].similarity_score, 0.5);
        assert_eq!(results[2].similarity_score, 0.5);
    }

    #[test]
    fn test_shard_search_k_limit() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add multiple postings with unit vectors for predictable cosine similarity
        let vectors = [
            [1.0, 0.0],  // Same direction as query
            [0.0, 1.0],  // Orthogonal to query
            [-1.0, 0.0], // Opposite to query
            [0.5, 0.5],  // At 45 degrees
            [1.0, 1.0],  // At 45 degrees but not normalized
        ];

        for (i, vector) in vectors.iter().enumerate() {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, i as u32 * 100, 50, vector.to_vec(), 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Query for top 2 results
        let query = vec![1.0, 0.0]; // Unit vector in x direction
        let results = shard.search(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        // Results should be sorted by similarity (highest first)
        assert!(results[0].similarity_score >= results[1].similarity_score);
        // All scores should be between 0.0 and 1.0
        for result in &results {
            assert!(
                result.similarity_score >= 0.0 && result.similarity_score <= 1.0,
                "Similarity score {} is out of valid range [0.0, 1.0]",
                result.similarity_score
            );
        }
    }

    #[test]
    fn test_shard_search_with_append_only_updates() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();

        // Add initial postings
        let posting1 = Posting::new(doc_id1, 100, 50, vec![1.0, 0.0], 2).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 75, vec![0.0, 1.0], 2).unwrap();

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();

        // Add "updated" posting for doc_id1 with same start/length (append-only semantics)
        let updated_posting1 = Posting::new(doc_id1, 100, 50, vec![0.5, 0.5], 2).unwrap();
        shard.add_posting(updated_posting1).unwrap();

        // Search should return only the unique postings (most recent version of doc_id1)
        let query = vec![1.0, 1.0];
        let results = shard.search(&query, 10).unwrap();

        // Should return 2 unique postings (one for each document_id)
        assert_eq!(results.len(), 2);

        // Find the results by document_id
        let doc1_result = results.iter().find(|r| r.document_id == doc_id1).unwrap();
        let doc2_result = results.iter().find(|r| r.document_id == doc_id2).unwrap();

        // doc_id1 should have the updated vector [0.5, 0.5] (most recent version)
        assert_eq!(doc1_result.start, 100);
        assert_eq!(doc1_result.length, 50);
        assert_eq!(doc2_result.document_id, doc_id2);
    }

    #[test]
    fn test_shard_search_dimension_validation() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Query with wrong dimension should fail
        let wrong_query = vec![1.0, 2.0]; // 2D instead of 3D
        let result = shard.search(&wrong_query, 5);

        match result {
            Err(ShardexError::InvalidDimension { expected, actual }) => {
                assert_eq!(expected, 3);
                assert_eq!(actual, 2);
            }
            _ => panic!("Expected InvalidDimension error"),
        }
    }

    #[test]
    fn test_shard_search_empty_k() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add a posting
        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 100, 50, vec![1.0, 2.0], 2).unwrap();
        shard.add_posting(posting).unwrap();

        // Search with k=0 should return empty results
        let query = vec![1.0, 2.0];
        let results = shard.search(&query, 0).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_shard_search_empty_shard() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Search empty shard should return empty results
        let query = vec![1.0, 2.0];
        let results = shard.search(&query, 5).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_cosine_similarity_calculation() {
        // Test the internal cosine similarity function via search
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();
        // Use unit vector for predictable cosine similarity
        let posting = Posting::new(doc_id, 100, 50, vec![1.0, 0.0], 2).unwrap();
        shard.add_posting(posting).unwrap();

        // Query with same direction - should give cosine similarity = 1.0 -> normalized to 1.0
        let query = vec![2.0, 0.0]; // Same direction, different magnitude
        let results = shard.search(&query, 1).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].similarity_score, 1.0); // Perfect similarity

        // Test orthogonal vectors - should give cosine similarity = 0.0 -> normalized to 0.5
        let doc_id2 = DocumentId::new();
        let posting2 = Posting::new(doc_id2, 200, 50, vec![0.0, 1.0], 2).unwrap();
        shard.add_posting(posting2).unwrap();

        let results2 = shard.search(&query, 2).unwrap();
        assert_eq!(results2.len(), 2);
        assert_eq!(results2[0].similarity_score, 1.0); // First posting (same direction)
        assert_eq!(results2[1].similarity_score, 0.5); // Second posting (orthogonal)
    }

    #[test]
    fn test_centroid_calculation_empty_shard() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Empty shard should have zero centroid
        let centroid = shard.calculate_centroid();
        assert_eq!(centroid, vec![0.0, 0.0, 0.0]);
        assert_eq!(shard.get_centroid(), &[0.0, 0.0, 0.0]);
        assert_eq!(shard.active_vector_count, 0);
    }

    #[test]
    fn test_centroid_single_vector() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add single vector
        let doc_id = DocumentId::new();
        let vector = vec![2.0, 4.0, 6.0];
        let posting = Posting::new(doc_id, 100, 50, vector.clone(), 3).unwrap();
        shard.add_posting(posting).unwrap();

        // Centroid should equal the single vector
        assert_eq!(shard.get_centroid(), &vector);
        assert_eq!(shard.active_vector_count, 1);

        // Calculate centroid should also return the same
        let calculated = shard.calculate_centroid();
        assert_eq!(calculated, vector);
    }

    #[test]
    fn test_centroid_multiple_vectors() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let doc_id3 = DocumentId::new();

        // Add three vectors: [1, 2], [3, 4], [5, 6]
        // Expected centroid: [(1+3+5)/3, (2+4+6)/3] = [3.0, 4.0]
        let vectors = [vec![1.0, 2.0], vec![3.0, 4.0], vec![5.0, 6.0]];

        let posting1 = Posting::new(doc_id1, 100, 50, vectors[0].clone(), 2).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 50, vectors[1].clone(), 2).unwrap();
        let posting3 = Posting::new(doc_id3, 300, 50, vectors[2].clone(), 2).unwrap();

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();
        shard.add_posting(posting3).unwrap();

        let expected_centroid = [3.0, 4.0];
        let centroid = shard.get_centroid();

        assert_eq!(centroid.len(), 2);
        assert_eq!(shard.active_vector_count, 3);

        // Use approximate comparison for floating-point
        assert!((centroid[0] - expected_centroid[0]).abs() < 1e-6);
        assert!((centroid[1] - expected_centroid[1]).abs() < 1e-6);
    }

    #[test]
    fn test_centroid_incremental_updates() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Test incremental add
        shard.update_centroid_add(&[2.0, 4.0]);
        assert_eq!(shard.get_centroid(), &[2.0, 4.0]);
        assert_eq!(shard.active_vector_count, 1);

        shard.update_centroid_add(&[4.0, 2.0]);
        assert_eq!(shard.get_centroid(), &[3.0, 3.0]); // Mean of [2,4] and [4,2]
        assert_eq!(shard.active_vector_count, 2);

        shard.update_centroid_add(&[0.0, 6.0]);
        assert_eq!(shard.get_centroid(), &[2.0, 4.0]); // Mean of [2,4], [4,2], [0,6]
        assert_eq!(shard.active_vector_count, 3);

        // Test incremental remove
        shard.update_centroid_remove(&[0.0, 6.0]);
        assert_eq!(shard.get_centroid(), &[3.0, 3.0]); // Back to mean of [2,4] and [4,2]
        assert_eq!(shard.active_vector_count, 2);

        shard.update_centroid_remove(&[4.0, 2.0]);
        assert_eq!(shard.get_centroid(), &[2.0, 4.0]); // Back to just [2,4]
        assert_eq!(shard.active_vector_count, 1);

        shard.update_centroid_remove(&[2.0, 4.0]);
        assert_eq!(shard.get_centroid(), &[0.0, 0.0]); // Empty
        assert_eq!(shard.active_vector_count, 0);
    }

    #[test]
    fn test_centroid_with_deleted_vectors() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let doc_id3 = DocumentId::new();

        // Add vectors
        let posting1 = Posting::new(doc_id1, 100, 50, vec![1.0, 2.0], 2).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 50, vec![3.0, 4.0], 2).unwrap();
        let posting3 = Posting::new(doc_id3, 300, 50, vec![5.0, 6.0], 2).unwrap();

        let idx1 = shard.add_posting(posting1).unwrap();
        let idx2 = shard.add_posting(posting2).unwrap();
        shard.add_posting(posting3).unwrap();

        // Initial centroid should be mean of all vectors: [3.0, 4.0]
        let initial_centroid = shard.get_centroid().to_vec();
        assert!((initial_centroid[0] - 3.0).abs() < 1e-6);
        assert!((initial_centroid[1] - 4.0).abs() < 1e-6);
        assert_eq!(shard.active_vector_count, 3);

        // Remove middle vector [3.0, 4.0]
        shard.remove_posting(idx2).unwrap();

        // Centroid should now be mean of [1,2] and [5,6]: [3.0, 4.0]
        let updated_centroid = shard.get_centroid().to_vec();
        assert!((updated_centroid[0] - 3.0).abs() < 1e-6);
        assert!((updated_centroid[1] - 4.0).abs() < 1e-6);
        assert_eq!(shard.active_vector_count, 2);

        // Remove first vector [1.0, 2.0]
        shard.remove_posting(idx1).unwrap();

        // Centroid should now just be [5.0, 6.0]
        let final_centroid = shard.get_centroid().to_vec();
        assert!((final_centroid[0] - 5.0).abs() < 1e-6);
        assert!((final_centroid[1] - 6.0).abs() < 1e-6);
        assert_eq!(shard.active_vector_count, 1);
    }

    #[test]
    fn test_centroid_recalculation() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();

        // Add vectors through regular posting API
        let posting1 = Posting::new(doc_id1, 100, 50, vec![2.0, 4.0], 2).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 50, vec![6.0, 8.0], 2).unwrap();
        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();

        let initial_centroid = shard.get_centroid().to_vec();

        // Artificially corrupt the centroid and count
        shard.centroid = vec![999.0, 999.0];
        shard.active_vector_count = 999;

        // Recalculate should restore correct centroid
        shard.recalculate_centroid();

        let corrected_centroid = shard.get_centroid().to_vec();
        assert!((corrected_centroid[0] - initial_centroid[0]).abs() < 1e-6);
        assert!((corrected_centroid[1] - initial_centroid[1]).abs() < 1e-6);
        assert_eq!(shard.active_vector_count, 2);
    }

    #[test]
    fn test_centroid_remove_document() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();

        // Add multiple postings for same document
        let posting1 = Posting::new(doc_id1, 100, 50, vec![1.0, 2.0], 2).unwrap();
        let posting2 = Posting::new(doc_id1, 200, 50, vec![3.0, 4.0], 2).unwrap();
        let posting3 = Posting::new(doc_id2, 300, 50, vec![5.0, 6.0], 2).unwrap();

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();
        shard.add_posting(posting3).unwrap();

        // Initial centroid: mean of [1,2], [3,4], [5,6] = [3.0, 4.0]
        let initial_centroid = shard.get_centroid().to_vec();
        assert!((initial_centroid[0] - 3.0).abs() < 1e-6);
        assert!((initial_centroid[1] - 4.0).abs() < 1e-6);
        assert_eq!(shard.active_vector_count, 3);

        // Remove all postings for doc_id1 (vectors [1,2] and [3,4])
        let removed_count = shard.remove_document(doc_id1).unwrap();
        assert_eq!(removed_count, 2);

        // Centroid should now just be [5.0, 6.0]
        let final_centroid = shard.get_centroid().to_vec();
        assert!((final_centroid[0] - 5.0).abs() < 1e-6);
        assert!((final_centroid[1] - 6.0).abs() < 1e-6);
        assert_eq!(shard.active_vector_count, 1);
    }

    #[test]
    fn test_centroid_persistence_on_reopen() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let directory = temp_dir.path().to_path_buf();

        let expected_centroid = [3.0, 4.0];

        // Create and populate shard
        {
            let mut shard = Shard::create(shard_id, 10, 2, directory.clone()).unwrap();

            let doc_id1 = DocumentId::new();
            let doc_id2 = DocumentId::new();
            let doc_id3 = DocumentId::new();

            let posting1 = Posting::new(doc_id1, 100, 50, vec![1.0, 2.0], 2).unwrap();
            let posting2 = Posting::new(doc_id2, 200, 50, vec![3.0, 4.0], 2).unwrap();
            let posting3 = Posting::new(doc_id3, 300, 50, vec![5.0, 6.0], 2).unwrap();

            shard.add_posting(posting1).unwrap();
            shard.add_posting(posting2).unwrap();
            shard.add_posting(posting3).unwrap();

            shard.sync().unwrap();
        }

        // Reopen and verify centroid is recalculated correctly
        {
            let shard = Shard::open(shard_id, &directory).unwrap();

            let centroid = shard.get_centroid();
            assert!((centroid[0] - expected_centroid[0]).abs() < 1e-6);
            assert!((centroid[1] - expected_centroid[1]).abs() < 1e-6);
            assert_eq!(shard.active_vector_count, 3);
        }
    }

    #[test]
    fn test_centroid_integrity_validation() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 100, 50, vec![1.0, 2.0, 3.0], 3).unwrap();
        shard.add_posting(posting).unwrap();

        // Should pass validation initially
        assert!(shard.validate_integrity().is_ok());

        // Corrupt the centroid dimension
        shard.centroid = vec![1.0, 2.0]; // Wrong dimension

        // Should fail validation
        let result = shard.validate_integrity();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Centroid dimension mismatch"));

        // Fix dimension but corrupt the values
        shard.centroid = vec![999.0, 999.0, 999.0];

        // Should fail validation due to centroid accuracy
        let result = shard.validate_integrity();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Centroid component"));

        // Fix centroid but corrupt active vector count
        shard.centroid = vec![1.0, 2.0, 3.0]; // Correct centroid
        shard.active_vector_count = 999; // Wrong count

        // Should fail validation due to count mismatch
        let result = shard.validate_integrity();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Active vector count mismatch"));
    }

    #[test]
    fn test_shard_search_sorting() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 1, temp_dir.path().to_path_buf()).unwrap();

        // Add postings with different similarity scores using unit vectors for predictable results
        let vectors = [
            [1.0],  // Same direction as query [1.0] -> cosine = 1.0 -> normalized = 1.0
            [-1.0], // Opposite direction -> cosine = -1.0 -> normalized = 0.0
            [0.5],  // Same direction, smaller magnitude -> cosine = 1.0 -> normalized = 1.0
            [2.0],  // Same direction, larger magnitude -> cosine = 1.0 -> normalized = 1.0
        ];

        for (i, vector) in vectors.iter().enumerate() {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, i as u32 * 100, 50, vector.to_vec(), 1).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Query in positive direction
        let query = vec![1.0];
        let results = shard.search(&query, 4).unwrap();

        assert_eq!(results.len(), 4);

        // Results should be sorted by similarity score (descending)
        // Vectors in same direction (positive) should have similarity 1.0
        // Vector in opposite direction (negative) should have similarity 0.0
        assert_eq!(results[0].similarity_score, 1.0); // Same direction vectors
        assert_eq!(results[1].similarity_score, 1.0);
        assert_eq!(results[2].similarity_score, 1.0);
        assert_eq!(results[3].similarity_score, 0.0); // Opposite direction

        // Verify they are in descending order
        for i in 0..results.len() - 1 {
            assert!(results[i].similarity_score >= results[i + 1].similarity_score);
        }
    }

    #[test]
    fn test_shard_search_with_metric_cosine() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add postings with predictable vectors
        let vectors = [
            [1.0, 0.0, 0.0],  // Same direction as query
            [-1.0, 0.0, 0.0], // Opposite direction
            [0.0, 1.0, 0.0],  // Orthogonal
        ];

        for (i, vector) in vectors.iter().enumerate() {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, i as u32 * 100, 50, vector.to_vec(), 3).unwrap();
            shard.add_posting(posting).unwrap();
        }

        let query = vec![1.0, 0.0, 0.0];
        let results = shard
            .search_with_metric(&query, 3, DistanceMetric::Cosine)
            .unwrap();

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].similarity_score, 1.0); // Same direction
        assert_eq!(results[1].similarity_score, 0.5); // Orthogonal
        assert_eq!(results[2].similarity_score, 0.0); // Opposite direction
    }

    #[test]
    fn test_shard_search_with_metric_euclidean() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add postings at different distances from origin
        let vectors = [
            [0.0, 0.0], // Same as query (distance = 0)
            [1.0, 0.0], // Distance = 1
            [3.0, 4.0], // Distance = 5
        ];

        for (i, vector) in vectors.iter().enumerate() {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, i as u32 * 100, 50, vector.to_vec(), 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        let query = vec![0.0, 0.0];
        let results = shard
            .search_with_metric(&query, 3, DistanceMetric::Euclidean)
            .unwrap();

        assert_eq!(results.len(), 3);

        // Results should be sorted by similarity (descending)
        assert!(results[0].similarity_score > results[1].similarity_score);
        assert!(results[1].similarity_score > results[2].similarity_score);

        // Check that closer points have higher similarity
        assert_eq!(results[0].similarity_score, 1.0); // Same point: 1/(1+0) = 1.0
        assert!((results[1].similarity_score - 0.5).abs() < 1e-6); // Distance 1: 1/(1+1) = 0.5
        assert!((results[2].similarity_score - 1.0 / 6.0).abs() < 1e-6); // Distance 5: 1/(1+5) ≈ 0.167
    }

    #[test]
    fn test_shard_search_with_metric_dot_product() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add postings with different dot products
        let vectors = [
            [2.0, 0.0],  // Dot product with query [1,0] = 2.0 (positive)
            [0.0, 1.0],  // Dot product = 0.0 (orthogonal)
            [-1.0, 0.0], // Dot product = -1.0 (negative)
        ];

        for (i, vector) in vectors.iter().enumerate() {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, i as u32 * 100, 50, vector.to_vec(), 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        let query = vec![1.0, 0.0];
        let results = shard
            .search_with_metric(&query, 3, DistanceMetric::DotProduct)
            .unwrap();

        assert_eq!(results.len(), 3);

        // Results should be sorted by similarity (higher dot product = higher similarity)
        assert!(results[0].similarity_score > results[1].similarity_score);
        assert!(results[1].similarity_score > results[2].similarity_score);

        // Positive dot product should give high similarity, negative should give low
        assert!(results[0].similarity_score > 0.8); // Dot product = 2.0
        assert!((results[1].similarity_score - 0.5).abs() < 0.1); // Dot product = 0.0
        assert!(results[2].similarity_score < 0.4); // Dot product = -1.0
    }

    #[test]
    fn test_search_with_metric_dimension_validation() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add a posting
        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 0, 50, vec![1.0, 2.0, 3.0], 3).unwrap();
        shard.add_posting(posting).unwrap();

        // Test with wrong dimension query
        let wrong_query = vec![1.0, 2.0]; // 2D instead of 3D
        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let result = shard.search_with_metric(&wrong_query, 5, metric);
            assert!(result.is_err());
            assert!(matches!(
                result.unwrap_err(),
                ShardexError::InvalidDimension { .. }
            ));
        }
    }

    #[test]
    fn test_search_with_metric_empty_k() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add a posting
        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 0, 50, vec![1.0, 2.0, 3.0], 3).unwrap();
        shard.add_posting(posting).unwrap();

        let query = vec![1.0, 0.0, 0.0];

        // Search with k=0 should return empty results for all metrics
        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let results = shard.search_with_metric(&query, 0, metric).unwrap();
            assert!(results.is_empty());
        }
    }

    #[test]
    fn test_search_with_metric_vs_regular_search() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add postings
        let vectors = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];

        for (i, vector) in vectors.iter().enumerate() {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, i as u32 * 100, 50, vector.to_vec(), 3).unwrap();
            shard.add_posting(posting).unwrap();
        }

        let query = vec![1.0, 0.0, 0.0];

        // Compare regular search (which uses cosine) with explicit cosine search
        let regular_results = shard.search(&query, 3).unwrap();
        let cosine_results = shard
            .search_with_metric(&query, 3, DistanceMetric::Cosine)
            .unwrap();

        assert_eq!(regular_results.len(), cosine_results.len());

        // Results should be identical (same order, same similarity scores)
        for (regular, cosine) in regular_results.iter().zip(cosine_results.iter()) {
            assert_eq!(regular.document_id, cosine.document_id);
            assert!((regular.similarity_score - cosine.similarity_score).abs() < 1e-6);
        }
    }

    // ========== Shard Splitting Tests ==========

    #[test]
    fn test_should_split_empty_shard() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let shard = Shard::create(shard_id, 100, 3, temp_dir.path().to_path_buf()).unwrap();

        // Empty shard should not need splitting
        assert!(!shard.should_split());
    }

    #[test]
    fn test_should_split_below_threshold() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add 8 postings (80% capacity - below 90% threshold)
        for i in 0..8 {
            let doc_id = DocumentId::new();
            let vector = vec![i as f32, (i + 1) as f32, (i + 2) as f32];
            let posting = Posting::new(doc_id, i * 100, 50, vector, 3).unwrap();
            shard.add_posting(posting).unwrap();
        }

        assert!(!shard.should_split());
    }

    #[test]
    fn test_should_split_at_threshold() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add 9 postings (90% capacity - at threshold)
        for i in 0..9 {
            let doc_id = DocumentId::new();
            let vector = vec![i as f32, (i + 1) as f32, (i + 2) as f32];
            let posting = Posting::new(doc_id, i * 100, 50, vector, 3).unwrap();
            shard.add_posting(posting).unwrap();
        }

        assert!(shard.should_split());
    }

    #[test]
    fn test_should_split_zero_capacity() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        // This shouldn't be possible in practice, but test the edge case
        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();
        // Artificially set capacity to 0 for testing
        shard.capacity = 0;

        assert!(!shard.should_split());
    }

    #[tokio::test]
    async fn test_split_read_only_shard() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();
        let directory = temp_dir.path().to_path_buf();

        // Create and populate shard
        {
            let mut shard = Shard::create(shard_id, 10, 2, directory.clone()).unwrap();

            for i in 0..9 {
                let doc_id = DocumentId::new();
                let vector = vec![i as f32, (i + 1) as f32];
                let posting = Posting::new(doc_id, i * 100, 50, vector, 2).unwrap();
                shard.add_posting(posting).unwrap();
            }

            shard.sync().unwrap();
        }

        // Open in read-only mode and try to split
        {
            let shard = Shard::open_read_only(shard_id, &directory).unwrap();
            assert!(shard.should_split());

            let result = shard.split().await;
            assert!(matches!(result, Err(ShardexError::Config(_))));
        }
    }

    #[tokio::test]
    async fn test_split_insufficient_data() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add only one posting
        let doc_id = DocumentId::new();
        let vector = vec![1.0, 2.0];
        let posting = Posting::new(doc_id, 100, 50, vector, 2).unwrap();
        shard.add_posting(posting).unwrap();

        // Should fail to split with insufficient data
        let result = shard.split().await;
        assert!(matches!(result, Err(ShardexError::Config(_))));
    }

    #[tokio::test]
    async fn test_debug_split_capacity() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        for i in 0..9 {
            let doc_id = DocumentId::new();
            let vector = vec![i as f32, i as f32];
            let posting = Posting::new(doc_id, 100, 50, vector, 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        eprintln!("Before split:");
        eprintln!("  Original capacity: {}", shard.capacity());
        eprintln!("  Should split: {}", shard.should_split());

        let (shard_a, shard_b) = shard.split().await.unwrap();

        eprintln!("After split:");
        eprintln!("  Shard A capacity: {}", shard_a.capacity());
        eprintln!("  Shard B capacity: {}", shard_b.capacity());

        // For now, let's just see what the actual capacities are
        assert!(shard_a.capacity() > 0);
        assert!(shard_b.capacity() > 0);
    }

    #[tokio::test]
    async fn test_split_basic_functionality() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add vectors with clear clustering - two groups
        let group_a_vectors = [[1.0, 1.0], [1.1, 1.1], [1.2, 1.2], [0.9, 0.9]];

        let group_b_vectors = [[5.0, 5.0], [5.1, 5.1], [4.9, 4.9], [5.2, 5.0]];

        // Add group A vectors
        for vector in group_a_vectors {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 100, 50, vector.to_vec(), 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Add group B vectors
        for vector in group_b_vectors {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 200, 50, vector.to_vec(), 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Add one more to reach 90% threshold (9/10)
        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 300, 50, vec![2.5, 2.5], 2).unwrap();
        shard.add_posting(posting).unwrap();

        assert!(shard.should_split());
        assert_eq!(shard.active_count(), 9);

        // Perform split
        let (shard_a, shard_b) = shard.split().await.unwrap();

        // Verify both shards have data
        assert!(shard_a.active_count() > 0);
        assert!(shard_b.active_count() > 0);
        assert_eq!(shard_a.active_count() + shard_b.active_count(), 9);

        // Verify capacity distribution
        assert_eq!(shard_a.capacity(), 5); // Half of original
        assert_eq!(shard_b.capacity(), 5); // Half of original
        assert_eq!(shard_a.vector_size(), 2);
        assert_eq!(shard_b.vector_size(), 2);

        // Verify shards have different IDs
        assert_ne!(shard_a.id(), shard_b.id());
        assert_ne!(shard_a.id(), shard.id());
        assert_ne!(shard_b.id(), shard.id());
    }

    #[tokio::test]
    async fn test_split_balanced_distribution() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 20, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add 18 vectors (90% capacity) in two clear clusters
        for i in 0..9 {
            // Cluster A: around (0, 0)
            let doc_id_a = DocumentId::new();
            let vector_a = vec![i as f32 * 0.1, i as f32 * 0.1];
            let posting_a = Posting::new(doc_id_a, i * 100, 50, vector_a, 2).unwrap();
            shard.add_posting(posting_a).unwrap();

            // Cluster B: around (10, 10)
            let doc_id_b = DocumentId::new();
            let vector_b = vec![10.0 + i as f32 * 0.1, 10.0 + i as f32 * 0.1];
            let posting_b = Posting::new(doc_id_b, (i + 100) * 100, 50, vector_b, 2).unwrap();
            shard.add_posting(posting_b).unwrap();
        }

        let (shard_a, shard_b) = shard.split().await.unwrap();

        // Should be reasonably balanced (within 1 posting difference is fine)
        let count_diff = (shard_a.active_count() as i32 - shard_b.active_count() as i32).abs();
        assert!(
            count_diff <= 1,
            "Split should be balanced: {} vs {}",
            shard_a.active_count(),
            shard_b.active_count()
        );

        // Total count should be preserved
        assert_eq!(shard_a.active_count() + shard_b.active_count(), 18);
    }

    #[tokio::test]
    async fn test_split_with_append_only_updates() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 20, 2, temp_dir.path().to_path_buf()).unwrap();

        // Create some document IDs for reuse
        let doc_ids: Vec<DocumentId> = (0..15).map(|_| DocumentId::new()).collect();

        // Add 15 initial postings
        for (i, &doc_id) in doc_ids.iter().enumerate() {
            let vector = vec![i as f32, i as f32];
            let posting = Posting::new(doc_id, (i * 100) as u32, 50, vector, 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Add some "updated" postings with same document_id, start, length (append-only updates)
        for i in [1, 3, 5] {
            let updated_vector = vec![(i + 100) as f32, (i + 100) as f32];
            let updated_posting =
                Posting::new(doc_ids[i], (i * 100) as u32, 50, updated_vector, 2).unwrap();
            shard.add_posting(updated_posting).unwrap();
        }

        assert_eq!(shard.current_count(), 18); // Total including superseded postings
        assert_eq!(shard.active_count(), 15); // Only unique postings

        let (shard_a, shard_b) = shard.split().await.unwrap();

        // Only unique postings should be transferred
        assert_eq!(shard_a.active_count() + shard_b.active_count(), 15);
        assert!(shard_a.active_count() > 0);
        assert!(shard_b.active_count() > 0);
    }

    #[tokio::test]
    async fn test_split_small_dataset() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add exactly 4 postings (small dataset that triggers simple splitting)
        let vectors = [
            [1.0, 2.0, 3.0],
            [4.0, 5.0, 6.0],
            [7.0, 8.0, 9.0],
            [10.0, 11.0, 12.0],
        ];

        for vector in vectors {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 100, 50, vector.to_vec(), 3).unwrap();
            shard.add_posting(posting).unwrap();
        }

        let (shard_a, shard_b) = shard.split().await.unwrap();

        // Should split roughly in half
        assert_eq!(shard_a.active_count(), 2);
        assert_eq!(shard_b.active_count(), 2);
    }

    #[test]
    fn test_euclidean_distance_calculation() {
        // Test basic distance calculation
        let a = [0.0, 0.0];
        let b = [3.0, 4.0];
        let distance = Shard::euclidean_distance(&a, &b);
        assert!((distance - 5.0).abs() < 1e-6); // 3-4-5 triangle

        // Test same vectors
        let c = [1.0, 2.0, 3.0];
        let d = [1.0, 2.0, 3.0];
        let distance2 = Shard::euclidean_distance(&c, &d);
        assert!(distance2 < 1e-6); // Should be ~0

        // Test unit vectors
        let e = [1.0, 0.0];
        let f = [0.0, 1.0];
        let distance3 = Shard::euclidean_distance(&e, &f);
        assert!((distance3 - 2.0_f32.sqrt()).abs() < 1e-6);
    }

    #[tokio::test]
    async fn test_split_data_integrity() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 12, 2, temp_dir.path().to_path_buf()).unwrap();

        // Keep track of all postings we add
        let mut original_postings = Vec::new();

        for i in 0..10 {
            let doc_id = DocumentId::new();
            let vector = vec![i as f32, (i * 2) as f32];
            let posting = Posting::new(doc_id, i * 100 + 1000, 50 + i, vector, 2).unwrap();
            original_postings.push(posting.clone());
            shard.add_posting(posting).unwrap();
        }

        let (shard_a, shard_b) = shard.split().await.unwrap();

        // Collect all postings from both new shards
        let mut recovered_postings = Vec::new();

        for i in 0..shard_a.active_count() {
            recovered_postings.push(shard_a.get_posting(i).unwrap());
        }

        for i in 0..shard_b.active_count() {
            recovered_postings.push(shard_b.get_posting(i).unwrap());
        }

        // Verify all postings are preserved (order may differ)
        assert_eq!(recovered_postings.len(), original_postings.len());

        for original in &original_postings {
            let found = recovered_postings.iter().any(|recovered| {
                recovered.document_id == original.document_id
                    && recovered.start == original.start
                    && recovered.length == original.length
                    && recovered.vector == original.vector
            });
            assert!(found, "Original posting not found in recovered postings");
        }
    }

    #[tokio::test]
    async fn test_split_centroid_calculation() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        // Create two distinct clusters for predictable centroids
        let group_a = [[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]]; // Centroid should be ~[1.0, 1.0]
        let group_b = [[8.0, 8.0], [9.0, 9.0], [10.0, 10.0]]; // Centroid should be ~[9.0, 9.0]

        for vector in group_a.iter().chain(group_b.iter()) {
            let doc_id = DocumentId::new();
            let posting = Posting::new(doc_id, 100, 50, vector.to_vec(), 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        let (shard_a, shard_b) = shard.split().await.unwrap();

        // Verify both shards have valid centroids
        assert_eq!(shard_a.get_centroid().len(), 2);
        assert_eq!(shard_b.get_centroid().len(), 2);

        // Centroids should be different (not both zero)
        let centroid_a = shard_a.get_centroid();
        let centroid_b = shard_b.get_centroid();

        let distance_between_centroids = Shard::euclidean_distance(centroid_a, centroid_b);
        assert!(
            distance_between_centroids > 1.0,
            "Centroids should be well separated: A={:?}, B={:?}",
            centroid_a,
            centroid_b
        );
    }

    #[tokio::test]
    async fn test_split_minimum_capacity() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        // Create shard with small capacity
        let mut shard = Shard::create(shard_id, 6, 2, temp_dir.path().to_path_buf()).unwrap();

        for i in 0..6 {
            let doc_id = DocumentId::new();
            let vector = vec![i as f32, i as f32];
            let posting = Posting::new(doc_id, i * 100, 50, vector, 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        let (shard_a, shard_b) = shard.split().await.unwrap();

        // Even with small original capacity, new shards should have minimum capacity of 5
        assert_eq!(shard_a.capacity(), 5);
        assert_eq!(shard_b.capacity(), 5);
    }

    #[tokio::test]
    async fn test_split_identical_vectors() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 12, 2, temp_dir.path().to_path_buf()).unwrap();

        // Add 4 identical vectors (pathological case)
        for _i in 0..4 {
            let doc_id = DocumentId::new();
            let vector = vec![5.0, 5.0]; // All identical
            let posting = Posting::new(doc_id, 100, 50, vector, 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Split should still work even with identical vectors
        let (shard_a, shard_b) = shard.split().await.unwrap();

        // Should be split roughly evenly
        assert!(shard_a.active_count() > 0);
        assert!(shard_b.active_count() > 0);
        assert_eq!(shard_a.active_count() + shard_b.active_count(), 4);

        // Both centroids should be approximately the same (the identical vector)
        let centroid_a = shard_a.get_centroid();
        let centroid_b = shard_b.get_centroid();

        assert!((centroid_a[0] - 5.0).abs() < 0.1);
        assert!((centroid_a[1] - 5.0).abs() < 0.1);
        assert!((centroid_b[0] - 5.0).abs() < 0.1);
        assert!((centroid_b[1] - 5.0).abs() < 0.1);
    }

    #[test]
    fn test_bloom_filter_integration_basic() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 100, 3, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let doc_id3 = DocumentId::new(); // Not added
        let vector = vec![1.0, 2.0, 3.0];

        // Add postings
        let posting1 = Posting::new(doc_id1, 100, 50, vector.clone(), 3).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 75, vector, 3).unwrap();

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();

        // Test bloom filter contains inserted documents
        assert!(tokio_test::block_on(shard.contains_document(doc_id1)));
        assert!(tokio_test::block_on(shard.contains_document(doc_id2)));
        // This might be false or true due to potential false positives, but we'll test removal optimization
        let doc_id3_maybe_present = tokio_test::block_on(shard.contains_document(doc_id3));

        // Test removal optimization
        let removed = shard.remove_document(doc_id3).unwrap();
        if !doc_id3_maybe_present {
            // If bloom filter correctly identified doc_id3 as not present, no removal should occur
            assert_eq!(removed, 0);
        }

        // Test that actual documents can still be removed
        let removed = shard.remove_document(doc_id1).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(shard.active_count(), 1);
    }

    #[tokio::test]
    async fn test_bloom_filter_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];

        // Create shard and add postings
        {
            let mut shard = Shard::create(shard_id, 100, 3, temp_dir.path().to_path_buf()).unwrap();

            let posting1 = Posting::new(doc_id1, 100, 50, vector.clone(), 3).unwrap();
            let posting2 = Posting::new(doc_id2, 200, 75, vector, 3).unwrap();

            shard.add_posting(posting1).unwrap();
            shard.add_posting(posting2).unwrap();
            shard.sync().unwrap();
        }

        // Reopen shard and verify bloom filter is populated
        {
            let shard = Shard::open(shard_id, temp_dir.path()).unwrap();

            // Bloom filter should be populated from existing postings
            assert!(shard.contains_document(doc_id1).await);
            assert!(shard.contains_document(doc_id2).await);
        }
    }

    #[tokio::test]
    async fn test_bloom_filter_split_maintenance() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();
        let mut doc_ids = Vec::new();

        // Add multiple postings to trigger split
        for i in 0..9 {
            let doc_id = DocumentId::new();
            doc_ids.push(doc_id);
            let vector = vec![i as f32, i as f32];
            let posting = Posting::new(doc_id, i * 100, 50, vector, 2).unwrap();
            shard.add_posting(posting).unwrap();
        }

        // Split the shard
        let (mut shard_a, mut shard_b) = shard.split().await.unwrap();

        // Each split shard should have bloom filters containing only its documents
        let mut docs_in_a = 0;
        let mut docs_in_b = 0;

        for doc_id in &doc_ids {
            if shard_a.contains_document(*doc_id).await {
                docs_in_a += 1;
            }
            if shard_b.contains_document(*doc_id).await {
                docs_in_b += 1;
            }
        }

        // The total found should be at least the actual count (allowing for false positives)
        assert!(docs_in_a >= shard_a.active_count());
        assert!(docs_in_b >= shard_b.active_count());

        // Verify that the bloom filters are working by testing removal optimization
        let unknown_doc = DocumentId::new();

        // If bloom filters correctly identify unknown document as not present
        if !shard_a.contains_document(unknown_doc).await {
            let removed_a = shard_a.remove_document(unknown_doc).unwrap();
            assert_eq!(removed_a, 0);
        }
        if !shard_b.contains_document(unknown_doc).await {
            let removed_b = shard_b.remove_document(unknown_doc).unwrap();
            assert_eq!(removed_b, 0);
        }
    }

    #[tokio::test]
    async fn test_bloom_filter_false_positive_handling() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 100, 3, temp_dir.path().to_path_buf()).unwrap();

        // Add a few documents
        let doc_ids: Vec<_> = (0..10)
            .map(|i| {
                let doc_id = DocumentId::new();
                let vector = vec![i as f32, i as f32, i as f32];
                let posting = Posting::new(doc_id, i * 100, 50, vector, 3).unwrap();
                shard.add_posting(posting).unwrap();
                doc_id
            })
            .collect();

        // Test many unknown documents - some might return true due to false positives
        let mut false_positives = 0;
        let mut true_negatives = 0;

        for _ in 0..1000 {
            let unknown_doc = DocumentId::new();
            if shard.contains_document(unknown_doc).await {
                false_positives += 1;
                // Even with false positive, removal should return 0 since document isn't actually there
                let removed = shard.remove_document(unknown_doc).unwrap();
                assert_eq!(removed, 0);
            } else {
                true_negatives += 1;
            }
        }

        // Should have many true negatives and few false positives (1% expected rate)
        assert!(true_negatives > false_positives);
        // False positive rate should be reasonable (allow some variance)
        let fp_rate = false_positives as f64 / 1000.0;
        assert!(fp_rate < 0.05, "False positive rate too high: {}", fp_rate);

        // Original documents should still be found
        for doc_id in doc_ids {
            assert!(shard.contains_document(doc_id).await);
        }
    }

    #[test]
    fn test_bloom_filter_metadata_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 100, 3, temp_dir.path().to_path_buf()).unwrap();

        // Check initial bloom filter state
        assert_eq!(shard.metadata.bloom_filter.inserted_count(), 0);
        assert_eq!(shard.metadata.bloom_filter.capacity(), 100);
        assert!((shard.metadata.bloom_filter.false_positive_rate() - 0.01).abs() < 1e-6);

        // Add postings and verify bloom filter stats update
        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();
        let vector = vec![1.0, 2.0, 3.0];

        let posting1 = Posting::new(doc_id1, 100, 50, vector.clone(), 3).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 75, vector, 3).unwrap();

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();

        // Bloom filter should track insertions
        assert_eq!(shard.metadata.bloom_filter.inserted_count(), 2);
        assert!(!shard.metadata.bloom_filter.is_at_capacity());

        // Get statistics
        let stats = shard.metadata.bloom_filter.stats();
        assert_eq!(stats.inserted_count, 2);
        assert_eq!(stats.capacity, 100);
        assert!(stats.load_factor > 0.0 && stats.load_factor < 1.0);
        assert!(stats.memory_usage > 0);
    }

    #[test]
    fn test_append_only_semantics() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 100, 3, temp_dir.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();
        let vector1 = vec![1.0, 2.0, 3.0];
        let vector2 = vec![4.0, 5.0, 6.0];

        // Add initial posting
        let posting1 = Posting::new(doc_id, 100, 50, vector1, 3).unwrap();
        let _idx1 = shard.add_posting(posting1).unwrap();

        // Add "updated" posting with same document_id, start, length but different vector
        let posting2 = Posting::new(doc_id, 100, 50, vector2.clone(), 3).unwrap();
        let idx2 = shard.add_posting(posting2).unwrap();

        // Both postings should exist in storage
        assert_eq!(shard.current_count(), 2);

        // Search should only return the most recent posting (append-only semantics)
        let results = shard.search(&[4.0, 5.0, 6.0], 10).unwrap();
        assert_eq!(results.len(), 1); // Only one unique posting should be returned
        assert_eq!(results[0].document_id, doc_id);
        assert_eq!(results[0].start, 100);
        assert_eq!(results[0].length, 50);

        // Verify backward iteration returns only unique postings
        let unique_postings: Vec<_> = shard
            .iter_unique_postings_backward()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(unique_postings.len(), 1); // Only one unique posting
        assert_eq!(unique_postings[0].1.document_id, doc_id);

        // Verify that the unique posting uses the most recent vector (vector2)
        let (unique_index, _) = &unique_postings[0];
        assert_eq!(*unique_index, idx2); // Should be the second (more recent) index

        // Add another unique posting with different document_id
        let doc_id2 = DocumentId::new();
        let posting3 = Posting::new(doc_id2, 200, 30, vector2.clone(), 3).unwrap();
        shard.add_posting(posting3).unwrap();

        // Now we should have 2 unique postings
        let unique_postings: Vec<_> = shard
            .iter_unique_postings_backward()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(unique_postings.len(), 2);

        // Search should return both unique postings
        let results = shard.search(&[4.0, 5.0, 6.0], 10).unwrap();
        assert_eq!(results.len(), 2);
    }
}

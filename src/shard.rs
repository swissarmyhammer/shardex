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
    pub fn new(read_only: bool) -> Self {
        Self {
            created_at: SystemTime::now(),
            current_count: 0,
            active_count: 0,
            disk_usage: 0,
            read_only,
        }
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
        let mut metadata = ShardMetadata::new(false);
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
        let mut metadata =
            ShardMetadata::new(vector_storage.is_read_only() || posting_storage.is_read_only());
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
        let mut metadata = ShardMetadata::new(true);
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
        self.metadata.active_count
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
        let vector = self
            .vector_storage
            .get_vector(index)
            .map_err(|e| ShardexError::Shard(format!("Failed to get vector for centroid update: {}", e)))?;
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
    /// This method performs a linear scan through all active (non-deleted) postings,
    /// calculates dot product similarity scores, and returns the top K results
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

        // Scan through all postings
        for index in 0..self.current_count() {
            // Skip deleted postings
            if self.is_deleted(index)? {
                continue;
            }

            // Get the vector for this posting
            let vector = self
                .vector_storage
                .get_vector(index)
                .map_err(|e| ShardexError::Shard(format!("Failed to get vector: {}", e)))?;

            // Calculate cosine similarity 
            let similarity_score = Self::calculate_cosine_similarity(query, vector);

            // Get posting metadata
            let (document_id, start, length) = self
                .posting_storage
                .get_posting(index)
                .map_err(|e| ShardexError::Shard(format!("Failed to get posting: {}", e)))?;

            // Create search result
            let search_result = SearchResult::from_posting(
                Posting::new(document_id, start, length, vector.to_vec(), self.vector_size)?,
                similarity_score,
            )?;

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
        
        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        // Handle zero vectors to avoid division by zero
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.5; // Neutral similarity for zero vectors (maps to 0.0 cosine)
        }
        
        let cosine = dot_product / (norm_a * norm_b);
        
        // Clamp cosine to valid range to handle floating point precision issues
        let cosine = cosine.clamp(-1.0, 1.0);
        
        // Normalize to 0.0-1.0 range: (cosine + 1.0) / 2.0
        // This maps -1.0 -> 0.0, 0.0 -> 0.5, 1.0 -> 1.0
        (cosine + 1.0) / 2.0
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
                self.active_vector_count,
                actual_active_count
            )));
        }

        // Check centroid accuracy (with floating-point tolerance)
        const CENTROID_TOLERANCE: f32 = 1e-5;
        for (i, (&stored, &expected)) in self.centroid.iter().zip(expected_centroid.iter()).enumerate() {
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

    /// Calculate centroid from all non-deleted vectors
    ///
    /// This method performs a fresh calculation of the centroid by scanning all
    /// non-deleted vectors in the shard. The centroid is the mean position of all
    /// active vectors.
    ///
    /// Returns a vector representing the centroid, or a zero vector if no active vectors exist.
    pub fn calculate_centroid(&self) -> Vec<f32> {
        let mut centroid = vec![0.0; self.vector_size];
        let mut active_count = 0;

        // Sum all non-deleted vectors
        for i in 0..self.current_count() {
            if let Ok(false) = self.is_deleted(i) {
                if let Ok(vector) = self.vector_storage.get_vector(i) {
                    for (j, &value) in vector.iter().enumerate() {
                        centroid[j] += value;
                    }
                    active_count += 1;
                }
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
            for i in 0..self.vector_size {
                self.centroid[i] += (vector[i] - self.centroid[i]) / count_f32;
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
            for i in 0..self.vector_size {
                self.centroid[i] = (self.centroid[i] * old_count_f32 - vector[i]) / new_count_f32;
            }
        }
    }

    /// Recalculate centroid from scratch for accuracy
    ///
    /// This method performs a full recalculation of the centroid by scanning all vectors.
    /// Use this method periodically to correct any drift from incremental updates due to
    /// floating-point precision errors.
    pub fn recalculate_centroid(&mut self) {
        self.centroid = self.calculate_centroid();
        
        // Update active vector count to match actual count
        let mut count = 0;
        for i in 0..self.current_count() {
            if let Ok(false) = self.is_deleted(i) {
                count += 1;
            }
        }
        self.active_vector_count = count;
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
            assert!(result.similarity_score >= 0.0 && result.similarity_score <= 1.0,
                    "Similarity score {} is out of valid range [0.0, 1.0]", result.similarity_score);
        }
    }

    #[test]
    fn test_shard_search_with_deleted_postings() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 2, temp_dir.path().to_path_buf()).unwrap();

        let doc_id1 = DocumentId::new();
        let doc_id2 = DocumentId::new();

        // Add postings
        let posting1 = Posting::new(doc_id1, 100, 50, vec![1.0, 0.0], 2).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 75, vec![0.0, 1.0], 2).unwrap();

        let idx1 = shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();

        // Remove one posting
        shard.remove_posting(idx1).unwrap();

        // Search should only return the non-deleted posting
        let query = vec![1.0, 1.0];
        let results = shard.search(&query, 10).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document_id, doc_id2);
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
        let vectors = vec![
            vec![1.0, 2.0],
            vec![3.0, 4.0],
            vec![5.0, 6.0],
        ];

        let posting1 = Posting::new(doc_id1, 100, 50, vectors[0].clone(), 2).unwrap();
        let posting2 = Posting::new(doc_id2, 200, 50, vectors[1].clone(), 2).unwrap();
        let posting3 = Posting::new(doc_id3, 300, 50, vectors[2].clone(), 2).unwrap();

        shard.add_posting(posting1).unwrap();
        shard.add_posting(posting2).unwrap();
        shard.add_posting(posting3).unwrap();

        let expected_centroid = vec![3.0, 4.0];
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

        let expected_centroid = vec![3.0, 4.0];

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
        assert!(result.unwrap_err().to_string().contains("Centroid dimension mismatch"));

        // Fix dimension but corrupt the values
        shard.centroid = vec![999.0, 999.0, 999.0];

        // Should fail validation due to centroid accuracy
        let result = shard.validate_integrity();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Centroid component"));

        // Fix centroid but corrupt active vector count
        shard.centroid = vec![1.0, 2.0, 3.0]; // Correct centroid
        shard.active_vector_count = 999; // Wrong count

        // Should fail validation due to count mismatch
        let result = shard.validate_integrity();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Active vector count mismatch"));
    }



    #[test]
    fn test_shard_search_sorting() {
        let temp_dir = TempDir::new().unwrap();
        let shard_id = ShardId::new();

        let mut shard = Shard::create(shard_id, 10, 1, temp_dir.path().to_path_buf()).unwrap();

        // Add postings with different similarity scores using unit vectors for predictable results
        let vectors = [
            [1.0],   // Same direction as query [1.0] -> cosine = 1.0 -> normalized = 1.0
            [-1.0],  // Opposite direction -> cosine = -1.0 -> normalized = 0.0
            [0.5],   // Same direction, smaller magnitude -> cosine = 1.0 -> normalized = 1.0
            [2.0],   // Same direction, larger magnitude -> cosine = 1.0 -> normalized = 1.0
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
}

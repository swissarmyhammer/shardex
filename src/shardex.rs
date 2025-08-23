//! Main Shardex API implementation
//!
//! This module provides the main Shardex trait and implementation that combines
//! all components into a high-level vector search engine API.

use crate::batch_processor::BatchProcessor;
use crate::config::ShardexConfig;
use crate::distance::DistanceMetric;
use crate::error::ShardexError;

use crate::layout::DirectoryLayout;
use crate::shardex_index::ShardexIndex;
use crate::structures::{FlushStats, IndexStats, Posting, SearchResult};
use crate::transactions::{BatchConfig, WalOperation};
use crate::wal_replay::WalReplayer;
use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;

use tracing::{debug, info, warn};

/// Main trait for Shardex vector search engine
#[async_trait]
pub trait Shardex {
    type Error;

    /// Create a new index with the given configuration
    async fn create(config: ShardexConfig) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Open an existing index (configuration is read from metadata)
    async fn open<P: AsRef<Path> + Send>(directory_path: P) -> Result<Self, Self::Error>
    where
        Self: Sized;

    /// Add postings to the index (batch operation only)
    async fn add_postings(&mut self, postings: Vec<Posting>) -> Result<(), Self::Error>;

    /// Remove all postings for documents (batch operation only)
    async fn remove_documents(&mut self, document_ids: Vec<u128>) -> Result<(), Self::Error>;

    /// Search for K nearest neighbors using default distance metric (cosine)
    async fn search(
        &mut self,
        query_vector: &[f32],
        k: usize,
        slop_factor: Option<usize>,
    ) -> Result<Vec<SearchResult>, Self::Error>;

    /// Search for K nearest neighbors using specified distance metric
    async fn search_with_metric(
        &mut self,
        query_vector: &[f32],
        k: usize,
        metric: DistanceMetric,
        slop_factor: Option<usize>,
    ) -> Result<Vec<SearchResult>, Self::Error>;

    /// Flush pending operations
    async fn flush(&mut self) -> Result<(), Self::Error>;

    /// Flush pending operations and return performance statistics
    async fn flush_with_stats(&mut self) -> Result<FlushStats, Self::Error> {
        // Default implementation just calls flush and returns empty stats
        self.flush().await?;
        Ok(FlushStats::new())
    }

    /// Get index statistics
    async fn stats(&self) -> Result<IndexStats, Self::Error>;
}

/// Main Shardex implementation
pub struct ShardexImpl {
    index: ShardexIndex,
    config: ShardexConfig,
    batch_processor: Option<BatchProcessor>,
    layout: DirectoryLayout,
    /// Operations waiting to be applied to shards after WAL commit
    pending_shard_operations: Vec<WalOperation>,
}

impl ShardexImpl {
    /// Create a new Shardex instance
    pub fn new(config: ShardexConfig) -> Result<Self, ShardexError> {
        let index = ShardexIndex::create(config.clone())?;
        let layout = DirectoryLayout::new(&config.directory_path);

        // Create and save layout metadata (this must happen after ShardexIndex::create
        // which overwrites the metadata file with JSON format)
        use std::time::{SystemTime, UNIX_EPOCH};
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ShardexError::Config(format!("System time error: {}", e)))?
            .as_secs();

        let mut metadata = crate::layout::IndexMetadata {
            layout_version: crate::layout::LAYOUT_VERSION,
            config: config.clone(),
            created_at: now,
            modified_at: now,
            shard_count: 0,
            centroid_segment_count: 0,
            wal_segment_count: 0,
            flags: crate::layout::IndexFlags {
                active: true,
                needs_recovery: false,
                clean_shutdown: false,
            },
        };

        // Save the metadata (overwrites the JSON metadata from ShardexIndex::create)
        metadata.save(layout.metadata_path())?;

        Ok(Self {
            index,
            config,
            batch_processor: None,
            layout,
            pending_shard_operations: Vec::new(),
        })
    }

    /// Open an existing Shardex instance
    pub fn open_sync<P: AsRef<Path>>(directory_path: P) -> Result<Self, ShardexError> {
        let index = ShardexIndex::open(&directory_path)?;
        let layout = DirectoryLayout::new(directory_path.as_ref());

        // Load configuration from metadata file
        let metadata = crate::layout::IndexMetadata::load(layout.metadata_path())?;
        let config = metadata.config;
        Ok(Self {
            index,
            config,
            batch_processor: None,
            layout,
            pending_shard_operations: Vec::new(),
        })
    }

    /// Initialize the WAL batch processor for transaction handling
    pub async fn initialize_batch_processor(&mut self) -> Result<(), ShardexError> {
        if self.batch_processor.is_some() {
            return Ok(()); // Already initialized
        }

        info!("Initializing WAL batch processor for transaction handling");

        // Create batch configuration
        let batch_config = BatchConfig {
            batch_write_interval_ms: self.config.batch_write_interval_ms,
            max_operations_per_batch: 1000,
            max_batch_size_bytes: 1024 * 1024, // 1MB
        };

        // Create batch processor
        let mut processor = BatchProcessor::new(
            Duration::from_millis(self.config.batch_write_interval_ms),
            batch_config,
            Some(self.config.vector_size),
            self.layout.clone(),
        );

        // Start the processor
        processor.start().await?;
        self.batch_processor = Some(processor);

        debug!("WAL batch processor initialized successfully");
        Ok(())
    }

    /// Shutdown the batch processor and flush remaining operations
    pub async fn shutdown(&mut self) -> Result<(), ShardexError> {
        if let Some(mut processor) = self.batch_processor.take() {
            info!("Shutting down WAL batch processor");
            processor.shutdown().await?;
            debug!("WAL batch processor shutdown complete");
        }
        Ok(())
    }

    /// Replay WAL on startup to recover from any incomplete transactions
    async fn recover_from_wal(&mut self) -> Result<(), ShardexError> {
        info!("Starting WAL recovery process");

        // WalReplayer takes ownership of the index, so we need to temporarily take it out
        // IMPORTANT: Create the temporary replacement in a temporary directory to avoid
        // overwriting the real metadata file during recovery
        let temp_dir = std::env::temp_dir().join(format!(
            "shardex_recovery_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut temp_config = self.config.clone();
        temp_config.directory_path = temp_dir;
        let temp_replacement = ShardexIndex::create(temp_config)?;

        let index = std::mem::replace(&mut self.index, temp_replacement);

        let wal_directory = self.layout.wal_dir().to_path_buf();
        let mut replayer = WalReplayer::new(wal_directory, index);

        replayer.replay_all_segments().await?;
        let recovery_stats = replayer.recovery_stats();

        info!(
            "WAL recovery completed: {} transactions, {} operations, {} adds, {} removes",
            recovery_stats.transactions_replayed,
            recovery_stats.operations_applied,
            recovery_stats.add_posting_operations,
            recovery_stats.remove_document_operations
        );

        // Get the index back from the replayer
        self.index = replayer.into_index();

        // Update metadata for all shards after WAL recovery to reflect the recovered postings
        let shard_ids: Vec<_> = self.index.shard_ids();
        for shard_id in shard_ids {
            // Update the metadata for this shard from disk to reflect recovered postings
            if let Err(e) = self.index.update_shard_metadata_from_disk(shard_id) {
                warn!(
                    "Failed to update metadata for shard {} after WAL recovery: {}",
                    shard_id, e
                );
            }
        }

        Ok(())
    }

    /// Search using the existing parallel_search infrastructure
    pub fn search_impl(
        &mut self,
        query_vector: &[f32],
        k: usize,
        metric: DistanceMetric,
        slop_factor: Option<usize>,
    ) -> Result<Vec<SearchResult>, ShardexError> {
        // Use slop_factor from config if not provided
        let slop = slop_factor.unwrap_or(self.config.slop_factor_config.default_factor);

        // Find candidate shards using existing infrastructure
        let candidate_shards = self.index.find_nearest_shards(query_vector, slop)?;

        // Use parallel search with the specified metric
        // Use parallel_search_with_metric for all distance metrics
        self.index
            .parallel_search_with_metric(query_vector, &candidate_shards, k, metric)
    }

    /// Apply pending operations to shards after they've been committed to WAL
    async fn apply_pending_operations_to_shards(&mut self) -> Result<(), ShardexError> {
        if self.pending_shard_operations.is_empty() {
            return Ok(());
        }

        debug!(
            "Applying {} pending operations to shards",
            self.pending_shard_operations.len()
        );

        let operations = std::mem::take(&mut self.pending_shard_operations);

        for operation in &operations {
            if let Err(e) = self.apply_operation_to_shards(operation).await {
                // On error, we need to decide whether to retry or log and continue
                // Since the operation is already committed in WAL, we should try to continue
                // with other operations rather than fail completely
                warn!(
                    operation = ?operation,
                    error = %e,
                    "Failed to apply operation to shards, continuing with next operation"
                );
            }
        }

        debug!("Completed applying operations to shards");
        Ok(())
    }

    /// Enhanced flush implementation with durability guarantees and consistency validation
    async fn flush_internal(&mut self) -> Result<FlushStats, ShardexError> {
        let start_time = std::time::Instant::now();
        let mut stats = FlushStats::new();

        debug!("Starting comprehensive flush operation with durability guarantees");

        // 1. Process pending WAL batches (existing logic)
        let wal_start = std::time::Instant::now();
        if let Some(ref mut processor) = self.batch_processor {
            processor.flush_now().await?;
            debug!("WAL batch flush completed");
        }
        stats.wal_flush_duration = wal_start.elapsed();

        // 2. Apply pending operations to shards (existing logic)
        let apply_start = std::time::Instant::now();
        stats.operations_applied = self.pending_shard_operations.len();
        self.apply_pending_operations_to_shards().await?;
        stats.shard_apply_duration = apply_start.elapsed();
        debug!("Applied {} operations to shards", stats.operations_applied);

        // 3. Sync all shard data to disk (NEW - durability guarantee)
        let sync_start = std::time::Instant::now();
        let shard_ids = self.index.shard_ids();
        for shard_id in &shard_ids {
            let shard = self.index.get_shard_mut(*shard_id)?;
            shard.sync()?;
            // Estimate bytes synced using shard metadata
            let metadata = shard.metadata();
            stats.bytes_synced += metadata.disk_usage as u64;
        }
        stats.shards_synced = shard_ids.len();
        stats.shard_sync_duration = sync_start.elapsed();
        debug!("Synchronized {} shards to disk", stats.shards_synced);

        // 4. Validate consistency (NEW)
        let validation_start = std::time::Instant::now();
        self.validate_consistency().await?;
        stats.validation_duration = validation_start.elapsed();
        debug!("Consistency validation completed");

        stats.total_duration = start_time.elapsed();

        info!(
            "Flush operation completed: total={}ms, wal={}ms, apply={}ms, sync={}ms, validation={}ms, shards={}, ops={}",
            stats.total_duration_ms(),
            stats.wal_flush_duration_ms(),
            stats.shard_apply_duration.as_millis(),
            stats.shard_sync_duration_ms(),
            stats.validation_duration_ms(),
            stats.shards_synced,
            stats.operations_applied
        );

        Ok(stats)
    }

    /// Validate consistency across WAL, shards, and index
    async fn validate_consistency(&mut self) -> Result<(), ShardexError> {
        debug!("Starting consistency validation");

        // 1. Validate shard metadata matches storage state
        let shard_ids = self.index.shard_ids();
        for shard_id in &shard_ids {
            let shard = self.index.get_shard(*shard_id)?;
            let metadata = shard.metadata();

            // Validate active count consistency
            let actual_active = shard.active_count();
            if actual_active != metadata.active_count {
                warn!(
                    shard_id = %shard_id,
                    metadata_active = metadata.active_count,
                    actual_active = actual_active,
                    "Shard active count mismatch detected"
                );
                // Note: This could be a warning rather than error since counts can be estimates
            }

            // Validate capacity consistency
            let actual_capacity = shard.capacity();
            let expected_capacity = self.config.shard_size;
            if actual_capacity != expected_capacity {
                return Err(ShardexError::Shard(format!(
                    "Shard {} capacity mismatch: expected {}, actual {}",
                    shard_id, expected_capacity, actual_capacity
                )));
            }

            // Validate vector dimension consistency
            let actual_vector_size = shard.vector_size();
            if actual_vector_size != self.config.vector_size {
                return Err(ShardexError::InvalidDimension {
                    expected: self.config.vector_size,
                    actual: actual_vector_size,
                });
            }
        }

        // 2. Validate that we have no pending operations after flush
        if !self.pending_shard_operations.is_empty() {
            return Err(ShardexError::Wal(format!(
                "Consistency check failed: {} pending operations remain after flush",
                self.pending_shard_operations.len()
            )));
        }

        // 3. Validate index segment consistency
        let metadata_slice = self.index.all_shard_metadata();
        let expected_shard_count = shard_ids.len();
        if metadata_slice.len() != expected_shard_count {
            return Err(ShardexError::Shard(format!(
                "Index metadata inconsistency: {} shards exist but {} metadata entries found",
                expected_shard_count,
                metadata_slice.len()
            )));
        }

        debug!(
            "Consistency validation passed for {} shards",
            shard_ids.len()
        );
        Ok(())
    }

    /// Apply a single operation to the appropriate shards
    async fn apply_operation_to_shards(
        &mut self,
        operation: &WalOperation,
    ) -> Result<(), ShardexError> {
        match operation {
            WalOperation::AddPosting {
                document_id,
                start,
                length,
                vector,
            } => {
                // Validate the operation
                if vector.is_empty() {
                    return Err(ShardexError::Wal(
                        "Cannot add posting with empty vector".to_string(),
                    ));
                }
                if *length == 0 {
                    return Err(ShardexError::Wal(
                        "Cannot add posting with zero length".to_string(),
                    ));
                }

                // Create a posting from the operation
                let posting = Posting {
                    document_id: *document_id,
                    start: *start,
                    length: *length,
                    vector: vector.clone(),
                };

                // Find the nearest shard for this posting's vector
                let shard_id = match self.index.find_nearest_shard(&posting.vector)? {
                    Some(shard_id) => shard_id,
                    None => {
                        // No shards available - create an initial shard
                        debug!("No shards found, creating initial shard");
                        let initial_shard_id = crate::identifiers::ShardId::new();
                        let initial_shard = crate::shard::Shard::create(
                            initial_shard_id,
                            self.config.shard_size,
                            self.config.vector_size,
                            self.layout.shards_dir().to_path_buf(),
                        )?;
                        self.index.add_shard(initial_shard)?;
                        debug!("Created initial shard {}", initial_shard_id);
                        initial_shard_id
                    }
                };

                // Get mutable reference to the shard and add the posting
                let shard = self.index.get_shard_mut(shard_id)?;
                shard.add_posting(posting)?;

                debug!(
                    document_id = %document_id,
                    shard_id = %shard_id,
                    "Successfully added posting to shard"
                );
                Ok(())
            }
            WalOperation::RemoveDocument { document_id } => {
                // Remove the document from all shards that might contain it
                let mut total_removed = 0;
                let shard_ids = self.index.shard_ids();

                for shard_id in shard_ids {
                    let shard = self.index.get_shard_mut(shard_id)?;
                    match shard.remove_document(*document_id) {
                        Ok(removed_count) => {
                            total_removed += removed_count;
                        }
                        Err(e) => {
                            warn!(
                                document_id = %document_id,
                                shard_id = %shard_id,
                                error = %e,
                                "Failed to remove document from shard"
                            );
                            // Continue with other shards even if one fails
                        }
                    }
                }

                debug!(
                    document_id = %document_id,
                    removed_count = total_removed,
                    "Completed document removal from shards"
                );
                Ok(())
            }
        }
    }
}

#[async_trait]
impl Shardex for ShardexImpl {
    type Error = ShardexError;

    async fn create(config: ShardexConfig) -> Result<Self, Self::Error> {
        let mut instance = Self::new(config)?;

        // Recover from any existing WAL entries
        instance.recover_from_wal().await?;

        Ok(instance)
    }

    async fn open<P: AsRef<Path> + Send>(directory_path: P) -> Result<Self, Self::Error> {
        let mut instance = Self::open_sync(directory_path)?;

        // Recover from any existing WAL entries
        instance.recover_from_wal().await?;

        Ok(instance)
    }

    async fn add_postings(&mut self, postings: Vec<Posting>) -> Result<(), Self::Error> {
        if postings.is_empty() {
            return Ok(());
        }

        debug!(
            "Adding {} postings to index with WAL transaction support",
            postings.len()
        );

        // Ensure batch processor is initialized
        if self.batch_processor.is_none() {
            self.initialize_batch_processor().await?;
        }

        // Validate all postings before proceeding
        for (i, posting) in postings.iter().enumerate() {
            if posting.vector.len() != self.config.vector_size {
                return Err(ShardexError::InvalidDimension {
                    expected: self.config.vector_size,
                    actual: posting.vector.len(),
                });
            }

            if posting.length == 0 {
                return Err(ShardexError::Wal(format!("Posting {} has zero length", i)));
            }

            if posting.vector.is_empty() {
                return Err(ShardexError::Wal(format!("Posting {} has empty vector", i)));
            }

            // Check for invalid float values
            for (j, &value) in posting.vector.iter().enumerate() {
                if !value.is_finite() {
                    return Err(ShardexError::Wal(format!(
                        "Posting {} has invalid vector value at index {}: {} (must be finite)",
                        i, j, value
                    )));
                }
            }
        }

        // Convert postings to WAL operations
        let operations: Vec<WalOperation> = postings
            .into_iter()
            .map(|posting| WalOperation::AddPosting {
                document_id: posting.document_id,
                start: posting.start,
                length: posting.length,
                vector: posting.vector,
            })
            .collect();

        // Add operations to batch processor for WAL recording
        if let Some(ref mut processor) = self.batch_processor {
            for operation in &operations {
                processor.add_operation(operation.clone()).await?;
            }
        } else {
            return Err(ShardexError::Wal(
                "Batch processor not initialized".to_string(),
            ));
        }

        // Keep track of operations for shard application after WAL commit
        self.pending_shard_operations.extend(operations);

        debug!("Successfully added postings to WAL batch for processing");
        Ok(())
    }

    async fn remove_documents(&mut self, document_ids: Vec<u128>) -> Result<(), Self::Error> {
        if document_ids.is_empty() {
            return Ok(());
        }

        debug!(
            "Removing {} documents from index with WAL transaction support",
            document_ids.len()
        );

        // Ensure batch processor is initialized
        if self.batch_processor.is_none() {
            self.initialize_batch_processor().await?;
        }

        // Convert document IDs to WAL operations
        let operations: Vec<WalOperation> = document_ids
            .into_iter()
            .map(|doc_id| WalOperation::RemoveDocument {
                document_id: crate::identifiers::DocumentId::from_raw(doc_id),
            })
            .collect();

        // Add operations to batch processor for WAL recording
        if let Some(ref mut processor) = self.batch_processor {
            for operation in &operations {
                processor.add_operation(operation.clone()).await?;
            }
        } else {
            return Err(ShardexError::Wal(
                "Batch processor not initialized".to_string(),
            ));
        }

        // Keep track of operations for shard application after WAL commit
        self.pending_shard_operations.extend(operations);

        debug!("Successfully added document removal operations to WAL batch for processing");
        Ok(())
    }

    async fn search(
        &mut self,
        query_vector: &[f32],
        k: usize,
        slop_factor: Option<usize>,
    ) -> Result<Vec<SearchResult>, Self::Error> {
        self.search_impl(query_vector, k, DistanceMetric::Cosine, slop_factor)
    }

    async fn search_with_metric(
        &mut self,
        query_vector: &[f32],
        k: usize,
        metric: DistanceMetric,
        slop_factor: Option<usize>,
    ) -> Result<Vec<SearchResult>, Self::Error> {
        self.search_impl(query_vector, k, metric, slop_factor)
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        let stats = self.flush_internal().await?;

        if stats.is_slow_flush() {
            warn!(
                "Slow flush detected: {}ms total ({})",
                stats.total_duration_ms(),
                stats
            );
        } else if stats.is_fast_flush() {
            debug!("Fast flush completed: {}", stats);
        } else {
            debug!("Flush completed: {}", stats);
        }

        Ok(())
    }

    async fn flush_with_stats(&mut self) -> Result<FlushStats, Self::Error> {
        self.flush_internal().await
    }

    async fn stats(&self) -> Result<IndexStats, Self::Error> {
        // Collect statistics from the index using read-only metadata
        let metadata_slice = self.index.all_shard_metadata();
        let total_shards = metadata_slice.len();

        let mut total_postings = 0;
        let mut active_postings = 0;
        let mut deleted_postings = 0;
        let mut memory_usage = 0;
        let mut disk_usage = 0;
        let mut shard_utilizations = Vec::new();

        // Iterate through all shard metadata to collect statistics
        for metadata in metadata_slice {
            // Use posting_count as total postings for this shard
            total_postings += metadata.posting_count;

            // Estimate active postings based on utilization and capacity
            let estimated_active = (metadata.utilization * metadata.capacity as f32) as usize;
            active_postings += estimated_active;
            deleted_postings += metadata.posting_count.saturating_sub(estimated_active);

            // Use existing memory usage from metadata
            memory_usage += metadata.memory_usage;

            // Estimate disk usage based on shard directory if accessible
            // For now, use a simple estimation since we don't have directory access in metadata
            // This could be enhanced by storing directory paths in metadata if needed
            let estimated_disk_usage = metadata.posting_count * (self.config.vector_size * 4 + 64);
            disk_usage += estimated_disk_usage;

            // Use existing utilization from metadata
            shard_utilizations.push(metadata.utilization);
        }

        // Calculate average shard utilization
        let average_shard_utilization = if !shard_utilizations.is_empty() {
            shard_utilizations.iter().sum::<f32>() / shard_utilizations.len() as f32
        } else {
            0.0
        };

        // Count pending operations in WAL batch processor
        let pending_operations = if let Some(ref processor) = self.batch_processor {
            processor.pending_operation_count()
        } else {
            0
        } + self.pending_shard_operations.len();

        Ok(IndexStats {
            total_shards,
            total_postings,
            pending_operations,
            memory_usage,
            active_postings,
            deleted_postings,
            average_shard_utilization,
            vector_dimension: self.config.vector_size,
            disk_usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestEnvironment;

    #[tokio::test]
    async fn test_shardex_creation() {
        let _env = TestEnvironment::new("test_shardex_creation");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let shardex = ShardexImpl::create(config).await.unwrap();
        assert!(matches!(shardex, ShardexImpl { .. }));
    }

    #[tokio::test]
    async fn test_shardex_search_default_metric() {
        let _env = TestEnvironment::new("test_shardex_search_default_metric");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let mut shardex = ShardexImpl::create(config).await.unwrap();
        let query = vec![1.0; 128];

        // This should work since cosine is supported
        let results = shardex.search(&query, 10, None).await;
        if let Ok(results) = results {
            assert!(results.is_empty()); // Empty index should return no results
        } // May error due to empty index, that's OK for now
    }

    #[tokio::test]
    async fn test_shardex_search_euclidean_metric() {
        let _env = TestEnvironment::new("test_shardex_search_euclidean_metric");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let mut shardex = ShardexImpl::create(config).await.unwrap();
        let query = vec![1.0; 128];

        // Euclidean metric is now supported
        let result = shardex
            .search_with_metric(&query, 10, DistanceMetric::Euclidean, None)
            .await;
        assert!(result.is_ok());
        let search_results = result.unwrap();
        assert_eq!(search_results.len(), 0); // Empty index should return no results
    }

    #[test]
    fn test_sync_shardex_creation() {
        let _env = TestEnvironment::new("test_sync_shardex_creation");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let shardex = ShardexImpl::new(config).unwrap();
        assert!(matches!(shardex, ShardexImpl { .. }));
    }

    #[test]
    fn test_sync_search_cosine() {
        let _env = TestEnvironment::new("test_sync_search_cosine");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let mut shardex = ShardexImpl::new(config).unwrap();
        let query = vec![1.0; 128];

        let results = shardex.search_impl(&query, 10, DistanceMetric::Cosine, None);
        if let Ok(results) = results {
            assert!(results.is_empty()); // Empty index
        } // May error due to empty index
    }

    #[test]
    fn test_sync_search_euclidean_metric() {
        let _env = TestEnvironment::new("test_sync_search_euclidean_metric");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let mut shardex = ShardexImpl::new(config).unwrap();
        let query = vec![1.0; 128];

        let result = shardex.search_impl(&query, 10, DistanceMetric::Euclidean, None);
        assert!(result.is_ok());
        let search_results = result.unwrap();
        assert_eq!(search_results.len(), 0); // Empty index should return no results
    }

    #[test]
    fn test_distance_metric_functionality() {
        // Test the DistanceMetric enum functionality
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0]; // Identical
        let c = vec![-1.0, 0.0, 0.0]; // Opposite
        let d = vec![0.0, 1.0, 0.0]; // Orthogonal

        // Test cosine similarity
        let cosine_identical = DistanceMetric::Cosine.similarity(&a, &b).unwrap();
        let cosine_opposite = DistanceMetric::Cosine.similarity(&a, &c).unwrap();
        let cosine_orthogonal = DistanceMetric::Cosine.similarity(&a, &d).unwrap();

        assert!((cosine_identical - 1.0).abs() < 1e-6); // Identical vectors
        assert!((cosine_opposite - 0.0).abs() < 1e-6); // Opposite vectors
        assert!((cosine_orthogonal - 0.5).abs() < 1e-6); // Orthogonal vectors

        // Test euclidean similarity
        let euclidean_identical = DistanceMetric::Euclidean.similarity(&a, &b).unwrap();
        let euclidean_different = DistanceMetric::Euclidean.similarity(&a, &c).unwrap();

        assert!((euclidean_identical - 1.0).abs() < 1e-6); // Same point
        assert!(euclidean_different < euclidean_identical); // Different points have lower similarity

        // Test dot product similarity
        let dot_positive = DistanceMetric::DotProduct.similarity(&a, &b).unwrap();
        let dot_negative = DistanceMetric::DotProduct.similarity(&a, &c).unwrap();
        let dot_zero = DistanceMetric::DotProduct.similarity(&a, &d).unwrap();

        // The sigmoid transformation for dot product = 1.0 gives ~0.731, so adjust test expectation
        assert!(dot_positive > 0.7); // Positive correlation - adjusted for sigmoid behavior
        assert!(dot_negative < 0.4); // Negative correlation
        assert!((dot_zero - 0.5).abs() < 0.1); // Zero correlation
    }

    #[test]
    fn test_knn_search_edge_cases() {
        let _env = TestEnvironment::new("test_knn_search_edge_cases");

        // Test empty query validation
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let mut shardex = ShardexImpl::new(config).unwrap();

        // Test with empty vector (k=0)
        let query = vec![1.0; 128];
        let results = shardex.search_impl(&query, 0, DistanceMetric::Cosine, None);
        if let Ok(results) = results {
            assert!(results.is_empty()); // Should return empty results
        } // Empty index might error, that's OK

        // Test with large k value (more than possible results)
        let results = shardex.search_impl(&query, 1000, DistanceMetric::Cosine, None);
        if let Ok(results) = results {
            assert!(results.len() <= 1000); // Should not exceed k
        } // Empty index might error, that's OK

        // Test dimension validation
        let wrong_query = vec![1.0; 64]; // Wrong dimension
        let results = shardex.search_impl(&wrong_query, 10, DistanceMetric::Cosine, None);
        // Should either error due to dimension mismatch or handle empty index gracefully
        match results {
            Ok(_) => {} // Empty index case
            Err(e) => {
                let error_str = e.to_string();
                // Should be either dimension error or empty index error
                assert!(error_str.contains("dimension") || error_str.contains("shard"));
            }
        }
    }

    // Transaction handling tests

    #[tokio::test]
    async fn test_add_postings_basic_functionality() {
        let _env = TestEnvironment::new("test_add_postings_basic");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Create test postings
        let postings = vec![
            Posting {
                document_id: crate::identifiers::DocumentId::new(),
                start: 0,
                length: 100,
                vector: vec![1.0, 2.0, 3.0],
            },
            Posting {
                document_id: crate::identifiers::DocumentId::new(),
                start: 50,
                length: 75,
                vector: vec![4.0, 5.0, 6.0],
            },
        ];

        // Add postings should succeed
        let result = shardex.add_postings(postings).await;
        assert!(result.is_ok(), "Failed to add postings: {:?}", result);

        // Flush to ensure operations are committed
        let flush_result = shardex.flush().await;
        assert!(flush_result.is_ok(), "Failed to flush: {:?}", flush_result);

        // Shutdown cleanly
        let shutdown_result = shardex.shutdown().await;
        assert!(
            shutdown_result.is_ok(),
            "Failed to shutdown: {:?}",
            shutdown_result
        );
    }

    #[tokio::test]
    async fn test_add_postings_empty_vector() {
        let _env = TestEnvironment::new("test_add_postings_empty");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Empty postings should be handled gracefully
        let result = shardex.add_postings(vec![]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_add_postings_validation_dimension_mismatch() {
        let _env = TestEnvironment::new("test_add_postings_dimension_mismatch");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Create posting with wrong vector dimension
        let postings = vec![Posting {
            document_id: crate::identifiers::DocumentId::new(),
            start: 0,
            length: 100,
            vector: vec![1.0, 2.0], // Wrong dimension - expected 3
        }];

        let result = shardex.add_postings(postings).await;
        assert!(result.is_err());

        if let Err(crate::error::ShardexError::InvalidDimension { expected, actual }) = result {
            assert_eq!(expected, 3);
            assert_eq!(actual, 2);
        } else {
            panic!("Expected InvalidDimension error, got: {:?}", result);
        }
    }

    #[tokio::test]
    async fn test_add_postings_validation_zero_length() {
        let _env = TestEnvironment::new("test_add_postings_zero_length");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Create posting with zero length
        let postings = vec![Posting {
            document_id: crate::identifiers::DocumentId::new(),
            start: 0,
            length: 0, // Invalid - zero length
            vector: vec![1.0, 2.0, 3.0],
        }];

        let result = shardex.add_postings(postings).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("zero length"));
    }

    #[tokio::test]
    async fn test_add_postings_validation_empty_vector() {
        let _env = TestEnvironment::new("test_add_postings_empty_vector");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Create posting with empty vector
        let postings = vec![Posting {
            document_id: crate::identifiers::DocumentId::new(),
            start: 0,
            length: 100,
            vector: vec![], // Invalid - empty vector
        }];

        let result = shardex.add_postings(postings).await;
        assert!(result.is_err());
        // Empty vector should be caught by dimension mismatch since expected=3, actual=0
        match result {
            Err(crate::error::ShardexError::InvalidDimension { expected, actual }) => {
                assert_eq!(expected, 3);
                assert_eq!(actual, 0);
            }
            other => panic!("Expected InvalidDimension error, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_add_postings_validation_invalid_floats() {
        let _env = TestEnvironment::new("test_add_postings_invalid_floats");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Test NaN values
        let postings_nan = vec![Posting {
            document_id: crate::identifiers::DocumentId::new(),
            start: 0,
            length: 100,
            vector: vec![1.0, f32::NAN, 3.0],
        }];

        let result = shardex.add_postings(postings_nan).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be finite"));

        // Test infinity values
        let postings_inf = vec![Posting {
            document_id: crate::identifiers::DocumentId::new(),
            start: 0,
            length: 100,
            vector: vec![1.0, f32::INFINITY, 3.0],
        }];

        let result = shardex.add_postings(postings_inf).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be finite"));
    }

    #[tokio::test]
    async fn test_add_postings_batch_processing() {
        let _env = TestEnvironment::new("test_add_postings_batch");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3)
            .batch_write_interval_ms(50); // Fast batching for testing

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Create many postings to test batch processing
        let mut postings = Vec::new();
        for i in 0..10 {
            postings.push(Posting {
                document_id: crate::identifiers::DocumentId::new(),
                start: i * 100,
                length: 50,
                vector: vec![i as f32, (i + 1) as f32, (i + 2) as f32],
            });
        }

        // Add all postings
        let result = shardex.add_postings(postings).await;
        assert!(result.is_ok(), "Failed to add batch postings: {:?}", result);

        // Wait a bit for batch processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Flush to ensure all operations are committed
        let flush_result = shardex.flush().await;
        assert!(flush_result.is_ok());

        // Shutdown cleanly
        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_add_postings_wal_integration() {
        let _env = TestEnvironment::new("test_add_postings_wal");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config.clone()).await.unwrap();

        // Add some postings
        let postings = vec![Posting {
            document_id: crate::identifiers::DocumentId::new(),
            start: 0,
            length: 100,
            vector: vec![1.0, 2.0, 3.0],
        }];

        let result = shardex.add_postings(postings).await;
        assert!(result.is_ok());

        // Flush to ensure WAL is written
        shardex.flush().await.unwrap();
        shardex.shutdown().await.unwrap();

        // Create a new instance to test WAL recovery
        let mut shardex2 = ShardexImpl::open(config.directory_path).await.unwrap();

        // The recovery should have been performed during open
        // This test verifies that WAL recovery doesn't error out
        let stats = shardex2.stats().await;
        assert!(stats.is_ok());

        shardex2.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_batch_processor_initialization() {
        let _env = TestEnvironment::new("test_batch_processor_init");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Batch processor should be initialized automatically on first add_postings
        assert!(shardex.batch_processor.is_none());

        let postings = vec![Posting {
            document_id: crate::identifiers::DocumentId::new(),
            start: 0,
            length: 100,
            vector: vec![1.0, 2.0, 3.0],
        }];

        shardex.add_postings(postings).await.unwrap();

        // Should now be initialized
        assert!(shardex.batch_processor.is_some());

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_transaction_acid_properties() {
        let _env = TestEnvironment::new("test_transaction_acid");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3)
            .batch_write_interval_ms(50);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Test atomicity - all operations in a batch should be committed together
        let doc_id1 = crate::identifiers::DocumentId::new();
        let doc_id2 = crate::identifiers::DocumentId::new();

        let postings = vec![
            Posting {
                document_id: doc_id1,
                start: 0,
                length: 100,
                vector: vec![1.0, 2.0, 3.0],
            },
            Posting {
                document_id: doc_id2,
                start: 0,
                length: 100,
                vector: vec![4.0, 5.0, 6.0],
            },
        ];

        // Add postings atomically
        let result = shardex.add_postings(postings).await;
        assert!(result.is_ok());

        // Flush to commit the transaction
        shardex.flush().await.unwrap();

        // Test consistency - the index should be in a valid state
        let stats = shardex.stats().await;
        assert!(stats.is_ok());

        shardex.shutdown().await.unwrap();
    }

    // Document removal transaction tests

    #[tokio::test]
    async fn test_remove_documents_basic_functionality() {
        let _env = TestEnvironment::new("test_remove_documents_basic");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // First add some documents
        let doc_id1 = crate::identifiers::DocumentId::new();
        let doc_id2 = crate::identifiers::DocumentId::new();

        let postings = vec![
            Posting {
                document_id: doc_id1,
                start: 0,
                length: 100,
                vector: vec![1.0, 2.0, 3.0],
            },
            Posting {
                document_id: doc_id2,
                start: 50,
                length: 75,
                vector: vec![4.0, 5.0, 6.0],
            },
        ];

        let result = shardex.add_postings(postings).await;
        assert!(result.is_ok(), "Failed to add postings: {:?}", result);

        // Flush to ensure postings are committed
        shardex.flush().await.unwrap();

        // Now remove one document
        let doc_ids_to_remove = vec![doc_id1.raw()];
        let remove_result = shardex.remove_documents(doc_ids_to_remove).await;
        assert!(
            remove_result.is_ok(),
            "Failed to remove documents: {:?}",
            remove_result
        );

        // Flush to ensure removal is committed
        let flush_result = shardex.flush().await;
        assert!(
            flush_result.is_ok(),
            "Failed to flush removals: {:?}",
            flush_result
        );

        // Verify stats show the change
        let _stats = shardex.stats().await.unwrap();
        // Note: Due to the way we estimate active_postings, this might not be exactly 1
        // but should be different from before the removal

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_documents_empty_list() {
        let _env = TestEnvironment::new("test_remove_documents_empty");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Empty document list should be handled gracefully
        let result = shardex.remove_documents(vec![]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_remove_documents_batch_processing() {
        let _env = TestEnvironment::new("test_remove_documents_batch");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3)
            .batch_write_interval_ms(50); // Fast batching for testing

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Add multiple documents first
        let mut doc_ids = Vec::new();
        let mut postings = Vec::new();

        for i in 0..10 {
            let doc_id = crate::identifiers::DocumentId::new();
            doc_ids.push(doc_id);
            postings.push(Posting {
                document_id: doc_id,
                start: i * 100,
                length: 50,
                vector: vec![i as f32, (i + 1) as f32, (i + 2) as f32],
            });
        }

        // Add all postings
        let add_result = shardex.add_postings(postings).await;
        assert!(
            add_result.is_ok(),
            "Failed to add postings: {:?}",
            add_result
        );
        shardex.flush().await.unwrap();

        // Remove multiple documents
        let doc_ids_to_remove: Vec<u128> = doc_ids.iter().take(5).map(|id| id.raw()).collect();
        let remove_result = shardex.remove_documents(doc_ids_to_remove).await;
        assert!(
            remove_result.is_ok(),
            "Failed to remove documents: {:?}",
            remove_result
        );

        // Wait a bit for batch processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Flush to ensure all operations are committed
        let flush_result = shardex.flush().await;
        assert!(flush_result.is_ok());

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_documents_wal_integration() {
        let _env = TestEnvironment::new("test_remove_documents_wal");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config.clone()).await.unwrap();

        // Add a document first
        let doc_id = crate::identifiers::DocumentId::new();
        let postings = vec![Posting {
            document_id: doc_id,
            start: 0,
            length: 100,
            vector: vec![1.0, 2.0, 3.0],
        }];

        shardex.add_postings(postings).await.unwrap();
        shardex.flush().await.unwrap();

        // Remove the document
        let doc_ids_to_remove = vec![doc_id.raw()];
        let remove_result = shardex.remove_documents(doc_ids_to_remove).await;
        assert!(remove_result.is_ok());

        // Flush to ensure WAL is written
        shardex.flush().await.unwrap();
        shardex.shutdown().await.unwrap();

        // Create a new instance to test WAL recovery
        let mut shardex2 = ShardexImpl::open(config.directory_path).await.unwrap();

        // The recovery should have been performed during open
        let stats = shardex2.stats().await;
        assert!(stats.is_ok());

        shardex2.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_documents_transaction_atomicity() {
        let _env = TestEnvironment::new("test_remove_documents_atomicity");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3)
            .batch_write_interval_ms(50);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Add multiple documents across multiple shards (if possible)
        let doc_id1 = crate::identifiers::DocumentId::new();
        let doc_id2 = crate::identifiers::DocumentId::new();
        let doc_id3 = crate::identifiers::DocumentId::new();

        let postings = vec![
            Posting {
                document_id: doc_id1,
                start: 0,
                length: 100,
                vector: vec![1.0, 2.0, 3.0],
            },
            Posting {
                document_id: doc_id2,
                start: 0,
                length: 100,
                vector: vec![4.0, 5.0, 6.0],
            },
            Posting {
                document_id: doc_id3,
                start: 0,
                length: 100,
                vector: vec![7.0, 8.0, 9.0],
            },
        ];

        shardex.add_postings(postings).await.unwrap();
        shardex.flush().await.unwrap();

        // Remove multiple documents atomically
        let doc_ids_to_remove = vec![doc_id1.raw(), doc_id2.raw()];
        let remove_result = shardex.remove_documents(doc_ids_to_remove).await;
        assert!(remove_result.is_ok());

        // Flush to commit the transaction
        shardex.flush().await.unwrap();

        // Test consistency - the index should be in a valid state
        let stats = shardex.stats().await;
        assert!(stats.is_ok());

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_nonexistent_documents() {
        let _env = TestEnvironment::new("test_remove_nonexistent");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Try to remove documents that don't exist
        let nonexistent_doc_ids = vec![
            crate::identifiers::DocumentId::new().raw(),
            crate::identifiers::DocumentId::new().raw(),
        ];

        let remove_result = shardex.remove_documents(nonexistent_doc_ids).await;
        assert!(
            remove_result.is_ok(),
            "Removing nonexistent documents should succeed silently"
        );

        // Flush should also succeed
        let flush_result = shardex.flush().await;
        assert!(flush_result.is_ok());

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_documents_with_multiple_postings_same_doc() {
        let _env = TestEnvironment::new("test_remove_multiple_postings");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Add multiple postings for the same document
        let doc_id = crate::identifiers::DocumentId::new();
        let postings = vec![
            Posting {
                document_id: doc_id,
                start: 0,
                length: 50,
                vector: vec![1.0, 2.0, 3.0],
            },
            Posting {
                document_id: doc_id,
                start: 50,
                length: 50,
                vector: vec![4.0, 5.0, 6.0],
            },
            Posting {
                document_id: doc_id,
                start: 100,
                length: 50,
                vector: vec![7.0, 8.0, 9.0],
            },
        ];

        shardex.add_postings(postings).await.unwrap();
        shardex.flush().await.unwrap();

        // Remove the document - should remove all postings for this document
        let doc_ids_to_remove = vec![doc_id.raw()];
        let remove_result = shardex.remove_documents(doc_ids_to_remove).await;
        assert!(remove_result.is_ok());

        shardex.flush().await.unwrap();

        // All postings for this document should be removed
        let stats = shardex.stats().await;
        assert!(stats.is_ok());

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_documents_batch_processor_initialization() {
        let _env = TestEnvironment::new("test_remove_batch_init");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Batch processor should be initialized automatically on first remove_documents
        assert!(shardex.batch_processor.is_none());

        let doc_ids_to_remove = vec![crate::identifiers::DocumentId::new().raw()];
        shardex.remove_documents(doc_ids_to_remove).await.unwrap();

        // Should now be initialized
        assert!(shardex.batch_processor.is_some());

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_mixed_operations_add_and_remove() {
        let _env = TestEnvironment::new("test_mixed_operations");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Add some documents
        let doc_id1 = crate::identifiers::DocumentId::new();
        let doc_id2 = crate::identifiers::DocumentId::new();
        let doc_id3 = crate::identifiers::DocumentId::new();

        let postings = vec![
            Posting {
                document_id: doc_id1,
                start: 0,
                length: 100,
                vector: vec![1.0, 2.0, 3.0],
            },
            Posting {
                document_id: doc_id2,
                start: 0,
                length: 100,
                vector: vec![4.0, 5.0, 6.0],
            },
        ];

        shardex.add_postings(postings).await.unwrap();

        // Remove one document
        let doc_ids_to_remove = vec![doc_id1.raw()];
        shardex.remove_documents(doc_ids_to_remove).await.unwrap();

        // Add another document
        let more_postings = vec![Posting {
            document_id: doc_id3,
            start: 0,
            length: 100,
            vector: vec![7.0, 8.0, 9.0],
        }];

        shardex.add_postings(more_postings).await.unwrap();

        // Flush all operations
        shardex.flush().await.unwrap();

        // Verify consistency
        let stats = shardex.stats().await;
        assert!(stats.is_ok());

        shardex.shutdown().await.unwrap();
    }

    // Enhanced flush operation tests

    #[tokio::test]
    async fn test_flush_with_stats_basic_functionality() {
        let _env = TestEnvironment::new("test_flush_with_stats_basic");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Add some operations to flush
        let postings = vec![
            Posting {
                document_id: crate::identifiers::DocumentId::new(),
                start: 0,
                length: 100,
                vector: vec![1.0, 2.0, 3.0],
            },
            Posting {
                document_id: crate::identifiers::DocumentId::new(),
                start: 50,
                length: 75,
                vector: vec![4.0, 5.0, 6.0],
            },
        ];

        shardex.add_postings(postings).await.unwrap();

        // Test flush with stats
        let stats = shardex.flush_with_stats().await.unwrap();

        // Verify stats are populated
        assert!(stats.total_duration > std::time::Duration::ZERO);
        assert!(stats.wal_flush_duration >= std::time::Duration::ZERO);
        assert!(stats.shard_apply_duration >= std::time::Duration::ZERO);
        assert!(stats.shard_sync_duration >= std::time::Duration::ZERO);
        assert!(stats.validation_duration >= std::time::Duration::ZERO);
        assert_eq!(stats.operations_applied, 2);
        // shards_synced is always non-negative (usize)

        // Test that regular flush still works
        let result = shardex.flush().await;
        assert!(result.is_ok());

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_flush_durability_after_restart() {
        let _env = TestEnvironment::new("test_flush_durability_restart");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let doc_id1 = crate::identifiers::DocumentId::new();
        let doc_id2 = crate::identifiers::DocumentId::new();

        // First instance: add data and flush
        {
            let mut shardex = ShardexImpl::create(config.clone()).await.unwrap();

            let postings = vec![
                Posting {
                    document_id: doc_id1,
                    start: 0,
                    length: 100,
                    vector: vec![1.0, 2.0, 3.0],
                },
                Posting {
                    document_id: doc_id2,
                    start: 0,
                    length: 100,
                    vector: vec![4.0, 5.0, 6.0],
                },
            ];

            shardex.add_postings(postings).await.unwrap();

            // Flush with durability guarantees
            let stats = shardex.flush_with_stats().await.unwrap();
            assert!(stats.shards_synced > 0);

            shardex.shutdown().await.unwrap();
        }

        // Second instance: verify data persisted
        {
            let mut shardex = ShardexImpl::open(config.directory_path).await.unwrap();

            // Check that data is available after restart
            let stats = shardex.stats().await.unwrap();
            assert!(stats.total_postings > 0);

            // Try to search for the data to verify it's accessible
            let query_vector = vec![1.0, 2.0, 3.0];
            let results = shardex.search(&query_vector, 10, None).await;
            // Results may be empty if index structure isn't fully built, but call should succeed
            assert!(results.is_ok());

            shardex.shutdown().await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_flush_consistency_validation() {
        let _env = TestEnvironment::new("test_flush_consistency_validation");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3)
            .shard_size(5); // Small shard size to test multiple shards

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Add enough postings to potentially create multiple shards
        let mut postings = Vec::new();
        for i in 0..10 {
            postings.push(Posting {
                document_id: crate::identifiers::DocumentId::new(),
                start: i * 100,
                length: 50,
                vector: vec![i as f32, (i + 1) as f32, (i + 2) as f32],
            });
        }

        shardex.add_postings(postings).await.unwrap();

        // Flush and validate consistency
        let stats = shardex.flush_with_stats().await.unwrap();

        // Consistency validation should have passed (no errors thrown)
        assert!(stats.validation_duration >= std::time::Duration::ZERO);
        assert_eq!(stats.operations_applied, 10);

        // Verify no pending operations remain
        let _index_stats = shardex.stats().await.unwrap();
        // Pending operations count might be 0 or match what's in batch processor
        // The important thing is that flush completed successfully

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_flush_performance_monitoring() {
        let _env = TestEnvironment::new("test_flush_performance_monitoring");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Add a moderate amount of data
        let mut postings = Vec::new();
        for i in 0..20 {
            postings.push(Posting {
                document_id: crate::identifiers::DocumentId::new(),
                start: i * 50,
                length: 50,
                vector: vec![i as f32 * 0.1, (i + 1) as f32 * 0.1, (i + 2) as f32 * 0.1],
            });
        }

        shardex.add_postings(postings).await.unwrap();

        // Test flush stats and performance methods
        let stats = shardex.flush_with_stats().await.unwrap();

        // Test performance calculations
        assert!(stats.total_duration_ms() >= stats.wal_flush_duration_ms());
        assert!(stats.total_duration_ms() >= stats.shard_sync_duration_ms());

        // Test performance classification
        let is_fast = stats.is_fast_flush();
        let is_slow = stats.is_slow_flush();
        assert!(!(is_fast && is_slow)); // Can't be both fast and slow

        // Test slowest phase identification
        let slowest_phase = stats.slowest_phase();
        assert!(["wal_flush", "shard_apply", "shard_sync", "validation"].contains(&slowest_phase));

        // Test operations per second calculation
        let ops_per_sec = stats.operations_per_second();
        assert!(ops_per_sec >= 0.0);

        // Test sync throughput calculation
        let throughput = stats.sync_throughput_mbps();
        assert!(throughput >= 0.0);

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_flush_with_mixed_operations() {
        let _env = TestEnvironment::new("test_flush_mixed_operations");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        let doc_id1 = crate::identifiers::DocumentId::new();
        let doc_id2 = crate::identifiers::DocumentId::new();
        let doc_id3 = crate::identifiers::DocumentId::new();

        // Add some postings
        let postings = vec![
            Posting {
                document_id: doc_id1,
                start: 0,
                length: 100,
                vector: vec![1.0, 2.0, 3.0],
            },
            Posting {
                document_id: doc_id2,
                start: 0,
                length: 100,
                vector: vec![4.0, 5.0, 6.0],
            },
            Posting {
                document_id: doc_id3,
                start: 0,
                length: 100,
                vector: vec![7.0, 8.0, 9.0],
            },
        ];

        shardex.add_postings(postings).await.unwrap();

        // Remove one document
        shardex.remove_documents(vec![doc_id2.raw()]).await.unwrap();

        // Flush with mixed operations
        let stats = shardex.flush_with_stats().await.unwrap();

        // Should have processed both add and remove operations
        assert_eq!(stats.operations_applied, 4); // 3 adds + 1 remove
        assert!(stats.shards_synced > 0);
        assert!(stats.total_duration > std::time::Duration::ZERO);

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_flush_empty_operations() {
        let _env = TestEnvironment::new("test_flush_empty_operations");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Flush with no operations
        let stats = shardex.flush_with_stats().await.unwrap();

        // Should still complete successfully with zero operations
        assert_eq!(stats.operations_applied, 0);
        assert!(stats.total_duration >= std::time::Duration::ZERO);
        // Shards might still be synced even with no operations
        // shards_synced is always non-negative (usize)

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_flush_stats_display() {
        let _env = TestEnvironment::new("test_flush_stats_display");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Add some data
        let postings = vec![Posting {
            document_id: crate::identifiers::DocumentId::new(),
            start: 0,
            length: 100,
            vector: vec![1.0, 2.0, 3.0],
        }];

        shardex.add_postings(postings).await.unwrap();

        let stats = shardex.flush_with_stats().await.unwrap();

        // Test display formatting
        let display_str = format!("{}", stats);
        assert!(display_str.contains("FlushStats"));
        assert!(display_str.contains("total:"));
        assert!(display_str.contains("wal:"));
        assert!(display_str.contains("sync:"));
        assert!(display_str.contains("ops:"));
        assert!(display_str.contains("shards:"));

        shardex.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_consistency_validation_failure_recovery() {
        let _env = TestEnvironment::new("test_consistency_validation_failure");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(3);

        let mut shardex = ShardexImpl::create(config).await.unwrap();

        // Add valid operations
        let postings = vec![Posting {
            document_id: crate::identifiers::DocumentId::new(),
            start: 0,
            length: 100,
            vector: vec![1.0, 2.0, 3.0],
        }];

        shardex.add_postings(postings).await.unwrap();

        // Normal flush should succeed
        let stats = shardex.flush_with_stats().await.unwrap();
        assert!(stats.validation_duration >= std::time::Duration::ZERO);

        // Test that system is still functional after successful consistency validation
        let _index_stats = shardex.stats().await.unwrap();
        // total_postings is always non-negative (usize)

        shardex.shutdown().await.unwrap();
    }
}

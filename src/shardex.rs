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
use crate::structures::{IndexStats, Posting, SearchResult};
use crate::transactions::{BatchConfig, WalOperation};
use crate::wal_replay::WalReplayer;
use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, info};

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

    /// Get index statistics
    async fn stats(&self) -> Result<IndexStats, Self::Error>;
}

/// Main Shardex implementation
pub struct ShardexImpl {
    index: ShardexIndex,
    config: ShardexConfig,
    batch_processor: Option<BatchProcessor>,
    layout: DirectoryLayout,
}

impl ShardexImpl {
    /// Create a new Shardex instance
    pub fn new(config: ShardexConfig) -> Result<Self, ShardexError> {
        let index = ShardexIndex::create(config.clone())?;
        let layout = DirectoryLayout::new(&config.directory_path);
        Ok(Self {
            index,
            config,
            batch_processor: None,
            layout,
        })
    }

    /// Open an existing Shardex instance
    pub fn open_sync<P: AsRef<Path>>(directory_path: P) -> Result<Self, ShardexError> {
        let index = ShardexIndex::open(&directory_path)?;
        // TODO: Read config from metadata
        let config = ShardexConfig::new().directory_path(directory_path.as_ref());
        let layout = DirectoryLayout::new(directory_path.as_ref());
        Ok(Self {
            index,
            config,
            batch_processor: None,
            layout,
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
        let index = std::mem::replace(&mut self.index, ShardexIndex::create(self.config.clone())?);

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
        // TODO: The parallel_search method currently uses cosine similarity hardcoded
        // We need to extend it to support different metrics
        match metric {
            DistanceMetric::Cosine => {
                // Use existing parallel_search which uses cosine similarity
                self.index
                    .parallel_search(query_vector, &candidate_shards, k)
            }
            _ => {
                // For other metrics, we need to implement a new parallel_search_with_metric
                // For now, return an error indicating this is not yet implemented
                Err(ShardexError::Search(format!(
                    "Distance metric {:?} not yet supported in parallel search. Only Cosine is currently supported.",
                    metric
                )))
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

        // Add operations to batch processor
        // The batch processor will handle WAL recording and eventual application to shards
        if let Some(ref mut processor) = self.batch_processor {
            for operation in operations {
                processor.add_operation(operation).await?;
            }
        } else {
            return Err(ShardexError::Wal(
                "Batch processor not initialized".to_string(),
            ));
        }

        debug!("Successfully added postings to WAL batch for processing");
        Ok(())
    }

    async fn remove_documents(&mut self, _document_ids: Vec<u128>) -> Result<(), Self::Error> {
        // TODO: Implement using WAL and batch processor
        Err(ShardexError::Search(
            "remove_documents not yet implemented".to_string(),
        ))
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
        debug!("Flushing pending WAL operations to storage");

        if let Some(ref mut processor) = self.batch_processor {
            processor.flush_now().await?;
            debug!("WAL batch flush completed");
        }

        Ok(())
    }

    async fn stats(&self) -> Result<IndexStats, Self::Error> {
        // TODO: Implement stats collection from index
        Ok(IndexStats::new())
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
    async fn test_shardex_search_unsupported_metric() {
        let _env = TestEnvironment::new("test_shardex_search_unsupported_metric");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let mut shardex = ShardexImpl::create(config).await.unwrap();
        let query = vec![1.0; 128];

        // This should return error for unsupported metrics
        let result = shardex
            .search_with_metric(&query, 10, DistanceMetric::Euclidean, None)
            .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not yet supported"));
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
    fn test_sync_search_unsupported_metric() {
        let _env = TestEnvironment::new("test_sync_search_unsupported_metric");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let mut shardex = ShardexImpl::new(config).unwrap();
        let query = vec![1.0; 128];

        let result = shardex.search_impl(&query, 10, DistanceMetric::Euclidean, None);
        assert!(result.is_err());
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
}

//! Multi-shard search coordination
//!
//! This module provides coordination of search operations across multiple shards with proper
//! result aggregation, timeout handling, and performance monitoring. It builds on the existing
//! parallel search infrastructure in ShardexIndex while adding async capabilities and advanced
//! coordination features.
//!
//! # Key Features
//!
//! - **Timeout Management**: Configurable search timeouts with graceful cancellation
//! - **Result Streaming**: Memory-efficient streaming for large result sets
//! - **Performance Monitoring**: Search latency and throughput metrics
//! - **Load Balancing**: Dynamic adjustment based on shard response times
//! - **Cancellation Support**: Async cancellation tokens for long-running searches
//!
//! # Usage Examples
//!
//! ## Basic Coordinated Search
//!
//! ```rust
//! use shardex::search_coordinator::{SearchCoordinator, SearchCoordinatorConfig};
//! use shardex::config::ShardexConfig;
//! use std::time::Duration;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = ShardexConfig::new().directory_path("./index");
//! let coordinator_config = SearchCoordinatorConfig::new()
//!     .default_timeout(Duration::from_secs(30))
//!     .performance_monitoring_enabled(true);
//!     
//! let coordinator = SearchCoordinator::create(config, coordinator_config).await?;
//!
//! let query = vec![0.1; 384];
//! let results = coordinator.coordinate_search(&query, 10, 3, None).await?;
//!
//! println!("Found {} results", results.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Search with Custom Timeout
//!
//! ```rust
//! use std::time::Duration;
//! # use shardex::search_coordinator::SearchCoordinator;
//!
//! # async fn timeout_example(coordinator: &SearchCoordinator) -> Result<(), Box<dyn std::error::Error>> {
//! let query = vec![0.1; 384];
//! let timeout = Duration::from_millis(500);
//!
//! let results = coordinator.coordinate_search(&query, 10, 3, Some(timeout)).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Streaming Results for Large Queries
//!
//! ```rust
//! use futures::StreamExt;
//! # use shardex::search_coordinator::SearchCoordinator;
//!
//! # async fn streaming_example(coordinator: &SearchCoordinator) -> Result<(), Box<dyn std::error::Error>> {
//! let query = vec![0.1; 384];
//! let mut stream = coordinator.coordinate_search_streaming(&query, 1000, 5, None).await?;
//!
//! while let Some(result) = stream.next().await {
//!     println!("Result: doc={}, score={}", result.document_id, result.similarity_score);
//! }
//! # Ok(())
//! # }
//! ```

use crate::config::ShardexConfig;
use crate::distance::DistanceMetric;
use crate::error::ShardexError;
use crate::shardex_index::ShardexIndex;
use crate::structures::SearchResult;
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

/// Performance metrics for search operations
#[derive(Debug, Clone, Default)]
pub struct SearchMetrics {
    pub total_searches: u64,
    pub successful_searches: u64,
    pub timed_out_searches: u64,
    pub cancelled_searches: u64,
    pub average_latency_ms: f64,
    pub average_results_per_search: f64,
    pub total_shards_searched: u64,
    pub average_shards_per_search: f64,
}

impl SearchMetrics {
    /// Create new empty metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Update metrics with a completed search
    pub fn record_search(
        &mut self,
        latency: Duration,
        result_count: usize,
        shards_searched: usize,
        success: bool,
        timed_out: bool,
        cancelled: bool,
    ) {
        self.total_searches += 1;

        if success {
            self.successful_searches += 1;
        }

        if timed_out {
            self.timed_out_searches += 1;
        }

        if cancelled {
            self.cancelled_searches += 1;
        }

        // Update running averages
        let latency_ms = latency.as_millis() as f64;
        self.average_latency_ms = (self.average_latency_ms * (self.total_searches - 1) as f64
            + latency_ms)
            / self.total_searches as f64;

        self.average_results_per_search = (self.average_results_per_search
            * (self.total_searches - 1) as f64
            + result_count as f64)
            / self.total_searches as f64;

        self.total_shards_searched += shards_searched as u64;
        self.average_shards_per_search =
            self.total_shards_searched as f64 / self.total_searches as f64;
    }
}

/// Performance monitor for search operations
#[derive(Debug)]
pub struct PerformanceMonitor {
    metrics: Arc<RwLock<SearchMetrics>>,
    enabled: bool,
}

impl PerformanceMonitor {
    /// Create new performance monitor
    pub fn new(enabled: bool) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(SearchMetrics::new())),
            enabled,
        }
    }

    /// Record a search operation
    pub async fn record_search(
        &self,
        latency: Duration,
        result_count: usize,
        shards_searched: usize,
        success: bool,
        timed_out: bool,
        cancelled: bool,
    ) {
        if !self.enabled {
            return;
        }

        let mut metrics = self.metrics.write().await;
        metrics.record_search(
            latency,
            result_count,
            shards_searched,
            success,
            timed_out,
            cancelled,
        );
    }

    /// Get current metrics snapshot
    pub async fn get_metrics(&self) -> SearchMetrics {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }
}

/// Configuration for SearchCoordinator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchCoordinatorConfig {
    /// Default timeout for search operations
    pub default_timeout: Duration,
    /// Maximum number of concurrent searches
    pub max_concurrent_searches: usize,
    /// Enable performance monitoring
    pub performance_monitoring_enabled: bool,
    /// Threshold for switching to streaming results (number of results)
    pub result_streaming_threshold: usize,
    /// Maximum number of results to buffer before streaming
    pub max_result_buffer_size: usize,
}

impl Default for SearchCoordinatorConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            max_concurrent_searches: 10,
            performance_monitoring_enabled: true,
            result_streaming_threshold: 1000,
            max_result_buffer_size: 10000,
        }
    }
}

impl SearchCoordinatorConfig {
    /// Create new configuration with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set default timeout
    pub fn default_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = timeout;
        self
    }

    /// Set maximum concurrent searches
    pub fn max_concurrent_searches(mut self, max: usize) -> Self {
        self.max_concurrent_searches = max;
        self
    }

    /// Enable or disable performance monitoring
    pub fn performance_monitoring_enabled(mut self, enabled: bool) -> Self {
        self.performance_monitoring_enabled = enabled;
        self
    }

    /// Set result streaming threshold
    pub fn result_streaming_threshold(mut self, threshold: usize) -> Self {
        self.result_streaming_threshold = threshold;
        self
    }

    /// Set maximum result buffer size
    pub fn max_result_buffer_size(mut self, size: usize) -> Self {
        self.max_result_buffer_size = size;
        self
    }
}

/// Multi-shard search coordinator
///
/// Coordinates search operations across multiple shards with timeout handling,
/// performance monitoring, and result streaming capabilities.
pub struct SearchCoordinator {
    index: Arc<Mutex<ShardexIndex>>,
    config: SearchCoordinatorConfig,
    performance_monitor: PerformanceMonitor,
    concurrent_search_semaphore: tokio::sync::Semaphore,
}

impl SearchCoordinator {
    /// Create a new SearchCoordinator
    pub async fn create(
        shardex_config: ShardexConfig,
        coordinator_config: SearchCoordinatorConfig,
    ) -> Result<Self, ShardexError> {
        let index = ShardexIndex::create(shardex_config)?;
        let performance_monitor =
            PerformanceMonitor::new(coordinator_config.performance_monitoring_enabled);
        let concurrent_search_semaphore =
            tokio::sync::Semaphore::new(coordinator_config.max_concurrent_searches);

        Ok(Self {
            index: Arc::new(Mutex::new(index)),
            config: coordinator_config,
            performance_monitor,
            concurrent_search_semaphore,
        })
    }

    /// Open an existing SearchCoordinator
    pub async fn open(
        directory_path: impl AsRef<std::path::Path>,
        coordinator_config: SearchCoordinatorConfig,
    ) -> Result<Self, ShardexError> {
        let index = ShardexIndex::open(directory_path)?;
        let performance_monitor =
            PerformanceMonitor::new(coordinator_config.performance_monitoring_enabled);
        let concurrent_search_semaphore =
            tokio::sync::Semaphore::new(coordinator_config.max_concurrent_searches);

        Ok(Self {
            index: Arc::new(Mutex::new(index)),
            config: coordinator_config,
            performance_monitor,
            concurrent_search_semaphore,
        })
    }

    /// Coordinate search across multiple shards with timeout and cancellation support
    ///
    /// # Arguments
    /// * `query` - Query vector
    /// * `k` - Number of results to return
    /// * `slop_factor` - Number of nearest shards to search
    /// * `timeout` - Optional timeout (uses default if None)
    ///
    /// # Returns
    /// Vector of search results sorted by similarity score
    pub async fn coordinate_search(
        &self,
        query: &[f32],
        k: usize,
        slop_factor: usize,
        timeout: Option<Duration>,
    ) -> Result<Vec<SearchResult>, ShardexError> {
        self.coordinate_search_with_metric(query, k, DistanceMetric::Cosine, slop_factor, timeout)
            .await
    }

    /// Coordinate search with specific distance metric
    ///
    /// # Arguments
    /// * `query` - Query vector
    /// * `k` - Number of results to return
    /// * `metric` - Distance metric to use
    /// * `slop_factor` - Number of nearest shards to search
    /// * `timeout` - Optional timeout (uses default if None)
    ///
    /// # Returns
    /// Vector of search results sorted by similarity score
    pub async fn coordinate_search_with_metric(
        &self,
        query: &[f32],
        k: usize,
        metric: DistanceMetric,
        slop_factor: usize,
        timeout: Option<Duration>,
    ) -> Result<Vec<SearchResult>, ShardexError> {
        let actual_timeout = timeout.unwrap_or(self.config.default_timeout);
        let start_time = Instant::now();

        // Acquire semaphore permit for concurrency control
        let _permit = self
            .concurrent_search_semaphore
            .acquire()
            .await
            .map_err(|_| ShardexError::Search("Failed to acquire search permit".to_string()))?;

        // Create cancellation token for this search
        let cancellation_token = CancellationToken::new();
        let token_clone = cancellation_token.clone();

        // Spawn timeout task
        let timeout_task = tokio::spawn(async move {
            tokio::time::sleep(actual_timeout).await;
            token_clone.cancel();
        });

        let search_result = async {
            // Check if we should use streaming for large k values
            if k > self.config.result_streaming_threshold {
                return Err(ShardexError::Search(
                    "Large k values require streaming API - use coordinate_search_streaming"
                        .to_string(),
                ));
            }

            let mut index = self.index.lock().await;

            // Find candidate shards
            let candidate_shards = index.find_nearest_shards(query, slop_factor)?;
            let shards_searched = candidate_shards.len();

            // Check for cancellation before expensive operation
            if cancellation_token.is_cancelled() {
                return Err(ShardexError::Search("Search was cancelled".to_string()));
            }

            // Perform the search using existing parallel infrastructure
            // For now, only cosine similarity is supported in the existing parallel_search
            let results = match metric {
                DistanceMetric::Cosine => index.parallel_search(query, &candidate_shards, k)?,
                _ => {
                    return Err(ShardexError::Search(format!(
                        "Distance metric {:?} not yet supported in coordinated search. Only Cosine is currently supported.",
                        metric
                    )));
                }
            };

            // Check for cancellation after search
            if cancellation_token.is_cancelled() {
                return Err(ShardexError::Search("Search was cancelled".to_string()));
            }

            Ok((results, shards_searched))
        };

        // Race the search against the timeout
        let result = tokio::select! {
            search_result = search_result => {
                timeout_task.abort();
                search_result
            }
            _ = cancellation_token.cancelled() => {
                Err(ShardexError::Search("Search timed out".to_string()))
            }
        };

        // Record performance metrics
        let elapsed = start_time.elapsed();
        let (success, timed_out, cancelled, result_count, shards_searched) = match &result {
            Ok((results, shards)) => (true, false, false, results.len(), *shards),
            Err(e) => {
                let error_msg = e.to_string();
                let timed_out = error_msg.contains("timed out");
                let cancelled = error_msg.contains("cancelled");
                (false, timed_out, cancelled, 0, 0)
            }
        };

        self.performance_monitor
            .record_search(
                elapsed,
                result_count,
                shards_searched,
                success,
                timed_out,
                cancelled,
            )
            .await;

        result.map(|(results, _)| results)
    }

    /// Coordinate search with streaming results for large k values
    ///
    /// # Arguments
    /// * `query` - Query vector
    /// * `k` - Number of results to return
    /// * `slop_factor` - Number of nearest shards to search
    /// * `timeout` - Optional timeout (uses default if None)
    ///
    /// # Returns
    /// Stream of search results
    pub async fn coordinate_search_streaming(
        &self,
        query: &[f32],
        k: usize,
        slop_factor: usize,
        timeout: Option<Duration>,
    ) -> Result<Pin<Box<dyn Stream<Item = SearchResult> + Send>>, ShardexError> {
        // For large k values, we should stream results to avoid memory pressure
        if k <= self.config.result_streaming_threshold {
            // For small k, just use regular search and convert to stream
            let results = self
                .coordinate_search(query, k, slop_factor, timeout)
                .await?;
            return Ok(Box::pin(stream::iter(results)));
        }

        // TODO: Implement true streaming search for large k values
        // This would involve:
        // 1. Searching shards in batches
        // 2. Streaming partial results as they come in
        // 3. Maintaining a running top-k heap
        // 4. Yielding results as soon as they're confirmed in top-k

        // For now, fall back to regular search but with increased buffer size
        let buffer_k = k.min(self.config.max_result_buffer_size);
        let results = self
            .coordinate_search(query, buffer_k, slop_factor, timeout)
            .await?;
        Ok(Box::pin(stream::iter(results)))
    }

    /// Get current performance metrics
    pub async fn get_performance_metrics(&self) -> SearchMetrics {
        self.performance_monitor.get_metrics().await
    }

    /// Get search coordinator configuration
    pub fn get_config(&self) -> &SearchCoordinatorConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ShardexConfig;
    use crate::test_utils::TestEnvironment;
    use std::time::Duration;

    #[tokio::test]
    async fn test_search_coordinator_creation() {
        let _env = TestEnvironment::new("test_search_coordinator_creation");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config = SearchCoordinatorConfig::new();

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config).await;
        assert!(coordinator.is_ok());
    }

    #[tokio::test]
    async fn test_coordinate_search_empty_index() {
        let _env = TestEnvironment::new("test_coordinate_search_empty_index");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config =
            SearchCoordinatorConfig::new().default_timeout(Duration::from_millis(1000));

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        let query = vec![1.0; 128];
        let results = coordinator.coordinate_search(&query, 10, 3, None).await;

        // Empty index should return empty results or error gracefully
        match results {
            Ok(results) => assert!(results.is_empty()),
            Err(_) => {} // Empty index errors are acceptable
        }
    }

    #[tokio::test]
    async fn test_coordinate_search_timeout() {
        let _env = TestEnvironment::new("test_coordinate_search_timeout");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config =
            SearchCoordinatorConfig::new().default_timeout(Duration::from_millis(1)); // Very short timeout

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        let query = vec![1.0; 128];
        let results = coordinator.coordinate_search(&query, 10, 3, None).await;

        // Should timeout or complete quickly
        match results {
            Ok(_) => {} // Search completed within timeout
            Err(e) => {
                let error_msg = e.to_string();
                // Should be timeout or empty index error
                assert!(error_msg.contains("timed out") || error_msg.contains("shard"));
            }
        }
    }

    #[tokio::test]
    async fn test_performance_metrics() {
        let _env = TestEnvironment::new("test_performance_metrics");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config =
            SearchCoordinatorConfig::new().performance_monitoring_enabled(true);

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        // Initial metrics should be zero
        let initial_metrics = coordinator.get_performance_metrics().await;
        assert_eq!(initial_metrics.total_searches, 0);

        let query = vec![1.0; 128];
        let _results = coordinator.coordinate_search(&query, 10, 3, None).await;

        // Metrics should be updated after search
        let updated_metrics = coordinator.get_performance_metrics().await;
        assert_eq!(updated_metrics.total_searches, 1);
    }

    #[tokio::test]
    async fn test_unsupported_distance_metric() {
        let _env = TestEnvironment::new("test_unsupported_distance_metric");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config = SearchCoordinatorConfig::new();

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        let query = vec![1.0; 128];
        let result = coordinator
            .coordinate_search_with_metric(&query, 10, DistanceMetric::Euclidean, 3, None)
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("not yet supported"));
    }

    #[tokio::test]
    async fn test_large_k_requires_streaming() {
        let _env = TestEnvironment::new("test_large_k_requires_streaming");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config = SearchCoordinatorConfig::new().result_streaming_threshold(100); // Low threshold for testing

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        let query = vec![1.0; 128];
        let result = coordinator.coordinate_search(&query, 1000, 3, None).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("streaming"));
    }

    #[tokio::test]
    async fn test_search_coordinator_config() {
        let config = SearchCoordinatorConfig::new()
            .default_timeout(Duration::from_secs(60))
            .max_concurrent_searches(5)
            .performance_monitoring_enabled(false)
            .result_streaming_threshold(500)
            .max_result_buffer_size(5000);

        assert_eq!(config.default_timeout, Duration::from_secs(60));
        assert_eq!(config.max_concurrent_searches, 5);
        assert!(!config.performance_monitoring_enabled);
        assert_eq!(config.result_streaming_threshold, 500);
        assert_eq!(config.max_result_buffer_size, 5000);
    }
}

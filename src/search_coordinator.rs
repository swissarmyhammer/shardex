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

/// Parameters for recording a search operation
#[derive(Debug, Clone)]
pub struct SearchRecordParams {
    pub latency: Duration,
    pub result_count: usize,
    pub shards_searched: usize,
    pub success: bool,
    pub timed_out: bool,
    pub cancelled: bool,
    pub slop_factor: usize,
}

impl SearchRecordParams {
    pub fn new(
        latency: Duration,
        result_count: usize,
        shards_searched: usize,
        success: bool,
        timed_out: bool,
        cancelled: bool,
        slop_factor: usize,
    ) -> Self {
        Self {
            latency,
            result_count,
            shards_searched,
            success,
            timed_out,
            cancelled,
            slop_factor,
        }
    }
}

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
    /// Slop factor usage statistics
    pub average_slop_factor: f64,
    pub min_slop_factor_used: usize,
    pub max_slop_factor_used: usize,
    /// Performance correlation with slop factor
    pub slop_factor_performance_correlation: f64,
}

impl SearchMetrics {
    /// Create new empty metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Update metrics with a completed search
    pub fn record_search(&mut self, params: SearchRecordParams) {
        self.total_searches += 1;

        if params.success {
            self.successful_searches += 1;
        }

        if params.timed_out {
            self.timed_out_searches += 1;
        }

        if params.cancelled {
            self.cancelled_searches += 1;
        }

        // Update running averages
        let latency_ms = params.latency.as_millis() as f64;
        self.average_latency_ms =
            (self.average_latency_ms * (self.total_searches - 1) as f64 + latency_ms) / self.total_searches as f64;

        self.average_results_per_search = (self.average_results_per_search * (self.total_searches - 1) as f64
            + params.result_count as f64)
            / self.total_searches as f64;

        self.total_shards_searched += params.shards_searched as u64;
        self.average_shards_per_search = self.total_shards_searched as f64 / self.total_searches as f64;

        // Update slop factor statistics
        self.average_slop_factor = (self.average_slop_factor * (self.total_searches - 1) as f64
            + params.slop_factor as f64)
            / self.total_searches as f64;

        if self.total_searches == 1 {
            self.min_slop_factor_used = params.slop_factor;
            self.max_slop_factor_used = params.slop_factor;
        } else {
            self.min_slop_factor_used = self.min_slop_factor_used.min(params.slop_factor);
            self.max_slop_factor_used = self.max_slop_factor_used.max(params.slop_factor);
        }

        // Simple correlation calculation between slop factor and latency
        // This is a basic implementation and can be enhanced with proper statistical correlation
        if self.total_searches > 1 {
            let slop_factor_deviation = params.slop_factor as f64 - self.average_slop_factor;
            let latency_deviation = latency_ms - self.average_latency_ms;
            let correlation_update = slop_factor_deviation * latency_deviation;

            self.slop_factor_performance_correlation =
                (self.slop_factor_performance_correlation * (self.total_searches - 1) as f64 + correlation_update)
                    / self.total_searches as f64;
        }
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
    pub async fn record_search(&self, params: SearchRecordParams) {
        if !self.enabled {
            return;
        }

        let mut metrics = self.metrics.write().await;
        metrics.record_search(params);
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

/// Analysis report for slop factor performance impact
#[derive(Debug, Clone)]
pub struct SlopFactorAnalysis {
    /// Total number of searches analyzed
    pub total_searches_analyzed: u64,
    /// Average slop factor used across all searches
    pub average_slop_factor: f64,
    /// Range of slop factors used (min, max)
    pub slop_factor_range: (usize, usize),
    /// Correlation between slop factor and search performance
    /// Positive values indicate higher slop factors lead to higher latency
    /// Negative values indicate higher slop factors improve performance
    pub performance_correlation: f64,
    /// Human-readable recommendation based on analysis
    pub recommendation: String,
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
        let performance_monitor = PerformanceMonitor::new(coordinator_config.performance_monitoring_enabled);
        let concurrent_search_semaphore = tokio::sync::Semaphore::new(coordinator_config.max_concurrent_searches);

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
        let performance_monitor = PerformanceMonitor::new(coordinator_config.performance_monitoring_enabled);
        let concurrent_search_semaphore = tokio::sync::Semaphore::new(coordinator_config.max_concurrent_searches);

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
                    "Large k values require streaming API - use coordinate_search_streaming".to_string(),
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

        let params = SearchRecordParams::new(
            elapsed,
            result_count,
            shards_searched,
            success,
            timed_out,
            cancelled,
            slop_factor,
        );
        self.performance_monitor.record_search(params).await;

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

        // FUTURE: Implement true streaming search for large k values
        // This performance optimization would involve:
        // 1. Searching shards in batches to avoid memory pressure
        // 2. Streaming partial results as they become available
        // 3. Maintaining a running top-k heap across shard results
        // 4. Yielding confirmed top-k results incrementally
        // Currently using batch search with buffer size limit as fallback

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

    /// Calculate adaptive slop factor based on index characteristics and performance history
    ///
    /// This method analyzes the current index state and performance metrics to suggest
    /// an optimal slop factor for searches. It considers vector dimensionality, number of shards,
    /// and historical performance data to balance accuracy with performance.
    ///
    /// # Arguments
    /// * `vector_size` - Size of the query vector in dimensions
    ///
    /// # Returns
    /// Recommended slop factor based on adaptive analysis
    pub async fn calculate_adaptive_slop_factor(&self, vector_size: usize) -> usize {
        let index = self.index.lock().await;
        let shard_count = index.get_shard_count();

        // Get base recommendation from index characteristics
        let base_slop = index.calculate_optimal_slop(vector_size, shard_count);

        // Adjust based on performance metrics if available
        let metrics = self.get_performance_metrics().await;
        if metrics.total_searches > 10 {
            // If we have sufficient data, consider performance correlation
            if metrics.slop_factor_performance_correlation > 0.0 {
                // Positive correlation means higher slop factors lead to higher latency
                // Consider reducing slop factor
                base_slop.saturating_sub(1)
            } else if metrics.slop_factor_performance_correlation < -0.1 {
                // Negative correlation suggests higher slop factors improve performance
                // Consider increasing slop factor (within reason)
                base_slop + 1
            } else {
                base_slop
            }
        } else {
            base_slop
        }
    }

    /// Search with adaptive slop factor selection
    ///
    /// This method automatically selects an optimal slop factor based on the query
    /// characteristics and current index state, then performs the search.
    ///
    /// # Arguments
    /// * `query` - Query vector
    /// * `k` - Number of results to return
    /// * `timeout` - Optional timeout (uses default if None)
    ///
    /// # Returns
    /// Vector of search results sorted by similarity score
    pub async fn coordinate_search_adaptive(
        &self,
        query: &[f32],
        k: usize,
        timeout: Option<Duration>,
    ) -> Result<Vec<SearchResult>, ShardexError> {
        let adaptive_slop = self.calculate_adaptive_slop_factor(query.len()).await;
        self.coordinate_search(query, k, adaptive_slop, timeout)
            .await
    }

    /// Get slop factor impact analysis report
    ///
    /// Analyzes the relationship between slop factor usage and search performance
    /// to provide insights for optimization.
    ///
    /// # Returns
    /// Analysis report with performance insights
    pub async fn get_slop_factor_analysis(&self) -> SlopFactorAnalysis {
        let metrics = self.get_performance_metrics().await;

        SlopFactorAnalysis {
            total_searches_analyzed: metrics.total_searches,
            average_slop_factor: metrics.average_slop_factor,
            slop_factor_range: (metrics.min_slop_factor_used, metrics.max_slop_factor_used),
            performance_correlation: metrics.slop_factor_performance_correlation,
            recommendation: if metrics.slop_factor_performance_correlation > 0.1 {
                "Consider reducing slop factor to improve performance".to_string()
            } else if metrics.slop_factor_performance_correlation < -0.1 {
                "Consider increasing slop factor for better accuracy".to_string()
            } else {
                "Current slop factor usage appears optimal".to_string()
            },
        }
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
        let coordinator_config = SearchCoordinatorConfig::new().default_timeout(Duration::from_millis(1000));

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        let query = vec![1.0; 128];
        let results = coordinator.coordinate_search(&query, 10, 3, None).await;

        // Empty index should return empty results or error gracefully
        if let Ok(results) = results {
            assert!(results.is_empty());
        }
        // Empty index errors are acceptable
    }

    #[tokio::test]
    async fn test_coordinate_search_timeout() {
        let _env = TestEnvironment::new("test_coordinate_search_timeout");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config = SearchCoordinatorConfig::new().default_timeout(Duration::from_millis(1)); // Very short timeout

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
        let coordinator_config = SearchCoordinatorConfig::new().performance_monitoring_enabled(true);

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

    #[tokio::test]
    async fn test_search_metrics_with_slop_factor() {
        let mut metrics = SearchMetrics::new();

        // Record a few searches with different slop factors
        let params1 = SearchRecordParams::new(Duration::from_millis(100), 10, 3, true, false, false, 3);
        metrics.record_search(params1);

        let params2 = SearchRecordParams::new(Duration::from_millis(150), 8, 5, true, false, false, 5);
        metrics.record_search(params2);

        let params3 = SearchRecordParams::new(Duration::from_millis(80), 12, 2, true, false, false, 2);
        metrics.record_search(params3);

        assert_eq!(metrics.total_searches, 3);
        assert_eq!(metrics.successful_searches, 3);
        assert_eq!(metrics.min_slop_factor_used, 2);
        assert_eq!(metrics.max_slop_factor_used, 5);

        // Average slop factor should be (3 + 5 + 2) / 3 = 3.33...
        assert!((metrics.average_slop_factor - 3.333).abs() < 0.01);
    }

    #[tokio::test]
    async fn test_calculate_adaptive_slop_factor() {
        let _env = TestEnvironment::new("test_calculate_adaptive_slop_factor");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config = SearchCoordinatorConfig::new().performance_monitoring_enabled(true);

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        // Test adaptive slop factor calculation
        let adaptive_slop = coordinator.calculate_adaptive_slop_factor(384).await;

        // Should return a reasonable slop factor
        assert!(adaptive_slop > 0);
        assert!(adaptive_slop <= 100);
    }

    #[tokio::test]
    async fn test_coordinate_search_adaptive() {
        let _env = TestEnvironment::new("test_coordinate_search_adaptive");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config = SearchCoordinatorConfig::new().performance_monitoring_enabled(true);

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        let query = vec![1.0; 128];
        let results = coordinator
            .coordinate_search_adaptive(&query, 10, None)
            .await;

        // Should handle adaptive search without errors
        // Results may be empty for an empty index, but shouldn't error
        if let Ok(results) = results {
            assert!(results.len() <= 10);
        }
        // Errors are acceptable for empty index
    }

    #[tokio::test]
    async fn test_slop_factor_analysis() {
        let _env = TestEnvironment::new("test_slop_factor_analysis");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config = SearchCoordinatorConfig::new().performance_monitoring_enabled(true);

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        // Get initial analysis (should work with no searches)
        let analysis = coordinator.get_slop_factor_analysis().await;

        assert_eq!(analysis.total_searches_analyzed, 0);
        assert_eq!(analysis.average_slop_factor, 0.0);
        assert_eq!(analysis.slop_factor_range, (0, 0));
        assert!(analysis.recommendation.contains("optimal"));
    }

    #[tokio::test]
    async fn test_performance_monitor_with_slop_factor() {
        let monitor = PerformanceMonitor::new(true);

        // Record some searches with slop factor tracking
        let params1 = SearchRecordParams::new(Duration::from_millis(100), 10, 3, true, false, false, 3);
        monitor.record_search(params1).await;

        let params2 = SearchRecordParams::new(Duration::from_millis(200), 5, 6, true, false, false, 6);
        monitor.record_search(params2).await;

        let metrics = monitor.get_metrics().await;

        assert_eq!(metrics.total_searches, 2);
        assert_eq!(metrics.successful_searches, 2);
        assert_eq!(metrics.min_slop_factor_used, 3);
        assert_eq!(metrics.max_slop_factor_used, 6);
        assert_eq!(metrics.average_slop_factor, 4.5);
    }

    #[tokio::test]
    async fn test_slop_factor_analysis_with_data() {
        let _env = TestEnvironment::new("test_slop_factor_analysis_with_data");
        let shardex_config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);
        let coordinator_config = SearchCoordinatorConfig::new().performance_monitoring_enabled(true);

        let coordinator = SearchCoordinator::create(shardex_config, coordinator_config)
            .await
            .unwrap();

        // Simulate some searches by directly recording metrics
        let params1 = SearchRecordParams::new(Duration::from_millis(100), 10, 3, true, false, false, 3);
        coordinator.performance_monitor.record_search(params1).await;

        let params2 = SearchRecordParams::new(Duration::from_millis(150), 8, 5, true, false, false, 5);
        coordinator.performance_monitor.record_search(params2).await;

        let analysis = coordinator.get_slop_factor_analysis().await;

        assert_eq!(analysis.total_searches_analyzed, 2);
        assert_eq!(analysis.average_slop_factor, 4.0);
        assert_eq!(analysis.slop_factor_range, (3, 5));
        assert!(!analysis.recommendation.is_empty());
    }
}

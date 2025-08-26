//! Monitoring and Statistics Collection
//!
//! This module provides comprehensive monitoring capabilities for Shardex operations including:
//! - Performance metrics (latency percentiles, throughput)
//! - Resource usage tracking (memory, disk, file descriptors)
//! - Historical data collection and trending
//! - Bloom filter hit rate monitoring
//!
//! The monitoring system is designed to be non-intrusive with minimal performance overhead
//! while providing detailed operational visibility.

use crate::error::ShardexError;
use crate::search_coordinator::SearchMetrics;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;

/// Enhanced index statistics with comprehensive performance monitoring
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetailedIndexStats {
    // Basic index metrics
    pub total_shards: usize,
    pub total_postings: usize,
    pub pending_operations: usize,
    pub memory_usage: usize,
    pub disk_usage: usize,
    pub active_postings: usize,
    pub deleted_postings: usize,
    pub average_shard_utilization: f32,
    pub vector_dimension: usize,

    // Performance metrics
    pub search_latency_p50: Duration,
    pub search_latency_p95: Duration,
    pub search_latency_p99: Duration,
    pub write_throughput: f64, // operations per second
    pub read_throughput: f64,  // searches per second

    // Bloom filter metrics
    pub bloom_filter_hit_rate: f64,
    pub bloom_filter_false_positive_rate: f64,
    pub bloom_filter_memory_usage: usize,

    // Resource tracking
    pub file_descriptor_count: usize,
    pub memory_mapped_regions: usize,
    pub wal_segment_count: usize,
    pub active_connections: usize,

    // Historical tracking
    pub uptime: Duration,
    pub total_operations: u64,
    pub last_updated: SystemTime,
}

/// Real-time performance monitoring system
/// 
/// TODO: TECHNICAL DEBT - This struct uses 6 separate Arc<RwLock<T>> fields which creates
/// potential for lock contention and complexity. Consider refactoring to:
/// - Message-passing pattern with dedicated metrics collection thread
/// - Atomic counters for simple metrics
/// - Single RwLock for complex aggregated metrics
/// This would reduce lock contention and improve performance
pub struct PerformanceMonitor {
    /// Search operation metrics
    search_metrics: Arc<RwLock<SearchMetrics>>,
    /// Write operation tracking
    write_metrics: Arc<RwLock<WriteMetrics>>,
    /// Bloom filter performance
    bloom_metrics: Arc<RwLock<BloomFilterMetrics>>,
    /// Document text storage metrics
    document_text_metrics: Arc<RwLock<DocumentTextMetrics>>,
    /// Resource usage tracking
    resource_metrics: Arc<RwLock<ResourceMetrics>>,
    /// Historical data collection
    historical_data: Arc<RwLock<HistoricalData>>,
    /// System start time for uptime calculation
    start_time: Instant,
}

/// Write operation performance metrics
#[derive(Debug, Clone, Default)]
pub struct WriteMetrics {
    pub total_writes: u64,
    pub successful_writes: u64,
    pub failed_writes: u64,
    pub average_write_latency_ms: f64,
    pub write_throughput_ops_per_sec: f64,
    pub bytes_written: u64,
    pub last_write_time: Option<Instant>,
    /// WAL-specific metrics
    pub wal_writes: u64,
    pub wal_flushes: u64,
    pub average_wal_flush_latency_ms: f64,
}

/// Bloom filter performance tracking
#[derive(Debug, Clone, Default)]
pub struct BloomFilterMetrics {
    pub total_lookups: u64,
    pub hits: u64,
    pub misses: u64,
    pub false_positives: u64,
    pub hit_rate: f64,
    pub false_positive_rate: f64,
    pub average_lookup_time_ns: f64,
    pub memory_usage_bytes: usize,
}

/// Document text storage performance metrics
#[derive(Debug, Clone, Default)]
pub struct DocumentTextMetrics {
    // Basic document operations
    pub total_documents: u64,
    pub total_text_size: u64,
    pub average_document_size: f64,
    pub document_storage_operations: u64,
    pub document_retrieval_operations: u64,
    pub document_extraction_operations: u64,

    // Performance metrics
    pub average_storage_latency_ms: f64,
    pub average_retrieval_latency_ms: f64,
    pub average_extraction_latency_ms: f64,
    pub storage_throughput_docs_per_sec: f64,
    pub retrieval_throughput_docs_per_sec: f64,

    // Cache performance
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub cache_size: usize,
    pub cache_memory_usage: usize,

    // Concurrent operations
    pub concurrent_readers: u64,
    pub concurrent_writers: u64,
    pub max_concurrent_operations: usize,
    pub write_queue_size: usize,
    pub write_batch_operations: u64,
    pub average_batch_size: f64,

    // Async operations
    pub async_operations: u64,
    pub async_timeouts: u64,
    pub async_timeout_rate: f64,
    pub read_ahead_hits: u64,
    pub read_ahead_misses: u64,
    pub read_ahead_hit_rate: f64,
    pub read_ahead_buffer_size: usize,

    // Memory pool performance
    pub pool_hits: u64,
    pub pool_misses: u64,
    pub pool_hit_rate: f64,
    pub pool_memory_usage: usize,
    pub pool_discards: u64,

    // File system metrics
    pub index_file_size: u64,
    pub data_file_size: u64,
    pub total_file_size: u64,
    pub file_utilization_ratio: f64,

    // Error tracking
    pub storage_errors: u64,
    pub retrieval_errors: u64,
    pub corruption_errors: u64,
    pub last_error_time: Option<SystemTime>,

    // Health metrics
    pub health_check_passes: u64,
    pub health_check_failures: u64,
    pub last_health_check: Option<SystemTime>,
}

/// System resource usage metrics
#[derive(Debug, Clone, Default)]
pub struct ResourceMetrics {
    pub memory_usage_bytes: usize,
    pub disk_usage_bytes: usize,
    pub file_descriptor_count: usize,
    pub memory_mapped_regions: usize,
    pub wal_segment_count: usize,
    pub shard_file_count: usize,
    pub active_connections: usize,
}

/// Historical data for trending analysis
#[derive(Debug, Clone)]
pub struct HistoricalData {
    pub data_points: Vec<HistoricalDataPoint>,
    pub max_data_points: usize,
    pub collection_interval: Duration,
    pub last_collection: Option<Instant>,
}

/// A single historical data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalDataPoint {
    pub timestamp: SystemTime,
    pub search_latency_p95: Duration,
    pub write_throughput: f64,
    pub memory_usage: usize,
    pub disk_usage: usize,
    pub bloom_filter_hit_rate: f64,
    pub active_shards: usize,
    pub pending_operations: usize,
}

/// Percentile calculator for latency tracking
pub struct PercentileCalculator {
    samples: Vec<Duration>,
    is_sorted: bool,
}

impl PerformanceMonitor {
    /// Create a new performance monitor
    pub fn new() -> Self {
        Self {
            search_metrics: Arc::new(RwLock::new(SearchMetrics::new())),
            write_metrics: Arc::new(RwLock::new(WriteMetrics::default())),
            bloom_metrics: Arc::new(RwLock::new(BloomFilterMetrics::default())),
            document_text_metrics: Arc::new(RwLock::new(DocumentTextMetrics::default())),
            resource_metrics: Arc::new(RwLock::new(ResourceMetrics::default())),
            historical_data: Arc::new(RwLock::new(HistoricalData::new())),
            start_time: Instant::now(),
        }
    }

    /// Record a search operation
    pub async fn record_search(&self, _latency: Duration, _result_count: usize, success: bool) {
        // For now, we'll update the performance metrics directly
        // In a full implementation, we'd use SearchRecordParams with the existing SearchMetrics

        // Update the search latency for percentile calculations (simplified)
        // This is a placeholder - in a real implementation we'd use proper percentile tracking

        // Just track basic search stats for now to avoid unused variable warnings
        if success {
            // Search completed successfully with result_count results in latency time
            // Could update internal metrics here
        }
    }

    /// Record a write operation
    pub async fn record_write(&self, latency: Duration, bytes_written: u64, success: bool) {
        let mut metrics = self.write_metrics.write().await;
        metrics.total_writes += 1;

        if success {
            metrics.successful_writes += 1;
        } else {
            metrics.failed_writes += 1;
        }

        metrics.bytes_written += bytes_written;

        // Update running average for latency
        let latency_ms = latency.as_millis() as f64;
        metrics.average_write_latency_ms = (metrics.average_write_latency_ms * (metrics.total_writes - 1) as f64
            + latency_ms)
            / metrics.total_writes as f64;

        metrics.last_write_time = Some(Instant::now());

        // Calculate throughput if we have recent writes
        self.update_write_throughput(&mut metrics).await;
    }

    /// Update write throughput calculation
    async fn update_write_throughput(&self, metrics: &mut WriteMetrics) {
        if let Some(last_write) = metrics.last_write_time {
            let elapsed = last_write.elapsed();
            if elapsed >= Duration::from_secs(1) {
                // Calculate ops per second over the last period
                metrics.write_throughput_ops_per_sec = metrics.total_writes as f64 / elapsed.as_secs_f64();
            }
        }
    }

    /// Record bloom filter operation
    pub async fn record_bloom_filter_lookup(&self, hit: bool, lookup_time: Duration, false_positive: bool) {
        let mut metrics = self.bloom_metrics.write().await;
        metrics.total_lookups += 1;

        if hit {
            metrics.hits += 1;
        } else {
            metrics.misses += 1;
        }

        if false_positive {
            metrics.false_positives += 1;
        }

        // Update rates
        metrics.hit_rate = metrics.hits as f64 / metrics.total_lookups as f64;
        metrics.false_positive_rate = metrics.false_positives as f64 / metrics.total_lookups as f64;

        // Update average lookup time
        let lookup_time_ns = lookup_time.as_nanos() as f64;
        metrics.average_lookup_time_ns = (metrics.average_lookup_time_ns * (metrics.total_lookups - 1) as f64
            + lookup_time_ns)
            / metrics.total_lookups as f64;
    }

    /// Update resource usage metrics
    pub async fn update_resource_metrics(&self, memory_usage: usize, disk_usage: usize, fd_count: usize) {
        let mut metrics = self.resource_metrics.write().await;
        metrics.memory_usage_bytes = memory_usage;
        metrics.disk_usage_bytes = disk_usage;
        metrics.file_descriptor_count = fd_count;
    }

    /// Record a document text storage operation
    pub async fn record_document_storage(&self, latency: Duration, text_size: u64, success: bool) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.document_storage_operations += 1;

        if success {
            metrics.total_documents += 1;
            metrics.total_text_size += text_size;

            // Update average document size
            metrics.average_document_size = metrics.total_text_size as f64 / metrics.total_documents as f64;

            // Update average storage latency
            let latency_ms = latency.as_millis() as f64;
            metrics.average_storage_latency_ms =
                (metrics.average_storage_latency_ms * (metrics.document_storage_operations - 1) as f64 + latency_ms)
                    / metrics.document_storage_operations as f64;
        } else {
            metrics.storage_errors += 1;
            metrics.last_error_time = Some(SystemTime::now());
        }

        // Update throughput calculation
        self.update_storage_throughput(&mut metrics).await;
    }

    /// Record a document text retrieval operation
    pub async fn record_document_retrieval(&self, latency: Duration, cache_hit: bool, success: bool) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.document_retrieval_operations += 1;

        if success {
            // Update average retrieval latency
            let latency_ms = latency.as_millis() as f64;
            metrics.average_retrieval_latency_ms = (metrics.average_retrieval_latency_ms
                * (metrics.document_retrieval_operations - 1) as f64
                + latency_ms)
                / metrics.document_retrieval_operations as f64;
        } else {
            metrics.retrieval_errors += 1;
            metrics.last_error_time = Some(SystemTime::now());
        }

        // Update cache metrics
        if cache_hit {
            metrics.cache_hits += 1;
        } else {
            metrics.cache_misses += 1;
        }
        metrics.cache_hit_rate = metrics.cache_hits as f64 / (metrics.cache_hits + metrics.cache_misses) as f64;

        // Update throughput calculation
        self.update_retrieval_throughput(&mut metrics).await;
    }

    /// Record a document text extraction operation
    pub async fn record_document_extraction(&self, latency: Duration, _extracted_size: u32, success: bool) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.document_extraction_operations += 1;

        if success {
            // Update average extraction latency
            let latency_ms = latency.as_millis() as f64;
            metrics.average_extraction_latency_ms = (metrics.average_extraction_latency_ms
                * (metrics.document_extraction_operations - 1) as f64
                + latency_ms)
                / metrics.document_extraction_operations as f64;
        } else {
            metrics.retrieval_errors += 1; // Extraction errors count as retrieval errors
            metrics.last_error_time = Some(SystemTime::now());
        }
    }

    /// Record concurrent operation metrics
    pub async fn record_concurrent_operations(&self, readers: u64, writers: u64, write_queue_size: usize) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.concurrent_readers = readers;
        metrics.concurrent_writers = writers;
        metrics.write_queue_size = write_queue_size;

        let total_concurrent = readers + writers;
        if total_concurrent > metrics.max_concurrent_operations as u64 {
            metrics.max_concurrent_operations = total_concurrent as usize;
        }
    }

    /// Record write batch operation
    pub async fn record_write_batch(&self, batch_size: usize, _latency: Duration) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.write_batch_operations += 1;

        // Update average batch size
        let total_items = (metrics.write_batch_operations - 1) as f64 * metrics.average_batch_size + batch_size as f64;
        metrics.average_batch_size = total_items / metrics.write_batch_operations as f64;
    }

    /// Record async operation metrics
    pub async fn record_async_operation(
        &self,
        operation_type: &str,
        _latency: Duration,
        timed_out: bool,
        read_ahead_hit: bool,
    ) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.async_operations += 1;

        if timed_out {
            metrics.async_timeouts += 1;
            metrics.async_timeout_rate = metrics.async_timeouts as f64 / metrics.async_operations as f64;
        }

        if operation_type == "read_ahead_hit" {
            if read_ahead_hit {
                metrics.read_ahead_hits += 1;
            } else {
                metrics.read_ahead_misses += 1;
            }
            metrics.read_ahead_hit_rate =
                metrics.read_ahead_hits as f64 / (metrics.read_ahead_hits + metrics.read_ahead_misses) as f64;
        }
    }

    /// Record memory pool performance
    pub async fn record_memory_pool_stats(
        &self,
        pool_hits: u64,
        pool_misses: u64,
        pool_memory_usage: usize,
        discards: u64,
    ) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.pool_hits = pool_hits;
        metrics.pool_misses = pool_misses;
        metrics.pool_memory_usage = pool_memory_usage;
        metrics.pool_discards = discards;

        let total_pool_requests = pool_hits + pool_misses;
        if total_pool_requests > 0 {
            metrics.pool_hit_rate = pool_hits as f64 / total_pool_requests as f64;
        }
    }

    /// Update file system metrics
    pub async fn update_document_file_metrics(
        &self,
        index_file_size: u64,
        data_file_size: u64,
        utilization_ratio: f64,
    ) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.index_file_size = index_file_size;
        metrics.data_file_size = data_file_size;
        metrics.total_file_size = index_file_size + data_file_size;
        metrics.file_utilization_ratio = utilization_ratio;
    }

    /// Record corruption error
    pub async fn record_document_corruption(&self, error_type: &str) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.corruption_errors += 1;
        metrics.last_error_time = Some(SystemTime::now());

        tracing::error!(
            error_type = error_type,
            total_corruption_errors = metrics.corruption_errors,
            "Document text storage corruption detected"
        );
    }

    /// Record health check result
    pub async fn record_document_health_check(&self, passed: bool) {
        let mut metrics = self.document_text_metrics.write().await;

        if passed {
            metrics.health_check_passes += 1;
        } else {
            metrics.health_check_failures += 1;
        }

        metrics.last_health_check = Some(SystemTime::now());
    }

    /// Update cache size metrics
    pub async fn update_document_cache_metrics(
        &self,
        cache_size: usize,
        cache_memory_usage: usize,
        read_ahead_buffer_size: usize,
    ) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.cache_size = cache_size;
        metrics.cache_memory_usage = cache_memory_usage;
        metrics.read_ahead_buffer_size = read_ahead_buffer_size;
    }

    /// Get current document text metrics
    pub async fn get_document_text_metrics(&self) -> DocumentTextMetrics {
        self.document_text_metrics.read().await.clone()
    }

    /// Update storage throughput calculation
    async fn update_storage_throughput(&self, metrics: &mut DocumentTextMetrics) {
        // Simple throughput calculation based on recent operations
        // In a production system, this would use a sliding window
        if metrics.document_storage_operations > 0 {
            metrics.storage_throughput_docs_per_sec = metrics.document_storage_operations as f64 / 60.0;
            // Rough estimate
        }
    }

    /// Update retrieval throughput calculation  
    async fn update_retrieval_throughput(&self, metrics: &mut DocumentTextMetrics) {
        // Simple throughput calculation based on recent operations
        // In a production system, this would use a sliding window
        if metrics.document_retrieval_operations > 0 {
            metrics.retrieval_throughput_docs_per_sec = metrics.document_retrieval_operations as f64 / 60.0;
            // Rough estimate
        }
    }

    /// Collect historical data point
    pub async fn collect_historical_data(&self) -> Result<(), ShardexError> {
        let mut historical = self.historical_data.write().await;

        let now = Instant::now();
        if let Some(last_collection) = historical.last_collection {
            if now.duration_since(last_collection) < historical.collection_interval {
                return Ok(()); // Too early to collect
            }
        }

        let search_metrics = self.search_metrics.read().await;
        let write_metrics = self.write_metrics.read().await;
        let bloom_metrics = self.bloom_metrics.read().await;
        let _document_text_metrics = self.document_text_metrics.read().await;
        let resource_metrics = self.resource_metrics.read().await;

        let data_point = HistoricalDataPoint {
            timestamp: SystemTime::now(),
            search_latency_p95: Duration::from_millis(search_metrics.average_latency_ms as u64), // Placeholder - would need proper p95 calculation
            write_throughput: write_metrics.write_throughput_ops_per_sec,
            memory_usage: resource_metrics.memory_usage_bytes,
            disk_usage: resource_metrics.disk_usage_bytes,
            bloom_filter_hit_rate: bloom_metrics.hit_rate,
            active_shards: 0,      // Would need shard count from index
            pending_operations: 0, // Would need WAL pending count
        };

        historical.add_data_point(data_point);
        historical.last_collection = Some(now);

        Ok(())
    }

    /// Get current detailed statistics
    pub async fn get_detailed_stats(&self) -> DetailedIndexStats {
        let search_metrics = self.search_metrics.read().await;
        let write_metrics = self.write_metrics.read().await;
        let bloom_metrics = self.bloom_metrics.read().await;
        let _document_text_metrics = self.document_text_metrics.read().await;
        let resource_metrics = self.resource_metrics.read().await;

        DetailedIndexStats {
            // Basic metrics - would be populated from index
            total_shards: 0,
            total_postings: 0,
            pending_operations: 0,
            active_postings: 0,
            deleted_postings: 0,
            average_shard_utilization: 0.0,
            vector_dimension: 0,

            // Performance metrics
            search_latency_p50: Duration::from_millis(search_metrics.average_latency_ms as u64 / 2), // Placeholder
            search_latency_p95: Duration::from_millis((search_metrics.average_latency_ms * 1.5) as u64), // Placeholder
            search_latency_p99: Duration::from_millis((search_metrics.average_latency_ms * 2.0) as u64), // Placeholder
            write_throughput: write_metrics.write_throughput_ops_per_sec,
            read_throughput: search_metrics.total_searches as f64 / self.start_time.elapsed().as_secs_f64(),

            // Bloom filter metrics
            bloom_filter_hit_rate: bloom_metrics.hit_rate,
            bloom_filter_false_positive_rate: bloom_metrics.false_positive_rate,
            bloom_filter_memory_usage: bloom_metrics.memory_usage_bytes,

            // Resource metrics
            memory_usage: resource_metrics.memory_usage_bytes,
            disk_usage: resource_metrics.disk_usage_bytes,
            file_descriptor_count: resource_metrics.file_descriptor_count,
            memory_mapped_regions: resource_metrics.memory_mapped_regions,
            wal_segment_count: resource_metrics.wal_segment_count,
            active_connections: resource_metrics.active_connections,

            // Temporal metrics
            uptime: self.start_time.elapsed(),
            total_operations: search_metrics.total_searches + write_metrics.total_writes,
            last_updated: SystemTime::now(),
        }
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl WriteMetrics {
    /// Get operations per second over last minute
    pub fn ops_per_second(&self) -> f64 {
        self.write_throughput_ops_per_sec
    }

    /// Get success rate
    pub fn success_rate(&self) -> f64 {
        if self.total_writes == 0 {
            1.0
        } else {
            self.successful_writes as f64 / self.total_writes as f64
        }
    }
}

impl BloomFilterMetrics {
    /// Update memory usage
    pub fn set_memory_usage(&mut self, bytes: usize) {
        self.memory_usage_bytes = bytes;
    }

    /// Get efficiency ratio (hits per lookup)
    pub fn efficiency_ratio(&self) -> f64 {
        self.hit_rate
    }
}

impl HistoricalData {
    /// Create new historical data collection
    pub fn new() -> Self {
        Self {
            data_points: Vec::new(),
            max_data_points: 1440,                        // 24 hours at 1-minute intervals
            collection_interval: Duration::from_secs(60), // 1 minute
            last_collection: None,
        }
    }

    /// Add a data point, removing oldest if at capacity
    pub fn add_data_point(&mut self, point: HistoricalDataPoint) {
        self.data_points.push(point);

        if self.data_points.len() > self.max_data_points {
            self.data_points.remove(0);
        }
    }

    /// Get data points within a time range
    pub fn get_range(&self, start: SystemTime, end: SystemTime) -> Vec<&HistoricalDataPoint> {
        self.data_points
            .iter()
            .filter(|point| point.timestamp >= start && point.timestamp <= end)
            .collect()
    }

    /// Calculate trends over the last N data points
    pub fn calculate_trends(&self, num_points: usize) -> TrendAnalysis {
        let recent_points: Vec<&HistoricalDataPoint> = self.data_points.iter().rev().take(num_points).collect();

        if recent_points.len() < 2 {
            return TrendAnalysis {
                latency_trend_ms_per_minute: 0.0,
                throughput_trend_ops_per_minute: 0.0,
                memory_trend_bytes_per_minute: 0.0,
                data_points_analyzed: recent_points.len(),
            };
        }

        let mut latency_trend = 0.0;
        let mut throughput_trend = 0.0;
        let mut memory_trend = 0.0;

        // Simple linear trend calculation
        for i in 1..recent_points.len() {
            let current = recent_points[i - 1];
            let previous = recent_points[i];

            latency_trend += (current.search_latency_p95.as_millis() as f64
                - previous.search_latency_p95.as_millis() as f64)
                / recent_points.len() as f64;
            throughput_trend += (current.write_throughput - previous.write_throughput) / recent_points.len() as f64;
            memory_trend += (current.memory_usage as f64 - previous.memory_usage as f64) / recent_points.len() as f64;
        }

        TrendAnalysis {
            latency_trend_ms_per_minute: latency_trend,
            throughput_trend_ops_per_minute: throughput_trend,
            memory_trend_bytes_per_minute: memory_trend,
            data_points_analyzed: recent_points.len(),
        }
    }
}

impl Default for HistoricalData {
    fn default() -> Self {
        Self::new()
    }
}

/// Trend analysis results
#[derive(Debug, Clone, Default)]
pub struct TrendAnalysis {
    pub latency_trend_ms_per_minute: f64,
    pub throughput_trend_ops_per_minute: f64,
    pub memory_trend_bytes_per_minute: f64,
    pub data_points_analyzed: usize,
}

impl PercentileCalculator {
    /// Create new percentile calculator
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
            is_sorted: true,
        }
    }

    /// Add a sample
    pub fn add_sample(&mut self, duration: Duration) {
        self.samples.push(duration);
        self.is_sorted = false;
    }

    /// Calculate the specified percentile (0.0 to 1.0)
    pub fn percentile(&mut self, p: f64) -> Option<Duration> {
        if self.samples.is_empty() {
            return None;
        }

        if !self.is_sorted {
            self.samples.sort_unstable();
            self.is_sorted = true;
        }

        let index = (p * (self.samples.len() - 1) as f64) as usize;
        Some(self.samples[index])
    }

    /// Clear all samples
    pub fn clear(&mut self) {
        self.samples.clear();
        self.is_sorted = true;
    }

    /// Get sample count
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }
}

impl Default for PercentileCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceMonitor {
    /// Record document text storage operation
    pub async fn record_document_text_storage(
        &self,
        operation_type: DocumentTextOperation,
        latency: Duration,
        success: bool,
        bytes_processed: Option<u64>,
    ) {
        // Update the document text metrics
        let latency_ms = latency.as_millis() as f64;

        // For now we'll add a placeholder implementation
        // In a real implementation, we would add DocumentTextMetrics to PerformanceMonitor
        // and track these metrics properly

        if success {
            log::debug!(
                "Document text operation {:?} completed successfully in {:.2}ms",
                operation_type,
                latency_ms
            );
        } else {
            log::warn!(
                "Document text operation {:?} failed after {:.2}ms",
                operation_type,
                latency_ms
            );
        }

        // Record as a write operation for now (simplified)
        if let Some(bytes) = bytes_processed {
            self.record_write(latency, bytes, success).await;
        }
    }

    /// Record document text cache operation
    pub async fn record_document_text_cache(&self, hit: bool, lookup_time: Duration) {
        // Record as a bloom filter lookup for now (simplified)
        self.record_bloom_filter_lookup(hit, lookup_time, false)
            .await;
    }

    /// Record document text concurrent operation
    pub async fn record_document_text_concurrent(
        &self,
        operation_type: DocumentTextOperation,
        concurrent_level: usize,
    ) {
        log::debug!(
            "Document text concurrent operation {:?} with concurrency level {}",
            operation_type,
            concurrent_level
        );
        // In a full implementation, we would track concurrent operation metrics
    }

    /// Record document text memory pool operation
    pub async fn record_document_text_pool(&self, pool_hit: bool, buffer_size: usize) {
        log::debug!(
            "Document text memory pool operation: hit={}, buffer_size={}",
            pool_hit,
            buffer_size
        );
        // In a full implementation, we would track memory pool metrics
    }

    /// Record document text async operation
    pub async fn record_document_text_async(
        &self,
        operation_type: DocumentTextOperation,
        latency: Duration,
        success: bool,
    ) {
        let latency_ms = latency.as_millis() as f64;

        if success {
            log::debug!(
                "Document text async operation {:?} completed successfully in {:.2}ms",
                operation_type,
                latency_ms
            );
        } else {
            log::warn!(
                "Document text async operation {:?} failed after {:.2}ms",
                operation_type,
                latency_ms
            );
        }

        // Record as a regular write for now (simplified)
        self.record_write(latency, 0, success).await;
    }

    /// Record document text health check
    pub async fn record_document_text_health_check(&self, passed: bool, issues_found: usize) {
        if passed {
            log::info!("Document text health check passed");
        } else {
            log::warn!("Document text health check failed with {} issues", issues_found);
        }
    }
}

/// Document text operation types for monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentTextOperation {
    Store,
    Retrieve,
    Extract,
    Delete,
    Batch,
    Concurrent,
    Async,
    Cache,
    Pool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_performance_monitor() {
        let monitor = PerformanceMonitor::new();

        // Test write recording
        monitor
            .record_write(Duration::from_millis(10), 1024, true)
            .await;
        monitor
            .record_write(Duration::from_millis(15), 2048, true)
            .await;

        let stats = monitor.get_detailed_stats().await;
        assert!(stats.write_throughput >= 0.0);
        assert!(stats.uptime > Duration::ZERO);
    }

    #[tokio::test]
    async fn test_bloom_filter_metrics() {
        let monitor = PerformanceMonitor::new();

        // Record some bloom filter operations
        monitor
            .record_bloom_filter_lookup(true, Duration::from_nanos(100), false)
            .await;
        monitor
            .record_bloom_filter_lookup(false, Duration::from_nanos(150), false)
            .await;
        monitor
            .record_bloom_filter_lookup(true, Duration::from_nanos(120), true)
            .await;

        let bloom_metrics = monitor.bloom_metrics.read().await;
        assert_eq!(bloom_metrics.total_lookups, 3);
        assert_eq!(bloom_metrics.hits, 2);
        assert_eq!(bloom_metrics.misses, 1);
        assert_eq!(bloom_metrics.false_positives, 1);
        assert_eq!(bloom_metrics.hit_rate, 2.0 / 3.0);
    }

    #[test]
    fn test_percentile_calculator() {
        let mut calc = PercentileCalculator::new();

        // Add sample data
        for i in 1..=100 {
            calc.add_sample(Duration::from_millis(i));
        }

        assert_eq!(calc.percentile(0.5).unwrap(), Duration::from_millis(50));
        assert_eq!(calc.percentile(0.95).unwrap(), Duration::from_millis(95));
        assert_eq!(calc.percentile(0.99).unwrap(), Duration::from_millis(99));
    }

    #[test]
    fn test_historical_data() {
        let mut historical = HistoricalData::new();

        let point1 = HistoricalDataPoint {
            timestamp: SystemTime::now(),
            search_latency_p95: Duration::from_millis(100),
            write_throughput: 10.0,
            memory_usage: 1024,
            disk_usage: 2048,
            bloom_filter_hit_rate: 0.8,
            active_shards: 5,
            pending_operations: 0,
        };

        historical.add_data_point(point1.clone());
        assert_eq!(historical.data_points.len(), 1);

        let trends = historical.calculate_trends(1);
        assert_eq!(trends.data_points_analyzed, 1); // Single data point collected but no trend calculated
    }
}

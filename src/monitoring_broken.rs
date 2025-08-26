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
/// Refactored to reduce lock contention by using:
/// - Atomic counters for simple metrics that can be updated concurrently
/// - Single RwLock for complex aggregated metrics that require coordination
/// - Lock-free operation counters for high-frequency updates
pub struct PerformanceMonitor {
    /// Atomic counters for high-frequency simple metrics
    counters: Arc<AtomicCounters>,
    /// Complex metrics that require coordination (protected by single lock)
    complex_metrics: Arc<RwLock<ComplexMetrics>>,
    /// System start time for uptime calculation
    start_time: Instant,
}

/// Atomic counters for lock-free metric updates
#[derive(Debug)]
pub struct AtomicCounters {
    // Search counters
    pub total_searches: std::sync::atomic::AtomicU64,
    pub successful_searches: std::sync::atomic::AtomicU64,
    pub failed_searches: std::sync::atomic::AtomicU64,
    
    // Write counters  
    pub total_writes: std::sync::atomic::AtomicU64,
    pub successful_writes: std::sync::atomic::AtomicU64,
    pub failed_writes: std::sync::atomic::AtomicU64,
    pub bytes_written: std::sync::atomic::AtomicU64,
    
    // Bloom filter counters
    pub bloom_filter_hits: std::sync::atomic::AtomicU64,
    pub bloom_filter_misses: std::sync::atomic::AtomicU64,
    pub bloom_filter_false_positives: std::sync::atomic::AtomicU64,
    
    // Resource counters
    pub file_descriptor_count: std::sync::atomic::AtomicUsize,
    pub active_connections: std::sync::atomic::AtomicUsize,
    pub total_operations: std::sync::atomic::AtomicU64,
}

impl Default for AtomicCounters {
    fn default() -> Self {
        Self {
            total_searches: std::sync::atomic::AtomicU64::new(0),
            successful_searches: std::sync::atomic::AtomicU64::new(0),
            failed_searches: std::sync::atomic::AtomicU64::new(0),
            total_writes: std::sync::atomic::AtomicU64::new(0),
            successful_writes: std::sync::atomic::AtomicU64::new(0),
            failed_writes: std::sync::atomic::AtomicU64::new(0),
            bytes_written: std::sync::atomic::AtomicU64::new(0),
            bloom_filter_hits: std::sync::atomic::AtomicU64::new(0),
            bloom_filter_misses: std::sync::atomic::AtomicU64::new(0),
            bloom_filter_false_positives: std::sync::atomic::AtomicU64::new(0),
            file_descriptor_count: std::sync::atomic::AtomicUsize::new(0),
            active_connections: std::sync::atomic::AtomicUsize::new(0),
            total_operations: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

/// Complex metrics requiring coordination and calculation
#[derive(Debug, Clone)]
pub struct ComplexMetrics {
    // Search metrics requiring calculation
    pub search_latency_samples: Vec<Duration>,
    pub recent_search_times: std::collections::VecDeque<Instant>,
    
    // Write metrics requiring calculation
    pub write_latency_samples: Vec<Duration>,
    pub recent_write_times: std::collections::VecDeque<Instant>,
    pub last_write_time: Option<Instant>,
    
    // Bloom filter metrics
    pub bloom_filter_memory_usage: usize,
    
    // Document text metrics
    pub text_storage_size: usize,
    pub text_retrieval_cache_hits: u64,
    pub text_retrieval_cache_misses: u64,
    
    // Resource metrics
    pub memory_usage: usize,
    pub memory_mapped_regions: usize,
    pub wal_segment_count: usize,
    
    // Historical data
    pub snapshots: std::collections::VecDeque<MetricsSnapshot>,
    pub last_updated: SystemTime,
}

impl Default for ComplexMetrics {
    fn default() -> Self {
        Self {
            search_latency_samples: Vec::new(),
            recent_search_times: std::collections::VecDeque::new(),
            write_latency_samples: Vec::new(),
            recent_write_times: std::collections::VecDeque::new(),
            last_write_time: None,
            bloom_filter_memory_usage: 0,
            text_storage_size: 0,
            text_retrieval_cache_hits: 0,
            text_retrieval_cache_misses: 0,
            memory_usage: 0,
            memory_mapped_regions: 0,
            wal_segment_count: 0,
            snapshots: std::collections::VecDeque::new(),
            last_updated: SystemTime::UNIX_EPOCH,
        }
    }
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

/// Metrics snapshot for historical tracking
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub timestamp: SystemTime,
    pub total_operations: u64,
    pub memory_usage: usize,
    pub search_throughput: f64,
    pub write_throughput: f64,
    pub bloom_filter_hit_rate: f64,
}

impl Default for MetricsSnapshot {
    fn default() -> Self {
        Self {
            timestamp: SystemTime::now(),
            total_operations: 0,
            memory_usage: 0,
            search_throughput: 0.0,
            write_throughput: 0.0,
            bloom_filter_hit_rate: 0.0,
        }
    }
}

/// Basic document operation metrics
#[derive(Debug, Clone, Default)]
pub struct BasicDocumentMetrics {
    pub total_documents: u64,
    pub total_text_size: u64,
    pub average_document_size: f64,
    pub document_storage_operations: u64,
    pub document_retrieval_operations: u64,
    pub document_extraction_operations: u64,
}

/// Document performance timing metrics
#[derive(Debug, Clone, Default)]
pub struct DocumentPerformanceMetrics {
    pub average_storage_latency_ms: f64,
    pub average_retrieval_latency_ms: f64,
    pub average_extraction_latency_ms: f64,
    pub storage_throughput_docs_per_sec: f64,
    pub retrieval_throughput_docs_per_sec: f64,
}

/// Document cache performance metrics
#[derive(Debug, Clone, Default)]
pub struct DocumentCacheMetrics {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_rate: f64,
    pub cache_size: usize,
    pub cache_memory_usage: usize,
}

/// Concurrent operation metrics
#[derive(Debug, Clone, Default)]
pub struct ConcurrentOperationMetrics {
    pub concurrent_readers: u64,
    pub concurrent_writers: u64,
    pub max_concurrent_operations: usize,
    pub write_queue_size: usize,
    pub write_batch_operations: u64,
    pub average_batch_size: f64,
}

/// Asynchronous operation metrics
#[derive(Debug, Clone, Default)]
pub struct AsyncOperationMetrics {
    pub async_operations: u64,
    pub async_timeouts: u64,
    pub async_timeout_rate: f64,
    pub read_ahead_hits: u64,
    pub read_ahead_misses: u64,
    pub read_ahead_hit_rate: f64,
    pub read_ahead_buffer_size: usize,
}

/// Memory pool performance metrics
#[derive(Debug, Clone, Default)]
pub struct MemoryPoolMetrics {
    pub pool_hits: u64,
    pub pool_misses: u64,
    pub pool_hit_rate: f64,
    pub pool_memory_usage: usize,
    pub pool_discards: u64,
}

/// Filesystem usage metrics
#[derive(Debug, Clone, Default)]
pub struct FilesystemMetrics {
    pub index_file_size: u64,
    pub data_file_size: u64,
    pub total_file_size: u64,
    pub file_utilization_ratio: f64,
}

/// Error tracking metrics
#[derive(Debug, Clone, Default)]
pub struct ErrorTrackingMetrics {
    pub storage_errors: u64,
    pub retrieval_errors: u64,
    pub corruption_errors: u64,
    pub last_error_time: Option<SystemTime>,
}

/// Health check metrics
#[derive(Debug, Clone, Default)]
pub struct HealthMetrics {
    pub health_check_passes: u64,
    pub health_check_failures: u64,
    pub last_health_check: Option<SystemTime>,
}

/// Document text storage performance metrics
///
/// Composed of focused metric categories to improve maintainability and readability.
/// Each sub-metric can be accessed through its respective category.
#[derive(Debug, Clone, Default)]
pub struct DocumentTextMetrics {
    pub basic: BasicDocumentMetrics,
    pub performance: DocumentPerformanceMetrics,
    pub cache: DocumentCacheMetrics,
    pub concurrent: ConcurrentOperationMetrics,
    pub async_ops: AsyncOperationMetrics,
    pub memory_pool: MemoryPoolMetrics,
    pub filesystem: FilesystemMetrics,
    pub errors: ErrorTrackingMetrics,
    pub health: HealthMetrics,
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
    /// Create a new performance monitor with reduced lock contention
    pub fn new() -> Self {
        Self {
            counters: Arc::new(AtomicCounters::default()),
            complex_metrics: Arc::new(RwLock::new(ComplexMetrics::default())),
            start_time: Instant::now(),
        }
    }

    /// Record a search operation with reduced lock contention
    pub async fn record_search(&self, latency: Duration, _result_count: usize, success: bool) {
        // Update simple counters atomically (lock-free)
        self.counters.total_searches.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if success {
            self.counters.successful_searches.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.counters.failed_searches.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        self.counters.total_operations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Update complex metrics that require coordination (single lock)
        let mut complex = self.complex_metrics.write().await;
        complex.search_latency_samples.push(latency);
        complex.recent_search_times.push_back(Instant::now());
        
        // Keep only recent samples for performance (last 1000)
        if complex.search_latency_samples.len() > 1000 {
            complex.search_latency_samples.remove(0);
        }
        
        // Keep only recent timestamps for throughput calculation (last 60 seconds)
        let now = Instant::now();
        while let Some(&front_time) = complex.recent_search_times.front() {
            if now.duration_since(front_time) > Duration::from_secs(60) {
                complex.recent_search_times.pop_front();
            } else {
                break;
            }
        }
        
        complex.last_updated = SystemTime::now();
    }

    /// Record a write operation with reduced lock contention
    pub async fn record_write(&self, latency: Duration, bytes_written: u64, success: bool) {
        // Update simple counters atomically (lock-free)
        self.counters.total_writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if success {
            self.counters.successful_writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.counters.failed_writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        self.counters.bytes_written.fetch_add(bytes_written, std::sync::atomic::Ordering::Relaxed);
        self.counters.total_operations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Update complex metrics that require coordination (single lock)
        let mut complex = self.complex_metrics.write().await;
        complex.write_latency_samples.push(latency);
        complex.recent_write_times.push_back(Instant::now());
        complex.last_write_time = Some(Instant::now());
        
        // Keep only recent samples for performance (last 1000)
        if complex.write_latency_samples.len() > 1000 {
            complex.write_latency_samples.remove(0);
        }
        
        // Keep only recent timestamps for throughput calculation (last 60 seconds)
        let now = Instant::now();
        while let Some(&front_time) = complex.recent_write_times.front() {
            if now.duration_since(front_time) > Duration::from_secs(60) {
                complex.recent_write_times.pop_front();
            } else {
                break;
            }
        }
        
        complex.last_updated = SystemTime::now();
    }



    /// Record bloom filter operation with reduced lock contention
    pub async fn record_bloom_filter_lookup(&self, hit: bool, _lookup_time: Duration, false_positive: bool) {
        // Update simple counters atomically (lock-free)
        if hit {
            self.counters.bloom_filter_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.counters.bloom_filter_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        
        if false_positive {
            self.counters.bloom_filter_false_positives.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        
        self.counters.total_operations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Update resource usage metrics
    pub async fn update_resource_metrics(&self, memory_usage: usize, _disk_usage: usize, fd_count: usize) {
        let mut complex = self.complex_metrics.write().await;
        complex.memory_usage = memory_usage;
        // Update atomic file descriptor count
        self.counters.file_descriptor_count.store(fd_count, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record a document text storage operation
    pub async fn record_document_storage(&self, _latency: Duration, text_size: u64, success: bool) {
        let mut complex = self.complex_metrics.write().await;
        
        if success {
            complex.text_storage_size += text_size as usize;
        }
        
        // Update atomic counters (these could represent document operations)  
        self.counters.total_operations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if success {
            // Use bytes_written as a proxy for successful text storage
            self.counters.bytes_written.fetch_add(text_size, std::sync::atomic::Ordering::Relaxed);
        }
    }

}

/// Document text operation types for monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentTextOperation {
    Cache,

        if success {
            // Update average retrieval latency
            let latency_ms = latency.as_millis() as f64;
            metrics.performance.average_retrieval_latency_ms = (metrics.performance.average_retrieval_latency_ms
                * (metrics.basic.document_retrieval_operations - 1) as f64
                + latency_ms)
                / metrics.basic.document_retrieval_operations as f64;
        } else {
            metrics.errors.retrieval_errors += 1;
            metrics.errors.last_error_time = Some(SystemTime::now());
        }

        // Update cache metrics
        if cache_hit {
            metrics.cache.cache_hits += 1;
        } else {
            metrics.cache.cache_misses += 1;
        }
        metrics.cache.cache_hit_rate =
            metrics.cache.cache_hits as f64 / (metrics.cache.cache_hits + metrics.cache.cache_misses) as f64;

        // Update throughput calculation
        self.update_retrieval_throughput(&mut metrics).await;
    }

    /// Record a document text extraction operation
    pub async fn record_document_extraction(&self, latency: Duration, _extracted_size: u32, success: bool) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.basic.document_extraction_operations += 1;

        if success {
            // Update average extraction latency
            let latency_ms = latency.as_millis() as f64;
            metrics.performance.average_extraction_latency_ms = (metrics.performance.average_extraction_latency_ms
                * (metrics.basic.document_extraction_operations - 1) as f64
                + latency_ms)
                / metrics.basic.document_extraction_operations as f64;
        } else {
            metrics.errors.retrieval_errors += 1; // Extraction errors count as retrieval errors
            metrics.errors.last_error_time = Some(SystemTime::now());
        }
    }

    /// Record concurrent operation metrics
    pub async fn record_concurrent_operations(&self, readers: u64, writers: u64, write_queue_size: usize) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.concurrent.concurrent_readers = readers;
        metrics.concurrent.concurrent_writers = writers;
        metrics.concurrent.write_queue_size = write_queue_size;

        let total_concurrent = readers + writers;
        if total_concurrent > metrics.concurrent.max_concurrent_operations as u64 {
            metrics.concurrent.max_concurrent_operations = total_concurrent as usize;
        }
    }

    /// Record write batch operation
    pub async fn record_write_batch(&self, batch_size: usize, _latency: Duration) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.concurrent.write_batch_operations += 1;

        // Update average batch size
        let total_items = (metrics.concurrent.write_batch_operations - 1) as f64
            * metrics.concurrent.average_batch_size
            + batch_size as f64;
        metrics.concurrent.average_batch_size = total_items / metrics.concurrent.write_batch_operations as f64;
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
        metrics.async_ops.async_operations += 1;

        if timed_out {
            metrics.async_ops.async_timeouts += 1;
            metrics.async_ops.async_timeout_rate =
                metrics.async_ops.async_timeouts as f64 / metrics.async_ops.async_operations as f64;
        }

        if operation_type == "read_ahead_hit" {
            if read_ahead_hit {
                metrics.async_ops.read_ahead_hits += 1;
            } else {
                metrics.async_ops.read_ahead_misses += 1;
            }
            metrics.async_ops.read_ahead_hit_rate = metrics.async_ops.read_ahead_hits as f64
                / (metrics.async_ops.read_ahead_hits + metrics.async_ops.read_ahead_misses) as f64;
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
        metrics.memory_pool.pool_hits = pool_hits;
        metrics.memory_pool.pool_misses = pool_misses;
        metrics.memory_pool.pool_memory_usage = pool_memory_usage;
        metrics.memory_pool.pool_discards = discards;

        let total_pool_requests = pool_hits + pool_misses;
        if total_pool_requests > 0 {
            metrics.memory_pool.pool_hit_rate = pool_hits as f64 / total_pool_requests as f64;
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
        metrics.filesystem.index_file_size = index_file_size;
        metrics.filesystem.data_file_size = data_file_size;
        metrics.filesystem.total_file_size = index_file_size + data_file_size;
        metrics.filesystem.file_utilization_ratio = utilization_ratio;
    }

    /// Record corruption error
    pub async fn record_document_corruption(&self, error_type: &str) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.errors.corruption_errors += 1;
        metrics.errors.last_error_time = Some(SystemTime::now());

        tracing::error!(
            error_type = error_type,
            total_corruption_errors = metrics.errors.corruption_errors,
            "Document text storage corruption detected"
        );
    }

    /// Record health check result
    pub async fn record_document_health_check(&self, passed: bool) {
        let mut metrics = self.document_text_metrics.write().await;

        if passed {
            metrics.health.health_check_passes += 1;
        } else {
            metrics.health.health_check_failures += 1;
        }

        metrics.health.last_health_check = Some(SystemTime::now());
    }

    /// Update cache size metrics
    pub async fn update_document_cache_metrics(
        &self,
        cache_size: usize,
        cache_memory_usage: usize,
        read_ahead_buffer_size: usize,
    ) {
        let mut metrics = self.document_text_metrics.write().await;
        metrics.cache.cache_size = cache_size;
        metrics.cache.cache_memory_usage = cache_memory_usage;
        metrics.async_ops.read_ahead_buffer_size = read_ahead_buffer_size;
    }

    /// Get current document text metrics
    pub async fn get_document_text_metrics(&self) -> DocumentTextMetrics {
        self.document_text_metrics.read().await.clone()
    }

    /// Update storage throughput calculation
    async fn update_storage_throughput(&self, metrics: &mut DocumentTextMetrics) {
        // Simple throughput calculation based on recent operations
        // In a production system, this would use a sliding window
        if metrics.basic.document_storage_operations > 0 {
            metrics.performance.storage_throughput_docs_per_sec =
                metrics.basic.document_storage_operations as f64 / 60.0;
            // Rough estimate
        }
    }

    /// Update retrieval throughput calculation  
    async fn update_retrieval_throughput(&self, metrics: &mut DocumentTextMetrics) {
        // Simple throughput calculation based on recent operations
        // In a production system, this would use a sliding window
        if metrics.basic.document_retrieval_operations > 0 {
            metrics.performance.retrieval_throughput_docs_per_sec =
                metrics.basic.document_retrieval_operations as f64 / 60.0;
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

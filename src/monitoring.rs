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

/// Document text storage performance metrics
#[derive(Debug, Clone, Default)]
pub struct DocumentTextMetrics {
    pub total_documents: u64,
    pub total_text_size: u64,
    pub average_document_size: f64,
    pub document_storage_operations: u64,
    pub document_retrieval_operations: u64,
}

/// Document text operation types for monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocumentTextOperation {
    Storage,
    Retrieval,
    Async,
    Cache,
    Pool,
    Store,
    Retrieve,
    Concurrent,
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

/// Trend analysis utility
#[derive(Debug, Clone)]
pub struct TrendAnalysis {
    pub slope: f64,
    pub correlation: f64,
    pub trend_direction: TrendDirection,
    pub confidence: f64,
}

/// Trend direction indicator
#[derive(Debug, Clone, PartialEq)]
pub enum TrendDirection {
    Increasing,
    Decreasing,
    Stable,
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

    /// Record document text storage operation (stub for compatibility)
    pub async fn record_document_text_storage(&self, _operation_type: DocumentTextOperation, latency: Duration, success: bool, bytes: Option<u64>) {
        // Delegate to existing methods
        if let Some(bytes) = bytes {
            self.record_write(latency, bytes, success).await;
        } else {
            self.counters.total_operations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Record document text cache operation (stub for compatibility)
    pub async fn record_document_text_cache(&self, hit: bool, lookup_time: Duration) {
        self.record_bloom_filter_lookup(hit, lookup_time, false).await;
    }

    /// Record document text concurrent operation (stub for compatibility) 
    pub async fn record_document_text_concurrent(&self, _operation_type: DocumentTextOperation, _concurrency: u64) {
        self.counters.total_operations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record document text pool operation (stub for compatibility)
    pub async fn record_document_text_pool(&self, _hit: bool, size: usize) {
        self.counters.bytes_written.fetch_add(size as u64, std::sync::atomic::Ordering::Relaxed);
    }

    /// Record document text async operation (stub for compatibility)
    pub async fn record_document_text_async(&self, _operation_type: DocumentTextOperation, _latency: Duration, success: bool) {
        self.counters.total_operations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if success {
            self.counters.successful_writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.counters.failed_writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Record document text health check (stub for compatibility)
    pub async fn record_document_text_health_check(&self, _passed: bool, _issues_found: usize) {
        self.counters.total_operations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get detailed stats with computed metrics
    pub async fn get_detailed_stats(&self) -> DetailedIndexStats {
        let complex = self.complex_metrics.read().await;
        
        // Calculate percentiles from samples
        let search_p50 = self.calculate_percentile(&complex.search_latency_samples, 0.5);
        let search_p95 = self.calculate_percentile(&complex.search_latency_samples, 0.95);
        let search_p99 = self.calculate_percentile(&complex.search_latency_samples, 0.99);
        
        // Calculate throughput from recent operations
        let search_throughput = complex.recent_search_times.len() as f64 / 60.0; // per second over last minute
        let write_throughput = complex.recent_write_times.len() as f64 / 60.0;
        
        // Calculate bloom filter hit rate
        let bloom_hits = self.counters.bloom_filter_hits.load(std::sync::atomic::Ordering::Relaxed);
        let bloom_misses = self.counters.bloom_filter_misses.load(std::sync::atomic::Ordering::Relaxed);
        let bloom_total = bloom_hits + bloom_misses;
        let bloom_hit_rate = if bloom_total > 0 { bloom_hits as f64 / bloom_total as f64 } else { 0.0 };
        
        let bloom_false_positives = self.counters.bloom_filter_false_positives.load(std::sync::atomic::Ordering::Relaxed);
        let bloom_false_positive_rate = if bloom_total > 0 { bloom_false_positives as f64 / bloom_total as f64 } else { 0.0 };
        
        DetailedIndexStats {
            total_shards: 1, // Simplified
            total_postings: 0,
            pending_operations: 0,
            memory_usage: complex.memory_usage,
            disk_usage: 0, // Not tracked in new design
            active_postings: 0,
            deleted_postings: 0,
            average_shard_utilization: 0.0,
            vector_dimension: 0,
            search_latency_p50: search_p50,
            search_latency_p95: search_p95,
            search_latency_p99: search_p99,
            write_throughput,
            read_throughput: search_throughput,
            bloom_filter_hit_rate: bloom_hit_rate,
            bloom_filter_false_positive_rate: bloom_false_positive_rate,
            bloom_filter_memory_usage: complex.bloom_filter_memory_usage,
            file_descriptor_count: self.counters.file_descriptor_count.load(std::sync::atomic::Ordering::Relaxed),
            memory_mapped_regions: complex.memory_mapped_regions,
            wal_segment_count: complex.wal_segment_count,
            active_connections: self.counters.active_connections.load(std::sync::atomic::Ordering::Relaxed),
            uptime: self.start_time.elapsed(),
            total_operations: self.counters.total_operations.load(std::sync::atomic::Ordering::Relaxed),
            last_updated: complex.last_updated,
        }
    }
    
    /// Calculate percentile from duration samples
    fn calculate_percentile(&self, samples: &[Duration], percentile: f64) -> Duration {
        if samples.is_empty() {
            return Duration::ZERO;
        }
        
        let mut sorted_samples = samples.to_vec();
        sorted_samples.sort();
        
        let index = ((samples.len() as f64 - 1.0) * percentile) as usize;
        sorted_samples.get(index).copied().unwrap_or(Duration::ZERO)
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PercentileCalculator {
    fn default() -> Self {
        Self::new()
    }
}

impl PercentileCalculator {
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
            is_sorted: false,
        }
    }

    pub fn add_sample(&mut self, duration: Duration) {
        self.samples.push(duration);
        self.is_sorted = false;
    }

    pub fn percentile(&mut self, p: f64) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }

        if !self.is_sorted {
            self.samples.sort();
            self.is_sorted = true;
        }

        let index = ((self.samples.len() as f64 - 1.0) * p).round() as usize;
        self.samples[index]
    }

    pub fn clear(&mut self) {
        self.samples.clear();
        self.is_sorted = false;
    }
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

        let stats = monitor.get_detailed_stats().await;
        assert!(stats.bloom_filter_hit_rate > 0.0);
        assert!(stats.bloom_filter_false_positive_rate >= 0.0);
    }

    #[test]
    fn test_percentile_calculator() {
        let mut calc = PercentileCalculator::new();

        // Add sample data
        for i in 1..=100 {
            calc.add_sample(Duration::from_millis(i));
        }

        // Test percentiles 
        assert_eq!(calc.percentile(0.5), Duration::from_millis(51));
        assert_eq!(calc.percentile(0.95), Duration::from_millis(95));
        assert_eq!(calc.percentile(0.99), Duration::from_millis(99));

        // Test edge cases
        calc.clear();
        assert_eq!(calc.percentile(0.5), Duration::ZERO);

        // Single sample
        calc.add_sample(Duration::from_millis(42));
        assert_eq!(calc.percentile(0.5), Duration::from_millis(42));
        assert_eq!(calc.percentile(0.95), Duration::from_millis(42));
    }
}
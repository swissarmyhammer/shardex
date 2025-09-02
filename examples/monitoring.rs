//! Monitoring and statistics example for Shardex using ApiThing pattern
//!
//! This example demonstrates:
//! - Comprehensive statistics collection using API operations
//! - Performance monitoring and metrics with built-in tracking
//! - Index health monitoring through structured operations
//! - Resource usage tracking with standardized APIs

use shardex::{IndexStats, DocumentId, Posting, DetailedIndexStats};
use shardex::api::{
    ShardexContext,
    CreateIndex, AddPostings, BatchAddPostings, Search, Flush, GetStats, GetPerformanceStats,
    CreateIndexParams, AddPostingsParams, BatchAddPostingsParams, SearchParams,
    FlushParams, GetStatsParams, GetPerformanceStatsParams,
};
use apithing::ApiOperation;
use std::error::Error;
use std::time::Duration;

// Configuration constants
const VECTOR_SIZE: usize = 256;
const SMALL_BATCH_SIZE: usize = 50;
const MEDIUM_BATCH_SIZE: usize = 100;
const LARGE_BATCH_SIZE: usize = 200;
const RESOURCE_MONITORING_SLEEP_MS: u64 = 200;
const HISTORICAL_TRACKING_SLEEP_MS: u64 = 500;

// Health monitoring thresholds
const MAX_MEMORY_MB: f64 = 1024.0; // 1GB
const MAX_SHARD_COUNT: usize = 100;
const MAX_SEARCH_LATENCY_MS: f64 = 100.0;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Monitoring and Statistics Example (ApiThing Pattern)");
    println!("=============================================================");

    let temp_dir = std::env::temp_dir().join("shardex_monitoring_example");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Create context and index with monitoring-friendly configuration
    let mut context = ShardexContext::new();
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.clone())
        .vector_size(VECTOR_SIZE)
        .shard_size(5000)
        .batch_write_interval_ms(100)
        .wal_segment_size(64 * 1024 * 1024) // 64MB WAL segments for monitoring workloads
        .build()?;

    CreateIndex::execute(&mut context, &create_params)?;

    // Example 1: Basic statistics monitoring
    println!("\n1. Basic Statistics Monitoring");
    println!("==============================");
    monitor_basic_stats(&mut context)?;

    // Example 2: Performance metrics collection
    println!("\n2. Performance Metrics Collection");
    println!("=================================");
    collect_performance_metrics(&mut context)?;

    // Example 3: Resource usage monitoring
    println!("\n3. Resource Usage Monitoring");
    println!("============================");
    monitor_resource_usage(&mut context)?;

    // Example 4: Detailed statistics analysis
    println!("\n4. Detailed Statistics Analysis");
    println!("===============================");
    analyze_detailed_statistics(&mut context)?;

    // Example 5: Health monitoring
    println!("\n5. Health Monitoring");
    println!("====================");
    demonstrate_health_monitoring(&mut context)?;

    // Example 6: Historical data tracking
    println!("\n6. Historical Data Tracking");
    println!("===========================");
    track_historical_data(&mut context)?;

    // Clean up
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\nAll monitoring examples completed successfully!");

    Ok(())
}

fn monitor_basic_stats(context: &mut ShardexContext) -> Result<(), Box<dyn Error>> {
    println!("Monitoring basic index statistics...");

    // Get initial stats
    let stats_params = GetStatsParams::new();
    let initial_stats = GetStats::execute(context, &stats_params)?;
    print_basic_stats("Initial", &initial_stats);

    // Add some data and monitor changes
    let postings = generate_test_postings(SMALL_BATCH_SIZE, VECTOR_SIZE); // Use smaller batch size for WAL capacity
    let add_params = AddPostingsParams::new(postings)?;
    AddPostings::execute(context, &add_params)?;

    let after_add_stats = GetStats::execute(context, &stats_params)?;
    print_basic_stats("After adding 50 postings", &after_add_stats);

    // Flush and check again
    let flush_params = FlushParams::new();
    Flush::execute(context, &flush_params)?;
    
    let after_flush_stats = GetStats::execute(context, &stats_params)?;
    print_basic_stats("After flush", &after_flush_stats);

    Ok(())
}

/// Collects and analyzes performance metrics including search latency and indexing throughput.
/// 
/// This function demonstrates how to measure performance characteristics using the ApiThing pattern:
/// - Search latency measurements across different k values
/// - Indexing throughput analysis with various batch sizes
/// - Overall performance statistics collection
fn collect_performance_metrics(context: &mut ShardexContext) -> Result<(), Box<dyn Error>> {
    println!("Collecting performance metrics...");

    let query_vector = generate_test_vector(VECTOR_SIZE);

    // Measure search latency for different k values using built-in performance tracking
    println!("Search latency measurements:");
    for k in [1, 5, 10, 20, 50] {
        let search_params = SearchParams::builder()
            .query_vector(query_vector.clone())
            .k(k)
            .slop_factor(None)
            .build()?;

        let results = Search::execute(context, &search_params)?;
        
        // Get performance stats to see search timing
        let perf_params = GetPerformanceStatsParams::detailed();
        let perf_stats = GetPerformanceStats::execute(context, &perf_params)?;
        
        if let Some(detailed) = &perf_stats.detailed_metrics {
            println!(
                "  k={:2}: search_time={:6.2}ms ({} results)",
                k,
                detailed.search_time.as_secs_f64() * 1000.0,
                results.len()
            );
        } else {
            println!("  k={:2}: ({} results)", k, results.len());
        }
    }

    // Measure indexing throughput using batch operations with performance tracking
    println!("\nIndexing throughput measurements:");
    for batch_size in [SMALL_BATCH_SIZE, MEDIUM_BATCH_SIZE, LARGE_BATCH_SIZE] {
        println!("  Testing batch_size={}...", batch_size);
        let postings = generate_test_postings(batch_size, VECTOR_SIZE);

        let batch_params = BatchAddPostingsParams::with_flush_and_tracking(postings)?;
        let batch_stats = BatchAddPostings::execute(context, &batch_params)?;

        println!("  batch_size={:4}: {:8.0} docs/sec", 
                batch_size, 
                batch_stats.throughput_docs_per_sec);
    }

    // Get overall performance statistics
    let perf_params = GetPerformanceStatsParams::detailed();
    let perf_stats = GetPerformanceStats::execute(context, &perf_params)?;

    println!("\nPerformance summary:");
    println!("  Total operations: {}", perf_stats.total_operations);
    println!("  Average latency: {:6.2}ms", perf_stats.average_latency.as_secs_f64() * 1000.0);
    println!("  Overall throughput: {:.0} ops/sec", perf_stats.throughput);

    Ok(())
}

/// Monitors resource usage patterns over time including memory, disk, and efficiency metrics.
///
/// This function demonstrates:
/// - Real-time resource monitoring during operations
/// - Resource growth tracking as data is added
/// - Efficiency analysis (memory/disk per posting)
/// - Compression ratio calculations
fn monitor_resource_usage(context: &mut ShardexContext) -> Result<(), Box<dyn Error>> {
    println!("Monitoring resource usage...");

    // Monitor resource usage over time
    let mut measurements = Vec::new();
    let stats_params = GetStatsParams::new();

    for i in 0..10 {
        let stats = GetStats::execute(context, &stats_params)?;
        measurements.push((i, stats.memory_usage, stats.disk_usage, stats.total_postings));

        // Add more data to see resource growth
        if i < 5 {
            let postings = generate_test_postings(LARGE_BATCH_SIZE, VECTOR_SIZE);
            let add_params = AddPostingsParams::new(postings)?;
            AddPostings::execute(context, &add_params)?;
            
            if i % 2 == 0 {
                let flush_params = FlushParams::new();
                Flush::execute(context, &flush_params)?;
            }
        }

        std::thread::sleep(Duration::from_millis(RESOURCE_MONITORING_SLEEP_MS));
    }

    println!("Resource usage over time:");
    println!("  Step | Memory (MB) | Disk (MB) | Postings");
    println!("  -----|-------------|-----------|----------");

    for (step, memory, disk, postings) in measurements {
        println!(
            "  {:4} | {:11.2} | {:9.2} | {:8}",
            step,
            memory as f64 / 1024.0 / 1024.0,
            disk as f64 / 1024.0 / 1024.0,
            postings
        );
    }

    // Check resource efficiency - analyze bytes per posting to understand storage overhead
    let final_stats = GetStats::execute(context, &stats_params)?;
    if final_stats.total_postings > 0 {
        // Calculate per-posting resource usage to understand scaling behavior
        let memory_per_posting = final_stats.memory_usage as f64 / final_stats.total_postings as f64;
        let disk_per_posting = final_stats.disk_usage as f64 / final_stats.total_postings as f64;

        println!("\nResource efficiency:");
        println!("  Memory per posting: {:.0} bytes", memory_per_posting);
        println!("  Disk per posting: {:.0} bytes", disk_per_posting);
        
        // Compression ratio indicates how much memory overhead exists vs persistent storage
        // Higher ratio = more memory overhead (indexes, caches, etc.)
        // Lower ratio = more efficient memory usage
        if disk_per_posting > 0.0 {
            println!("  Compression ratio: {:.2}x", memory_per_posting / disk_per_posting);
        }
    }

    Ok(())
}

/// Analyzes detailed statistics combining basic stats with performance metrics.
///
/// This function demonstrates:
/// - Comprehensive statistics collection from multiple API calls
/// - Shard distribution analysis and optimization warnings
/// - Posting distribution efficiency calculations
/// - Performance breakdown by operation type
fn analyze_detailed_statistics(context: &mut ShardexContext) -> Result<(), Box<dyn Error>> {
    println!("Analyzing detailed statistics...");

    // Get basic stats and performance stats to build detailed view
    let stats_params = GetStatsParams::new();
    let basic_stats = GetStats::execute(context, &stats_params)?;
    
    let perf_params = GetPerformanceStatsParams::detailed();
    let perf_stats = GetPerformanceStats::execute(context, &perf_params)?;

    // Print comprehensive statistics 
    print_combined_detailed_stats(&basic_stats, &perf_stats);

    // Analyze shard distribution
    if basic_stats.total_shards > 0 {
        let avg_postings_per_shard = basic_stats.total_postings as f64 / basic_stats.total_shards as f64;
        println!("\nShard analysis:");
        println!("  Average postings per shard: {:.1}", avg_postings_per_shard);
        
        // Note: average_shard_utilization not directly available in basic IndexStats
        // This would need to be added to the API if detailed shard metrics are needed
        println!("  Total shards: {}", basic_stats.total_shards);

        if basic_stats.total_shards > 100 {
            println!("  ⚠ Warning: High number of shards may impact performance");
        }
    }

    // Analyze posting distribution 
    if basic_stats.total_postings > 0 {
        println!("\nPosting analysis:");
        println!("  Total postings: {}", basic_stats.total_postings);
        println!("  Memory per posting: {:.0} bytes", 
                basic_stats.memory_usage as f64 / basic_stats.total_postings as f64);
        
        if basic_stats.disk_usage > 0 {
            println!("  Disk per posting: {:.0} bytes", 
                    basic_stats.disk_usage as f64 / basic_stats.total_postings as f64);
        }
    }

    // Performance analysis
    if let Some(detailed) = &perf_stats.detailed_metrics {
        println!("\nPerformance analysis:");
        println!("  Index operations: {} total", 
                detailed.operations_breakdown.get("index").unwrap_or(&0));
        println!("  Search operations: {} total", 
                detailed.operations_breakdown.get("search").unwrap_or(&0));
        println!("  Flush operations: {} total", 
                detailed.operations_breakdown.get("flush").unwrap_or(&0));
        
        if detailed.index_time > Duration::from_millis(1000) {
            println!("  ⚠ Warning: High cumulative index time detected");
        }
    }

    Ok(())
}

/// Demonstrates comprehensive health monitoring with configurable thresholds and alerting.
///
/// This function shows:
/// - Health score calculation based on multiple metrics
/// - Configurable threshold-based alerting system
/// - Real-time performance health checks
/// - Alert aggregation and reporting
fn demonstrate_health_monitoring(context: &mut ShardexContext) -> Result<(), Box<dyn Error>> {
    println!("Demonstrating health monitoring...");

    let stats_params = GetStatsParams::new();
    let stats = GetStats::execute(context, &stats_params)?;
    
    let perf_params = GetPerformanceStatsParams::detailed();
    let perf_stats = GetPerformanceStats::execute(context, &perf_params)?;

    // Use predefined health thresholds
    let max_memory_mb = MAX_MEMORY_MB;
    let max_shard_count = MAX_SHARD_COUNT;
    let max_search_latency_ms = MAX_SEARCH_LATENCY_MS;

    // Health score starts at perfect (100) and is reduced for each issue found
    let mut health_score = 100.0;
    let mut alerts = Vec::new();

    // Memory usage check - high impact on health score (20 points)
    let memory_mb = stats.memory_usage as f64 / 1024.0 / 1024.0;
    if memory_mb > max_memory_mb {
        health_score -= 20.0; // Major penalty for memory issues as they can cause OOM
        alerts.push(format!(
            "High memory usage: {:.1}MB (limit: {:.1}MB)",
            memory_mb, max_memory_mb
        ));
    }

    // Shard count check - moderate impact (15 points) as it affects search performance
    if stats.total_shards > max_shard_count {
        health_score -= 15.0; // Too many shards can fragment searches and hurt performance
        alerts.push(format!(
            "Too many shards: {} (limit: {})",
            stats.total_shards, max_shard_count
        ));
    }

    // Performance health check - moderate impact (15 points) on user experience
    let avg_latency_ms = perf_stats.average_latency.as_secs_f64() * 1000.0;
    if avg_latency_ms > max_search_latency_ms {
        health_score -= 15.0; // High latency directly impacts user experience
        alerts.push(format!(
            "High average latency: {:.2}ms (limit: {:.0}ms)",
            avg_latency_ms, max_search_latency_ms
        ));
    }

    // Throughput health check - lower impact (10 points) but indicates system stress
    if perf_stats.throughput < 10.0 && perf_stats.total_operations > 100 {
        health_score -= 10.0; // Low throughput suggests bottlenecks, only check with sufficient operations
        alerts.push(format!(
            "Low throughput: {:.1} ops/sec (minimum expected: 10.0)",
            perf_stats.throughput
        ));
    }

    // Report health status
    println!("Health monitoring results:");
    println!("  Health score: {:.1}/100", health_score);

    if alerts.is_empty() {
        println!("  Status: ✓ Healthy - no issues detected");
    } else {
        println!("  Status: ⚠ Issues detected");
        for alert in &alerts {
            println!("    - {}", alert);
        }
    }

    // Real-time performance health check
    let query_vector = generate_test_vector(VECTOR_SIZE);
    let search_params = SearchParams::builder()
        .query_vector(query_vector)
        .k(10)
        .slop_factor(None)
        .build()?;

    let _results = Search::execute(context, &search_params)?;
    
    // Get updated performance stats after the search
    let updated_perf_stats = GetPerformanceStats::execute(context, &perf_params)?;
    let current_latency_ms = updated_perf_stats.average_latency.as_secs_f64() * 1000.0;

    println!("  Current search performance: {:.2}ms", current_latency_ms);
    if current_latency_ms > max_search_latency_ms {
        println!("    ⚠ Search time exceeds threshold ({:.0}ms)", max_search_latency_ms);
    } else {
        println!("    ✓ Search performance within limits");
    }

    Ok(())
}

/// Tracks historical performance data over time and analyzes trends.
///
/// This function demonstrates:
/// - Time-series data collection during operations
/// - Historical trend analysis (growth rates, efficiency changes)
/// - Performance trend identification
/// - Long-term monitoring patterns
fn track_historical_data(context: &mut ShardexContext) -> Result<(), Box<dyn Error>> {
    println!("Tracking historical data...");

    let mut history = Vec::new();
    let stats_params = GetStatsParams::new();

    // Simulate operations over time and collect metrics
    for minute in 0..5 {
        let timestamp = std::time::SystemTime::now();

        // Perform some operations using batch processing with tracking
        let postings = generate_test_postings(MEDIUM_BATCH_SIZE, VECTOR_SIZE);
        let batch_params = BatchAddPostingsParams::with_flush_and_tracking(postings)?;
        let batch_stats = BatchAddPostings::execute(context, &batch_params)?;

        // Collect metrics after operation
        let stats = GetStats::execute(context, &stats_params)?;

        history.push(HistoricalDataPoint {
            timestamp,
            total_postings: stats.total_postings,
            memory_usage: stats.memory_usage,
            operation_duration: batch_stats.processing_time,
            minute,
        });

        println!(
            "  Minute {}: {} postings, {:.1}MB memory, {:.0}ms operation, {:.0} docs/sec",
            minute + 1,
            stats.total_postings,
            stats.memory_usage as f64 / 1024.0 / 1024.0,
            batch_stats.processing_time.as_secs_f64() * 1000.0,
            batch_stats.throughput_docs_per_sec
        );

        std::thread::sleep(Duration::from_millis(HISTORICAL_TRACKING_SLEEP_MS));
    }

    // Analyze trends - requires at least 2 data points for comparison
    println!("\nTrend analysis:");

    if history.len() >= 2 {
        let first = &history[0];
        let last = &history[history.len() - 1];

        // Calculate absolute growth in key metrics
        let posting_growth = last.total_postings - first.total_postings;
        let memory_growth = (last.memory_usage as i64) - (first.memory_usage as i64);

        println!("  Posting growth: +{} documents", posting_growth);
        println!("  Memory growth: {:+.1}MB", memory_growth as f64 / 1024.0 / 1024.0);

        // Calculate average operation time across all recorded operations
        let avg_operation_time = history
            .iter()
            .map(|h| h.operation_duration.as_secs_f64())
            .sum::<f64>()
            / history.len() as f64;

        println!("  Average operation time: {:.0}ms", avg_operation_time * 1000.0);

        // Memory efficiency trend - indicates if memory usage per posting is improving or degrading
        if last.total_postings > 0 && first.total_postings > 0 {
            let first_memory_per_posting = first.memory_usage as f64 / first.total_postings as f64;
            let last_memory_per_posting = last.memory_usage as f64 / last.total_postings as f64;
            // Positive percentage = memory usage per posting increased (worse efficiency)
            // Negative percentage = memory usage per posting decreased (better efficiency)  
            let memory_efficiency_change = ((last_memory_per_posting - first_memory_per_posting) / first_memory_per_posting) * 100.0;

            println!("  Memory efficiency change: {:+.1}%", memory_efficiency_change);
        }
    }

    Ok(())
}

/// Prints basic index statistics in a concise format.
///
/// # Parameters
/// * `label` - Descriptive label for the statistics snapshot
/// * `stats` - IndexStats containing basic index metrics
fn print_basic_stats(label: &str, stats: &IndexStats) {
    println!(
        "  {}: {} postings in {} shards, {:.1}MB memory",
        label,
        stats.total_postings,
        stats.total_shards,
        stats.memory_usage as f64 / 1024.0 / 1024.0
    );
}

/// Prints detailed index statistics in a formatted display.
///
/// # Parameters
/// * `stats` - DetailedIndexStats containing comprehensive index metrics
#[allow(dead_code)]
fn print_detailed_stats(stats: &DetailedIndexStats) {
    println!("Detailed index statistics:");
    println!("  Shards: {}", stats.total_shards);
    println!("  Total postings: {}", stats.total_postings);
    println!("  Active postings: {}", stats.active_postings);
    println!("  Deleted postings: {}", stats.deleted_postings);
    println!("  Vector dimension: {}", stats.vector_dimension);
    println!("  Memory usage: {:.2} MB", stats.memory_usage as f64 / 1024.0 / 1024.0);
    println!("  Disk usage: {:.2} MB", stats.disk_usage as f64 / 1024.0 / 1024.0);
    println!(
        "  Average shard utilization: {:.1}%",
        stats.average_shard_utilization * 100.0
    );

    println!("  Bloom filter hit rate: {:.1}%", stats.bloom_filter_hit_rate * 100.0);
}

/// Prints combined detailed statistics from basic stats and performance metrics.
///
/// # Parameters
/// * `stats` - Basic IndexStats containing core index metrics
/// * `perf_stats` - PerformanceStats containing timing and operation metrics
fn print_combined_detailed_stats(
    stats: &IndexStats, 
    perf_stats: &shardex::api::PerformanceStats
) {
    println!("Combined detailed statistics:");
    println!("  Shards: {}", stats.total_shards);
    println!("  Total postings: {}", stats.total_postings);
    println!("  Vector dimension: {}", stats.vector_dimension);
    println!("  Memory usage: {:.2} MB", stats.memory_usage as f64 / 1024.0 / 1024.0);
    println!("  Disk usage: {:.2} MB", stats.disk_usage as f64 / 1024.0 / 1024.0);
    
    // Performance metrics
    println!("  Total operations: {}", perf_stats.total_operations);
    println!("  Average latency: {:.2}ms", perf_stats.average_latency.as_secs_f64() * 1000.0);
    println!("  Throughput: {:.0} ops/sec", perf_stats.throughput);
    
    if let Some(detailed) = &perf_stats.detailed_metrics {
        println!("  Index time: {:.2}ms", detailed.index_time.as_secs_f64() * 1000.0);
        println!("  Flush time: {:.2}ms", detailed.flush_time.as_secs_f64() * 1000.0);
        println!("  Search time: {:.2}ms", detailed.search_time.as_secs_f64() * 1000.0);
    }
}

/// Generates test postings with sequential document IDs and normalized random vectors.
///
/// # Parameters
/// * `count` - Number of postings to generate
/// * `vector_size` - Dimensionality of the vectors
///
/// # Returns
/// A vector of Posting objects with normalized random vectors
fn generate_test_postings(count: usize, vector_size: usize) -> Vec<Posting> {
    (0..count)
        .map(|i| {
            let document_id = DocumentId::from_raw((i + 1) as u128);
            let vector = generate_test_vector(vector_size);

            Posting {
                document_id,
                start: 0,
                length: 100,
                vector,
            }
        })
        .collect()
}

/// Generates a normalized random vector using deterministic hashing.
///
/// # Parameters
/// * `size` - Dimensionality of the vector
///
/// # Returns
/// A normalized vector with magnitude 1.0
fn generate_test_vector(size: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut vector = Vec::with_capacity(size);
    let mut hasher = DefaultHasher::new();
    
    // Hash the size once to get a base seed, then use arithmetic progression
    // This avoids creating a new hasher for each element
    size.hash(&mut hasher);
    let mut seed = hasher.finish();

    for _i in 0..size {
        // Use arithmetic progression instead of rehashing for efficiency
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345); // Linear congruential generator
        let value = (seed % 1000) as f32 / 1000.0;
        vector.push(value);
    }

    // Normalize the vector to unit magnitude
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut vector {
            *value /= magnitude;
        }
    }

    vector
}

/// Historical data point for trend analysis containing metrics at a specific time.
struct HistoricalDataPoint {
    #[allow(dead_code)]
    timestamp: std::time::SystemTime,
    total_postings: usize,
    memory_usage: usize,
    operation_duration: Duration,
    #[allow(dead_code)]
    minute: usize,
}

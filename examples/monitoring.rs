//! Monitoring and statistics example for Shardex
//!
//! This example demonstrates:
//! - Comprehensive statistics collection
//! - Performance monitoring and metrics
//! - Index health monitoring
//! - Resource usage tracking

use shardex::{Shardex, ShardexConfig, ShardexImpl, Posting, DocumentId, DetailedIndexStats};
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Monitoring and Statistics Example");
    println!("=========================================");

    let temp_dir = std::env::temp_dir().join("shardex_monitoring_example");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Create index with monitoring-friendly configuration
    let config = ShardexConfig::new()
        .directory_path(&temp_dir)
        .vector_size(256)
        .shard_size(5000)
        .batch_write_interval_ms(100);

    let mut index = ShardexImpl::create(config).await?;

    // Example 1: Basic statistics monitoring
    println!("\n1. Basic Statistics Monitoring");
    println!("==============================");

    monitor_basic_stats(&mut index).await?;

    // Example 2: Performance metrics collection
    println!("\n2. Performance Metrics Collection");
    println!("=================================");

    collect_performance_metrics(&mut index).await?;

    // Example 3: Resource usage monitoring
    println!("\n3. Resource Usage Monitoring");
    println!("============================");

    monitor_resource_usage(&mut index).await?;

    // Example 4: Detailed statistics analysis
    println!("\n4. Detailed Statistics Analysis");
    println!("===============================");

    analyze_detailed_statistics(&index).await?;

    // Example 5: Health monitoring and alerts
    println!("\n5. Health Monitoring and Alerts");
    println!("===============================");

    demonstrate_health_monitoring(&index).await?;

    // Example 6: Historical data tracking
    println!("\n6. Historical Data Tracking");
    println!("===========================");

    track_historical_data(&mut index).await?;

    // Clean up
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\nMonitoring examples completed!");

    Ok(())
}

async fn monitor_basic_stats(index: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("Monitoring basic index statistics...");

    // Get initial stats
    let initial_stats = index.stats().await?;
    print_basic_stats("Initial", &initial_stats);

    // Add some data and monitor changes
    let postings = generate_test_postings(1000, 256);
    index.add_postings(postings).await?;
    
    let after_add_stats = index.stats().await?;
    print_basic_stats("After adding 1000 postings", &after_add_stats);

    // Flush and check again
    index.flush().await?;
    let after_flush_stats = index.stats().await?;
    print_basic_stats("After flush", &after_flush_stats);

    // Remove some documents
    let docs_to_remove: Vec<u128> = (1..=100).collect();
    index.remove_documents(docs_to_remove).await?;
    index.flush().await?;
    
    let after_removal_stats = index.stats().await?;
    print_basic_stats("After removing 100 documents", &after_removal_stats);

    Ok(())
}

async fn collect_performance_metrics(index: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("Collecting performance metrics...");

    let query_vector = generate_test_vector(256);
    let mut search_times = Vec::new();
    let mut throughput_measurements = Vec::new();

    // Measure search latency for different k values
    println!("Search latency measurements:");
    for k in [1, 5, 10, 20, 50] {
        let start = Instant::now();
        let results = index.search(&query_vector, k, None).await?;
        let duration = start.elapsed();
        
        search_times.push((k, duration));
        println!("  k={:2}: {:6.2}ms ({} results)", 
            k, duration.as_secs_f64() * 1000.0, results.len());
    }

    // Measure indexing throughput
    println!("\nIndexing throughput measurements:");
    for batch_size in [100, 500, 1000, 2000] {
        let postings = generate_test_postings(batch_size, 256);
        
        let start = Instant::now();
        index.add_postings(postings).await?;
        index.flush().await?;
        let duration = start.elapsed();
        
        let throughput = batch_size as f64 / duration.as_secs_f64();
        throughput_measurements.push((batch_size, throughput));
        
        println!("  batch_size={:4}: {:8.0} docs/sec", batch_size, throughput);
    }

    // Calculate statistics
    let avg_throughput = throughput_measurements.iter()
        .map(|(_, t)| t)
        .sum::<f64>() / throughput_measurements.len() as f64;
    
    println!("\nPerformance summary:");
    println!("  Average throughput: {:.0} docs/sec", avg_throughput);
    
    if let Some(&(_, fastest_search)) = search_times.iter().min_by_key(|(_, d)| d) {
        println!("  Fastest search: {:.2}ms", fastest_search.as_secs_f64() * 1000.0);
    }

    Ok(())
}

async fn monitor_resource_usage(index: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("Monitoring resource usage...");

    // Monitor resource usage over time
    let mut measurements = Vec::new();
    
    for i in 0..10 {
        let stats = index.stats().await?;
        measurements.push((
            i,
            stats.memory_usage,
            stats.disk_usage,
            stats.total_postings,
        ));

        // Add more data to see resource growth
        if i < 5 {
            let postings = generate_test_postings(200, 256);
            index.add_postings(postings).await?;
            if i % 2 == 0 {
                index.flush().await?;
            }
        }

        sleep(Duration::from_millis(200)).await;
    }

    println!("Resource usage over time:");
    println!("  Step | Memory (MB) | Disk (MB) | Postings");
    println!("  -----|-------------|-----------|----------");
    
    for (step, memory, disk, postings) in measurements {
        println!("  {:4} | {:11.2} | {:9.2} | {:8}",
            step,
            memory as f64 / 1024.0 / 1024.0,
            disk as f64 / 1024.0 / 1024.0,
            postings
        );
    }

    // Check resource efficiency
    let final_stats = index.stats().await?;
    if final_stats.total_postings > 0 {
        let memory_per_posting = final_stats.memory_usage as f64 / final_stats.total_postings as f64;
        let disk_per_posting = final_stats.disk_usage as f64 / final_stats.total_postings as f64;
        
        println!("\nResource efficiency:");
        println!("  Memory per posting: {:.0} bytes", memory_per_posting);
        println!("  Disk per posting: {:.0} bytes", disk_per_posting);
        println!("  Compression ratio: {:.2}x", memory_per_posting / disk_per_posting);
    }

    Ok(())
}

async fn analyze_detailed_statistics(index: &ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("Analyzing detailed statistics...");

    let detailed_stats = index.detailed_stats().await?;
    print_detailed_stats(&detailed_stats);

    // Analyze shard distribution
    if detailed_stats.total_shards > 0 {
        let avg_postings_per_shard = detailed_stats.total_postings as f64 / detailed_stats.total_shards as f64;
        println!("\nShard analysis:");
        println!("  Average postings per shard: {:.1}", avg_postings_per_shard);
        println!("  Shard utilization: {:.1}%", detailed_stats.average_shard_utilization * 100.0);
        
        if detailed_stats.average_shard_utilization < 0.5 {
            println!("  ⚠ Warning: Low shard utilization detected");
        }
    }

    // Analyze deletion efficiency
    if detailed_stats.total_postings > 0 {
        let deletion_ratio = detailed_stats.deleted_postings as f64 / detailed_stats.total_postings as f64;
        println!("\nDeletion analysis:");
        println!("  Active postings: {}", detailed_stats.active_postings);
        println!("  Deleted postings: {}", detailed_stats.deleted_postings);
        println!("  Deletion ratio: {:.1}%", deletion_ratio * 100.0);
        
        if deletion_ratio > 0.3 {
            println!("  ⚠ Warning: High deletion ratio - consider compaction");
        }
    }

    Ok(())
}

async fn demonstrate_health_monitoring(index: &ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("Demonstrating health monitoring...");

    let stats = index.stats().await?;
    let detailed_stats = index.detailed_stats().await?;
    
    // Define health thresholds
    let max_memory_mb = 1024.0; // 1GB
    let max_shard_count = 100;
    let min_utilization = 0.3;
    let max_deletion_ratio = 0.4;
    
    let mut health_score = 100.0;
    let mut alerts = Vec::new();

    // Memory usage check
    let memory_mb = stats.memory_usage as f64 / 1024.0 / 1024.0;
    if memory_mb > max_memory_mb {
        health_score -= 20.0;
        alerts.push(format!("High memory usage: {:.1}MB (limit: {:.1}MB)", 
            memory_mb, max_memory_mb));
    }

    // Shard count check
    if stats.total_shards > max_shard_count {
        health_score -= 15.0;
        alerts.push(format!("Too many shards: {} (limit: {})", 
            stats.total_shards, max_shard_count));
    }

    // Utilization check
    if detailed_stats.average_shard_utilization < min_utilization {
        health_score -= 10.0;
        alerts.push(format!("Low shard utilization: {:.1}% (minimum: {:.1}%)", 
            detailed_stats.average_shard_utilization * 100.0, min_utilization * 100.0));
    }

    // Deletion ratio check
    if detailed_stats.total_postings > 0 {
        let deletion_ratio = detailed_stats.deleted_postings as f64 / detailed_stats.total_postings as f64;
        if deletion_ratio > max_deletion_ratio {
            health_score -= 15.0;
            alerts.push(format!("High deletion ratio: {:.1}% (maximum: {:.1}%)", 
                deletion_ratio * 100.0, max_deletion_ratio * 100.0));
        }
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

    // Performance health check
    let query_vector = generate_test_vector(256);
    let search_start = Instant::now();
    let _results = index.search(&query_vector, 10, None).await?;
    let search_duration = search_start.elapsed();
    
    let max_search_time_ms = 100.0;
    let search_time_ms = search_duration.as_secs_f64() * 1000.0;
    
    println!("  Search performance: {:.2}ms", search_time_ms);
    if search_time_ms > max_search_time_ms {
        println!("    ⚠ Search time exceeds threshold ({:.0}ms)", max_search_time_ms);
    } else {
        println!("    ✓ Search performance within limits");
    }

    Ok(())
}

async fn track_historical_data(index: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("Tracking historical data...");

    let mut history = Vec::new();
    
    // Simulate operations over time and collect metrics
    for minute in 0..5 {
        let timestamp = std::time::SystemTime::now();
        
        // Perform some operations
        let postings = generate_test_postings(100, 256);
        let start = Instant::now();
        index.add_postings(postings).await?;
        index.flush().await?;
        let operation_duration = start.elapsed();
        
        // Collect metrics
        let stats = index.stats().await?;
        
        history.push(HistoricalDataPoint {
            timestamp,
            total_postings: stats.total_postings,
            memory_usage: stats.memory_usage,
            operation_duration,
            minute,
        });
        
        println!("  Minute {}: {} postings, {:.1}MB memory, {:.0}ms operation",
            minute + 1,
            stats.total_postings,
            stats.memory_usage as f64 / 1024.0 / 1024.0,
            operation_duration.as_secs_f64() * 1000.0
        );
        
        sleep(Duration::from_millis(500)).await;
    }

    // Analyze trends
    println!("\nTrend analysis:");
    
    if history.len() >= 2 {
        let first = &history[0];
        let last = &history[history.len() - 1];
        
        let posting_growth = last.total_postings - first.total_postings;
        let memory_growth = (last.memory_usage as i64) - (first.memory_usage as i64);
        
        println!("  Posting growth: +{} documents", posting_growth);
        println!("  Memory growth: {:+.1}MB", memory_growth as f64 / 1024.0 / 1024.0);
        
        // Calculate average operation time
        let avg_operation_time = history.iter()
            .map(|h| h.operation_duration.as_secs_f64())
            .sum::<f64>() / history.len() as f64;
            
        println!("  Average operation time: {:.0}ms", avg_operation_time * 1000.0);
    }

    Ok(())
}

fn print_basic_stats(label: &str, stats: &shardex::IndexStats) {
    println!("  {}: {} postings in {} shards, {:.1}MB memory",
        label,
        stats.total_postings,
        stats.total_shards,
        stats.memory_usage as f64 / 1024.0 / 1024.0
    );
}

fn print_detailed_stats(stats: &DetailedIndexStats) {
    println!("Detailed index statistics:");
    println!("  Shards: {}", stats.total_shards);
    println!("  Total postings: {}", stats.total_postings);
    println!("  Active postings: {}", stats.active_postings);
    println!("  Deleted postings: {}", stats.deleted_postings);
    println!("  Vector dimension: {}", stats.vector_dimension);
    println!("  Memory usage: {:.2} MB", stats.memory_usage as f64 / 1024.0 / 1024.0);
    println!("  Disk usage: {:.2} MB", stats.disk_usage as f64 / 1024.0 / 1024.0);
    println!("  Average shard utilization: {:.1}%", stats.average_shard_utilization * 100.0);
    
    println!("  Bloom filter hit rate: {:.1}%", stats.bloom_filter_hit_rate * 100.0);
}

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

fn generate_test_vector(size: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    let mut vector = Vec::with_capacity(size);
    let mut hasher = DefaultHasher::new();
    
    for i in 0..size {
        i.hash(&mut hasher);
        let value = (hasher.finish() % 1000) as f32 / 1000.0;
        vector.push(value);
    }
    
    // Normalize
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut vector {
            *value /= magnitude;
        }
    }
    
    vector
}

struct HistoricalDataPoint {
    timestamp: std::time::SystemTime,
    total_postings: usize,
    memory_usage: usize,
    operation_duration: Duration,
    minute: usize,
}
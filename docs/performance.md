# Shardex Performance Tuning Guide

This guide provides comprehensive information on optimizing Shardex performance for different workloads and environments.

## Performance Fundamentals

Shardex performance depends on several key factors:

1. **Memory Usage**: How efficiently memory is used for vectors and metadata
2. **Disk I/O**: How quickly data can be written to and read from disk
3. **Vector Operations**: How efficiently similarity calculations are performed
4. **Index Structure**: How well the shard organization supports your query patterns

## Configuration Parameters

### Vector Dimensions

The number of dimensions directly affects memory usage and computation time:

```rust
// Higher dimensions = more memory and computation
let config = ShardexConfig::new()
    .vector_size(128)   // ~512 bytes per vector
    .vector_size(384)   // ~1536 bytes per vector  
    .vector_size(768)   // ~3072 bytes per vector
    .vector_size(1536); // ~6144 bytes per vector
```

**Recommendations:**
- Use the smallest dimension that maintains acceptable accuracy
- Consider dimensionality reduction techniques (PCA, t-SNE) for large vectors
- Monitor memory usage as dimensions increase

### Shard Size Optimization

Shard size affects both memory usage and search performance:

```rust
let config = ShardexConfig::new()
    .shard_size(1000)    // Small shards: less memory per search
    .shard_size(10000)   // Default: balanced performance
    .shard_size(50000)   // Large shards: fewer splits, more memory
    .shard_size(100000); // Very large: maximum throughput
```

**Trade-offs:**
- **Small shards (1K-5K vectors)**:
  - ✅ Lower memory per search operation
  - ✅ Faster individual shard searches
  - ❌ More shards to manage (higher overhead)
  - ❌ More frequent splits

- **Large shards (50K-100K vectors)**:
  - ✅ Fewer shards to manage
  - ✅ Better batch insertion performance
  - ❌ Higher memory per search
  - ❌ Slower individual shard searches

### Batch Write Configuration

Optimize write performance through batching:

```rust
let config = ShardexConfig::new()
    .batch_write_interval_ms(10)   // Very responsive (higher CPU)
    .batch_write_interval_ms(100)  // Default: balanced
    .batch_write_interval_ms(500); // High throughput (higher latency)
```

**Guidelines:**
- **10-50ms**: Interactive applications requiring low latency
- **100-200ms**: General purpose applications  
- **500-1000ms**: Batch processing with high throughput requirements

### Slop Factor Tuning

Balance search speed vs. accuracy:

```rust
// During search
let results = index.search(&query, k, Some(1)).await?;  // Fastest
let results = index.search(&query, k, Some(3)).await?;  // Balanced  
let results = index.search(&query, k, Some(10)).await?; // Most accurate
```

**Performance Impact:**
- **Slop 1**: Searches only the closest shard (fastest, lowest accuracy)
- **Slop 3**: Searches 3 closest shards (balanced)
- **Slop 5+**: Searches many shards (slower, higher accuracy)

### Bloom Filter Optimization

Configure bloom filters for deletion performance:

```rust
let config = ShardexConfig::new()
    .bloom_filter_size(512)   // Memory optimized
    .bloom_filter_size(1024)  // Default  
    .bloom_filter_size(4096); // Deletion optimized
```

**Recommendations:**
- **Small datasets (<10K docs)**: 512-1024 bits
- **Medium datasets (10K-100K docs)**: 1024-2048 bits
- **Large datasets (>100K docs)**: 2048-4096 bits
- **Heavy deletion workloads**: 4096+ bits

## Workload-Specific Optimizations

### High-Throughput Indexing

For maximum indexing performance:

```rust
let config = ShardexConfig::new()
    .vector_size(384)                    // Match your embedding model
    .shard_size(50000)                   // Large shards
    .shardex_segment_size(5000)          // More centroids per segment
    .wal_segment_size(4 * 1024 * 1024)  // 4MB WAL segments
    .batch_write_interval_ms(250)        // Longer batching window
    .bloom_filter_size(2048);            // Adequate for large datasets

// Batch your inserts
let batch_size = 5000;
let postings = generate_postings(batch_size);
index.add_postings(postings).await?;
```

### Low-Latency Search

For responsive search applications:

```rust
let config = ShardexConfig::new()
    .vector_size(256)                    // Smaller vectors if possible
    .shard_size(10000)                   // Smaller shards
    .shardex_segment_size(1000)          // Faster shard selection
    .batch_write_interval_ms(50)         // Responsive writes
    .default_slop_factor(2);             // Narrow search

// Search with low slop factor
let results = index.search(&query, 10, Some(1)).await?;
```

### Memory-Constrained Environments

For systems with limited memory:

```rust
let config = ShardexConfig::new()
    .vector_size(128)                    // Smallest acceptable dimension
    .shard_size(5000)                    // Small shards
    .shardex_segment_size(500)           // Fewer centroids in memory
    .wal_segment_size(256 * 1024)        // 256KB WAL segments
    .batch_write_interval_ms(200)        // Less frequent batching
    .bloom_filter_size(512);             // Minimal bloom filters
```

### Large-Scale Datasets

For million+ document datasets:

```rust
let config = ShardexConfig::new()
    .vector_size(768)                    // Full-precision vectors
    .shard_size(100000)                  // Very large shards
    .shardex_segment_size(10000)         // Many centroids per segment
    .wal_segment_size(8 * 1024 * 1024)  // 8MB WAL segments
    .batch_write_interval_ms(500)        // High-throughput batching
    .default_slop_factor(5)              // Accurate search
    .bloom_filter_size(4096);            // Large bloom filters
```

## Performance Monitoring

### Key Metrics to Track

```rust
// Get comprehensive statistics
let stats = index.detailed_stats().await?;

// Monitor these key metrics:
println!("Throughput: {:.0} ops/sec", stats.write_throughput);
println!("Search P95: {:?}", stats.search_latency_p95);
println!("Memory: {:.1}MB", stats.memory_usage as f64 / 1024.0 / 1024.0);
println!("Shard utilization: {:.1}%", stats.average_shard_utilization * 100.0);
println!("Bloom hit rate: {:.1}%", stats.bloom_filter_hit_rate * 100.0);
```

### Performance Alerts

Set up monitoring for these conditions:

```rust
// Check for performance issues
let stats = index.detailed_stats().await?;

// High memory usage
if stats.memory_usage > 2 * 1024 * 1024 * 1024 { // 2GB
    println!("⚠ High memory usage detected");
}

// Poor shard utilization  
if stats.average_shard_utilization < 0.3 {
    println!("⚠ Low shard utilization - consider smaller shard_size");
}

// Too many shards
if stats.total_shards > 1000 {
    println!("⚠ Many shards detected - consider larger shard_size");
}

// High deletion ratio
let deletion_ratio = stats.deleted_postings as f64 / stats.total_postings as f64;
if deletion_ratio > 0.4 {
    println!("⚠ High deletion ratio - consider index rebuild");
}
```

## Benchmarking Your Setup

### Create a Performance Test

```rust
use std::time::Instant;
use shardex::{Shardex, ShardexConfig, ShardexImpl, Posting, DocumentId};

async fn benchmark_indexing(
    config: ShardexConfig,
    num_docs: usize,
    batch_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut index = ShardexImpl::create(config).await?;
    
    let start_time = Instant::now();
    let mut total_docs = 0;
    
    while total_docs < num_docs {
        let current_batch = (num_docs - total_docs).min(batch_size);
        let postings = generate_random_postings(current_batch, 384);
        
        index.add_postings(postings).await?;
        index.flush().await?;
        
        total_docs += current_batch;
        
        // Progress reporting
        if total_docs % (batch_size * 10) == 0 {
            let elapsed = start_time.elapsed();
            let rate = total_docs as f64 / elapsed.as_secs_f64();
            println!("Indexed {} docs, rate: {:.0} docs/sec", total_docs, rate);
        }
    }
    
    let total_time = start_time.elapsed();
    let final_rate = num_docs as f64 / total_time.as_secs_f64();
    
    println!("Final indexing rate: {:.0} docs/sec", final_rate);
    
    Ok(())
}

async fn benchmark_search(
    index: &ShardexImpl,
    num_queries: usize,
    k: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let query_vector = generate_random_vector(384);
    let mut search_times = Vec::new();
    
    // Warmup
    for _ in 0..10 {
        index.search(&query_vector, k, None).await?;
    }
    
    // Actual benchmark
    for _ in 0..num_queries {
        let start = Instant::now();
        let _results = index.search(&query_vector, k, None).await?;
        search_times.push(start.elapsed());
    }
    
    // Calculate statistics
    search_times.sort();
    let p50 = search_times[search_times.len() / 2];
    let p95 = search_times[search_times.len() * 95 / 100];
    let p99 = search_times[search_times.len() * 99 / 100];
    
    println!("Search performance (k={}):", k);
    println!("  P50: {:.2}ms", p50.as_secs_f64() * 1000.0);
    println!("  P95: {:.2}ms", p95.as_secs_f64() * 1000.0);
    println!("  P99: {:.2}ms", p99.as_secs_f64() * 1000.0);
    
    Ok(())
}
```

## Hardware Considerations

### CPU Optimization

- **SIMD Support**: Ensure your CPU supports AVX2 or AVX-512 for faster vector operations
- **Multi-core**: Shardex uses Rayon for parallel operations - more cores help
- **Cache Size**: Larger L3 cache improves performance for large shard searches

### Memory Recommendations

Calculate memory requirements:

```
Estimated Memory = (num_vectors × vector_dimension × 4 bytes) × 1.5
```

Example for 1M vectors at 384 dimensions:
```
Memory = 1,000,000 × 384 × 4 bytes × 1.5 = ~2.3GB
```

### Storage Optimization

#### SSD vs HDD
- **SSD Recommended**: 10-100x faster for random I/O operations
- **NVMe Best**: Lowest latency for WAL operations
- **HDD Acceptable**: For read-heavy workloads with sufficient RAM

#### Disk Space Planning
```
Disk Usage = (vector storage) + (posting storage) + (index overhead) + (WAL)
           ≈ (num_vectors × vector_dimension × 4) × 1.8
```

## Common Performance Issues

### Problem: Slow Searches

**Symptoms**: High search latency, timeout errors
**Solutions**:
1. Reduce slop factor
2. Use smaller shard sizes
3. Reduce vector dimensions if possible
4. Add more RAM for better caching

### Problem: High Memory Usage

**Symptoms**: Out of memory errors, system swapping
**Solutions**:
1. Reduce shard size
2. Use smaller vector dimensions
3. Implement batch processing with smaller batches
4. Increase system RAM

### Problem: Slow Indexing

**Symptoms**: Low throughput, high CPU usage during indexing
**Solutions**:
1. Increase batch size
2. Increase batch write interval
3. Use larger shards
4. Optimize vector generation pipeline

### Problem: Frequent Shard Splits

**Symptoms**: Many small shards, degraded performance over time
**Solutions**:
1. Increase shard size
2. Pre-allocate capacity if dataset size is known
3. Monitor shard utilization metrics

## Production Deployment Tips

### Resource Allocation
```bash
# Recommended minimum resources
CPU: 4 cores (8+ preferred)
RAM: 4GB + (dataset_size × 1.5)
Storage: SSD with 100GB+ free space
```

### Operating System Tuning
```bash
# Increase memory map limits
echo 'vm.max_map_count = 262144' >> /etc/sysctl.conf

# Optimize for memory-mapped files
echo 'vm.swappiness = 1' >> /etc/sysctl.conf

# Increase file descriptor limits
echo '* soft nofile 65536' >> /etc/security/limits.conf
echo '* hard nofile 65536' >> /etc/security/limits.conf
```

### Monitoring in Production
```rust
// Set up regular monitoring
tokio::spawn(async move {
    loop {
        let stats = index.stats().await?;
        
        // Log key metrics
        log::info!("Index stats: {} docs, {:.1}MB, {} shards", 
            stats.total_postings,
            stats.memory_usage as f64 / 1024.0 / 1024.0,
            stats.total_shards
        );
        
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
});
```

## Performance Comparison

### Typical Performance Ranges

| Configuration | Indexing Rate | Search P95 | Memory/1M docs |
|---------------|---------------|------------|-----------------|
| Memory-optimized | 5K docs/sec | 15ms | 1.2GB |
| Balanced | 8K docs/sec | 8ms | 1.8GB |
| Speed-optimized | 15K docs/sec | 3ms | 2.5GB |
| Large-scale | 12K docs/sec | 5ms | 2.0GB |

### Scaling Characteristics

- **Linear scaling**: Performance scales roughly linearly with data size up to memory limits
- **Search performance**: Degrades logarithmically with dataset size
- **Memory usage**: Scales linearly with dataset size and vector dimensions

## Advanced Optimization Techniques

### Custom Distance Metrics
For specialized use cases, implement custom distance functions:

```rust
use shardex::DistanceMetric;

// Use specialized distance metric for your data
let results = index.search_with_metric(
    &query_vector, 
    10, 
    DistanceMetric::Euclidean,  // or Cosine, DotProduct
    Some(3)
).await?;
```

### Batch Operations
Optimize for bulk operations:

```rust
// Process in optimal batch sizes
const OPTIMAL_BATCH_SIZE: usize = 5000;

for chunk in documents.chunks(OPTIMAL_BATCH_SIZE) {
    let postings: Vec<_> = chunk.iter()
        .map(|doc| create_posting(doc))
        .collect();
    
    index.add_postings(postings).await?;
    
    // Flush periodically, not after every batch
    if chunk_count % 10 == 0 {
        index.flush().await?;
    }
}
```

### Monitoring and Alerting
Set up comprehensive monitoring:

```rust
// Custom performance monitor
struct PerformanceMonitor {
    search_times: VecDeque<Duration>,
    index_times: VecDeque<Duration>,
    last_stats: Option<DetailedIndexStats>,
}

impl PerformanceMonitor {
    fn check_performance(&mut self, current_stats: &DetailedIndexStats) {
        // Detect performance regressions
        if let Some(ref last) = self.last_stats {
            if current_stats.search_latency_p95 > last.search_latency_p95 * 1.5 {
                log::warn!("Search performance degradation detected");
            }
        }
        
        self.last_stats = Some(current_stats.clone());
    }
}
```
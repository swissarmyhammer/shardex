# Shardex Troubleshooting Guide

This guide helps you diagnose and resolve common issues with Shardex.

## General Troubleshooting

### Enable Debug Logging

First, enable debug logging to get more information:

```rust
// Add to your Cargo.toml
[dependencies]
tracing = "0.1"
tracing-subscriber = "0.3"

// In your main function
use tracing_subscriber;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    // Your Shardex code here
}
```

### Check System Resources

```rust
use std::fs;

// Check available disk space
if let Ok(metadata) = fs::metadata("/your/index/path") {
    println!("Available space: {} bytes", metadata.len());
}

// Monitor memory usage
let stats = index.stats().await?;
println!("Memory usage: {:.1}MB", stats.memory_usage as f64 / 1024.0 / 1024.0);
```

## Configuration Issues

### Error: "Invalid vector dimension"

**Symptoms:**
```
Error: InvalidDimension { expected: 384, actual: 128 }
```

**Cause:** Vector dimensions don't match the index configuration.

**Solutions:**
1. **Check your vector generation:**
   ```rust
   // Make sure vectors match config
   let config = ShardexConfig::new().vector_size(384);
   let vector = vec![0.1; 384]; // Must be exactly 384 elements
   ```

2. **Verify embedding model output:**
   ```rust
   let embedding = your_embedding_model.encode(text)?;
   assert_eq!(embedding.len(), 384, "Embedding size mismatch");
   ```

3. **Check existing index configuration:**
   ```rust
   // When opening existing index
   let index = ShardexImpl::open("./existing_index").await?;
   let stats = index.stats().await?;
   println!("Index expects {} dimensions", stats.vector_dimension);
   ```

### Error: "Configuration error"

**Symptoms:**
```
Error: Config("Invalid shard size: 0")
```

**Cause:** Invalid configuration parameters.

**Solutions:**
1. **Validate configuration values:**
   ```rust
   let config = ShardexConfig::new()
       .vector_size(128)        // Must be > 0
       .shard_size(1000)        // Must be > 0
       .batch_write_interval_ms(100); // Must be > 0
   ```

2. **Check directory path:**
   ```rust
   use std::path::Path;
   
   let path = Path::new("./index");
   if path.exists() && !path.is_dir() {
       eprintln!("Path exists but is not a directory");
   }
   ```

## File System Issues

### Error: "Permission denied"

**Symptoms:**
```
Error: Io(Os { code: 13, kind: PermissionDenied, message: "Permission denied" })
```

**Solutions:**
1. **Check directory permissions:**
   ```bash
   ls -la /path/to/index/
   chmod 755 /path/to/index/
   ```

2. **Ensure write access:**
   ```rust
   use std::fs::OpenOptions;
   
   // Test write access
   let test_file = index_path.join("test_write");
   match OpenOptions::new().create(true).write(true).open(&test_file) {
       Ok(_) => {
           std::fs::remove_file(&test_file)?;
           println!("Write access confirmed");
       }
       Err(e) => eprintln!("Write access denied: {}", e),
   }
   ```

### Error: "No space left on device"

**Symptoms:**
```
Error: Io(Os { code: 28, kind: Other, message: "No space left on device" })
```

**Solutions:**
1. **Check disk space:**
   ```bash
   df -h /path/to/index/
   ```

2. **Estimate space requirements:**
   ```rust
   // Rough estimate: vectors × dimensions × 4 bytes × 1.8 overhead
   let estimated_size = num_vectors * vector_dimensions * 4 * 1.8;
   println!("Estimated space needed: {:.1}MB", estimated_size / 1024.0 / 1024.0);
   ```

3. **Clean up old WAL files:**
   ```rust
   // Check WAL directory size
   let wal_path = index_path.join("wal");
   let wal_size = calculate_directory_size(&wal_path)?;
   println!("WAL directory size: {:.1}MB", wal_size as f64 / 1024.0 / 1024.0);
   ```

### Error: "Too many open files"

**Symptoms:**
```
Error: Io(Os { code: 24, kind: Other, message: "Too many open files" })
```

**Solutions:**
1. **Increase file descriptor limit:**
   ```bash
   # Temporary fix
   ulimit -n 65536
   
   # Permanent fix (add to /etc/security/limits.conf)
   * soft nofile 65536
   * hard nofile 65536
   ```

2. **Check current limits:**
   ```bash
   ulimit -n  # Current limit
   lsof -p $(pid_of_your_process) | wc -l  # Current usage
   ```

## Memory Issues

### Error: "Cannot allocate memory"

**Symptoms:**
- Out of memory errors
- System becomes unresponsive
- Swap usage increases dramatically

**Solutions:**
1. **Reduce memory usage:**
   ```rust
   let config = ShardexConfig::new()
       .shard_size(5000)           // Smaller shards
       .vector_size(128)           // Smaller dimensions if possible
       .batch_write_interval_ms(200); // Less frequent batching
   ```

2. **Monitor memory usage:**
   ```rust
   async fn monitor_memory(index: &ShardexImpl) {
       let stats = index.stats().await?;
       let memory_mb = stats.memory_usage as f64 / 1024.0 / 1024.0;
       
       if memory_mb > 2048.0 { // 2GB threshold
           log::warn!("High memory usage: {:.1}MB", memory_mb);
       }
   }
   ```

3. **Use smaller batches:**
   ```rust
   // Instead of large batches
   // index.add_postings(vec_of_10000_postings).await?;
   
   // Use smaller batches
   for chunk in postings.chunks(1000) {
       index.add_postings(chunk.to_vec()).await?;
   }
   ```

### Memory Leaks

**Symptoms:**
- Memory usage continuously increases
- No corresponding increase in index size

**Debugging:**
```rust
// Monitor memory over time
let mut previous_memory = 0;
for i in 0..100 {
    let stats = index.stats().await?;
    let memory_diff = (stats.memory_usage as i64) - (previous_memory as i64);
    
    println!("Iteration {}: Memory usage: {:.1}MB (Δ{:+.1}MB)", 
        i, 
        stats.memory_usage as f64 / 1024.0 / 1024.0,
        memory_diff as f64 / 1024.0 / 1024.0
    );
    
    previous_memory = stats.memory_usage;
    
    // Your operations here
    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

## Performance Issues

### Slow Search Performance

**Symptoms:**
- Search queries take >100ms consistently
- CPU usage is high during searches

**Diagnosis:**
```rust
use std::time::Instant;

let start = Instant::now();
let results = index.search(&query_vector, 10, None).await?;
let duration = start.elapsed();

println!("Search took {:?} for {} results", duration, results.len());

// Check index statistics
let stats = index.stats().await?;
println!("Searching {} postings across {} shards", 
    stats.total_postings, stats.total_shards);
```

**Solutions:**
1. **Reduce slop factor:**
   ```rust
   // Instead of searching many shards
   let results = index.search(&query, 10, Some(10)).await?;
   
   // Search fewer shards
   let results = index.search(&query, 10, Some(2)).await?;
   ```

2. **Optimize shard sizes:**
   ```rust
   let stats = index.detailed_stats().await?;
   let avg_per_shard = stats.total_postings as f64 / stats.total_shards as f64;
   
   if avg_per_shard < 1000.0 {
       println!("Too many small shards - consider larger shard_size");
   } else if avg_per_shard > 50000.0 {
       println!("Very large shards - consider smaller shard_size");
   }
   ```

### Slow Indexing Performance

**Symptoms:**
- Low indexing throughput (<1000 docs/sec)
- High CPU usage during indexing

**Solutions:**
1. **Optimize batch size:**
   ```rust
   // Test different batch sizes
   for batch_size in [100, 500, 1000, 5000] {
       let start = Instant::now();
       let postings = generate_test_postings(batch_size);
       index.add_postings(postings).await?;
       index.flush().await?;
       
       let duration = start.elapsed();
       let rate = batch_size as f64 / duration.as_secs_f64();
       println!("Batch size {}: {:.0} docs/sec", batch_size, rate);
   }
   ```

2. **Increase batch interval:**
   ```rust
   let config = ShardexConfig::new()
       .batch_write_interval_ms(500); // Longer batching window
   ```

## Data Corruption Issues

### Error: "Index corruption detected"

**Symptoms:**
```
Error: Corruption("Checksum mismatch in shard file")
```

**Recovery Steps:**
1. **Check file integrity:**
   ```bash
   # Check for truncated files
   ls -la /path/to/index/shards/
   
   # Look for zero-byte files
   find /path/to/index/ -size 0
   ```

2. **Attempt recovery:**
   ```rust
   match ShardexImpl::open("./corrupted_index").await {
       Ok(index) => println!("Index opened successfully"),
       Err(ShardexError::Corruption(msg)) => {
           println!("Corruption detected: {}", msg);
           // Try to recover from WAL
           // (This would require additional recovery tools)
       }
       Err(e) => println!("Other error: {}", e),
   }
   ```

3. **Prevention:**
   ```rust
   // Always flush before shutdown
   index.flush().await?;
   
   // Regular integrity checks
   let stats = index.detailed_stats().await?;
   if stats.total_postings == 0 && expected_postings > 0 {
       log::warn!("Possible data loss detected");
   }
   ```

## Concurrent Access Issues

### Error: "Resource temporarily unavailable"

**Symptoms:**
```
Error: Io(Os { code: 11, kind: WouldBlock, message: "Resource temporarily unavailable" })
```

**Cause:** Multiple processes trying to access the same index.

**Solutions:**
1. **Check for other processes:**
   ```bash
   # Find processes using the index directory
   lsof +D /path/to/index/
   
   # Check for lock files
   ls -la /path/to/index/*.lock
   ```

2. **Implement proper shutdown:**
   ```rust
   // Ensure clean shutdown
   impl Drop for MyApplication {
       fn drop(&mut self) {
           // Flush any pending operations
           if let Some(ref mut index) = self.index {
               tokio::task::block_in_place(|| {
                   Handle::current().block_on(async {
                       let _ = index.flush().await;
                   })
               });
           }
       }
   }
   ```

## Search Result Issues

### Empty Search Results

**Symptoms:**
- Search returns no results when results are expected
- All similarity scores are 0.0

**Debugging:**
```rust
// Check if index has data
let stats = index.stats().await?;
println!("Index has {} active postings", stats.active_postings);

if stats.active_postings == 0 {
    println!("No active postings - check if documents were actually added");
}

// Check query vector
let query_magnitude: f32 = query_vector.iter().map(|x| x * x).sum::<f32>().sqrt();
if query_magnitude == 0.0 {
    println!("Query vector is zero - this will not produce meaningful results");
}

// Check vector normalization
println!("Query vector magnitude: {:.6}", query_magnitude);
```

### Unexpected Search Results

**Symptoms:**
- Results don't match expected similarity
- Similarity scores seem wrong

**Debugging:**
```rust
// Verify vector data
for (i, result) in results.iter().take(3).enumerate() {
    println!("Result {}: doc_id={}, similarity={:.6}", 
        i, result.document_id.to_u128(), result.similarity_score);
    
    // Check if vector data looks reasonable
    let vector_stats = VectorStats::new(&result.vector);
    println!("  Vector stats: min={:.6}, max={:.6}, mean={:.6}", 
        vector_stats.min, vector_stats.max, vector_stats.mean);
}

struct VectorStats {
    min: f32,
    max: f32,
    mean: f32,
}

impl VectorStats {
    fn new(vector: &[f32]) -> Self {
        let min = vector.iter().copied().fold(f32::INFINITY, f32::min);
        let max = vector.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let mean = vector.iter().sum::<f32>() / vector.len() as f32;
        
        Self { min, max, mean }
    }
}
```

## WAL (Write-Ahead Log) Issues

### Error: "WAL replay failed"

**Symptoms:**
```
Error: Corruption("WAL segment is corrupted")
```

**Recovery:**
```rust
// Check WAL directory
let wal_path = index_path.join("wal");
for entry in std::fs::read_dir(&wal_path)? {
    let entry = entry?;
    let size = entry.metadata()?.len();
    println!("WAL file: {} (size: {} bytes)", 
        entry.file_name().to_string_lossy(), size);
    
    if size == 0 {
        println!("  Warning: Zero-size WAL file detected");
    }
}
```

## Getting Help

### Collecting Debug Information

Before reporting issues, collect this information:

```rust
async fn collect_debug_info(index: &ShardexImpl) -> Result<String, Box<dyn Error>> {
    let stats = index.detailed_stats().await?;
    let mut debug_info = String::new();
    
    debug_info.push_str(&format!("Shardex Debug Information\n"));
    debug_info.push_str(&format!("========================\n"));
    debug_info.push_str(&format!("Version: {}\n", env!("CARGO_PKG_VERSION")));
    debug_info.push_str(&format!("Rust version: {}\n", env!("RUSTC_VERSION")));
    debug_info.push_str(&format!("OS: {}\n", std::env::consts::OS));
    debug_info.push_str(&format!("Architecture: {}\n", std::env::consts::ARCH));
    debug_info.push_str(&format!("\n"));
    
    debug_info.push_str(&format!("Index Statistics:\n"));
    debug_info.push_str(&format!("  Total shards: {}\n", stats.total_shards));
    debug_info.push_str(&format!("  Total postings: {}\n", stats.total_postings));
    debug_info.push_str(&format!("  Active postings: {}\n", stats.active_postings));
    debug_info.push_str(&format!("  Deleted postings: {}\n", stats.deleted_postings));
    debug_info.push_str(&format!("  Vector dimension: {}\n", stats.vector_dimension));
    debug_info.push_str(&format!("  Memory usage: {:.1}MB\n", 
        stats.memory_usage as f64 / 1024.0 / 1024.0));
    debug_info.push_str(&format!("  Disk usage: {:.1}MB\n", 
        stats.disk_usage as f64 / 1024.0 / 1024.0));
    debug_info.push_str(&format!("  Avg shard utilization: {:.1}%\n", 
        stats.average_shard_utilization * 100.0));
    
    Ok(debug_info)
}
```

### Creating Minimal Reproduction

When reporting bugs, create a minimal reproduction:

```rust
use shardex::{Shardex, ShardexConfig, ShardexImpl, Posting, DocumentId};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Minimal reproduction case
    let temp_dir = std::env::temp_dir().join("shardex_repro");
    let _ = std::fs::remove_dir_all(&temp_dir);
    
    let config = ShardexConfig::new()
        .directory_path(&temp_dir)
        .vector_size(3); // Small for easy debugging
    
    let mut index = ShardexImpl::create(config).await?;
    
    // Reproduce the issue here
    let posting = Posting {
        document_id: DocumentId::from_u128(1),
        start: 0,
        length: 10,
        vector: vec![1.0, 0.0, 0.0],
    };
    
    index.add_postings(vec![posting]).await?;
    index.flush().await?;
    
    // The problematic operation
    let query = vec![0.5, 0.5, 0.0];
    let results = index.search(&query, 1, None).await?;
    
    println!("Results: {:?}", results);
    
    // Cleanup
    std::fs::remove_dir_all(&temp_dir)?;
    
    Ok(())
}
```

### Where to Get Help

1. **Documentation**: Check the API docs at docs.rs/shardex
2. **Examples**: Look at the examples/ directory in the repository
3. **GitHub Issues**: Search existing issues or create a new one
4. **Debug logs**: Enable debug logging and include relevant output
5. **System information**: Include OS, Rust version, and hardware details

Remember to always include:
- Shardex version
- Rust version (`rustc --version`)
- Operating system
- Minimal reproduction case
- Full error messages with stack traces
- Debug logs if available
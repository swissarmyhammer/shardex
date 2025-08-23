# Troubleshooting Guide

This guide covers common issues and solutions when working with Shardex.

## Common Issues

### WAL Segment Full Errors

**Problem**: Error message "Segment full: cannot append record of X bytes"

**Cause**: The WAL (Write-Ahead Log) segment size is too small for the batch of operations being written.

**Solutions**:
1. **Increase WAL segment size** in your configuration:
   ```rust
   let config = ShardexConfig::new()
       .wal_segment_size(16 * 1024 * 1024) // 16MB instead of default 1MB
       .build();
   ```

2. **Reduce batch size** to fit within segments:
   ```rust
   let config = ShardexConfig::new()
       .batch_write_interval_ms(50) // More frequent, smaller batches
       .build();
   ```

3. **Use smaller vectors** if possible, as vector size directly impacts serialized operation size.

### Configuration Example Timeout

**Problem**: The configuration example hangs or times out during execution.

**Cause**: This can be caused by async/sync mixing issues or resource contention.

**Solutions**:
1. Check that you're running with sufficient resources
2. Reduce the test data size in examples if needed
3. Enable debug logging: `RUST_LOG=debug cargo run --example configuration`

### Memory Usage Issues

**Problem**: High memory usage or out-of-memory errors.

**Cause**: Large shard caches or too many concurrent operations.

**Solutions**:
1. **Reduce shard cache size**:
   ```rust
   index.set_cache_limit(50); // Reduce from default 100
   ```

2. **Clear cache periodically**:
   ```rust
   index.clear_cache(); // Free memory when not actively searching
   ```

3. **Use smaller shard sizes**:
   ```rust
   let config = ShardexConfig::new()
       .shard_size(5000) // Smaller shards use less memory
       .build();
   ```

### Index Corruption

**Problem**: Index fails to open or reports validation errors.

**Cause**: Unclean shutdowns, disk corruption, or concurrent access issues.

**Solutions**:
1. **Use index recovery**:
   ```rust
   let mut index = ShardexImpl::open(path).await?;
   index.attempt_index_recovery().await?;
   ```

2. **Validate index consistency**:
   ```rust
   let issues = index.validate_index().await?;
   if !issues.is_empty() {
       // Handle validation issues
   }
   ```

3. **Recreate index from backup** if recovery fails.

### Performance Issues

**Problem**: Slow search or indexing performance.

**Cause**: Suboptimal configuration, fragmented shards, or inappropriate slop factors.

**Solutions**:
1. **Optimize slop factor**:
   ```rust
   let results = index.search(&query, 10, Some(5)).await?; // Higher slop for accuracy
   ```

2. **Use performance configuration**:
   ```rust
   let config = ShardexConfig::new()
       .shard_size(50000)              // Larger shards
       .shardex_segment_size(5000)     // More centroids per segment  
       .wal_segment_size(16 * 1024 * 1024) // Larger WAL segments
       .batch_write_interval_ms(50)    // Faster batching
       .build();
   ```

3. **Monitor with detailed statistics**:
   ```rust
   let stats = index.detailed_stats().await?;
   println!("Memory usage: {:.2} MB", stats.memory_usage as f64 / 1024.0 / 1024.0);
   ```

## Getting Help

If you encounter issues not covered in this guide:

1. Check the [API Reference](api-reference.md) for detailed method documentation
2. Review the [Architecture Guide](architecture.md) to understand system components
3. Enable debug logging to get more detailed error information
4. File an issue with reproduction steps and debug output

## Debug Logging

Enable detailed logging to help diagnose issues:

```bash
# Debug level
RUST_LOG=debug cargo run --example configuration

# Trace level (very verbose)
RUST_LOG=trace cargo run --example configuration

# Target specific modules
RUST_LOG=shardex::wal=debug,shardex::batch_processor=debug cargo run
```
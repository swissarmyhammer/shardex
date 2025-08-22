# Step 39: Statistics and Monitoring

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement comprehensive statistics collection and monitoring capabilities for operational visibility.

## Tasks
- Create IndexStats implementation with all specified metrics
- Add performance monitoring for operations (latency, throughput)
- Implement resource usage tracking (memory, disk, file descriptors)
- Support monitoring integration and metrics export
- Include historical statistics and trending

## Acceptance Criteria
- [ ] IndexStats provides all specified metrics accurately
- [ ] Performance monitoring tracks operation latencies and throughput
- [ ] Resource usage monitoring prevents resource exhaustion
- [ ] Metrics can be exported for external monitoring systems
- [ ] Tests verify statistics accuracy and completeness
- [ ] Historical data enables performance trending and analysis

## Technical Details
```rust
pub struct IndexStats {
    pub total_shards: usize,
    pub total_postings: usize,
    pub pending_operations: usize,
    pub memory_usage: usize,
    pub disk_usage: usize,
    pub search_latency_p50: Duration,
    pub search_latency_p95: Duration,
    pub search_latency_p99: Duration,
    pub write_throughput: f64,
    pub bloom_filter_hit_rate: f64,
}

impl Shardex {
    pub async fn stats(&self) -> Result<IndexStats, ShardexError>;
    pub async fn detailed_stats(&self) -> Result<DetailedIndexStats, ShardexError>;
}
```

Include comprehensive metrics collection and efficient statistics computation.
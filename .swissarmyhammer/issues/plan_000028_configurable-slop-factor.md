# Step 28: Configurable Slop Factor

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement configurable slop factor system for balancing search accuracy versus performance.

## Tasks
- Create slop factor configuration and validation
- Implement dynamic shard selection based on slop factor
- Add performance monitoring for different slop values
- Support adaptive slop factor based on index characteristics
- Include slop factor impact analysis and reporting

## Acceptance Criteria
- [ ] Slop factor controls number of candidate shards searched
- [ ] Configuration validates slop factor ranges and constraints
- [ ] Performance monitoring shows slop factor impact
- [ ] Adaptive selection optimizes for index characteristics
- [ ] Tests verify slop factor behavior and performance
- [ ] Documentation explains slop factor trade-offs

## Technical Details
```rust
pub struct SlopFactorConfig {
    default_factor: usize,
    min_factor: usize,
    max_factor: usize,
    adaptive_enabled: bool,
    performance_threshold_ms: u64,
}

impl ShardexIndex {
    pub fn calculate_optimal_slop(&self, vector_size: usize, shard_count: usize) -> usize;
    pub fn select_shards_with_slop(&self, query: &[f32], slop: usize) -> Vec<ShardId>;
}
```

Include automatic slop factor tuning based on search performance and accuracy metrics.
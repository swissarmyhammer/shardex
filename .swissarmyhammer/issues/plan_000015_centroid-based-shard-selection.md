# Step 15: Centroid-Based Shard Selection

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement efficient centroid-based shard selection for both writes and searches.

## Tasks
- Implement nearest centroid calculation for single vectors
- Add configurable slop factor for selecting multiple candidate shards
- Support parallel centroid distance calculations
- Optimize for cache-friendly memory access patterns
- Include centroid distance caching where beneficial

## Acceptance Criteria
- [ ] Nearest centroid selection finds optimal shard for writes
- [ ] Slop factor selects multiple candidate shards for searches
- [ ] Parallel calculation improves performance on multi-core systems
- [ ] Memory access patterns are cache-friendly
- [ ] Tests verify selection accuracy and performance
- [ ] Algorithm handles edge cases (empty index, single shard)

## Technical Details
```rust
impl ShardexIndex {
    pub fn find_nearest_shard(&self, vector: &[f32]) -> Option<ShardId>;
    pub fn find_candidate_shards(&self, vector: &[f32], slop_factor: usize) -> Vec<ShardId>;
    pub fn calculate_centroid_distances(&self, vector: &[f32]) -> Vec<f32>;
}
```

Use SIMD operations where possible for vector distance calculations. Consider using rayon for parallel processing.
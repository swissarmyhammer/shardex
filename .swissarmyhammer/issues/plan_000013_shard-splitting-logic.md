# Step 13: Shard Splitting Logic with Clustering

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement shard splitting when capacity is reached, using clustering to create balanced sub-shards.

## Tasks
- Detect when shards reach capacity and need splitting
- Implement k-means clustering to split vectors into two groups
- Create two new shards from split results
- Handle posting reassignment during splits
- Support cascading splits when reassignment causes overflow

## Acceptance Criteria
- [ ] Capacity detection triggers splitting automatically
- [ ] K-means clustering creates balanced sub-shards
- [ ] New shards maintain all original postings
- [ ] Cascading splits handle chain reactions properly
- [ ] Tests verify split correctness and data integrity
- [ ] Performance is acceptable for large shards

## Technical Details
```rust
impl Shard {
    pub async fn should_split(&self) -> bool;
    pub async fn split(&self) -> Result<(Shard, Shard), ShardexError>;
    fn cluster_vectors(&self) -> Result<(Vec<usize>, Vec<usize>), ShardexError>;
}
```

Use k-means with k=2 for binary splitting. Consider vector distribution to avoid pathological splits.
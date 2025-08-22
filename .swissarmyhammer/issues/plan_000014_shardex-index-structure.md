# Step 14: In-Memory Index (Shardex) Structure

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement the main Shardex in-memory index that manages shard metadata and centroids.

## Tasks
- Create ShardexIndex struct for managing shard collection
- Implement centroid-based shard lookup and selection
- Add shard metadata tracking (IDs, centroids, statistics)
- Support dynamic shard addition and removal
- Include index persistence and loading

## Acceptance Criteria
- [ ] ShardexIndex manages collection of shard metadata
- [ ] Centroid-based lookup efficiently finds candidate shards
- [ ] Dynamic shard management handles splits and additions
- [ ] Index persistence survives restarts
- [ ] Tests verify index operations and shard management
- [ ] Memory usage is optimized for large shard counts

## Technical Details
```rust
pub struct ShardexIndex {
    shards: Vec<ShardMetadata>,
    centroids: Vec<Vec<f32>>,
    directory: PathBuf,
    vector_size: usize,
}

pub struct ShardMetadata {
    id: ShardId,
    centroid: Vec<f32>,
    posting_count: usize,
    capacity: usize,
    last_modified: SystemTime,
}
```

Use parallel vectors for cache-friendly access patterns.
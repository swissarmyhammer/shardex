# Step 23: Integration with Shard Metadata

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Integrate bloom filters with shard metadata to enable efficient document deletion and lookups.

## Tasks
- Add bloom filter to ShardMetadata structure
- Implement bloom filter updates during shard operations
- Support bloom filter persistence and loading
- Add document lookup optimization using bloom filters
- Include bloom filter maintenance during shard splits

## Acceptance Criteria
- [ ] Each shard has an associated bloom filter for document IDs
- [ ] Bloom filters are updated during add/remove operations
- [ ] Persistence preserves bloom filter state across restarts
- [ ] Document lookups skip shards efficiently using bloom filters
- [ ] Tests verify integration correctness and performance
- [ ] Shard splits handle bloom filter partitioning correctly

## Technical Details
```rust
pub struct ShardMetadata {
    id: ShardId,
    centroid: Vec<f32>,
    posting_count: usize,
    capacity: usize,
    last_modified: SystemTime,
    bloom_filter: BloomFilter,  // Added bloom filter
}

impl Shard {
    pub async fn contains_document(&self, doc_id: DocumentId) -> bool;
    pub async fn update_bloom_filter(&mut self, doc_id: DocumentId, insert: bool);
}
```

Use bloom filter checks before expensive shard scans to improve deletion performance.
# Step 24: Efficient Document Deletion Support

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement efficient document deletion across all shards using bloom filter optimization.

## Tasks
- Create document deletion algorithm using bloom filter pre-filtering
- Implement parallel deletion across candidate shards
- Add deletion result tracking and reporting
- Support batch deletion for multiple documents
- Include centroid updates after deletions

## Acceptance Criteria
- [ ] Bloom filters eliminate unnecessary shard scans
- [ ] Parallel deletion improves performance for multi-shard documents
- [ ] Deletion results report actual removed postings count
- [ ] Batch deletion processes multiple documents efficiently
- [ ] Tests verify deletion correctness and performance
- [ ] Centroids are updated properly after deletions

## Technical Details
```rust
impl ShardexIndex {
    pub async fn delete_document(&mut self, doc_id: DocumentId) -> Result<usize, ShardexError>;
    pub async fn delete_documents(&mut self, doc_ids: &[DocumentId]) -> Result<Vec<usize>, ShardexError>;
    
    fn find_candidate_shards_for_deletion(&self, doc_id: DocumentId) -> Vec<ShardId>;
}
```

Use bloom filter false positive handling and maintain statistics on deletion efficiency.
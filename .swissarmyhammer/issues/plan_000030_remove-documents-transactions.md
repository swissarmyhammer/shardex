# Step 30: Remove Documents Transaction Handling

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement complete transaction handling for document removal with WAL integration and bloom filter updates.

## Tasks
- Create remove documents transaction workflow
- Implement WAL recording for removal operations
- Add efficient document lookup using bloom filters
- Support batch document removal
- Include centroid updates after removals

## Acceptance Criteria
- [ ] Remove operations are recorded in WAL before shard updates
- [ ] Bloom filters optimize document lookup performance
- [ ] Batch removal processes multiple documents efficiently
- [ ] Centroids are updated correctly after removals
- [ ] Tests verify removal correctness and performance
- [ ] Transaction integrity is maintained throughout operations

## Technical Details
```rust
impl Shardex {
    pub async fn remove_documents(&mut self, document_ids: Vec<DocumentId>) -> Result<(), ShardexError>;
    
    async fn remove_documents_transaction(&mut self, document_ids: Vec<DocumentId>) -> Result<(), ShardexError> {
        // 1. Record in WAL
        // 2. Find candidate shards using bloom filters
        // 3. Remove from each shard
        // 4. Update bloom filters
        // 5. Update centroids
        // 6. Advance WAL pointer
    }
}
```

Include removal statistics reporting and efficient multi-shard removal coordination.
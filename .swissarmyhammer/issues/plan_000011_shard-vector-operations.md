# Step 11: Shard Vector Operations (Add, Remove, Search)

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement core vector operations within individual shards: add, remove, and basic search.

## Tasks
- Implement adding vectors and postings to shards
- Add vector removal with deleted bit tracking
- Create basic vector similarity search within shard
- Support batch operations for efficiency
- Include capacity checking and overflow detection

## Acceptance Criteria
- [ ] Shard can add vectors with corresponding postings
- [ ] Vector removal marks postings as deleted without data loss
- [ ] Basic similarity search returns nearest vectors
- [ ] Batch operations improve performance over individual calls
- [ ] Capacity limits are enforced with proper errors
- [ ] Tests verify all vector operations and edge cases

## Technical Details
```rust
impl Shard {
    pub async fn add_posting(&mut self, posting: Posting) -> Result<(), ShardexError>;
    pub async fn remove_document(&mut self, doc_id: DocumentId) -> Result<usize, ShardexError>;
    pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>, ShardexError>;
    pub fn is_full(&self) -> bool;
    pub fn available_capacity(&self) -> usize;
}
```

Use dot product or cosine similarity for vector comparisons.
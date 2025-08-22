# Step 10: Individual Shard Creation and Management

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement individual shard creation, initialization, and basic management operations.

## Tasks
- Create Shard struct combining vector and posting storage
- Implement shard creation with configurable capacity
- Add shard loading from existing files
- Support shard metadata tracking (ID, creation time, stats)
- Include basic shard validation and integrity checks

## Acceptance Criteria
- [ ] Shard combines VectorStorage and PostingStorage
- [ ] Shard creation initializes both vector and posting files
- [ ] Loading validates shard integrity and configuration
- [ ] Shard metadata tracks essential statistics
- [ ] Tests verify shard creation, loading, and validation
- [ ] Error handling covers all failure scenarios

## Technical Details
```rust
pub struct Shard {
    id: ShardId,
    vector_storage: VectorStorage,
    posting_storage: PostingStorage,
    capacity: usize,
    vector_size: usize,
    directory: PathBuf,
}

impl Shard {
    pub async fn create(id: ShardId, capacity: usize, vector_size: usize, directory: PathBuf) -> Result<Self, ShardexError>;
    pub async fn open(id: ShardId, directory: PathBuf) -> Result<Self, ShardexError>;
}
```

Ensure atomic creation and proper cleanup on failures.
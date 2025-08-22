# Step 17: Copy-on-Write Index Updates

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement copy-on-write mechanics for the Shardex index to enable non-blocking reads during updates.

## Tasks
- Implement copy-on-write semantics for ShardexIndex
- Support atomic index updates without blocking readers
- Add reference counting for safe memory management
- Handle concurrent access patterns safely
- Include proper cleanup of old index versions

## Acceptance Criteria
- [ ] Index updates don't block concurrent readers
- [ ] Copy-on-write semantics preserve consistency
- [ ] Memory management prevents leaks and use-after-free
- [ ] Atomic updates maintain index integrity
- [ ] Tests verify concurrent access safety
- [ ] Performance overhead is minimal for typical workloads

## Technical Details
```rust
pub struct CowShardexIndex {
    inner: Arc<ShardexIndex>,
}

impl CowShardexIndex {
    pub fn clone_for_write(&self) -> Self;
    pub async fn commit_changes(&mut self, new_index: ShardexIndex);
    pub fn read(&self) -> &ShardexIndex;
}
```

Use Arc and atomic swaps for efficient copy-on-write updates. Consider using epoch-based memory management for cleanup.
# Step 31: Flush Operations and Consistency

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement flush operations that ensure data consistency and durability across all components.

## Tasks
- Create flush operation that syncs all pending changes
- Implement consistency checks across WAL, shards, and index
- Add durability guarantees through proper file synchronization
- Support both automatic and manual flush operations
- Include flush performance monitoring

## Acceptance Criteria
- [ ] Flush ensures all pending operations are durably stored
- [ ] Consistency checks validate state across all components
- [ ] File synchronization provides durability guarantees
- [ ] Manual flush allows immediate consistency when needed
- [ ] Tests verify flush correctness and durability
- [ ] Performance is acceptable for typical flush frequencies

## Technical Details
```rust
impl Shardex {
    pub async fn flush(&mut self) -> Result<(), ShardexError>;
    
    async fn flush_internal(&mut self) -> Result<(), ShardexError> {
        // 1. Process pending WAL batches
        // 2. Flush all shard data to disk
        // 3. Update index metadata
        // 4. Sync all file descriptors
        // 5. Advance WAL pointers
        // 6. Validate consistency
    }
}
```

Include fsync/fdatasync for durability and comprehensive consistency validation.
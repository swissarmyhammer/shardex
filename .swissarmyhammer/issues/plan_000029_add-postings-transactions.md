# Step 29: Add Postings Transaction Handling

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement complete transaction handling for adding postings with WAL integration and error recovery.

## Tasks
- Create add postings transaction workflow
- Implement WAL recording before shard updates
- Add transaction rollback on failures
- Support batch posting additions
- Include transaction validation and integrity checks

## Acceptance Criteria
- [ ] Add operations are recorded in WAL before shard updates
- [ ] Transaction rollback restores consistent state on failures
- [ ] Batch operations improve performance for large additions
- [ ] Validation prevents invalid postings from being added
- [ ] Tests verify transaction correctness and error handling
- [ ] ACID properties are maintained throughout operations

## Technical Details
```rust
impl Shardex {
    pub async fn add_postings(&mut self, postings: Vec<Posting>) -> Result<(), ShardexError>;
    
    async fn add_postings_transaction(&mut self, postings: Vec<Posting>) -> Result<(), ShardexError> {
        // 1. Validate all postings
        // 2. Record in WAL
        // 3. Apply to shards
        // 4. Update bloom filters
        // 5. Update centroids
        // 6. Advance WAL pointer
    }
}
```

Include comprehensive validation, atomic operations, and proper error propagation.
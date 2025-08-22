# Step 20: WAL Replay for Recovery

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement WAL replay functionality for crash recovery and startup.

## Tasks
- Create WAL replay engine that processes transaction records
- Implement idempotent operation replay to handle duplicates
- Add recovery validation and consistency checks
- Support partial transaction recovery
- Include progress tracking and error reporting

## Acceptance Criteria
- [ ] WAL replay accurately reconstructs index state
- [ ] Idempotent operations handle duplicate records correctly
- [ ] Partial transaction recovery maintains consistency
- [ ] Progress tracking provides recovery visibility
- [ ] Tests verify recovery correctness and edge cases
- [ ] Performance is acceptable for large WAL files

## Technical Details
```rust
pub struct WalReplayer {
    wal_directory: PathBuf,
    shardex_index: ShardexIndex,
}

impl WalReplayer {
    pub async fn replay_all_segments(&mut self) -> Result<(), ShardexError>;
    pub async fn replay_segment(&mut self, segment: &WalSegment) -> Result<usize, ShardexError>;
    fn apply_operation(&mut self, op: &WalOperation) -> Result<(), ShardexError>;
}
```

Include duplicate detection using transaction IDs and maintain operation order during replay.
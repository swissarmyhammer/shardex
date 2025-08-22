# Step 34: WAL-Based Crash Recovery

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement comprehensive crash recovery using Write-Ahead Log replay with consistency validation.

## Tasks
- Create crash detection and recovery initialization
- Implement complete WAL replay with progress tracking
- Add consistency validation after recovery
- Support partial recovery from corrupted WAL segments
- Include recovery performance optimization

## Acceptance Criteria
- [ ] Crash detection identifies incomplete operations
- [ ] WAL replay restores all committed transactions
- [ ] Consistency validation ensures recovered state is valid
- [ ] Partial recovery handles corrupted segments gracefully
- [ ] Tests verify recovery correctness for various crash scenarios
- [ ] Performance is acceptable for large WAL files

## Technical Details
```rust
pub struct CrashRecovery {
    index_directory: PathBuf,
    config: ShardexConfig,
    recovery_stats: RecoveryStats,
}

impl CrashRecovery {
    pub async fn detect_crash(&self) -> Result<bool, ShardexError>;
    pub async fn recover(&mut self) -> Result<ShardexIndex, ShardexError>;
    pub async fn validate_consistency(&self, index: &ShardexIndex) -> Result<(), ShardexError>;
}
```

Include comprehensive logging and progress reporting during recovery operations.
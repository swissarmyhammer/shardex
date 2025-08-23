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

## Proposed Solution

Based on my analysis of the existing codebase, I will implement a comprehensive WAL replay system that integrates with the current architecture:

### Architecture Design

1. **WalReplayer struct**: Core replay engine that processes WAL segments in order
   - Integrates with existing `WalManager` for segment discovery
   - Uses `ShardexIndex` for state reconstruction
   - Implements idempotent operations with transaction ID tracking

2. **Recovery Process**:
   - Discover all WAL segments using existing `DirectoryLayout` and `FileDiscovery`
   - Sort segments by ID to ensure correct temporal ordering
   - Process each segment sequentially, replaying all valid transactions
   - Track applied transaction IDs to prevent duplicates during recovery

3. **Idempotency System**:
   - Maintain a hash set of processed transaction IDs in memory during replay
   - Skip transactions that have already been applied
   - Use transaction checksums for additional validation

4. **Integration Points**:
   - Leverage existing `WalTransaction::deserialize()` for transaction parsing
   - Use existing `WalOperation` validation methods
   - Integrate with `ShardexIndex` for applying operations to the index state
   - Use existing error handling patterns with `ShardexError`

### Implementation Steps

1. **Create WalReplayer structure** with necessary dependencies
2. **Implement segment discovery and ordering** using existing file discovery
3. **Add transaction deserialization and validation** using existing methods
4. **Implement idempotent operation application** with duplicate detection
5. **Add progress tracking and error reporting** with detailed recovery info
6. **Comprehensive testing** covering normal replay, corruption handling, and edge cases

### Key Components

```rust
pub struct WalReplayer {
    wal_manager: WalManager,
    shardex_index: ShardexIndex,
    processed_transactions: HashSet<TransactionId>,
    recovery_stats: RecoveryStats,
}

pub struct RecoveryStats {
    segments_processed: usize,
    transactions_replayed: usize,
    transactions_skipped: usize,
    operations_applied: usize,
    errors_encountered: Vec<String>,
}
```

This approach leverages all existing infrastructure while providing robust recovery capabilities.
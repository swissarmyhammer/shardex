# Step 19: Transaction Recording and Batching

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement transaction recording in WAL with efficient batching for performance.

## Tasks
- Create transaction record format for WAL entries
- Implement add/remove operation recording
- Add batching mechanism with timer-based flushing
- Support atomic batch commits with proper ordering
- Include transaction validation and error handling

## Acceptance Criteria
- [ ] Transaction records capture all necessary operation data
- [ ] Batching improves write throughput significantly
- [ ] Timer-based flushing ensures bounded latency
- [ ] Atomic commits maintain transactional integrity
- [ ] Tests verify transaction recording and batching
- [ ] Error handling covers all failure scenarios

## Technical Details
```rust
#[derive(Debug, Clone)]
pub enum WalOperation {
    AddPosting { document_id: DocumentId, start: u32, length: u32, vector: Vec<f32> },
    RemoveDocument { document_id: DocumentId },
}

pub struct WalTransaction {
    id: u64,
    timestamp: SystemTime,
    operations: Vec<WalOperation>,
    checksum: u32,
}
```

Use a configurable batch timer (default 100ms) and include operation ordering guarantees.
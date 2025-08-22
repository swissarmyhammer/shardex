# Step 21: Batch Processing Timer

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement timer-based batch processing that flushes WAL entries to shards at regular intervals.

## Tasks
- Create configurable timer for batch processing intervals
- Implement automatic flush triggering every n milliseconds
- Add manual flush capability for immediate processing
- Support graceful shutdown with pending batch processing
- Include batch size optimization based on workload

## Acceptance Criteria
- [ ] Timer triggers batch processing at configured intervals
- [ ] Manual flush works for immediate consistency needs
- [ ] Graceful shutdown processes all pending batches
- [ ] Batch size adapts to improve throughput
- [ ] Tests verify timer behavior and flush operations
- [ ] Performance is optimized for typical workloads

## Technical Details
```rust
pub struct BatchProcessor {
    batch_interval: Duration,
    pending_operations: Vec<WalOperation>,
    timer_handle: Option<JoinHandle<()>>,
    shutdown_signal: Arc<AtomicBool>,
}

impl BatchProcessor {
    pub async fn start(&mut self);
    pub async fn flush_now(&mut self) -> Result<(), ShardexError>;
    pub async fn shutdown(&mut self) -> Result<(), ShardexError>;
}
```

Use tokio::time for async timer functionality and atomic operations for thread-safe coordination.
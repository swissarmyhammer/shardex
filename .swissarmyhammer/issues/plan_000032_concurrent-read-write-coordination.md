# Step 32: Concurrent Read/Write Coordination

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement coordination between concurrent read and write operations to ensure data consistency without blocking.

## Tasks
- Create reader/writer coordination using copy-on-write semantics
- Implement non-blocking reads during write operations
- Add proper synchronization for index updates
- Support graceful handling of concurrent access patterns
- Include deadlock prevention and detection

## Acceptance Criteria
- [ ] Readers don't block on write operations
- [ ] Writers don't corrupt data visible to concurrent readers
- [ ] Index updates are atomic from reader perspective
- [ ] Synchronization is efficient and deadlock-free
- [ ] Tests verify concurrent access correctness
- [ ] Performance degrades gracefully under high concurrency

## Technical Details
```rust
pub struct ConcurrentShardex {
    index: Arc<RwLock<CowShardexIndex>>,
    write_coordinator: Arc<Mutex<WriteCoordinator>>,
    active_readers: Arc<AtomicUsize>,
}

impl ConcurrentShardex {
    pub async fn read_operation<F, R>(&self, f: F) -> Result<R, ShardexError>
    where F: FnOnce(&ShardexIndex) -> Result<R, ShardexError>;
    
    pub async fn write_operation<F, R>(&self, f: F) -> Result<R, ShardexError>
    where F: FnOnce(&mut ShardexIndex) -> Result<R, ShardexError>;
}
```

Use copy-on-write and epoch-based memory management for safe concurrent access.
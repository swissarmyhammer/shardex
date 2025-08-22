# Step 18: WAL Segment Management

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement Write-Ahead Log segment management with fixed-size memory-mapped segments.

## Tasks
- Create WAL segment structure with fixed-size layout
- Implement segment creation, opening, and rotation
- Add segment pointer management for current write position
- Support segment cleanup after successful flush
- Include segment integrity validation

## Acceptance Criteria
- [ ] WAL segments are fixed-size and memory-mapped
- [ ] Segment rotation handles capacity limits properly
- [ ] Write pointers track current position accurately
- [ ] Cleanup removes obsolete segments safely
- [ ] Tests verify segment operations and edge cases
- [ ] Recovery can handle partial segment writes

## Technical Details
```rust
pub struct WalSegment {
    id: u64,
    memory_map: MemoryMappedFile,
    write_pointer: AtomicUsize,
    capacity: usize,
    file_path: PathBuf,
}

pub struct WalManager {
    current_segment: WalSegment,
    segment_size: usize,
    directory: PathBuf,
    next_segment_id: u64,
}
```

Use atomic operations for thread-safe write pointer updates and include segment checksums for integrity.
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

## Proposed Solution

After analyzing the existing codebase, I've identified that the current `flush()` implementation only flushes the WAL batch processor and applies pending operations to shards. To implement a comprehensive flush operation that ensures data consistency and durability, I propose the following approach:

### Current State Analysis
- **Existing flush**: Only handles WAL batch processor flush and shard operation application
- **Missing components**: File synchronization (fsync/fdatasync), consistency validation, performance monitoring
- **Available sync methods**: Found `sync()` methods in `Shard`, `VectorStorage`, `PostingStorage`, and `WalSegment`

### Implementation Plan

1. **Enhanced Flush Internal Method**: Create `flush_internal()` method that coordinates all flush operations:
   - Process pending WAL batches (existing)
   - Apply operations to shards (existing) 
   - Sync all shard data to disk using existing `shard.sync()` methods
   - Sync WAL segments to disk
   - Update index metadata
   - Validate consistency across components

2. **File Synchronization**: Utilize existing sync infrastructure:
   - `VectorStorage::sync()` - calls `mmap_file.sync()` which uses `mmap.flush()`
   - `PostingStorage::sync()` - similar memory-mapped file sync
   - `WalSegment::sync()` - syncs WAL data to disk
   - `Shard::sync()` - coordinates vector and posting storage sync

3. **Consistency Validation**: Implement checks to verify:
   - WAL pointer consistency with applied operations
   - Shard metadata matches actual storage state
   - Index centroid data reflects current shard contents
   - Bloom filters are synchronized with posting data

4. **Performance Monitoring**: Add metrics for:
   - Flush duration breakdown by component
   - Data volume flushed
   - Consistency check timing
   - Error rates and recovery actions

5. **Manual vs Automatic Flush**: Support both modes:
   - Automatic: Triggered by batch processor timer (existing)
   - Manual: `flush()` method for immediate consistency (enhance existing)

### Technical Implementation

```rust
impl ShardexImpl {
    async fn flush_internal(&mut self) -> Result<FlushStats, ShardexError> {
        let start_time = std::time::Instant::now();
        let mut stats = FlushStats::new();
        
        // 1. Process pending WAL batches (existing logic)
        if let Some(ref mut processor) = self.batch_processor {
            processor.flush_now().await?;
            stats.wal_flush_duration = start_time.elapsed();
        }
        
        // 2. Apply pending operations to shards (existing logic)
        let apply_start = std::time::Instant::now();
        self.apply_pending_operations_to_shards().await?;
        stats.shard_apply_duration = apply_start.elapsed();
        
        // 3. Sync all shard data to disk (NEW)
        let sync_start = std::time::Instant::now();
        let shard_ids: Vec<_> = self.index.shard_ids().collect();
        for shard_id in shard_ids {
            let shard = self.index.get_shard_mut(shard_id)?;
            shard.sync()?;
        }
        stats.shard_sync_duration = sync_start.elapsed();
        
        // 4. Sync WAL segments (NEW)
        // Access WAL manager through layout and sync current segment
        
        // 5. Validate consistency (NEW)
        let validation_start = std::time::Instant::now();
        self.validate_consistency().await?;
        stats.validation_duration = validation_start.elapsed();
        
        stats.total_duration = start_time.elapsed();
        Ok(stats)
    }
    
    async fn validate_consistency(&self) -> Result<(), ShardexError> {
        // Validate shard metadata matches storage state
        // Check bloom filters are synchronized
        // Verify centroid calculations are current
        // Validate WAL pointers
    }
}

#[derive(Debug, Clone)]
pub struct FlushStats {
    pub wal_flush_duration: Duration,
    pub shard_apply_duration: Duration, 
    pub shard_sync_duration: Duration,
    pub validation_duration: Duration,
    pub total_duration: Duration,
    pub shards_synced: usize,
    pub operations_applied: usize,
}
```

This approach leverages the existing sync infrastructure while adding the missing durability guarantees and consistency validation that the issue requires.
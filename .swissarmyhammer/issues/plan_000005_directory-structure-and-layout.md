# Step 5: Directory Structure and File Layout

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement the file system layout and directory management for Shardex indexes.

## Tasks
- Create directory structure management utilities
- Implement file naming conventions for shards and segments
- Add index metadata file handling
- Create utilities for file discovery and validation
- Implement cleanup procedures for incomplete operations

## Acceptance Criteria
- [ ] Directory structure matches specification layout
- [ ] File naming follows ULID-based conventions
- [ ] Metadata file tracks index configuration and state
- [ ] File discovery handles missing or corrupted files
- [ ] Cleanup removes temporary and orphaned files
- [ ] Tests verify directory operations and error cases

## Technical Details
Directory structure per specification:
```
shardex_index/
├── shardex.meta
├── centroids/
│   ├── segment_000001.shx
│   └── segment_000002.shx
├── shards/
│   ├── {shard_ulid}.vectors
│   └── {shard_ulid}.postings
└── wal/
    ├── wal_000001.log
    └── wal_000002.log
```

Include atomic file operations and proper error recovery.
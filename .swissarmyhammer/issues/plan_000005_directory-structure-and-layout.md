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

## Proposed Solution

Based on the analysis of the existing codebase and the specification, I will implement a comprehensive directory structure and file layout management system for Shardex indexes. The solution will include:

### 1. Directory Structure Management
- Create `DirectoryLayout` struct to manage the complete file layout
- Implement directory creation with proper error handling
- Support both creating new indexes and opening existing ones

### 2. File Naming Conventions
- Use ULID-based naming for shards (`{shard_ulid}.vectors`, `{shard_ulid}.postings`)
- Sequential naming for segments (`segment_000001.shx`, `wal_000001.log`)
- Metadata file with standardized name (`shardex.meta`)

### 3. Index Metadata Management
- Create `IndexMetadata` struct containing configuration and state
- Implement serialization/deserialization using serde
- Include versioning for future compatibility
- Add integrity validation with checksums

### 4. File Discovery and Validation
- Implement file discovery methods for each component type
- Add validation for file existence, format, and integrity
- Handle partial or corrupted files gracefully
- Provide detailed error reporting

### 5. Cleanup Procedures
- Implement atomic operations for file creation and deletion
- Create temporary file handling with automatic cleanup
- Add orphaned file detection and removal
- Support transaction-like operations for file management

### 6. Implementation Plan
1. Create `layout` module with directory structure management
2. Implement `IndexMetadata` with proper serialization
3. Add file discovery and validation utilities
4. Create cleanup and recovery procedures
5. Write comprehensive tests with TDD approach

This solution will provide a solid foundation for the file system layer of Shardex, ensuring data integrity, proper error handling, and maintainable code structure.

## Implementation Progress Report

### ✅ Completed Implementation

I have successfully implemented the complete directory structure and file layout management system for Shardex indexes. The implementation includes:

#### 1. Directory Structure Management ✅
- Created `DirectoryLayout` struct that manages the complete file layout according to specification
- Implemented directory creation with proper error handling 
- Added path resolution methods for all file types (shards, segments, WAL, metadata)
- Supports both creating new indexes and opening existing ones

#### 2. File Naming Conventions ✅
- Implemented ULID-based naming for shards: `{shard_ulid}.vectors`, `{shard_ulid}.postings`
- Sequential naming for segments: `segment_000001.shx`, `wal_000001.log`
- Standardized metadata file name: `shardex.meta`
- All naming follows the specification exactly

#### 3. Index Metadata Management ✅
- Created `IndexMetadata` struct containing configuration and state information
- Implemented TOML serialization/deserialization for human-readable metadata files
- Added versioning support for future compatibility (`LAYOUT_VERSION = 1`)
- Includes comprehensive state tracking (active, needs_recovery, clean_shutdown flags)
- Atomic file operations for metadata persistence

#### 4. File Discovery and Validation ✅
- Implemented `FileDiscovery` utility for finding and cataloging index files
- Added validation for file existence, format, and integrity
- Handles partial or corrupted file scenarios gracefully
- Provides detailed error reporting with specific file paths and issues
- Detects orphaned files that don't match expected patterns

#### 5. Cleanup Procedures ✅
- Created `CleanupManager` for atomic operations and cleanup
- Implemented temporary file tracking with automatic cleanup
- Added orphaned file detection and removal capabilities
- Support for transaction-like operations for file management
- Automatic cleanup on `Drop` for safety

#### 6. Integration and Testing ✅
- Added `layout` module to the main library exports
- Updated configuration to include required serde traits
- Added `toml` dependency for metadata serialization
- Comprehensive test suite with 14 tests covering all functionality
- All 96 total tests pass, including integration with existing modules

### Technical Highlights

#### Directory Structure (per specification):
```
shardex_index/
├── shardex.meta          # Index metadata and configuration
├── centroids/            # Shardex segments (centroids + metadata + bloom filters)
│   ├── segment_000001.shx
│   └── segment_000002.shx
├── shards/              # Individual shard data
│   ├── {shard_ulid}.vectors
│   └── {shard_ulid}.postings
└── wal/                 # Write-ahead log segments
    ├── wal_000001.log
    └── wal_000002.log
```

#### Key Features:
- **Type Safety**: Strong typing with ShardId and DocumentId prevents mixing identifiers
- **Error Handling**: Comprehensive error types with detailed context
- **Memory Mapping Ready**: Designed to work seamlessly with the existing memory mapping infrastructure
- **Atomic Operations**: All critical operations are atomic (metadata saves, file creation)
- **Recovery Support**: Built-in support for crash recovery and consistency checking
- **Cross-Platform**: Uses standard library path operations for Windows/Linux/macOS compatibility

#### Test Coverage:
- Directory creation and validation
- File path generation and naming conventions
- Metadata serialization/deserialization and compatibility checking
- File discovery with various scenarios (empty, populated, corrupted)
- Orphaned file detection and cleanup
- Atomic operations and cleanup procedures
- Integration with existing type system

### Code Quality
- ✅ All 96 tests pass
- ✅ Formatted with `cargo fmt`
- ✅ Clippy warnings resolved (except intentional unused field for future extensibility)
- ✅ Comprehensive documentation with examples
- ✅ Follows existing codebase patterns and conventions

The implementation provides a solid, production-ready foundation for the file system layer of Shardex, ensuring data integrity, proper error handling, and maintainable code structure. This completes Step 5 of the implementation plan.
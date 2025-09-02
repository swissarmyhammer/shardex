# Create Basic Operation Parameter Structures

## Goal
Define parameter structures for the most fundamental Shardex operations based on the `basic_usage.rs` example.

Refer to /Users/wballard/github/shardex/ideas/api.md

## Context
The ApiThing pattern requires parameter objects for each operation. We'll start with the core operations from `basic_usage.rs` example: create index, add postings, search, flush, and get stats.

## Tasks

### 1. Create Parameter Structures
Define parameter structs in `src/api/parameters.rs`:

```rust
// Index creation parameters
#[derive(Debug, Clone)]
pub struct CreateIndexParams {
    pub directory_path: PathBuf,
    pub vector_size: usize,
    pub shard_size: usize,
    pub batch_write_interval_ms: u64,
    pub wal_segment_size: usize,
    pub bloom_filter_size: usize,
    pub default_slop_factor: u32,
}

// Add postings parameters  
#[derive(Debug, Clone)]
pub struct AddPostingsParams {
    pub postings: Vec<Posting>,
}

// Search parameters
#[derive(Debug, Clone)]
pub struct SearchParams {
    pub query_vector: Vec<f32>,
    pub k: usize,
    pub slop_factor: Option<u32>,
}

// Flush parameters (may be empty for now)
#[derive(Debug, Clone, Default)]
pub struct FlushParams {
    pub with_stats: bool,
}

// Stats parameters (may be empty for now)
#[derive(Debug, Clone, Default)]
pub struct GetStatsParams {}
```

### 2. Add Parameter Validation
- Implement validation methods for each parameter struct
- Add `validate()` methods that return `Result<(), ShardexError>`
- Validate vector dimensions, directory paths, etc.

### 3. Add Builder Patterns
- Implement `From<ShardexConfig>` for `CreateIndexParams`
- Add convenience constructors where appropriate
- Enable easy migration from existing config patterns

### 4. Integration and Documentation
- Add parameters module to API exports
- Document each parameter struct with usage examples
- Add docstring examples showing typical usage

## Success Criteria
- ✅ All basic parameter structures defined with proper types
- ✅ Parameter validation methods implemented
- ✅ Builder patterns and conversions available
- ✅ Comprehensive documentation with examples
- ✅ Integration into API module structure

## Implementation Notes
- Focus on the 5 core operations from `basic_usage.rs` example
- Use existing Shardex types where possible (Posting, etc.)
- Don't implement the actual operations yet - just parameters
- Ensure parameters are compatible with existing Shardex API

## Files to Create/Modify
- `src/api/parameters.rs` (new file)
- `src/api/mod.rs` (update exports)

## Estimated Lines Changed
~150-200 lines

## Proposed Solution

Based on examining the `basic_usage.rs` example and existing Shardex types, I will implement parameter structures for the 5 core operations:

1. **CreateIndexParams** - For `ShardexImpl::create(config)`
2. **AddPostingsParams** - For `index.add_postings(postings)`  
3. **SearchParams** - For `index.search(&query_vector, k, slop_factor)`
4. **FlushParams** - For `index.flush_with_stats()`
5. **GetStatsParams** - For `index.stats()`

### Implementation Steps:

1. Create `src/api/parameters.rs` with parameter structs derived from ShardexConfig fields
2. Implement validation methods using existing ShardexError patterns
3. Add `From<ShardexConfig>` conversion for CreateIndexParams 
4. Provide builder patterns for ease of use
5. Add comprehensive documentation with usage examples
6. Update exports in `src/api/mod.rs`
7. Write comprehensive tests

The parameters will leverage existing types like `Posting`, `DocumentId`, and use the established error handling patterns with `ShardexError::config_error()`.
## Implementation Complete

I have successfully implemented all the basic operation parameter structures for the Shardex ApiThing pattern conversion:

### Files Created/Modified:
- ✅ **Created** `src/api/parameters.rs` (21,878 bytes) - Complete parameter structures with validation
- ✅ **Modified** `src/api/mod.rs` - Added exports for all parameter types

### Parameter Structures Implemented:

1. **`CreateIndexParams`** - Index creation parameters with builder pattern
   - All ShardexConfig fields mapped appropriately  
   - Comprehensive validation with helpful error messages
   - `From<ShardexConfig>` conversion implemented
   - Builder pattern with sensible defaults

2. **`AddPostingsParams`** - Batch posting insertion parameters
   - Validates non-empty posting collections
   - Checks vector dimension consistency across all postings
   - Validates finite vector values (no NaN/infinity)

3. **`SearchParams`** - Similarity search parameters with builder
   - Query vector validation (non-empty, finite values)
   - k parameter bounds checking (1-10,000)
   - Optional slop factor with validation
   - Builder pattern for ease of use

4. **`FlushParams`** - Flush operation parameters
   - Simple boolean flag for including stats
   - Convenience constructors

5. **`GetStatsParams`** - Statistics retrieval parameters  
   - Empty struct for consistency and future extensibility

### Key Features Implemented:
- ✅ **Comprehensive Validation**: All parameters validate inputs with descriptive error messages
- ✅ **Builder Patterns**: CreateIndexParams and SearchParams support fluent builder APIs
- ✅ **Error Integration**: Uses existing ShardexError::config_error() pattern
- ✅ **Documentation**: Extensive rustdoc with usage examples for each struct
- ✅ **Test Coverage**: 12 comprehensive unit tests covering validation and builders
- ✅ **Type Safety**: Leverages existing Shardex types (Posting, DocumentId, etc.)

### Testing Results:
- ✅ All 12 unit tests pass
- ✅ Full codebase builds successfully with no errors
- ✅ Code formatted with `cargo fmt`

The implementation follows all Shardex coding standards and provides a solid foundation for the ApiThing pattern conversion. Each parameter struct includes thorough validation and helpful error messages that guide users to correct usage patterns.

## Code Review Fixes Completed

### Fixed Issues
1. ✅ **Fixed clippy warning in `src/api/parameters.rs:579`**
   - Changed `Self::default()` to `Self` in `GetStatsParams::new()`
   - Clippy warning resolved for unnecessary use of `default()` on unit struct

2. ✅ **Addressed dead code warnings in `src/api/context.rs`**
   - Added `#[allow(dead_code)]` annotations with TODO comments for infrastructure methods:
     - `set_index()` - Will be used by CreateIndexOperation and OpenIndexOperation
     - `get_index()` - Will be used by SearchOperation and GetStatsOperation  
     - `get_index_mut()` - Will be used by AddPostingsOperation and FlushOperation
     - `set_stats()` - Will be used by FlushOperation and GetStatsOperation
     - `set_monitor()` - Will be used by monitoring-enabled operations
     - `clear_index()` - Will be used by CloseIndexOperation and error handling

### Verification Results
- ✅ All 788 tests pass
- ✅ Code compiles successfully with no errors
- ✅ Clippy passes with no warnings or errors
- ✅ All parameter structures working as designed

### Implementation Status
The basic operation parameter structures are complete and ready for the next phase of implementing actual operations. All lint issues have been resolved while preserving the infrastructure needed for future operation implementations.
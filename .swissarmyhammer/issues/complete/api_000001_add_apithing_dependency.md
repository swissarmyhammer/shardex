# Add ApiThing Dependency and Initial Setup

## Goal
Add the `apithing` crate dependency and set up the foundational imports needed for the API conversion.

Refer to /Users/wballard/github/shardex/ideas/api.md

## Context
This is the first step in converting Shardex to use the ApiThing pattern. We need to add the dependency and ensure the crate builds with the new dependency.

## Tasks

### 1. Update Cargo.toml
Add the apithing dependency:
```toml
[dependencies]
apithing = { git = "https://github.com/swissarmyhammer/apithing" }
```

### 2. Verify Build
- Run `cargo check` to ensure the dependency resolves correctly
- Fix any initial compilation issues
- Ensure all existing examples still build

### 3. Create Initial Module Structure  
- Create `src/api/mod.rs` for the new API module
- Add basic imports for `ApiOperation` trait
- Add module declaration to `src/lib.rs`

## Success Criteria
- ✅ Cargo.toml includes apithing dependency
- ✅ Project builds with `cargo check`
- ✅ All existing examples still compile
- ✅ New api module created with proper structure

## Implementation Notes
- Keep changes minimal - only add dependency and basic structure
- Don't modify existing functionality yet
- Focus on ensuring the build system works with the new dependency

## Files to Modify
- `Cargo.toml`
- `src/lib.rs` 
- `src/api/mod.rs` (new file)

## Estimated Lines Changed
< 20 lines

## Proposed Solution

Based on my analysis of the current API, I see the main public interfaces are:

- `ShardexImpl::create(config)` - Creates a new index  
- `ShardexImpl::open(path)` - Opens existing index
- `index.add_postings(postings)` - Adds vector postings
- `index.search(vector, k, slop)` - Performs similarity search
- `index.flush()` - Flushes changes to disk
- `index.stats()` - Gets index statistics

The apithing pattern will establish:
1. A single `ShardexContext` that holds the index instance
2. Parameter structs for each operation (CreateParams, OpenParams, AddPostingsParams, SearchParams, etc.)
3. Implementation of the `ApiOperation` trait for each operation

### Implementation Steps:

1. **Add apithing dependency** to Cargo.toml
2. **Create src/api/mod.rs** with basic ApiOperation imports  
3. **Add api module** to src/lib.rs
4. **Verify build** with cargo check
5. **Test examples** still work

This keeps changes minimal while establishing the foundation for the ApiThing conversion. The actual API operations will be implemented in subsequent issues.

## Implementation Notes

✅ **Successfully completed all tasks**:

1. **Added apithing dependency** - Added `apithing = { git = "https://github.com/swissarmyhammer/apithing" }` to Cargo.toml
2. **Created api module** - Added `src/api/mod.rs` with basic ApiOperation re-export  
3. **Updated lib.rs** - Added `pub mod api;` to expose the new module
4. **Verified build** - `cargo check` passes successfully
5. **Tested examples** - All existing examples (`cargo check --examples`) build correctly

### Key Findings from Current API Analysis:

The current Shardex API centers around:
- `ShardexImpl::create(config)` / `ShardexImpl::open(path)` - Index lifecycle 
- `index.add_postings(postings)` - Batch vector insertion
- `index.search(vector, k, slop)` - Similarity search with optional slop factor
- `index.flush()` / `index.flush_with_stats()` - Persistence operations
- `index.stats()` - Index statistics and monitoring

### Next Steps for API Conversion:

The foundation is now in place. Future issues should focus on:
1. Creating ShardexContext struct
2. Converting each operation to ApiOperation trait implementations
3. Creating parameter structs for each operation
4. Updating examples one by one

### Files Modified:
- `Cargo.toml` - Added apithing dependency 
- `src/api/mod.rs` - New API module (7 lines)
- `src/lib.rs` - Added api module declaration (1 line)

Total: **8 lines changed** (well under the 20 line estimate)
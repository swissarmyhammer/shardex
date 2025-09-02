# Convert basic_usage.rs Example to ApiThing Pattern

## Goal
Convert the `examples/basic_usage.rs` file to use the new ApiThing-based API instead of direct ShardexImpl calls.

Refer to /Users/wballard/github/shardex/ideas/api.md

## Context
This is the first concrete test of our new API. The basic_usage example should work identically but use the new operation-based pattern.

## Tasks

### 1. Update Example Imports
Change imports from:
```rust
use shardex::{DocumentId, Posting, Shardex, ShardexConfig, ShardexImpl};
```

To:
```rust
use shardex::{
    DocumentId, Posting, 
    api::{
        ShardexContext,
        CreateIndex, AddPostings, Search, Flush, GetStats,
        CreateIndexParams, AddPostingsParams, SearchParams, 
        FlushParams, GetStatsParams
    }
};
use apithing::ApiOperation;
```

### 2. Convert Index Creation
Change from:
```rust
let config = ShardexConfig::new()
    .directory_path(&temp_dir)
    .vector_size(128)
    .shard_size(10000)
    .batch_write_interval_ms(100);

let mut index = ShardexImpl::create(config).await?;
```

To:
```rust
let mut context = ShardexContext::new();
let create_params = CreateIndexParams {
    directory_path: temp_dir.clone(),
    vector_size: 128,
    shard_size: 10000,
    batch_write_interval_ms: 100,
    // ... other defaults
};

CreateIndex::execute(&mut context, &create_params).await?;
```

### 3. Convert All Operations
- Replace `index.add_postings(postings).await?` with `AddPostings::execute(&mut context, &AddPostingsParams { postings }).await?`
- Replace `index.flush_with_stats().await?` with `Flush::execute(&mut context, &FlushParams { with_stats: true }).await?`
- Replace `index.stats().await?` with `GetStats::execute(&mut context, &GetStatsParams {}).await?`
- Replace `index.search(&query_vector, 3, None).await?` with `Search::execute(&mut context, &SearchParams { query_vector, k: 3, slop_factor: None }).await?`

### 4. Update Documentation and Comments
- Update example documentation to reflect new pattern
- Add comments explaining the ApiThing approach
- Ensure all code comments are still accurate

### 5. Test and Validate
- Ensure the example compiles without errors
- Run the example to verify identical behavior
- Check output formatting and structure

## Success Criteria
- ✅ Example compiles successfully with new API
- ✅ Example runs and produces identical output to original
- ✅ All operations use ApiThing pattern correctly  
- ✅ Documentation reflects new API approach
- ✅ Code is clean and well-commented

## Implementation Notes
- Preserve exact same functionality and output
- Use the new pattern consistently throughout
- Ensure error handling works identically
- Test that temporary directory cleanup still works

## Files to Modify
- `examples/basic_usage.rs`

## Estimated Lines Changed
~50-80 lines (mostly pattern changes, not new functionality)

## Proposed Solution

Based on my analysis of the existing API structure, here's my implementation plan:

### 1. Update Imports
Change from:
```rust
use shardex::{DocumentId, Posting, Shardex, ShardexConfig, ShardexImpl};
```
To:
```rust
use shardex::{DocumentId, Posting};
use shardex::api::{
    ShardexContext,
    CreateIndex, AddPostings, Search, Flush, GetStats,
    CreateIndexParams, AddPostingsParams, SearchParams, 
    FlushParams, GetStatsParams
};
use apithing::ApiOperation;
```

### 2. Convert Index Creation
Replace the ShardexConfig/ShardexImpl::create pattern with CreateIndex operation using builder pattern:
```rust
let mut context = ShardexContext::new();
let create_params = CreateIndexParams::builder()
    .directory_path(temp_dir.clone())
    .vector_size(128)
    .shard_size(10000)
    .batch_write_interval_ms(100)
    .build()?;

CreateIndex::execute(&mut context, &create_params)?;
```

### 3. Convert Operations Pattern
Each operation follows the same pattern: `Operation::execute(&mut context, &params)?`
- AddPostings: `AddPostings::execute(&mut context, &AddPostingsParams::new(postings)?)?`
- Search: `Search::execute(&mut context, &SearchParams::builder().query_vector(vec).k(k).slop_factor(slop).build()?)?`
- Flush: `Flush::execute(&mut context, &FlushParams::with_stats())?`
- GetStats: `GetStats::execute(&mut context, &GetStatsParams::new())?`

### 4. Key Changes
- Single `ShardexContext` manages all state
- Parameters use builder patterns with validation
- Error handling remains identical
- Output types and behavior remain the same
- All operations are now explicit function calls rather than method calls

This approach maintains the exact same functionality while demonstrating the new ApiThing pattern with proper parameter validation and builder patterns.

## Implementation Challenge Discovered

During conversion, I discovered a runtime architecture issue:

### Problem
The ApiThing operations in `src/api/operations.rs` create new Tokio runtimes using:
```rust
let rt = tokio::runtime::Runtime::new().map_err(...)?;
rt.block_on(async_operation)?;
```

This approach fails when there's already an active runtime context, causing the error:
> Cannot start a runtime from within a runtime

### Root Cause
The operations are designed to be synchronous entry points that manage their own async runtime, but this conflicts with examples that are already in an async context or have an existing runtime.

### Proposed Solutions
1. **Runtime Detection**: Modify operations to detect existing runtime and use `tokio::runtime::Handle::current()` when available
2. **Async Operations**: Create async versions of the operations that don't create new runtimes
3. **Context-aware Execution**: Make operations adapt to the calling context

### Current Status
The conversion is functionally complete but cannot run due to this runtime conflict. This issue affects the entire ApiThing pattern implementation and needs to be resolved for the API to be usable in async contexts.
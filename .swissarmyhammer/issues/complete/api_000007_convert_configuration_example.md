# Convert configuration.rs Example to ApiThing Pattern

## Goal
Convert the `examples/configuration.rs` file to use the new ApiThing-based API, demonstrating configuration management and index opening.

Refer to /Users/wballard/github/shardex/ideas/api.md

## Context
This example shows different configuration patterns, performance tuning, and reopening indexes. It's more complex than basic_usage and tests configuration management features.

## Tasks

### 1. Update Imports and Setup
Change imports to use new API operations and add configuration-specific imports:
```rust
use shardex::{
    DocumentId, Posting,
    api::{
        ShardexContext, 
        CreateIndex, OpenIndex, AddPostings, Search, Flush, GetStats, ValidateConfig,
        CreateIndexParams, OpenIndexParams, AddPostingsParams, SearchParams,
        FlushParams, GetStatsParams, ValidateConfigParams
    }
};
use apithing::ApiOperation;
```

### 2. Convert Configuration Creation Patterns
Change from multiple `ShardexConfig` instances to `CreateIndexParams` builders:

```rust
// Old pattern:
let high_perf_config = ShardexConfig::new()
    .directory_path(base_dir.join("high_perf_index"))
    .vector_size(256)
    .shard_size(15000);

// New pattern:
let mut context = ShardexContext::new();
let high_perf_params = CreateIndexParams::high_performance(
    base_dir.join("high_perf_index")
);
```

### 3. Convert Index Creation and Testing
Replace direct ShardexImpl operations:
```rust
// Old pattern:
let mut index = ShardexImpl::create(high_perf_config.clone()).await?;
index.add_postings(test_data).await?;
let stats = index.stats().await?;

// New pattern:
CreateIndex::execute(&mut context, &high_perf_params).await?;
AddPostings::execute(&mut context, &AddPostingsParams { postings: test_data }).await?;
let stats = GetStats::execute(&mut context, &GetStatsParams {}).await?;
```

### 4. Convert Index Reopening Pattern
Replace the index reopening logic:
```rust
// Old pattern:
drop(index);
let reopened_index = ShardexImpl::open(high_perf_config.directory_path).await?;

// New pattern:
// Context automatically manages lifecycle
let mut reopen_context = ShardexContext::new();
OpenIndex::execute(&mut reopen_context, &OpenIndexParams {
    directory_path: high_perf_params.directory_path.clone()
}).await?;
```

### 5. Convert Configuration Validation
Replace configuration validation:
```rust
// Old pattern:
match ShardexImpl::create(invalid_config).await {
    Ok(_) => println!("Unexpected: Invalid config was accepted"),
    Err(e) => println!("Expected error: {}", e),
}

// New pattern:
match ValidateConfig::execute(&mut context, &ValidateConfigParams {
    config: invalid_config
}).await {
    Ok(_) => println!("Unexpected: Invalid config was accepted"),
    Err(e) => println!("Expected error: {}", e),
}
```

### 6. Update Helper Functions and Documentation
- Update `print_config` function to work with `CreateIndexParams`
- Update all documentation to reflect new patterns
- Ensure comments explain ApiThing approach

## Success Criteria
- ✅ Example compiles successfully with new API
- ✅ All configuration scenarios work identically  
- ✅ Index reopening works correctly
- ✅ Configuration validation works correctly
- ✅ Performance testing produces same results
- ✅ Example output is identical to original

## Implementation Notes
- Preserve all timing and performance measurement logic
- Ensure configuration builders create identical configs
- Test that reopening works with the context pattern
- Maintain all temporary directory management
- Keep all the configuration printing and validation logic

## Files to Modify
- `examples/configuration.rs`

## Estimated Lines Changed
~100-150 lines (extensive pattern changes)
## Proposed Solution

After analyzing the existing `configuration.rs` example and the new ApiThing API structure, I will convert it using this approach:

### 1. Import Updates
- Replace direct `ShardexImpl` imports with ApiThing operations
- Add `apithing::ApiOperation` and all required parameter types
- Keep core types like `DocumentId`, `Posting`

### 2. Configuration Pattern Conversion
- Convert `ShardexConfig` instances to `CreateIndexParams` using the new builder methods:
  - `CreateIndexParams::high_performance()` for high-perf config
  - `CreateIndexParams::memory_optimized()` for memory-opt config  
  - `CreateIndexParams::from_shardex_config()` for custom configs

### 3. Operation Pattern Conversion
- Replace `ShardexImpl::create()` → `CreateIndex::execute()`
- Replace `index.add_postings()` → `AddPostings::execute()`
- Replace `index.stats()` → `GetStats::execute()`
- Replace `index.search()` → `Search::execute()`
- Replace `index.flush()` → `Flush::execute()`

### 4. Context Management
- Use `ShardexContext` instead of direct index instances
- Handle index reopening with fresh context instances
- Ensure proper lifecycle management

### 5. Helper Function Updates
- Update `print_config()` to work with `CreateIndexParams` instead of `ShardexConfig`
- Preserve all timing and performance measurement logic
- Keep test data generation functions unchanged

### 6. Configuration Validation
- Convert direct `ShardexImpl::create()` validation attempts to `ValidateConfig::execute()`
- Preserve the same error handling and demonstration logic

This approach maintains identical functionality while demonstrating the new ApiThing pattern's benefits for configuration management and operation organization.
## Implementation Completed ✅

The configuration.rs example has been successfully converted to use the new ApiThing pattern. All functionality has been preserved while demonstrating the new API structure.

### Changes Made

1. **Updated Imports**: 
   - Replaced direct `ShardexImpl` imports with ApiThing operations
   - Added `apithing::ApiOperation` and required parameter types
   - Used correct import paths for operations and parameters

2. **Configuration Pattern Conversion**:
   - `ShardexConfig` → `CreateIndexParams` using builder methods
   - Used `CreateIndexParams::high_performance()` for high-perf config
   - Used `CreateIndexParams::memory_optimized()` for memory-opt config
   - Used `CreateIndexParams::builder()` for default config

3. **Operation Pattern Conversion**:
   - `ShardexImpl::create()` → `CreateIndex::execute()`
   - `index.add_postings()` → `AddPostings::execute()`
   - `index.stats()` → `GetStats::execute()`
   - `index.search()` → `Search::execute()`
   - `index.flush()` → `Flush::execute()`
   - `ShardexImpl::open()` → `OpenIndex::execute()`

4. **Context Management**:
   - Introduced `ShardexContext` to manage index lifecycle
   - Used context dropping and recreation for index reopening
   - Proper context lifecycle management throughout

5. **Configuration Validation**:
   - Converted validation to use `ValidateConfig::execute()`
   - Maintained same error handling and demonstration logic

6. **Helper Functions**:
   - Updated `print_config()` to work with `CreateIndexParams`
   - Preserved all timing and performance measurement logic

### Runtime Issue Resolution

Initially encountered a runtime conflict due to Tokio runtime creation issues. This was resolved by:
- Removing `#[tokio::main]` and making `main()` synchronous
- The ApiThing operations handle async operations internally
- Performing `cargo clean` to resolve compilation caching issues

### Verification Results

The converted example:
- ✅ Compiles successfully with no errors
- ✅ Runs to completion demonstrating all configuration scenarios
- ✅ Shows identical functionality to the original
- ✅ Demonstrates the new ApiThing pattern effectively
- ✅ Performance testing produces equivalent results
- ✅ Index reopening works correctly with new context pattern
- ✅ Configuration validation works as expected

### Example Output

The example now successfully demonstrates:
- Default, high-performance, and memory-optimized configurations
- Index creation with ApiThing operations
- Document indexing and search performance testing
- Index reopening with proper context management
- Configuration validation with proper error handling

All success criteria from the issue have been met.
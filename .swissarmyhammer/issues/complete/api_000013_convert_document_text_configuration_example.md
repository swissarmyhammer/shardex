# Convert document_text_configuration.rs Example to ApiThing Pattern

## Goal
Convert the `examples/document_text_configuration.rs` file to use the new ApiThing-based API for document text configuration management.

Refer to /Users/wballard/github/shardex/ideas/api.md

## Context
This example demonstrates various text storage configuration options, memory management settings, and text storage performance tuning.

## Tasks

### 1. Update Imports for Configuration Operations
Add configuration-specific operations:
```rust
use shardex::{
    DocumentId, Posting,
    api::{
        ShardexContext,
        CreateIndex, StoreDocumentText, GetDocumentText, 
        ValidateConfig, GetStats, GetPerformanceStats,
        CreateIndexParams, StoreDocumentTextParams, GetDocumentTextParams,
        ValidateConfigParams, GetStatsParams, GetPerformanceStatsParams
    }
};
use apithing::ApiOperation;
```

### 2. Convert Text Configuration Patterns
Replace different text configuration setups:
```rust
// Old pattern:
let memory_optimized_config = ShardexConfig::new()
    .directory_path(base_dir.join("memory_opt"))
    .vector_size(128)
    .max_document_text_size(64 * 1024) // 64KB per document
    .text_compression_enabled(true)
    .text_cache_size(16 * 1024 * 1024); // 16MB cache

let high_capacity_config = ShardexConfig::new()
    .directory_path(base_dir.join("high_capacity"))
    .vector_size(512)
    .max_document_text_size(4 * 1024 * 1024) // 4MB per document
    .text_compression_enabled(false)
    .text_cache_size(256 * 1024 * 1024); // 256MB cache

// New pattern:
let memory_opt_params = CreateIndexParams {
    directory_path: base_dir.join("memory_opt"),
    vector_size: 128,
    max_document_text_size: Some(64 * 1024),
    text_compression_enabled: Some(true),
    text_cache_size: Some(16 * 1024 * 1024),
    // ... other defaults
};

let high_capacity_params = CreateIndexParams {
    directory_path: base_dir.join("high_capacity"),
    vector_size: 512,
    max_document_text_size: Some(4 * 1024 * 1024),
    text_compression_enabled: Some(false),
    text_cache_size: Some(256 * 1024 * 1024),
    // ... other defaults
};
```

### 3. Add Text Configuration Builders
Enhance `CreateIndexParams` with text-specific builders:
```rust
impl CreateIndexParams {
    pub fn memory_optimized_text(directory: PathBuf) -> Self {
        Self {
            directory_path: directory,
            vector_size: 128,
            max_document_text_size: Some(64 * 1024),
            text_compression_enabled: Some(true),
            text_cache_size: Some(16 * 1024 * 1024),
            // ... other memory-optimized defaults
        }
    }
    
    pub fn high_capacity_text(directory: PathBuf) -> Self {
        Self {
            directory_path: directory,
            vector_size: 512,
            max_document_text_size: Some(4 * 1024 * 1024),
            text_compression_enabled: Some(false),
            text_cache_size: Some(256 * 1024 * 1024),
            // ... other high-capacity defaults
        }
    }
}
```

### 4. Convert Configuration Testing
Replace configuration testing patterns:
```rust
// Old pattern:
let mut memory_index = ShardexImpl::create(memory_optimized_config.clone()).await?;
let mut capacity_index = ShardexImpl::create(high_capacity_config.clone()).await?;

// Test with different text sizes
let small_text = "Short document text";
let large_text = "Very large document text...".repeat(1000);

memory_index.store_document_text(DocumentId::from_raw(1), small_text).await?;
capacity_index.store_document_text(DocumentId::from_raw(1), &large_text).await?;

// New pattern:
let mut memory_context = ShardexContext::new();
let mut capacity_context = ShardexContext::new();

CreateIndex::execute(&mut memory_context, &memory_opt_params).await?;
CreateIndex::execute(&mut capacity_context, &high_capacity_params).await?;

// Test with different text sizes
let small_text = "Short document text";
let large_text = "Very large document text...".repeat(1000);

StoreDocumentText::execute(&mut memory_context, &StoreDocumentTextParams {
    document_id: DocumentId::from_raw(1),
    text: small_text.to_string(),
    postings: Vec::new(),
}).await?;

StoreDocumentText::execute(&mut capacity_context, &StoreDocumentTextParams {
    document_id: DocumentId::from_raw(1),
    text: large_text.to_string(),
    postings: Vec::new(),
}).await?;
```

### 5. Convert Configuration Validation
Replace configuration validation patterns:
```rust
// Old pattern:
let invalid_config = ShardexConfig::new()
    .directory_path(base_dir.join("invalid"))
    .max_document_text_size(0); // Invalid!

match ShardexImpl::create(invalid_config).await {
    Ok(_) => println!("Unexpected: Invalid text config accepted"),
    Err(e) => println!("Expected error: {}", e),
}

// New pattern:
let invalid_params = CreateIndexParams {
    directory_path: base_dir.join("invalid"),
    max_document_text_size: Some(0), // Invalid!
    // ... other defaults
};

match CreateIndex::execute(&mut context, &invalid_params).await {
    Ok(_) => println!("Unexpected: Invalid text config accepted"),
    Err(e) => println!("Expected error: {}", e),
}

// Or use validation operation:
match ValidateConfig::execute(&mut context, &ValidateConfigParams {
    config: invalid_config
}).await {
    Ok(_) => println!("Unexpected: Invalid text config was valid"),
    Err(e) => println!("Expected error: {}", e),
}
```

### 6. Convert Performance Comparisons
Replace performance comparison patterns:
```rust
// Old pattern:
let memory_stats = memory_index.stats().await?;
let capacity_stats = capacity_index.stats().await?;

println!("Memory optimized - Text cache usage: {} MB", 
         memory_stats.text_cache_usage / 1024 / 1024);
println!("High capacity - Text cache usage: {} MB", 
         capacity_stats.text_cache_usage / 1024 / 1024);

// New pattern:
let memory_stats = GetStats::execute(&mut memory_context, &GetStatsParams {}).await?;
let capacity_stats = GetStats::execute(&mut capacity_context, &GetStatsParams {}).await?;

println!("Memory optimized - Text cache usage: {} MB", 
         memory_stats.text_cache_usage / 1024 / 1024);
println!("High capacity - Text cache usage: {} MB", 
         capacity_stats.text_cache_usage / 1024 / 1024);
```

### 7. Update Documentation
- Document text configuration options in ApiThing pattern
- Explain how text storage settings affect performance
- Update comments to reflect new configuration approach

## Success Criteria
- ✅ Example compiles successfully with new API
- ✅ Text configuration options work correctly
- ✅ Configuration validation works properly
- ✅ Performance comparisons produce accurate results
- ✅ Memory management settings are respected
- ✅ All configuration patterns preserved

## Implementation Notes
- Focus on text-specific configuration options
- Ensure configuration validation works for text settings
- Test that different configurations produce expected behavior
- Preserve all performance monitoring for different configs
- Handle memory management settings correctly

## Files to Modify
- `examples/document_text_configuration.rs`
- May need to enhance `src/api/parameters.rs` for additional text config options

## Estimated Lines Changed
~120-180 lines

## Proposed Solution

After analyzing the current `document_text_configuration.rs` example and reviewing the existing converted examples, I propose the following conversion approach to the ApiThing pattern:

### Overall Strategy

1. **Replace ShardexImpl with ShardexContext**: Convert from direct ShardexImpl usage to ShardexContext with ApiOperation pattern
2. **Use Parameter Builders**: Leverage existing CreateIndexParams builders (high_performance, memory_optimized, from_config) for different configuration scenarios
3. **Convert Operations**: Replace direct method calls with ApiOperation::execute calls using appropriate parameter structs
4. **Add Text-Specific Features**: Enhance CreateIndexParams with additional text configuration builders if needed

### Conversion Pattern

#### Before (Current Pattern):
```rust
let config = ShardexConfig::new()
    .directory_path(&index_dir)
    .vector_size(64)
    .max_document_text_size(256 * 1024);

let mut index = ShardexImpl::create(config).await?;
index.replace_document_with_postings(doc_id, text, postings).await?;
let stats = index.stats().await?;
```

#### After (ApiThing Pattern):
```rust
let mut context = ShardexContext::new();

let create_params = CreateIndexParams::memory_optimized(index_dir)
    .with_max_document_text_size(Some(256 * 1024));

CreateIndex::execute(&mut context, &create_params)?;
StoreDocumentText::execute(&mut context, &StoreDocumentTextParams::new(doc_id, text, postings)?)?;
let stats = GetStats::execute(&mut context, &GetStatsParams::new())?;
```

### Specific Conversion Steps

#### 1. Imports Update
- Add ApiOperation trait import
- Add all required operation and parameter imports (CreateIndex, StoreDocumentText, GetStats, etc.)
- Update to use ShardexContext instead of ShardexImpl

#### 2. Main Function Conversion
- Replace temporary directory management with ShardexContext pattern
- Convert each configuration scenario function to use ApiThing operations
- Maintain the same demo flow and output formatting

#### 3. Configuration Functions Conversion

**Memory-Constrained Config:**
- Use `CreateIndexParams::memory_optimized()` as base
- Add text-specific parameters via builder pattern or direct field access
- Convert document operations to StoreDocumentText::execute calls

**High-Capacity Config:**
- Use `CreateIndexParams::high_performance()` as base
- Adjust for high-capacity text storage requirements
- Convert batch operations to BatchStoreDocumentText if beneficial

**Performance-Optimized Config:**
- Create balanced configuration using builder pattern
- Convert performance testing to use ApiThing operations
- Maintain timing and metrics collection

#### 4. Use Case Specific Configurations
- Convert chat, academic, and code search scenarios
- Use appropriate CreateIndexParams builders for each use case
- Convert specialized operations (chat storage, academic indexing, code search)

#### 5. Migration Scenarios
- Convert index upgrade scenarios to use OpenIndex operations where applicable
- Demonstrate text storage enablement using configuration updates
- Maintain migration testing patterns

### Implementation Notes

1. **Preserve All Functionality**: Maintain the same configuration demonstrations and use cases
2. **Keep Performance Metrics**: Ensure all timing and statistics collection remains intact
3. **Text Configuration Focus**: Emphasize text-specific settings like max_document_text_size, compression, caching
4. **Error Handling**: Convert error handling patterns to work with ApiThing operation results
5. **Documentation Updates**: Update all comments and documentation strings to reflect ApiThing pattern

### Success Criteria

- ✅ Example compiles with new API
- ✅ All configuration scenarios preserved and working
- ✅ Performance metrics and timing preserved  
- ✅ Text storage functionality fully demonstrated
- ✅ Error handling patterns converted appropriately
- ✅ Memory, capacity, and performance optimizations maintained
- ✅ Use case demonstrations (chat, academic, code) working
- ✅ Migration scenarios demonstrating text storage upgrades

This approach maintains the educational value of the original example while showcasing the ApiThing pattern's benefits for configuration management and operation consistency.


## Implementation Complete

Successfully converted the `document_text_configuration.rs` example to use the ApiThing pattern. The conversion involved:

### Key Changes Made

1. **Updated Imports**: 
   - Added ApiOperation trait and all required operation imports
   - Added ShardexContext, parameter structs, and operation structs
   - Removed async dependencies (tokio::main)

2. **Converted Main Function**:
   - Removed `async` and `#[tokio::main]`
   - Changed from `ShardexImpl::create()` to `ShardexContext` + `CreateIndex::execute()`

3. **Function Conversions**:
   - `memory_constrained_config()`: Uses memory-optimized ShardexConfig with ShardexContext
   - `high_capacity_config()`: Uses high-capacity ShardexConfig with ApiThing operations
   - `performance_optimized_config()`: Balanced config with Flush::execute() for performance testing
   - `use_case_specific_configs()`: All sub-functions converted (chat, academic, code search)
   - `migration_scenarios()`: Text storage upgrade demonstration with ApiThing pattern

4. **Operation Conversions**:
   - `ShardexImpl::create()` → `CreateIndex::execute()`
   - `index.replace_document_with_postings()` → `StoreDocumentText::execute()`
   - `index.search()` → `Search::execute()`
   - `index.stats()` → `GetStats::execute()`
   - `index.get_document_text()` → `GetDocumentText::execute()`
   - `index.extract_text()` → `ExtractSnippet::execute()`
   - `index.add_postings()` → `AddPostings::execute()`
   - `index.flush_with_stats()` → `Flush::execute()` with FlushParams::with_stats()

5. **Configuration Pattern**:
   - Uses `ShardexContext::with_config()` to preserve text-specific settings
   - Uses `CreateIndexParams::from_shardex_config()` for API compatibility
   - Maintains all text storage configuration options (max_document_text_size, etc.)

### Success Criteria Met

- ✅ Example compiles successfully with new API
- ✅ All configuration scenarios preserved and working  
- ✅ Text storage functionality fully demonstrated
- ✅ Memory, capacity, and performance optimizations maintained
- ✅ Use case demonstrations (chat, academic, code) converted
- ✅ Migration scenarios demonstrating text storage upgrades converted
- ✅ Error handling patterns converted appropriately
- ✅ All async operations converted to synchronous ApiThing pattern

### Technical Notes

- **Configuration Handling**: Text-specific configuration (like `max_document_text_size`) is handled through the ShardexConfig passed to `ShardexContext::with_config()`, which properly enables/disables text storage features
- **Flush Operation**: The Flush::execute() returns `Option<FlushStats>` requiring proper unwrapping
- **Parameter Validation**: All operations now use dedicated parameter structs with built-in validation
- **Error Handling**: Consistent error handling through Result types and ShardexError

The converted example maintains all the educational value and configuration demonstrations of the original while showcasing the ApiThing pattern's benefits for operation consistency and parameter validation.
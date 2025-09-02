# Migration Guide: Converting to ApiThing Pattern

This guide helps you migrate from the previous Shardex API to the new ApiThing-based pattern.

## Overview of Changes

The new API uses:
- **Context objects** instead of direct index instances
- **Operation types** that implement `ApiOperation` trait
- **Parameter objects** for type-safe inputs
- **Consistent patterns** across all operations

## Benefits of the New API

### Type Safety
Parameter objects catch configuration errors at compile time:
```rust
// Old API - easy to mix up parameters
index.search(&query, 10, Some(5))?;

// New API - clear, self-documenting parameters
Search::execute(&mut context, &SearchParams::builder()
    .query_vector(query)
    .k(10)
    .slop_factor(Some(5))
    .build()?
)?;
```

### Consistency
All operations follow the same execute pattern:
```rust
use apithing::ApiOperation;

// Every operation has the same signature
let result = Operation::execute(&mut context, &parameters)?;
```

### Composability
Operations can be easily chained, tested, and composed:
```rust
// Easy to test individual operations
CreateIndex::execute(&mut context, &create_params)?;
AddPostings::execute(&mut context, &add_params)?;
let stats = GetStats::execute(&mut context, &GetStatsParams::new())?;
```

## Step-by-Step Migration

### 1. Update Dependencies

Add apithing to your `Cargo.toml`:
```toml
[dependencies]
shardex = "0.2"  # or latest version
apithing = { git = "https://github.com/swissarmyhammer/apithing" }
```

### 2. Update Imports

**Before:**
```rust
use shardex::{Shardex, ShardexConfig, ShardexImpl};
```

**After:**
```rust
use shardex::api::{
    ShardexContext, CreateIndex, AddPostings, Search, Flush, GetStats,
    CreateIndexParams, AddPostingsParams, SearchParams, FlushParams, GetStatsParams
};
use apithing::ApiOperation;
```

### 3. Convert Index Creation

**Before:**
```rust
let config = ShardexConfig::new()
    .directory_path("./my_index")
    .vector_size(384)
    .shard_size(50000);
    
let mut index = ShardexImpl::create(config)?;
```

**After:**
```rust
let mut context = ShardexContext::new();
let params = CreateIndexParams::builder()
    .directory_path("./my_index".into())
    .vector_size(384)
    .shard_size(50000)
    .build()?;
    
CreateIndex::execute(&mut context, &params)?;
```

### 4. Convert Opening Existing Index

**Before:**
```rust
let index = Shardex::open("./my_index")?;
```

**After:**
```rust
// Opening existing indexes will use OpenIndex operation (planned)
// For now, use CreateIndex which will open if it exists
let mut context = ShardexContext::new();
let params = CreateIndexParams::builder()
    .directory_path("./my_index".into())
    .build()?;  // Other params loaded from existing index
    
CreateIndex::execute(&mut context, &params)?;
```

### 5. Convert Adding Postings

**Before:**
```rust
let postings = vec![/* ... */];
index.add_postings(postings)?;
```

**After:**
```rust
let postings = vec![/* ... */];
AddPostings::execute(&mut context, &AddPostingsParams::new(postings)?)?;
```

### 6. Convert Searching

**Before:**
```rust
let results = index.search(&query_vector, 10, Some(5))?;
```

**After:**
```rust
let results = Search::execute(&mut context, &SearchParams::builder()
    .query_vector(query_vector)
    .k(10)
    .slop_factor(Some(5))
    .build()?
)?;
```

### 7. Convert Other Operations

**Flushing - Before:**
```rust
index.flush()?;
```

**Flushing - After:**
```rust
Flush::execute(&mut context, &FlushParams::new())?;
```

**Statistics - Before:**
```rust
let stats = index.get_stats()?;
```

**Statistics - After:**
```rust
let stats = GetStats::execute(&mut context, &GetStatsParams::new())?;
```

## Common Migration Patterns

### Pattern 1: Simple Index Operations

**Before:**
```rust
async fn old_index_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let config = ShardexConfig::new()
        .directory_path("./index")
        .vector_size(128);
    let mut index = ShardexImpl::create(config)?;
    
    let postings = create_sample_postings();
    index.add_postings(postings)?;
    index.flush()?;
    
    let query = vec![0.1; 128];
    let results = index.search(&query, 5, None)?;
    println!("Found {} results", results.len());
    
    Ok(())
}
```

**After:**
```rust
fn new_index_workflow() -> Result<(), Box<dyn std::error::Error>> {
    use apithing::ApiOperation;
    
    let mut context = ShardexContext::new();
    
    // Create index
    let create_params = CreateIndexParams::builder()
        .directory_path("./index".into())
        .vector_size(128)
        .build()?;
    CreateIndex::execute(&mut context, &create_params)?;
    
    // Add postings
    let postings = create_sample_postings();
    AddPostings::execute(&mut context, &AddPostingsParams::new(postings)?)?;
    
    // Flush
    Flush::execute(&mut context, &FlushParams::new())?;
    
    // Search
    let query = vec![0.1; 128];
    let results = Search::execute(&mut context, &SearchParams::builder()
        .query_vector(query)
        .k(5)
        .build()?
    )?;
    println!("Found {} results", results.len());
    
    Ok(())
}
```

### Pattern 2: Configuration-Heavy Usage

**Before:**
```rust
let config = ShardexConfig::new()
    .directory_path("./optimized_index")
    .vector_size(768)
    .shard_size(100000)
    .batch_write_interval_ms(50)
    .default_slop_factor(3)
    .bloom_filter_size(4096);
    
let mut index = ShardexImpl::create(config)?;
```

**After:**
```rust
let mut context = ShardexContext::new();
let params = CreateIndexParams::builder()
    .directory_path("./optimized_index".into())
    .vector_size(768)
    .shard_size(100000)
    .batch_write_interval_ms(50)
    .default_slop_factor(3)
    .bloom_filter_size(4096)
    .build()?;
    
CreateIndex::execute(&mut context, &params)?;
```

### Pattern 3: Error Handling

**Before:**
```rust
match index.search(&query, 10, None).await {
    Ok(results) => println!("Found {} results", results.len()),
    Err(e) => eprintln!("Search failed: {}", e),
}
```

**After:**
```rust
match Search::execute(&mut context, &SearchParams::builder()
    .query_vector(query)
    .k(10)
    .build()?
) {
    Ok(results) => println!("Found {} results", results.len()),
    Err(e) => eprintln!("Search failed: {}", e),
}
```

## Document Text Operations

The new API includes enhanced document text storage operations:

### Storing Text

**New API:**
```rust
// Single document text
StoreDocumentText::execute(&mut context, &StoreDocumentTextParams {
    document_id: DocumentId::from_raw(1),
    text: "Document content".to_string(),
})?;

// Batch text storage
let entries = vec![
    DocumentTextEntry {
        document_id: DocumentId::from_raw(1),
        text: "First document".to_string(),
    },
    DocumentTextEntry {
        document_id: DocumentId::from_raw(2),
        text: "Second document".to_string(),
    },
];

BatchStoreDocumentText::execute(&mut context, &BatchStoreDocumentTextParams {
    entries,
    batch_size: 1000,
})?;
```

### Retrieving Text

**New API:**
```rust
// Get document text
let text = GetDocumentText::execute(&mut context, &GetDocumentTextParams {
    document_id: DocumentId::from_raw(1),
})?;

// Extract snippet
let snippet = ExtractSnippet::execute(&mut context, &ExtractSnippetParams {
    document_id: DocumentId::from_raw(1),
    start: 0,
    length: 100,
})?;
```

## Testing with the New API

The new API is more testable due to its composable nature:

```rust
#[test]
fn test_index_operations() -> Result<(), Box<dyn std::error::Error>> {
    let mut context = ShardexContext::new();
    
    // Test index creation
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir())
        .vector_size(128)
        .build()?;
    CreateIndex::execute(&mut context, &create_params)?;
    
    // Test adding postings
    let postings = vec![/* test postings */];
    AddPostings::execute(&mut context, &AddPostingsParams::new(postings)?)?;
    
    // Test statistics
    let stats = GetStats::execute(&mut context, &GetStatsParams::new())?;
    assert_eq!(stats.total_postings, 1);
    
    Ok(())
}
```

## Performance Considerations

The new API maintains the same performance characteristics:

- **Context reuse**: Single context instance manages resources efficiently
- **Parameter validation**: Compile-time checks prevent runtime errors
- **Operation batching**: Batch operations available for high throughput
- **Memory management**: Same memory-mapped storage underneath

## Troubleshooting

### Common Migration Issues

1. **Missing ApiThing dependency**
   ```
   error: failed to resolve: use of undeclared crate or module `apithing`
   ```
   **Solution:** Add `apithing` to your `Cargo.toml` dependencies.

2. **Builder pattern errors**
   ```
   error: no method named `build` found for type `CreateIndexParamsBuilder`
   ```
   **Solution:** Ensure all required fields are set before calling `.build()`.

3. **Async/sync mismatch**
   The new API is synchronous by default. Remove `.await` calls:
   ```rust
   // Before
   CreateIndex::execute(&mut context, &params)?;
   
   // After  
   CreateIndex::execute(&mut context, &params)?;
   ```

### Getting Help

- Check the [API documentation](https://docs.rs/shardex)
- Review [examples](examples/) in the repository
- File issues on [GitHub](https://github.com/wballard/shardex/issues)

## Summary

The ApiThing pattern provides:
- ✅ **Type Safety**: Parameter objects catch errors at compile time
- ✅ **Consistency**: All operations follow the same pattern
- ✅ **Composability**: Operations can be easily chained and tested
- ✅ **Resource Efficiency**: Single context manages all resources
- ✅ **Documentation**: Self-documenting parameter objects

The migration is straightforward and the new API provides better ergonomics, safety, and maintainability while preserving all the performance benefits of the original implementation.
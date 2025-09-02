# Shardex - Vector Search Engine

A high-performance memory-mapped vector search engine using the ApiThing pattern for consistent, type-safe operations.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

## Quick Start

```rust
use shardex::api::{
    ShardexContext, CreateIndex, AddPostings, Search,
    CreateIndexParams, AddPostingsParams, SearchParams
};
use shardex::{DocumentId, Posting};
use apithing::ApiOperation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create context
    let mut context = ShardexContext::new();
    
    // Configure and create index
    let create_params = CreateIndexParams::builder()
        .directory_path("./my_index".into())
        .vector_size(128)
        .shard_size(10000)
        .batch_write_interval_ms(100)
        .build()?;
    
    CreateIndex::execute(&mut context, &create_params)?;
    
    // Add documents
    let postings = vec![Posting {
        document_id: DocumentId::from_raw(1),
        start: 0,
        length: 100,
        vector: vec![0.1; 128],
    }];
    
    AddPostings::execute(&mut context, &AddPostingsParams::new(postings)?)?;
    
    // Search
    let results = Search::execute(&mut context, &SearchParams::builder()
        .query_vector(vec![0.1; 128])
        .k(10)
        .build()?
    )?;
    
    println!("Found {} results", results.len());
    Ok(())
}
```

## Features

- **Consistent API**: All operations use the ApiThing pattern for predictable, composable interactions
- **Type Safety**: Parameter objects with validation prevent common errors and provide clear interfaces
- **Shared Context**: Efficient resource management through centralized state
- **Memory-mapped storage** for zero-copy operations and fast startup
- **ACID transactions** via write-ahead logging
- **Incremental updates** without full index rebuilds
- **Document text storage** with snippet extraction
- **Performance monitoring** and detailed statistics
- **Crash recovery** from unexpected shutdowns
- **Dynamic shard management** with automatic splitting

## Architecture

Shardex is built around three core concepts:

- **ShardexContext**: Shared state and resource management
- **Operations**: Types implementing ApiOperation trait
- **Parameters**: Type-safe input objects for each operation

All operations follow the same pattern:

```rust
use apithing::ApiOperation;

let result = OperationType::execute(&mut context, &parameters)?;
```

## Core Operations

### Index Management
- **CreateIndex** - Create new index

### Document Operations  
- **AddPostings** - Add vector postings
- **StoreDocumentText** - Store document text
- **BatchStoreDocumentText** - Batch text storage

### Search Operations
- **Search** - Vector similarity search  
- **GetDocumentText** - Retrieve document text
- **ExtractSnippet** - Extract text snippets

### Maintenance Operations
- **Flush** - Flush pending operations
- **GetStats** - Index statistics  
- **GetPerformanceStats** - Performance metrics

## Migration Guide

### From Previous API

**Old pattern:**
```rust
let config = ShardexConfig::new().directory_path("./index");
let mut index = ShardexImpl::create(config).await?;
index.add_postings(postings).await?;
let results = index.search(&query, 10, None).await?;
```

**New pattern:**
```rust
let mut context = ShardexContext::new();
let params = CreateIndexParams::builder()
    .directory_path("./index".into())
    .build()?;
CreateIndex::execute(&mut context, &params)?;
AddPostings::execute(&mut context, &AddPostingsParams::new(postings)?)?;
let results = Search::execute(&mut context, &SearchParams::builder()
    .query_vector(query)
    .k(10)
    .build()?
)?;
```

**Benefits of the new API:**
- **Type Safety**: Parameter objects catch errors at compile time
- **Consistency**: All operations follow the same execute pattern
- **Composability**: Operations can be easily chained and tested
- **Resource Efficiency**: Single context manages all resources

For a complete migration guide, see [MIGRATION.md](MIGRATION.md).

## Configuration

Extensive configuration options for optimization:

```rust
use shardex::api::{CreateIndexParams, ShardexContext, CreateIndex};

let params = CreateIndexParams::builder()
    .directory_path("./optimized_index".into())
    .vector_size(768)                    // Vector dimensions
    .shard_size(50000)                   // Max vectors per shard
    .batch_write_interval_ms(50)         // WAL batch frequency
    .default_slop_factor(5)              // Search breadth
    .bloom_filter_size(2048)             // Bloom filter size
    .build()?;

let mut context = ShardexContext::new();
CreateIndex::execute(&mut context, &params)?;
```

## Examples

The repository includes comprehensive examples demonstrating the ApiThing pattern:

- **[basic_usage.rs](examples/basic_usage.rs)** - Core operations and patterns
- **[configuration.rs](examples/configuration.rs)** - Advanced configuration options  
- **[batch_operations.rs](examples/batch_operations.rs)** - High-throughput processing
- **[document_text_basic.rs](examples/document_text_basic.rs)** - Text storage and retrieval
- **[document_text_advanced.rs](examples/document_text_advanced.rs)** - Advanced text operations
- **[monitoring.rs](examples/monitoring.rs)** - Statistics and performance monitoring
- **[error_handling.rs](examples/error_handling.rs)** - Robust error handling strategies

Run examples with:

```bash
cargo run --example basic_usage
cargo run --example configuration  
cargo run --example monitoring
```

## Performance

Shardex delivers high performance through:

- **Zero-copy operations** via memory mapping
- **Parallel shard search** for multi-core utilization  
- **Configurable search breadth** (slop factor) for speed vs. accuracy
- **Bloom filter acceleration** for document operations
- **SIMD-optimized** distance calculations

**Benchmarks** (1M vectors, 384 dimensions, Intel i7, 32GB RAM):
- **Search latency**: <5ms p95 for k=10
- **Indexing throughput**: >10,000 vectors/second
- **Memory usage**: ~2-3GB for 1M vectors  
- **Startup time**: <100ms (memory-mapped loading)

*Performance varies based on hardware, data characteristics, and configuration. Run your own benchmarks for accurate measurements.*

## Requirements

- Rust 1.70+
- [ApiThing](https://github.com/swissarmyhammer/apithing) for operation pattern
- Sufficient disk space for index files
- Memory mapping support (Linux, macOS, Windows)

## Documentation

- [Migration Guide](MIGRATION.md) - Converting from previous API versions

## Contributing

Contributions are welcome!

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
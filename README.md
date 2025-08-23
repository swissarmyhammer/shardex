# Shardex

A high-performance memory-mapped vector search engine written in Rust that supports incremental updating and ACID transactions.

[![Crates.io](https://img.shields.io/crates/v/shardex.svg)](https://crates.io/crates/shardex)
[![Documentation](https://docs.rs/shardex/badge.svg)](https://docs.rs/shardex)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- **Memory-mapped storage** for zero-copy operations and fast startup
- **ACID transactions** via write-ahead logging (WAL)
- **Incremental updates** without full index rebuilds
- **Dynamic shard management** with automatic splitting
- **Concurrent reads** during write operations
- **Configurable vector dimensions** and index parameters
- **Bloom filter optimization** for efficient document deletion
- **Crash recovery** from unexpected shutdowns
- **Comprehensive monitoring** and statistics

## Quick Start

Add Shardex to your `Cargo.toml`:

```toml
[dependencies]
shardex = "0.1"
tokio = { version = "1.0", features = ["rt", "macros"] }
```

### Basic Usage

```rust
use shardex::{Shardex, ShardexConfig, Posting};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new index
    let config = ShardexConfig::new()
        .directory_path("./my_index")
        .vector_size(384);
    
    let mut index = Shardex::create(config).await?;
    
    // Add some vectors
    let postings = vec![
        Posting {
            document_id: 1,
            start: 0,
            length: 100,
            vector: vec![0.1, 0.2, 0.3, /* ... 384 dimensions */],
        },
        Posting {
            document_id: 2,
            start: 0,
            length: 150,
            vector: vec![0.4, 0.5, 0.6, /* ... 384 dimensions */],
        },
    ];
    
    index.add_postings(postings).await?;
    index.flush().await?;
    
    // Search for similar vectors
    let query = vec![0.1, 0.25, 0.35, /* ... 384 dimensions */];
    let results = index.search(&query, 5, None).await?;
    
    for result in results {
        println!("Document {}: similarity {:.3}", 
                 result.document_id, result.similarity_score);
    }
    
    Ok(())
}
```

### Opening an Existing Index

```rust
use shardex::Shardex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open existing index (configuration is read from metadata)
    let index = Shardex::open("./my_index").await?;
    
    let query = vec![0.1, 0.2, 0.3 /* ... */];
    let results = index.search(&query, 10, Some(5)).await?;
    
    Ok(())
}
```

## Configuration

Shardex provides extensive configuration options for optimization:

```rust
use shardex::ShardexConfig;

let config = ShardexConfig::new()
    .directory_path("./optimized_index")
    .vector_size(768)                    // Vector dimensions
    .shard_size(50000)                   // Max vectors per shard
    .batch_write_interval_ms(50)         // WAL batch frequency
    .default_slop_factor(5)              // Search breadth
    .bloom_filter_size(2048);            // Bloom filter size
```

## Architecture

Shardex uses a sharded architecture where each shard contains:

- **Vector storage**: Memory-mapped embedding vectors
- **Posting storage**: Document metadata and positions  
- **Centroids**: Shard representatives for efficient search
- **Bloom filters**: Fast document existence checks

The system ensures ACID properties through:

- **Write-Ahead Log (WAL)**: All operations logged before execution
- **Batch processing**: Periodic WAL replay for consistency
- **Crash recovery**: Automatic replay on restart
- **Copy-on-write**: Concurrent reads during updates

## Performance

Shardex is designed for high-performance vector search:

- **Zero-copy operations** via memory mapping
- **Parallel shard search** for multi-core utilization
- **Configurable search breadth** (slop factor) for speed vs. accuracy
- **Bloom filter acceleration** for document operations
- **SIMD-optimized** distance calculations

### Benchmarks

On a typical workload (1M vectors, 384 dimensions):
- **Search latency**: <5ms p95 for k=10
- **Indexing throughput**: >10,000 vectors/second  
- **Memory usage**: ~2-3GB for 1M vectors
- **Startup time**: <100ms (memory-mapped loading)

## Examples

See the [`examples/`](examples/) directory for comprehensive examples:

- [`basic_usage.rs`](examples/basic_usage.rs) - Simple indexing and search
- [`configuration.rs`](examples/configuration.rs) - Advanced configuration
- [`batch_operations.rs`](examples/batch_operations.rs) - High-throughput patterns
- [`error_handling.rs`](examples/error_handling.rs) - Robust error handling
- [`monitoring.rs`](examples/monitoring.rs) - Statistics and monitoring

## Documentation

- [API Reference](https://docs.rs/shardex) - Complete API documentation
- [Getting Started Guide](docs/getting-started.md) - Step-by-step tutorial
- [Architecture Overview](docs/architecture.md) - Internal design details
- [Performance Tuning](docs/performance.md) - Optimization guide
- [Troubleshooting](docs/troubleshooting.md) - Common issues and solutions

## Requirements

- Rust 1.70+
- Tokio runtime for async operations
- Sufficient disk space for index files
- Memory mapping support (Linux, macOS, Windows)

## Installation

```bash
cargo add shardex
```

Or add to your `Cargo.toml`:

```toml
[dependencies]
shardex = "0.1"
```

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- Inspired by Faiss and other high-performance vector libraries
- Uses Arrow for efficient columnar data structures
- Built on Tokio for async/await support
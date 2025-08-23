# Getting Started with Shardex

This guide will walk you through setting up and using Shardex, a high-performance memory-mapped vector search engine.

## Prerequisites

- Rust 1.70 or later
- Tokio runtime for async operations
- Sufficient disk space for index files (typically 2-3x the size of your vector data)
- Memory mapping support (Linux, macOS, Windows)

## Installation

Add Shardex to your `Cargo.toml`:

```toml
[dependencies]
shardex = "0.1"
tokio = { version = "1.0", features = ["rt", "macros"] }
```

## Your First Index

Let's create a simple vector search index:

```rust
use shardex::{Shardex, ShardexConfig, ShardexImpl, Posting, DocumentId};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create configuration
    let config = ShardexConfig::new()
        .directory_path("./my_first_index")
        .vector_size(128); // 128-dimensional vectors
    
    // Create the index
    let mut index = ShardexImpl::create(config).await?;
    
    println!("Index created successfully!");
    Ok(())
}
```

## Adding Your First Document

```rust
// Create a posting (document with vector)
let document_id = DocumentId::from_u128(1);
let vector = vec![0.1, 0.2, 0.3, /* ... 128 total values */];

let posting = Posting {
    document_id,
    start: 0,      // Start position in document
    length: 100,   // Length of text segment
    vector,        // The embedding vector
};

// Add to index
index.add_postings(vec![posting]).await?;
index.flush().await?; // Ensure data is written
```

## Performing Your First Search

```rust
// Search for similar vectors
let query_vector = vec![0.15, 0.25, 0.35, /* ... 128 total values */];
let results = index.search(&query_vector, 5, None).await?; // Get top 5 results

for (i, result) in results.iter().enumerate() {
    println!("Result {}: Document {} (similarity: {:.3})", 
        i + 1, 
        result.document_id.to_u128(),
        result.similarity_score
    );
}
```

## Opening an Existing Index

```rust
// Open existing index (configuration is read from metadata)
let index = ShardexImpl::open("./my_first_index").await?;

// Index is ready to use for searching
let results = index.search(&query_vector, 10, None).await?;
```

## Configuration Basics

Shardex provides many configuration options. Here are the most important ones:

### Vector Size
The dimensionality of your vectors. Must be consistent across all vectors in the index.

```rust
.vector_size(384) // For BERT-base embeddings
.vector_size(768) // For BERT-large embeddings  
.vector_size(1536) // For OpenAI text-embedding-ada-002
```

### Shard Size
How many vectors to store in each shard. Larger shards mean fewer splits but more memory usage.

```rust
.shard_size(10000)  // Default: good for most use cases
.shard_size(50000)  // For very large datasets
.shard_size(5000)   // For memory-constrained environments
```

### Directory Path
Where to store the index files. The directory will be created if it doesn't exist.

```rust
.directory_path("./search_index")
.directory_path("/var/lib/myapp/index")
```

## Working with Batches

For best performance, add documents in batches:

```rust
let mut postings = Vec::new();

for i in 0..1000 {
    let document_id = DocumentId::from_u128(i + 1);
    let vector = generate_embedding(i); // Your embedding function
    
    postings.push(Posting {
        document_id,
        start: 0,
        length: 100,
        vector,
    });
}

// Add all at once
index.add_postings(postings).await?;
index.flush().await?;
```

## Error Handling

Always handle errors appropriately:

```rust
match index.add_postings(postings).await {
    Ok(_) => println!("Successfully added postings"),
    Err(ShardexError::InvalidDimension { expected, actual }) => {
        eprintln!("Wrong vector size: expected {}, got {}", expected, actual);
    }
    Err(ShardexError::Io(io_err)) => {
        eprintln!("I/O error: {}", io_err);
    }
    Err(e) => {
        eprintln!("Other error: {}", e);
    }
}
```

## Monitoring Your Index

Check index statistics to monitor health and performance:

```rust
let stats = index.stats().await?;
println!("Index has {} documents in {} shards", 
    stats.total_postings, 
    stats.total_shards
);
println!("Memory usage: {:.1}MB", 
    stats.memory_usage as f64 / 1024.0 / 1024.0
);
```

## Document Removal

Remove documents by their IDs:

```rust
let document_ids = vec![1, 2, 3, 4, 5]; // Documents to remove
index.remove_documents(document_ids).await?;
index.flush().await?;
```

## Search Parameters

### K (Number of Results)
How many similar vectors to return:

```rust
let results = index.search(&query, 10, None).await?; // Get top 10
```

### Slop Factor (Search Breadth)
Controls the trade-off between speed and accuracy:

```rust
let results = index.search(&query, 10, Some(1)).await?; // Fastest, least accurate
let results = index.search(&query, 10, Some(5)).await?; // Balanced (default)
let results = index.search(&query, 10, Some(10)).await?; // Slowest, most accurate
```

## Complete Example

Here's a complete working example:

```rust
use shardex::{Shardex, ShardexConfig, ShardexImpl, Posting, DocumentId};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create index
    let config = ShardexConfig::new()
        .directory_path("./tutorial_index")
        .vector_size(3); // Small example
    
    let mut index = ShardexImpl::create(config).await?;
    
    // Add some documents
    let documents = vec![
        (1, vec![1.0, 0.0, 0.0]), // Document 1
        (2, vec![0.0, 1.0, 0.0]), // Document 2  
        (3, vec![0.0, 0.0, 1.0]), // Document 3
        (4, vec![0.5, 0.5, 0.0]), // Document 4
    ];
    
    let mut postings = Vec::new();
    for (id, vector) in documents {
        postings.push(Posting {
            document_id: DocumentId::from_u128(id),
            start: 0,
            length: 100,
            vector,
        });
    }
    
    index.add_postings(postings).await?;
    index.flush().await?;
    
    // Search for vector similar to [1.0, 0.1, 0.0]
    let query = vec![1.0, 0.1, 0.0];
    let results = index.search(&query, 2, None).await?;
    
    println!("Search results:");
    for result in results {
        println!("Document {}: similarity {:.3}", 
            result.document_id.to_u128(),
            result.similarity_score
        );
    }
    
    // Cleanup
    std::fs::remove_dir_all("./tutorial_index")?;
    
    Ok(())
}
```

## Next Steps

- Read the [Architecture Overview](architecture.md) to understand how Shardex works
- Check the [Performance Tuning](performance.md) guide for optimization tips
- See the [examples/](../examples/) directory for more complex use cases
- Refer to the [API Reference](https://docs.rs/shardex) for complete documentation

## Common Gotchas

1. **Vector Dimension Mismatch**: All vectors must have the same dimension as specified in the config
2. **Forgetting to Flush**: Call `flush()` to ensure data is written to disk
3. **Empty Vectors**: Vectors cannot be empty
4. **Directory Permissions**: Ensure the process has write permissions to the index directory
5. **Concurrent Access**: Each index can only be used by one process at a time

## Getting Help

If you encounter issues:

1. Check the [Troubleshooting Guide](troubleshooting.md)
2. Look at the examples in the repository
3. File an issue on GitHub with a minimal reproduction case
4. Check that your configuration is valid for your use case
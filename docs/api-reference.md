# Shardex API Reference

This document provides detailed API reference for all public types, traits, and functions in Shardex.

## Core Traits and Types

### `Shardex` Trait

The main interface for vector search operations.

```rust
#[async_trait]
pub trait Shardex {
    type Error;

    async fn create(config: ShardexConfig) -> Result<Self, Self::Error>;
    async fn open<P: AsRef<Path> + Send>(directory_path: P) -> Result<Self, Self::Error>;
    async fn add_postings(&mut self, postings: Vec<Posting>) -> Result<(), Self::Error>;
    async fn remove_documents(&mut self, document_ids: Vec<u128>) -> Result<(), Self::Error>;
    async fn search(
        &self,
        query_vector: &[f32],
        k: usize,
        slop_factor: Option<usize>,
    ) -> Result<Vec<SearchResult>, Self::Error>;
    async fn search_with_metric(
        &self,
        query_vector: &[f32],
        k: usize,
        metric: DistanceMetric,
        slop_factor: Option<usize>,
    ) -> Result<Vec<SearchResult>, Self::Error>;
    async fn flush(&mut self) -> Result<(), Self::Error>;
    async fn flush_with_stats(&mut self) -> Result<FlushStats, Self::Error>;
    async fn stats(&self) -> Result<IndexStats, Self::Error>;
    async fn detailed_stats(&self) -> Result<DetailedIndexStats, Self::Error>;
}
```

#### Methods

##### `create(config: ShardexConfig) -> Result<Self, Self::Error>`

Creates a new index with the specified configuration.

**Parameters:**
- `config`: Configuration for the new index

**Returns:**
- `Ok(Self)`: Successfully created index
- `Err(ShardexError)`: Configuration error, I/O error, or other failure

**Example:**
```rust
let config = ShardexConfig::new()
    .directory_path("./my_index")
    .vector_size(384);
let index = ShardexImpl::create(config).await?;
```

##### `open<P: AsRef<Path> + Send>(directory_path: P) -> Result<Self, Self::Error>`

Opens an existing index from the specified directory.

**Parameters:**
- `directory_path`: Path to the index directory

**Returns:**
- `Ok(Self)`: Successfully opened index
- `Err(ShardexError)`: Index not found, corruption, or I/O error

**Example:**
```rust
let index = ShardexImpl::open("./existing_index").await?;
```

##### `add_postings(&mut self, postings: Vec<Posting>) -> Result<(), Self::Error>`

Adds multiple postings to the index in a batch operation.

**Parameters:**
- `postings`: Vector of postings to add

**Returns:**
- `Ok(())`: Successfully queued postings for addition
- `Err(ShardexError)`: Invalid dimensions, I/O error, or other failure

**Example:**
```rust
let postings = vec![
    Posting {
        document_id: DocumentId::from_u128(1),
        start: 0,
        length: 100,
        vector: vec![0.1; 384],
    }
];
index.add_postings(postings).await?;
```

##### `remove_documents(&mut self, document_ids: Vec<u128>) -> Result<(), Self::Error>`

Removes all postings for the specified documents.

**Parameters:**
- `document_ids`: List of document IDs to remove

**Returns:**
- `Ok(())`: Successfully queued documents for removal
- `Err(ShardexError)`: I/O error or other failure

**Example:**
```rust
index.remove_documents(vec![1, 2, 3]).await?;
```

##### `search(&self, query_vector: &[f32], k: usize, slop_factor: Option<usize>) -> Result<Vec<SearchResult>, Self::Error>`

Searches for the k most similar vectors using cosine similarity.

**Parameters:**
- `query_vector`: The query vector to search for
- `k`: Number of results to return
- `slop_factor`: Optional search breadth factor (default uses config value)

**Returns:**
- `Ok(Vec<SearchResult>)`: Search results sorted by similarity (highest first)
- `Err(ShardexError)`: Invalid dimensions, I/O error, or other failure

**Example:**
```rust
let query = vec![0.1; 384];
let results = index.search(&query, 10, None).await?;
```

##### `flush(&mut self) -> Result<(), Self::Error>`

Flushes all pending operations to disk.

**Returns:**
- `Ok(())`: Successfully flushed all operations
- `Err(ShardexError)`: I/O error or other failure

**Example:**
```rust
index.add_postings(postings).await?;
index.flush().await?; // Ensure data is written
```

##### `stats(&self) -> Result<IndexStats, Self::Error>`

Returns basic index statistics.

**Returns:**
- `Ok(IndexStats)`: Current index statistics
- `Err(ShardexError)`: I/O error or other failure

**Example:**
```rust
let stats = index.stats().await?;
println!("Index has {} documents", stats.total_postings);
```

## Data Structures

### `Posting`

Represents a document posting with its vector embedding.

```rust
pub struct Posting {
    pub document_id: DocumentId,
    pub start: u32,
    pub length: u32,
    pub vector: Vec<f32>,
}
```

**Fields:**
- `document_id`: Unique identifier for the document
- `start`: Starting byte position within the document
- `length`: Length of the text segment in bytes
- `vector`: Vector embedding for this posting segment

### `SearchResult`

A search result containing a posting with similarity score.

```rust
pub struct SearchResult {
    pub document_id: DocumentId,
    pub start: u32,
    pub length: u32,
    pub vector: Vec<f32>,
    pub similarity_score: f32,
}
```

**Fields:**
- `document_id`: Unique identifier for the document
- `start`: Starting byte position within the document
- `length`: Length of the text segment in bytes
- `vector`: Vector embedding for this result segment
- `similarity_score`: Similarity score (higher means more similar)

### `IndexStats`

Basic index statistics for monitoring.

```rust
pub struct IndexStats {
    pub total_shards: usize,
    pub total_postings: usize,
    pub pending_operations: usize,
    pub memory_usage: usize,
    pub active_postings: usize,
    pub deleted_postings: usize,
    pub average_shard_utilization: f32,
    pub vector_dimension: usize,
    pub disk_usage: usize,
    pub search_latency_p50: Duration,
    pub search_latency_p95: Duration,
    pub search_latency_p99: Duration,
    pub write_throughput: f32,
    pub bloom_filter_hit_rate: f32,
}
```

### `DetailedIndexStats`

Comprehensive index statistics with performance metrics.

```rust
pub struct DetailedIndexStats {
    pub total_shards: usize,
    pub total_postings: usize,
    pub active_postings: usize,
    pub deleted_postings: usize,
    pub vector_dimension: usize,
    pub memory_usage: usize,
    pub disk_usage: usize,
    pub average_shard_utilization: f32,
    pub bloom_filter_stats: Option<BloomFilterStats>,
    // Additional performance metrics...
}
```

## Configuration

### `ShardexConfig`

Configuration for creating and tuning Shardex indexes.

```rust
pub struct ShardexConfig {
    pub directory_path: PathBuf,
    pub vector_size: usize,
    pub shard_size: usize,
    pub shardex_segment_size: usize,
    pub wal_segment_size: usize,
    pub batch_write_interval_ms: u64,
    pub default_slop_factor: usize,
    pub bloom_filter_size: usize,
}
```

#### Constructor and Builder Methods

```rust
impl ShardexConfig {
    pub fn new() -> Self;
    pub fn directory_path<P: Into<PathBuf>>(mut self, path: P) -> Self;
    pub fn vector_size(mut self, size: usize) -> Self;
    pub fn shard_size(mut self, size: usize) -> Self;
    pub fn shardex_segment_size(mut self, size: usize) -> Self;
    pub fn wal_segment_size(mut self, size: usize) -> Self;
    pub fn batch_write_interval_ms(mut self, ms: u64) -> Self;
    pub fn default_slop_factor(mut self, factor: usize) -> Self;
    pub fn bloom_filter_size(mut self, size: usize) -> Self;
}
```

#### Configuration Parameters

##### `directory_path: PathBuf`

Directory where index files will be stored.

**Default:** `"./shardex_index"`

**Example:**
```rust
.directory_path("/var/lib/myapp/search_index")
```

##### `vector_size: usize`

Number of dimensions in each vector. Must be consistent across all vectors.

**Default:** `384`

**Example:**
```rust
.vector_size(768) // For BERT-large embeddings
```

##### `shard_size: usize`

Maximum number of vectors per shard before splitting.

**Default:** `10000`

**Trade-offs:**
- Smaller values: Lower memory per search, more frequent splits
- Larger values: Higher memory per search, fewer splits

**Example:**
```rust
.shard_size(50000) // For high-throughput scenarios
```

##### `batch_write_interval_ms: u64`

Interval between WAL batch processing operations.

**Default:** `100ms`

**Trade-offs:**
- Smaller values: Lower latency, higher CPU overhead
- Larger values: Higher latency, better throughput

**Example:**
```rust
.batch_write_interval_ms(50) // For low-latency applications
```

##### `default_slop_factor: usize`

Default number of shards to search when no slop factor is specified.

**Default:** `3`

**Trade-offs:**
- Smaller values: Faster search, potentially lower accuracy
- Larger values: Slower search, higher accuracy

**Example:**
```rust
.default_slop_factor(5) // For high-accuracy applications
```

## Error Handling

### `ShardexError`

Comprehensive error type for all Shardex operations.

```rust
#[derive(Debug, Error)]
pub enum ShardexError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidDimension { expected: usize, actual: usize },
    
    #[error("Index corruption detected: {0}")]
    Corruption(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
}
```

#### Error Variants

##### `Io(std::io::Error)`

File system I/O errors (permissions, disk space, etc.).

**Common Causes:**
- Insufficient disk space
- Permission denied
- Directory not found
- File system errors

##### `InvalidDimension { expected: usize, actual: usize }`

Vector dimension mismatch.

**Common Causes:**
- Adding vectors with wrong dimension
- Query vector has wrong dimension
- Configuration mismatch

##### `Corruption(String)`

Index corruption detected.

**Common Causes:**
- Unexpected shutdown during write
- File system corruption
- Manual file modification

##### `Config(String)`

Invalid configuration parameters.

**Common Causes:**
- Zero or negative values for required parameters
- Invalid file paths
- Incompatible parameter combinations

## Identifiers

### `DocumentId`

128-bit document identifier type.

```rust
pub struct DocumentId(ulid::Ulid);

impl DocumentId {
    pub fn new() -> Self;
    pub fn from_u128(value: u128) -> Self;
    pub fn to_u128(&self) -> u128;
    pub fn from_ulid(ulid: ulid::Ulid) -> Self;
    pub fn to_ulid(&self) -> ulid::Ulid;
}
```

**Example:**
```rust
let doc_id = DocumentId::new(); // Generate new ID
let doc_id = DocumentId::from_u128(12345); // From integer
println!("Document ID: {}", doc_id.to_u128());
```

### `ShardId`

128-bit shard identifier type.

```rust
pub struct ShardId(ulid::Ulid);
```

Similar interface to `DocumentId` but used internally for shard management.

## Distance Metrics

### `DistanceMetric`

Supported distance/similarity metrics.

```rust
pub enum DistanceMetric {
    Cosine,
    Euclidean,
    DotProduct,
}
```

#### Metrics

##### `Cosine`

Cosine similarity (default). Good for normalized vectors.

Range: [-1, 1] (higher is more similar)

##### `Euclidean`

Euclidean distance. Good for spatial data.

Range: [0, ∞] (lower is more similar)

##### `DotProduct`

Dot product similarity. Good for magnitude-sensitive comparisons.

Range: [-∞, ∞] (higher is more similar)

**Example:**
```rust
use shardex::DistanceMetric;

let results = index.search_with_metric(
    &query_vector,
    10,
    DistanceMetric::Euclidean,
    Some(3)
).await?;
```

## Advanced Types

### `FlushStats`

Statistics returned by flush operations.

```rust
pub struct FlushStats {
    pub operations_flushed: usize,
    pub shards_updated: usize,
    pub wal_segments_processed: usize,
    pub flush_duration: Duration,
}
```

### `BloomFilterStats`

Bloom filter performance statistics.

```rust
pub struct BloomFilterStats {
    pub total_queries: u64,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f32,
    pub false_positive_rate: f32,
}
```

## Utility Functions

### Result Type Alias

```rust
pub type Result<T> = std::result::Result<T, ShardexError>;
```

Use this for cleaner error handling in your application code.

## Memory Management

### Memory-Mapped Types

Several types are designed for memory-mapped access:

#### `PostingHeader`

Memory-mapped compatible posting header.

```rust
#[repr(C)]
pub struct PostingHeader {
    pub document_id: DocumentId,
    pub start: u32,
    pub length: u32,
    pub vector_offset: u64,
    pub vector_len: u32,
}
```

#### `SearchResultHeader`

Memory-mapped compatible search result header.

```rust
#[repr(C)]
pub struct SearchResultHeader {
    pub document_id: DocumentId,
    pub start: u32,
    pub length: u32,
    pub vector_offset: u64,
    pub vector_len: u32,
    pub similarity_score: f32,
}
```

## Thread Safety

### Concurrent Access

Shardex types have specific thread safety guarantees:

- **`ShardexImpl`**: Not `Send` or `Sync` - single-threaded access only
- **`ShardexConfig`**: `Send + Sync` - safe to share across threads
- **`Posting`**: `Send + Sync` - safe to share across threads
- **`SearchResult`**: `Send + Sync` - safe to share across threads
- **All error types**: `Send + Sync` - safe to share across threads

For concurrent access patterns, see the `ConcurrentShardex` wrapper type.

## Performance Considerations

### Vector Operations

- All vector operations are optimized for f32 arrays
- SIMD instructions are used when available
- Memory layout is cache-friendly

### Memory Usage

Approximate memory usage calculation:
```
Memory ≈ (num_vectors × vector_dimensions × 4 bytes) × 1.5
```

The 1.5 multiplier accounts for:
- Posting metadata
- Index structures
- Bloom filters
- Operating overhead

### Disk Usage

Approximate disk usage calculation:
```
Disk ≈ Memory × 1.2
```

The multiplier accounts for:
- WAL files
- Metadata files
- Alignment padding

## Version Compatibility

Shardex follows semantic versioning:

- **Major version**: Breaking API changes
- **Minor version**: New features, backward compatible
- **Patch version**: Bug fixes, backward compatible

Index file formats are versioned separately and include migration support for minor version upgrades.
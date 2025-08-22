# Shardex Implementation Plan

## Project Overview
Shardex is a memory mapped vector search engine implemented in Rust. It supports incremental updating of index entries and document postings which are kept in parallel to the vectors. Indexes are arranged in shards, each of which consists of two memory mapped data structures: embedding vectors and postings. An index will have a dynamic number of shards. When an index starts, it has one shard. The main interface to use a Shardex is async. Each index can only be used by a single process.

## Architecture

### Core Components

#### Shards
- Each shard consists of two memory mapped data structures: embedding vectors and postings
- Each shard has a 128-bit identifier (ULID) for direct identification
- Configurable maximum number of entries (vectors paired with postings)
- Embedding vector size configured at creation time
- Represented by a centroid of the vectors of non-deleted postings
- Smart enough to only consider postings that have been made (may not be full)

#### Postings
- 128-bit user selected identifier (UUID or ULID typically)
- 32-bit starting location
- 32-bit length  
- Fixed size vector
- Deleted bit in metadata
- Can be Added or Removed
- Multiple postings per document ID supported

#### Write-Ahead Log (WAL)
- Transactional system with memory mapped log
- WAL is memory mapped, so it will need to be in fixed size segments
- Add/Remove postings stored in WAL first
- Entries copied from WAL to shard with nearest centroid
- May trigger shard splits
- WAL pointer advanced after successful flush
- Bloom filters included in WAL transactions
- Batch writing every n milliseconds for performance

#### Bloom Filter Index
- Memory mapped bloom filter index integrated into centroid index
- Stored as parallel vector to centroids and shard metadata
- Allows finding candidate shards given a document identifier
- Enables efficient deletion of all postings for a document

#### Shardex (The In-Memory Index)
- This in-memory index IS the Shardex - it keeps track of the shards
- Memory mapped so it can load directly into memory on startup
- Maintained as copy-on-write memory map for efficient updates
- Allows chunk-based resizing
- Parallel memory mapped structure with centroid vector parallel indexed to shard metadata vector
- Each shard identified by 128-bit ULID for direct shard access
- Contains memory mapped bloom filter based on document identifier kept in parallel vector to centroids and metadata
- When the index grows large enough to require expanding (from shard splits), a new segment is created

#### Search System
- K nearest neighbors search
- Uses the Shardex (in-memory index) to determine nearest shards
- Finding of near shard centroids can be done in parallel across segments
- Configurable 'slop' factor for number of near shards to scan
- Zero-copy operations
- Duplicate elimination in search results

### Data Flow

#### Write Operations
1. Postings added to WAL
2. Batch processing every n milliseconds
3. Copy from WAL to appropriate shard (nearest centroid)
4. Shard split if full (clustering into two half-full shards)
5. During split, check if each posting should remain in its new sub-shard or be reassigned to another existing shard with closer centroid
6. Handle cascading splits if reassigned postings cause other shards to overflow
7. Flush shards to disk
8. Advance WAL pointer
9. Update bloom filters

#### Read Operations
- Non-blocking (readers don't block batch writer)
- Search via vector similarity using shard centroids
- Return postings for K nearest neighbors
- Eliminate duplicates from results

#### Recovery
- On startup, recover from WAL
- Prone to duplicate postings
- Duplicate elimination during search and shard splits



## Technical Requirements

### Performance
- Zero-copy search operations
- Non-blocking reads during writes
- Batch processing for write optimization
- Memory mapped file access throughout

### Reliability
- ACID transactions via WAL
- Recovery from unexpected shutdowns
- Duplicate handling and elimination
- Data integrity verification

### Scalability
- Dynamic shard management
- Configurable shard sizes
- Efficient bloom filter lookups
- Optimized clustering algorithms

## Testing Strategy

### Unit Tests
- Individual component testing using TestEnvironment RAII blocks
- Each test uses isolated temporary directory for parallel execution
- #[serial] is strictly forbidden - all tests must run in parallel
- Memory mapping validation
- Vector operation correctness
- WAL transaction integrity

### Integration Tests
- End-to-end write/read cycles using TestEnvironment RAII blocks
- Shard splitting scenarios in isolated temporary directories
- Recovery testing with per-test environments
- Concurrent access patterns within single process

### Performance Tests
- Large dataset handling with TestEnvironment isolation
- Search performance benchmarks in temporary directories
- Memory usage optimization testing
- Load testing within single process constraints

## API Design & Interface

### Core Trait
```rust
#[async_trait]
pub trait Shardex {
    type Error;
    
    /// Create a new index with the given configuration
    async fn create(config: ShardexConfig) -> Result<Self, Self::Error>
    where
        Self: Sized;
    
    /// Open an existing index (configuration is read from metadata)
    async fn open<P: AsRef<Path>>(directory_path: P) -> Result<Self, Self::Error>
    where
        Self: Sized;
    
    /// Add a posting to the index
    async fn add_posting(&mut self, posting: Posting) -> Result<(), Self::Error>;
    
    /// Remove all postings for a document
    async fn remove_document(&mut self, document_id: u128) -> Result<(), Self::Error>;
    
    /// Search for K nearest neighbors
    async fn search(
        &self,
        query_vector: &[f32],
        k: usize,
        slop_factor: Option<usize>
    ) -> Result<Vec<SearchResult>, Self::Error>;
    
    /// Flush pending operations
    async fn flush(&mut self) -> Result<(), Self::Error>;
    
    /// Get index statistics
    async fn stats(&self) -> Result<IndexStats, Self::Error>;
}

pub struct Posting {
    pub document_id: u128,
    pub start: u32,
    pub length: u32,
    pub vector: Vec<f32>,
}

pub struct SearchResult {
    pub document_id: u128,
    pub start: u32,
    pub length: u32,
    pub vector: Vec<f32>,
    pub similarity_score: f32,
}

pub struct IndexStats {
    pub total_shards: usize,
    pub total_postings: usize,
    pub pending_operations: usize,
    pub memory_usage: usize,
}
```

### Error Types
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

### Configuration API
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

impl Default for ShardexConfig {
    fn default() -> Self {
        Self {
            directory_path: PathBuf::from("./shardex_index"),
            vector_size: 384,
            shard_size: 10000,
            shardex_segment_size: 1000,
            wal_segment_size: 1024 * 1024, // 1MB
            batch_write_interval_ms: 100,
            default_slop_factor: 3,
            bloom_filter_size: 1024,
        }
    }
}

impl ShardexConfig {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn directory_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.directory_path = path.into();
        self
    }
    
    pub fn vector_size(mut self, size: usize) -> Self {
        self.vector_size = size;
        self
    }
    
    pub fn shard_size(mut self, size: usize) -> Self {
        self.shard_size = size;
        self
    }
    
    pub fn shardex_segment_size(mut self, size: usize) -> Self {
        self.shardex_segment_size = size;
        self
    }
    
    pub fn wal_segment_size(mut self, size: usize) -> Self {
        self.wal_segment_size = size;
        self
    }
    
    pub fn batch_write_interval_ms(mut self, ms: u64) -> Self {
        self.batch_write_interval_ms = ms;
        self
    }
    
    pub fn default_slop_factor(mut self, factor: usize) -> Self {
        self.default_slop_factor = factor;
        self
    }
    
    pub fn bloom_filter_size(mut self, size: usize) -> Self {
        self.bloom_filter_size = size;
        self
    }
}
```

### Create vs Open Behavior
- `Shardex::create(config)`: Creates new index with specified configuration
  - If index already exists with matching configuration, effectively acts as `open`
  - If index exists with different configuration, returns error
- `Shardex::open(path)`: Opens existing index, reads configuration from metadata
  - Returns error if index doesn't exist

## Persistence & Storage

### File Layout
```
shardex_index/
├── shardex.meta          # Index metadata and configuration
├── centroids/            # Shardex segments (centroids + metadata + bloom filters)
│   ├── segment_000001.shx
│   ├── segment_000002.shx
│   └── ...
├── shards/              # Individual shard data
│   ├── {shard_ulid}.vectors
│   ├── {shard_ulid}.postings
│   └── ...
└── wal/                 # Write-ahead log segments
    ├── wal_000001.log
    ├── wal_000002.log
    └── ...
```

### Storage Format
- All files are fixed size given that they are memory mapped (shards, Shardex segments, and WAL)
- Direct representation memory maps - no serializing or deserializing
- Structures mapped directly from file bytes to memory
- Fixed-size layouts for direct pointer arithmetic
- Little-endian byte ordering for cross-platform compatibility
- Minimal headers with magic bytes and version information
- Data integrity via checksums but no complex serialization overhead

### Backup Procedures
- Copy-on-write enables consistent snapshots
- Incremental backup via WAL segment tracking
- Metadata backup for recovery coordination

## Monitoring & Observability

### Metrics Collection
- Search latency percentiles (p50, p95, p99)
- Throughput (operations/second)
- Shard distribution and balance
- Memory usage per component
- WAL segment utilization
- Bloom filter hit rates

### Logging Strategy
- Structured logging with trace/debug/info/warn/error levels
- Performance-sensitive paths use conditional compilation
- Async logging to prevent blocking operations
- Log rotation and retention policies

### Health Checks
- Index consistency verification
- Memory usage thresholds
- Disk space monitoring
- WAL replay capability

### Diagnostics
- Shard statistics and distribution analysis
- Vector quality metrics (centroid stability)
- Performance bottleneck identification

## Resource Management

### Memory Usage Bounds
- Configurable limits on in-memory structures
- Memory-mapped file size limits
- WAL buffer size constraints
- Search result set size limits

### File Descriptor Management
- Pool management for shard files
- Lazy loading and unloading strategies
- Operating system limits consideration
- Resource cleanup on shutdown

### Cleanup Procedures
- Automated garbage collection of deleted segments
- WAL segment archival and deletion
- Temporary file cleanup on crash recovery
- Memory deallocation strategies

## Edge Cases & Error Scenarios

### Corruption Detection
- Checksums on all data structures
- Magic number validation
- Structural integrity checks
- Cross-reference validation between components

### Recovery Procedures
- WAL replay with idempotency
- Partial corruption recovery strategies
- Fallback to previous consistent state
- Manual recovery tools and procedures

### Resource Exhaustion
- Out of disk space handling
- Memory pressure response
- File descriptor limit mitigation
- Network timeout handling

### Input Validation
- Vector dimension consistency checks
- Document ID format validation
- Parameter range validation
- Malformed data rejection

## Performance Tuning

### Benchmarking Methodology
- Synthetic workload generation
- Real-world usage pattern simulation
- Performance regression testing
- Memory usage profiling

### Optimization Strategies
- Memory layout optimization (cache-friendly access)
- SIMD vector operations where applicable
- Prefetching strategies for memory-mapped files
- Batch operation optimization

### Tuning Parameters
- Shard size vs search performance trade-offs
- Bloom filter false positive rate tuning
- WAL batch size optimization
- Centroid update frequency

## Deployment Considerations

### Single Process Design
- Each index can only be used by a single process
- File locking to prevent multiple process access
- Process crash recovery for orphaned locks
- No shared memory coordination needed

### Hot Backup Procedures
- Online backup without service interruption
- Consistent snapshot creation
- Backup validation and testing
- Restore time optimization

### Version Migration
- Backward compatibility strategies
- Data format migration tools
- Rolling upgrade procedures
- Rollback capabilities

### Production Readiness
- Load testing procedures
- Capacity planning guidelines
- Monitoring and alerting setup
- Operational runbooks

## Configuration Parameters

### Shard Configuration
- Maximum entries per shard
- Embedding vector dimensions
- Splitting threshold

### Performance Tuning
- Batch write interval (milliseconds)
- Search slop factor
- Bloom filter size parameters

### System Parameters
- WAL size limits
- Recovery timeout settings
- Memory mapping options

---
*Last updated: 2025-08-22*
# Shardex Architecture Overview

This document provides a comprehensive overview of Shardex's internal architecture, data structures, and design decisions.

## High-Level Architecture

Shardex is built around a sharded architecture where data is distributed across multiple shards, each optimized for vector similarity search. The system uses memory-mapped files for zero-copy operations and provides ACID transactions through a write-ahead log (WAL).

```
┌─────────────────────────────────────────────────────────────┐
│                        Shardex API                          │
├─────────────────────────────────────────────────────────────┤
│                   Search Coordinator                        │
├─────────────────────────────────────────────────────────────┤
│              ShardexIndex (In-Memory Index)                 │
├─────────────────────────────────────────────────────────────┤
│    Shard 1     │    Shard 2     │    Shard 3     │    ...  │
│  ┌─────────────┼─────────────────┼─────────────────┼───────  │
│  │ Vectors     │    Vectors     │    Vectors     │         │
│  │ Postings    │    Postings    │    Postings    │         │
│  │ Centroid    │    Centroid    │    Centroid    │         │
│  └─────────────┴─────────────────┴─────────────────┴───────  │
├─────────────────────────────────────────────────────────────┤
│                    Write-Ahead Log                          │
├─────────────────────────────────────────────────────────────┤
│                   Memory-Mapped Files                       │
└─────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Shards

Shards are the fundamental storage units in Shardex. Each shard contains:

#### Vector Storage
- Memory-mapped file storing f32 vectors
- Fixed-size vectors (dimension specified at index creation)
- Contiguous memory layout for cache efficiency
- Automatic alignment for SIMD operations

#### Posting Storage
- Memory-mapped file storing document metadata
- Each posting contains:
  - Document ID (128-bit ULID)
  - Start position (32-bit)
  - Length (32-bit)
  - Deletion marker

#### Shard Metadata
- Centroid vector (represents the "center" of all vectors in the shard)
- Current capacity and utilization
- Creation timestamp and statistics

### 2. ShardexIndex (In-Memory Index)

The ShardexIndex is the central coordinator that tracks all shards:

```rust
pub struct ShardexIndex {
    centroids: Vec<Vec<f32>>,       // Shard centroids for search
    metadata: Vec<ShardMetadata>,   // Shard information
    bloom_filters: Vec<BloomFilter>, // Document existence filters
    config: ShardexConfig,          // Index configuration
}
```

Key responsibilities:
- **Shard Selection**: Find the best shards for a given query vector
- **Load Balancing**: Distribute new postings across shards
- **Split Management**: Handle shard splits when capacity is exceeded
- **Bloom Filter Management**: Optimize document deletion operations

### 3. Document Text Storage Architecture

Document text storage is implemented at the index level (not shard level) using append-only memory-mapped files for efficient access and ACID transactions.

#### File Layout

When text storage is enabled (`max_document_text_size > 0`), additional files are created:

```
index_directory/
├── text_index.dat     # Document text index entries
├── text_data.dat      # Raw document text data
├── shards/            # Vector postings (unchanged)
│   ├── {shard_ulid}.vectors
│   ├── {shard_ulid}.postings
│   └── ...
├── centroids/         # Shardex segments (unchanged)
│   └── ...
└── wal/              # Write-ahead log (includes text operations)
    └── ...
```

#### Text Storage Components

##### Text Index File (`text_index.dat`)
Memory-mapped file containing index entries for document text lookup:

```rust
#[repr(C)]
struct TextIndexHeader {
    magic: [u8; 4],           // "SIDX" - Shardex Index
    version: u32,             // Format version
    entry_count: u64,         // Number of index entries
    max_document_size: u64,   // Maximum allowed document size
    checksum: u32,            // Header integrity check
}

#[repr(C)]
struct DocumentTextEntry {
    document_id: DocumentId,  // 128-bit document identifier
    text_offset: u64,         // Byte offset in text_data.dat
    text_length: u64,         // Length of text in bytes
    timestamp: u64,           // Creation timestamp
}
```

##### Text Data File (`text_data.dat`)
Append-only file containing raw document text:

```rust
#[repr(C)]
struct TextDataHeader {
    magic: [u8; 4],           // "SDAT" - Shardex Data
    version: u32,             // Format version
    total_size: u64,          // Total bytes of text data
    document_count: u64,      // Number of documents stored
    checksum: u32,            // Header integrity check
}

// Followed by raw text data with no delimiters
// Offsets and lengths from index entries define boundaries
```

#### Memory Mapping Strategy

Both text files use memory mapping for performance:

- **Text Index**: O(n) backward search to find latest document versions
- **Text Data**: O(1) access after index lookup using memory offsets
- **OS Page Cache**: Leverages operating system page caching for hot data
- **Prefaulting**: Strategic page loading to minimize page faults during search

#### Transaction Coordination

Document text operations are fully integrated with Shardex's WAL system:

```rust
// Extended WAL operations for text storage
pub enum WalOperation {
    AddPosting { shard_id: ShardId, posting: Posting },
    RemoveDocument { document_id: DocumentId },
    SplitShard { old_shard: ShardId, new_shards: Vec<ShardId> },
    
    // Text storage operations
    StoreDocumentText { 
        document_id: DocumentId, 
        text_offset: u64, 
        text_length: u64 
    },
    ReplaceDocumentWithPostings { 
        document_id: DocumentId, 
        text_offset: u64, 
        text_length: u64,
        postings: Vec<Posting> 
    },
}
```

#### Atomic Replacement Workflow

The `replace_document_with_postings` operation provides ACID guarantees:

```
Replace Document Request
        ↓
1. Validate Text Size (vs max_document_text_size)
        ↓
2. Create WAL Transaction
        ↓
3. Append Text to text_data.dat
        ↓
4. Create Text Index Entry
        ↓
5. Remove Old Document Postings
        ↓
6. Add New Document Postings
        ↓
7. Append Text Index Entry
        ↓
8. Commit WAL Transaction
        ↓
Response to Client
```

#### Performance Characteristics

- **Text Storage**: O(n) space where n is total text size across all document versions
- **Text Lookup**: O(d) time where d is number of document versions (typically small)
- **Text Extraction**: O(1) time after lookup (memory-mapped access)  
- **Document Versioning**: Multiple versions stored until compaction
- **Concurrency**: Multiple readers, single writer per transaction

#### Error Handling and Recovery

Text storage includes specific error handling:

```rust
pub enum TextStorageError {
    DocumentTextNotFound { document_id: DocumentId },
    InvalidRange { start: u32, length: u32, document_length: u64 },
    DocumentTooLarge { size: usize, max_size: usize },
    TextCorruption { details: String },
}
```

Recovery procedures:
1. **Index Validation**: Verify text index entries are consistent
2. **Data Validation**: Check text data integrity using offsets
3. **Cross-Reference**: Ensure index entries point to valid data ranges
4. **Corruption Isolation**: Continue operation with valid entries

#### Backward Compatibility

Text storage is designed for full backward compatibility:
- Indexes without text storage continue to work unchanged
- Text storage is opt-in via `max_document_text_size` configuration
- No changes to existing vector search APIs
- Text methods return appropriate errors when storage is disabled

### 4. Write-Ahead Log (WAL)

The WAL provides ACID guarantees and crash recovery:

#### Transaction Structure
```rust
pub struct WalTransaction {
    id: TransactionId,
    operations: Vec<WalOperation>,
    timestamp: SystemTime,
    checksum: u32,
}

pub enum WalOperation {
    AddPosting { shard_id: ShardId, posting: Posting },
    RemoveDocument { document_id: DocumentId },
    SplitShard { old_shard: ShardId, new_shards: Vec<ShardId> },
}
```

#### WAL Workflow
1. **Log First**: Operations are written to WAL before execution
2. **Batch Processing**: Operations are batched for efficiency  
3. **Apply to Shards**: Successful WAL write triggers shard updates
4. **Commit**: WAL pointer is advanced after successful shard updates
5. **Cleanup**: Old WAL segments are archived/deleted

### 4. Search Coordinator

The search coordinator orchestrates multi-shard searches:

#### Search Process
1. **Query Preprocessing**: Normalize query vector
2. **Shard Selection**: Find candidate shards using centroids
3. **Parallel Search**: Search selected shards concurrently
4. **Result Merging**: Combine and rank results
5. **Deduplication**: Remove duplicate documents
6. **Top-K Selection**: Return the best k results

#### Distance Metrics
- **Cosine Similarity** (default): Good for normalized vectors
- **Euclidean Distance**: Good for spatial data
- **Dot Product**: Good for magnitude-sensitive comparisons

## Data Flow

### Write Operations

#### Standard Posting Operations
```
Add Postings Request
        ↓
1. Validate Input (dimensions, format)
        ↓
2. Create WAL Transaction
        ↓
3. Write to WAL (with checksum)
        ↓
4. Select Target Shards (using centroids)
        ↓
5. Update Shard Data (vectors + postings)
        ↓
6. Update Shard Centroids
        ↓
7. Check for Shard Splits
        ↓
8. Update Bloom Filters
        ↓
9. Commit WAL Transaction
        ↓
Response to Client
```

#### Atomic Document Replacement (with Text)
```
Replace Document with Postings Request
        ↓
1. Validate Input (text size, dimensions, format)
        ↓
2. Create WAL Transaction
        ↓
3. Write to WAL (with checksum)
        ↓
4. Append Text to text_data.dat
        ↓
5. Remove Old Document from All Shards
        ↓  
6. Select Target Shards for New Postings
        ↓
7. Update Shard Data (vectors + postings)
        ↓
8. Update Shard Centroids
        ↓
9. Append Text Index Entry
        ↓
10. Update Bloom Filters
        ↓
11. Check for Shard Splits
        ↓
12. Commit WAL Transaction
        ↓
Response to Client
```

### Read Operations

#### Vector Search Operations
```
Search Request
        ↓
1. Validate Query Vector
        ↓
2. Calculate Shard Similarities
        ↓
3. Select Top Shards (using slop factor)
        ↓
4. Parallel Shard Search
        ↓
5. Collect Results
        ↓
6. Deduplicate Documents
        ↓
7. Sort by Similarity
        ↓
8. Return Top K Results
```

#### Text Retrieval Operations
```
Get Document Text Request
        ↓
1. Validate Document ID
        ↓
2. Search Text Index (backward scan)
        ↓
3. Find Latest Entry for Document
        ↓
4. Memory-Mapped Access to Text Data
        ↓
5. Return Text String

Extract Text Request (from Posting)
        ↓
1. Validate Posting Coordinates
        ↓
2. Get Document Text (as above)
        ↓
3. Validate Range (start + length ≤ document length)
        ↓
4. Extract Substring
        ↓
5. Return Text Snippet
```

## File Layout and Storage

### Directory Structure
```
index_directory/
├── shardex.meta              # Index metadata and configuration
├── text_index.dat            # Document text index (if text storage enabled)
├── text_data.dat             # Document text data (if text storage enabled)
├── centroids/                # Shardex segments
│   ├── segment_000001.shx    # Centroids + metadata + bloom filters
│   ├── segment_000002.shx
│   └── ...
├── shards/                   # Individual shard data
│   ├── {shard_ulid}.vectors  # Vector storage
│   ├── {shard_ulid}.postings # Posting storage
│   └── ...
└── wal/                      # Write-ahead log
    ├── wal_000001.log
    ├── wal_000002.log
    └── ...
```

### Memory Mapping Strategy

All data structures are designed for direct memory mapping:

```rust
// Example: Vector storage header
#[repr(C)]
struct VectorStorageHeader {
    magic: [u8; 4],          // File format identifier
    version: u32,            // Format version
    vector_size: u32,        // Dimensions per vector
    capacity: u32,           // Maximum vectors in this file
    count: u32,              // Current number of vectors
    checksum: u32,           // Data integrity check
}
```

Benefits:
- **Zero-copy access**: Data is used directly from mapped memory
- **Fast startup**: No deserialization required
- **Memory efficiency**: OS manages paging automatically
- **Concurrent reads**: Multiple readers can access safely

## Shard Management

### Shard Splitting Algorithm

When a shard reaches capacity, it splits using k-means clustering:

1. **Trigger**: Shard reaches maximum capacity
2. **Clustering**: Apply k-means (k=2) to split vectors
3. **Redistribution**: Move vectors to appropriate new shards
4. **Centroid Update**: Recalculate centroids for new shards
5. **Index Update**: Update ShardexIndex with new shard information
6. **Cleanup**: Remove old shard files

### Centroid Calculation

Shard centroids are calculated as the mean of all non-deleted vectors:

```rust
fn calculate_centroid(vectors: &[Vec<f32>], deleted_mask: &BitVec) -> Vec<f32> {
    let active_vectors: Vec<_> = vectors.iter()
        .enumerate()
        .filter(|(i, _)| !deleted_mask[*i])
        .map(|(_, v)| v)
        .collect();
    
    if active_vectors.is_empty() {
        return vec![0.0; vectors[0].len()];
    }
    
    let mut centroid = vec![0.0; active_vectors[0].len()];
    for vector in active_vectors.iter() {
        for (i, &value) in vector.iter().enumerate() {
            centroid[i] += value;
        }
    }
    
    let count = active_vectors.len() as f32;
    for value in centroid.iter_mut() {
        *value /= count;
    }
    
    centroid
}
```

## Bloom Filters

Bloom filters accelerate document deletion and existence checks:

### Structure
- One bloom filter per shard
- Configurable size (trade-off between memory and accuracy)
- Stores document IDs for fast existence checks

### Usage
- **Document Deletion**: Quickly identify which shards might contain a document
- **Search Optimization**: Skip shards that definitely don't contain target documents
- **Batch Operations**: Efficiently process large deletion batches

## Concurrency Model

### Read-Write Coordination

Shardex uses a copy-on-write strategy for concurrent access:

```rust
// Simplified concurrency model
pub struct ConcurrentShardex {
    current_index: Arc<ShardexIndex>,        // Current readable state
    write_lock: Mutex<()>,                   // Exclusive write access
    pending_writes: Arc<Mutex<Vec<Operation>>>, // Batched operations
}
```

### Safety Guarantees
- **Readers never block**: Always access consistent snapshots
- **Writers are serialized**: One write operation at a time
- **Atomic updates**: All changes are applied atomically
- **Crash recovery**: WAL ensures consistency after crashes

## Performance Characteristics

### Time Complexity
- **Search**: O(log S + k * V) where S = shards, V = vectors per shard, k = slop factor
- **Insert**: O(log S) for shard selection + O(1) for insertion
- **Delete**: O(S) for bloom filter checks + O(V) for actual deletion

### Space Complexity
- **Vector Storage**: O(N * D) where N = documents, D = dimensions
- **Index Overhead**: O(S * D) for centroids + O(S) for metadata
- **WAL**: O(pending operations) bounded by configuration

### Optimization Strategies
- **SIMD Instructions**: Vector operations use SIMD when available
- **Cache-Friendly Layout**: Data structures optimized for CPU cache
- **Memory Prefetching**: Proactive memory loading for search operations
- **Batch Processing**: Reduce syscall overhead through batching

## Configuration Trade-offs

### Shard Size
- **Large shards**: Fewer splits, more memory per search, potentially slower searches
- **Small shards**: More splits, less memory per search, potentially faster searches

### Slop Factor
- **Low values**: Faster searches, potentially lower accuracy
- **High values**: Slower searches, higher accuracy

### Bloom Filter Size
- **Large filters**: Lower false positive rate, more memory usage
- **Small filters**: Higher false positive rate, less memory usage

### Batch Interval
- **Short intervals**: Lower latency, higher CPU overhead
- **Long intervals**: Higher latency, better throughput

## Error Handling and Recovery

### Corruption Detection
- **Checksums**: All data structures include integrity checks
- **Magic Numbers**: File format validation
- **Structural Validation**: Cross-reference checks between components

### Recovery Procedures
1. **WAL Replay**: Reconstruct state from write-ahead log
2. **Shard Validation**: Verify individual shard integrity
3. **Index Reconstruction**: Rebuild centroids and bloom filters if needed
4. **Partial Recovery**: Continue with valid shards if some are corrupted

## Future Architecture Considerations

### Scalability Enhancements
- **Distributed Sharding**: Scale beyond single-machine limits
- **Hierarchical Clustering**: Multi-level shard organization
- **Adaptive Splitting**: Dynamic shard sizing based on usage patterns

### Performance Optimizations
- **GPU Acceleration**: Offload vector operations to GPU
- **Compressed Vectors**: Reduce memory usage with quantization
- **Asynchronous I/O**: Non-blocking disk operations

### Feature Additions
- **Multiple Distance Metrics**: Support for custom similarity functions
- **Filtered Search**: Attribute-based result filtering
- **Incremental Backup**: Efficient backup and replication
# Shardex Implementation Plan

## Project Overview
Shardex is a memory mapped vector search engine implemented in Rust. It supports incremental updating of index entries and document postings which are kept in parallel to the vectors. Indexes are arranged in shards, each of which consists of two memory mapped data structures: embedding vectors and postings. An index will have a dynamic number of shards. When an index starts, it has one shard. The main interface to use a Shardex is async.

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

## Implementation Phases

### Phase 1: Core Data Structures
- [ ] Design and implement posting structure
- [ ] Create memory mapped shard structures
- [ ] Implement basic vector operations
- [ ] Create shard centroid calculation

### Phase 2: Write-Ahead Logging
- [ ] Implement WAL structure and operations
- [ ] Create batch writing system
- [ ] Add transaction management
- [ ] Implement recovery mechanism

### Phase 3: Shardex (In-Memory Index) and Search Infrastructure
- [ ] Implement copy-on-write memory mapped Shardex structure
- [ ] Create parallel centroid vector, shard metadata, and bloom filter structures
- [ ] Add chunk-based resizing for Shardex with new segment creation
- [ ] Handle index expansion triggered by shard splits
- [ ] Integrate bloom filter into parallel vector architecture
- [ ] Enable direct memory loading on startup
- [ ] Implement K-nearest neighbor search
- [ ] Add centroid-based shard selection using the Shardex
- [ ] Enable parallel search across Shardex segments
- [ ] Create slop factor configuration
- [ ] Ensure zero-copy operations

### Phase 4: Dynamic Sharding
- [ ] Implement shard splitting algorithm
- [ ] Add clustering for split decisions
- [ ] Handle shard management and indexing
- [ ] Implement posting reassignment during splits (check if postings should move to other existing shards with closer centroids)
- [ ] Handle cascading splits when reassigned postings cause other shards to overflow
- [ ] Optimize shard rebalancing

### Phase 5: Bloom Filter Integration
- [ ] Design bloom filter structure
- [ ] Integrate with WAL transactions
- [ ] Implement document ID lookup
- [ ] Add efficient deletion by document ID

### Phase 6: Concurrency and Performance
- [ ] Implement async interface for Shardex operations
- [ ] Implement non-blocking readers
- [ ] Optimize batch writing performance
- [ ] Add configurable timing parameters
- [ ] Handle duplicate elimination efficiently

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
- Individual component testing
- Memory mapping validation
- Vector operation correctness
- WAL transaction integrity

### Integration Tests
- End-to-end write/read cycles
- Shard splitting scenarios
- Recovery testing
- Concurrent access patterns

### Performance Tests
- Large dataset handling
- Search performance benchmarks
- Memory usage optimization
- Concurrent load testing

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
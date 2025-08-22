# Step 22: Bloom Filter Implementation for Document IDs

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement bloom filter functionality for efficient document ID lookups across shards.

## Tasks
- Create bloom filter data structure with configurable size
- Implement hash functions for document ID insertion and testing
- Add serialization support for memory-mapped storage
- Support bloom filter merging for shard operations
- Include false positive rate configuration and monitoring

## Acceptance Criteria
- [ ] Bloom filter efficiently tests document ID membership
- [ ] Configurable size balances memory usage and false positive rate
- [ ] Hash functions provide good distribution for ULIDs
- [ ] Serialization works with memory-mapped files
- [ ] Tests verify correctness and false positive rates
- [ ] Performance meets requirements for high-frequency operations

## Technical Details
```rust
pub struct BloomFilter {
    bit_array: Vec<u64>,    // Memory-mapped bitset
    hash_functions: usize,  // Number of hash functions
    capacity: usize,        // Expected number of elements
    inserted_count: usize,  // Actual number of insertions
}

impl BloomFilter {
    pub fn new(capacity: usize, false_positive_rate: f64) -> Self;
    pub fn insert(&mut self, document_id: DocumentId);
    pub fn contains(&self, document_id: DocumentId) -> bool;
    pub fn merge(&mut self, other: &BloomFilter) -> Result<(), ShardexError>;
}
```

Use multiple hash functions (typically 3-5) for optimal false positive rate.
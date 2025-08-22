# Step 7: Memory-Mapped Posting Storage

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement fixed-size memory-mapped storage for posting metadata with efficient lookups.

## Tasks
- Create PostingStorage struct for posting metadata arrays
- Implement storage for document IDs, starts, lengths, and deleted flags
- Add efficient posting lookup and iteration
- Support posting addition, removal, and updates
- Include deleted bit tracking in metadata

## Acceptance Criteria
- [ ] PostingStorage handles fixed-size posting arrays
- [ ] Efficient lookup by index and document ID
- [ ] Deleted bit tracking prevents accessing removed postings
- [ ] Storage aligns with VectorStorage for parallel access
- [ ] Tests verify posting operations and deletion handling

## Technical Details
```rust
pub struct PostingStorage {
    document_ids: Vec<DocumentId>,  // Memory-mapped array
    starts: Vec<u32>,              // Memory-mapped array  
    lengths: Vec<u32>,             // Memory-mapped array
    deleted_flags: Vec<bool>,      // Memory-mapped bitset
    capacity: usize,
    current_count: usize,
}
```

Ensure postings align with vectors for parallel iteration and access.
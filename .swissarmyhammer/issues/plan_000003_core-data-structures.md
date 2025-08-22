# Step 3: Core Data Structures

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement the core data structures (Posting, SearchResult, IndexStats) that form the API surface.

## Tasks
- Implement `Posting` struct with memory-mapped layout
- Implement `SearchResult` struct for search responses
- Implement `IndexStats` for monitoring and observability
- Add proper validation for vector dimensions
- Ensure structures are memory-map compatible

## Acceptance Criteria
- [ ] Posting struct supports document ID, start, length, and vector
- [ ] SearchResult includes similarity scores
- [ ] IndexStats provides comprehensive metrics
- [ ] All structures support zero-copy serialization
- [ ] Vector dimension validation prevents mismatched data
- [ ] Tests cover structure creation and validation

## Technical Details
```rust
pub struct Posting {
    pub document_id: DocumentId,
    pub start: u32,
    pub length: u32, 
    pub vector: Vec<f32>,
}

pub struct SearchResult {
    pub document_id: DocumentId,
    pub start: u32,
    pub length: u32,
    pub vector: Vec<f32>,
    pub similarity_score: f32,
}
```

Structures must be compatible with bytemuck for direct memory mapping.
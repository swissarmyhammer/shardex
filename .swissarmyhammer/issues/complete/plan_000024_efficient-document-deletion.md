# Step 24: Efficient Document Deletion Support

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement efficient document deletion across all shards using bloom filter optimization.

## Tasks
- Create document deletion algorithm using bloom filter pre-filtering
- Implement parallel deletion across candidate shards
- Add deletion result tracking and reporting
- Support batch deletion for multiple documents
- Include centroid updates after deletions

## Acceptance Criteria
- [ ] Bloom filters eliminate unnecessary shard scans
- [ ] Parallel deletion improves performance for multi-shard documents
- [ ] Deletion results report actual removed postings count
- [ ] Batch deletion processes multiple documents efficiently
- [ ] Tests verify deletion correctness and performance
- [ ] Centroids are updated properly after deletions

## Technical Details
```rust
impl ShardexIndex {
    pub async fn delete_document(&mut self, doc_id: DocumentId) -> Result<usize, ShardexError>;
    pub async fn delete_documents(&mut self, doc_ids: &[DocumentId]) -> Result<Vec<usize>, ShardexError>;
    
    fn find_candidate_shards_for_deletion(&self, doc_id: DocumentId) -> Vec<ShardId>;
}
```

Use bloom filter false positive handling and maintain statistics on deletion efficiency.

## Proposed Solution

After analyzing the codebase, I will implement efficient document deletion across all shards using bloom filter optimization and parallel processing. The design leverages existing infrastructure while adding the missing high-level deletion methods.

### Implementation Steps:

1. **Add deletion methods to ShardexIndex**:
   - `delete_document(&mut self, doc_id: DocumentId) -> Result<usize, ShardexError>`
   - `delete_documents(&mut self, doc_ids: &[DocumentId]) -> Result<Vec<usize>, ShardexError>`
   - `find_candidate_shards_for_deletion(&self, doc_id: DocumentId) -> Vec<ShardId>`

2. **Bloom Filter Optimization Strategy**:
   - Use existing bloom filters in each shard to pre-filter candidates
   - Only search shards where bloom filter indicates the document might exist
   - This eliminates unnecessary scans of shards that definitely don't contain the document

3. **Parallel Deletion Process**:
   - For each document, identify candidate shards using bloom filters  
   - Execute deletion across candidate shards in parallel using rayon
   - Collect and sum deletion counts from each shard
   - Update centroids automatically (already handled by existing shard.remove_document())

4. **Batch Deletion Efficiency**:
   - Group deletions by candidate shards to minimize shard loading
   - Process multiple documents per shard in single operation where beneficial
   - Maintain deletion count tracking per document for reporting

5. **Centroid Updates**:
   - Leverage existing `update_centroid_remove()` in Shard (already implemented)
   - Centroids are updated automatically as part of shard.remove_document()
   - No additional centroid logic needed at ShardexIndex level

6. **Error Handling and Robustness**:
   - Handle missing documents gracefully (return 0 count)
   - Continue processing other documents if one fails
   - Provide detailed error information for debugging

### Key Benefits:
- **Performance**: Bloom filters eliminate scans of irrelevant shards
- **Scalability**: Parallel processing across candidate shards
- **Accuracy**: Returns actual count of deleted postings per document
- **Consistency**: Centroids automatically updated for all affected shards
- **Robustness**: Graceful handling of edge cases and errors

This approach builds on the existing architecture and maximizes efficiency by only targeting shards that could possibly contain each document.
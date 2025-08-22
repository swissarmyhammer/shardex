# Step 26: Multi-Shard Search Coordination

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement coordination of search operations across multiple shards with proper result aggregation.

## Tasks
- Create search coordinator for managing multi-shard queries
- Implement result aggregation and deduplication
- Add load balancing and parallel execution
- Support search cancellation and timeouts
- Include search performance monitoring

## Acceptance Criteria
- [ ] Search coordinator efficiently manages multi-shard queries
- [ ] Result aggregation maintains correct ranking order
- [ ] Deduplication handles multiple postings per document
- [ ] Parallel execution improves search performance
- [ ] Tests verify coordination correctness and performance
- [ ] Search timeouts prevent hanging operations

## Technical Details
```rust
pub struct SearchCoordinator {
    shards: HashMap<ShardId, Shard>,
    index: ShardexIndex,
}

impl SearchCoordinator {
    pub async fn coordinate_search(
        &self,
        query: &[f32],
        k: usize,
        slop_factor: usize,
        timeout: Duration
    ) -> Result<Vec<SearchResult>, ShardexError>;
    
    fn aggregate_results(&self, results: Vec<Vec<SearchResult>>, k: usize) -> Vec<SearchResult>;
}
```

Use timeout mechanisms and result streaming for large result sets.
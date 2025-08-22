# Step 25: K-Nearest Neighbors Search

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement complete K-nearest neighbors search functionality with configurable parameters.

## Tasks
- Create KNN search algorithm using centroid-based shard selection
- Implement configurable k parameter and slop factor
- Add similarity score calculation and ranking
- Support different distance metrics (cosine, euclidean)
- Include search result validation and filtering

## Acceptance Criteria
- [ ] KNN search returns exactly k results (or fewer if unavailable)
- [ ] Slop factor controls trade-off between accuracy and performance
- [ ] Similarity scores are computed correctly
- [ ] Multiple distance metrics are supported
- [ ] Tests verify search accuracy and performance
- [ ] Search handles edge cases (empty index, k > total vectors)

## Technical Details
```rust
pub enum DistanceMetric {
    Cosine,
    Euclidean,
    DotProduct,
}

impl Shardex {
    pub async fn search(
        &self,
        query_vector: &[f32],
        k: usize,
        slop_factor: Option<usize>
    ) -> Result<Vec<SearchResult>, ShardexError>;
    
    pub async fn search_with_metric(
        &self,
        query_vector: &[f32],
        k: usize,
        metric: DistanceMetric,
        slop_factor: Option<usize>
    ) -> Result<Vec<SearchResult>, ShardexError>;
}
```

Use a min-heap for efficient top-k selection and include similarity score normalization.
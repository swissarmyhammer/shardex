# Step 12: Shard Centroid Calculation and Updates

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement centroid calculation and maintenance for shards to enable efficient shard selection.

## Tasks
- Implement centroid calculation from non-deleted vectors
- Add incremental centroid updates on vector addition/removal
- Support centroid recalculation for accuracy
- Include centroid persistence and loading
- Add centroid validation and error handling

## Acceptance Criteria
- [ ] Centroid accurately represents non-deleted vectors
- [ ] Incremental updates maintain centroid accuracy
- [ ] Periodic recalculation corrects drift
- [ ] Centroid persistence survives shard restarts
- [ ] Tests verify centroid accuracy and updates
- [ ] Performance is optimized for frequent updates

## Technical Details
```rust
impl Shard {
    pub fn calculate_centroid(&self) -> Vec<f32>;
    pub fn update_centroid_add(&mut self, vector: &[f32]);
    pub fn update_centroid_remove(&mut self, vector: &[f32]);
    pub fn recalculate_centroid(&mut self);
    pub fn get_centroid(&self) -> &[f32];
}
```

Use incremental mean calculation for efficiency:
```
new_centroid = old_centroid + (new_vector - old_centroid) / count
```
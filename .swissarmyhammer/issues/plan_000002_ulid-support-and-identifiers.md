# Step 2: ULID Support and Identifiers

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement ULID support for shard and document identifiers as specified in the architecture.

## Tasks
- Add ulid crate dependency to Cargo.toml
- Create wrapper types for shard and document identifiers
- Implement proper serialization/deserialization
- Add identifier generation utilities
- Create type-safe identifier handling

## Acceptance Criteria
- [ ] ShardId and DocumentId wrapper types prevent mixing identifiers
- [ ] ULID generation is properly implemented
- [ ] Identifiers can be serialized to/from bytes for memory mapping
- [ ] Type safety prevents using document IDs as shard IDs
- [ ] Tests verify identifier uniqueness and ordering

## Technical Details
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShardId(u128);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]  
pub struct DocumentId(u128);
```

Ensure identifiers are 128-bit and support memory mapping through bytemuck traits.
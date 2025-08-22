# Step 1: Error Types and Configuration

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Create the foundational error types and configuration structures for Shardex.

## Tasks
- Implement `ShardexError` enum with proper error chain support
- Create `ShardexConfig` struct with builder pattern
- Add validation for configuration parameters
- Set up basic project structure with `lib.rs`

## Acceptance Criteria
- [ ] `ShardexError` enum covers all major error categories
- [ ] `ShardexConfig` has sensible defaults and validation
- [ ] Configuration can be created via builder pattern
- [ ] All error types implement proper Debug and Display traits
- [ ] Tests verify configuration validation works

## Technical Details
```rust
#[derive(Debug, Error)]
pub enum ShardexError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidDimension { expected: usize, actual: usize },
    #[error("Index corruption detected: {0}")]
    Corruption(String),
    #[error("Configuration error: {0}")]
    Config(String),
}
```

Use thiserror for error handling and ensure comprehensive test coverage.
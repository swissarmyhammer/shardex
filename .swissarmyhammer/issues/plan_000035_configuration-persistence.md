# Step 35: Configuration Persistence

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement configuration persistence and loading to maintain index settings across restarts.

## Tasks
- Create configuration serialization and deserialization
- Implement metadata file management for configuration storage
- Add configuration validation and migration support
- Support configuration updates and versioning
- Include configuration backup and recovery

## Acceptance Criteria
- [ ] Configuration is persisted durably in metadata files
- [ ] Loading validates configuration consistency
- [ ] Migration handles configuration format changes
- [ ] Updates maintain configuration integrity
- [ ] Tests verify persistence and loading correctness
- [ ] Backward compatibility is maintained where possible

## Technical Details
```rust
pub struct PersistedConfig {
    version: u32,
    config: ShardexConfig,
    created_at: SystemTime,
    modified_at: SystemTime,
    checksum: u32,
}

impl PersistedConfig {
    pub async fn save(&self, path: &Path) -> Result<(), ShardexError>;
    pub async fn load(path: &Path) -> Result<Self, ShardexError>;
    pub fn validate_compatibility(&self, other: &PersistedConfig) -> Result<(), ShardexError>;
}
```

Use JSON or binary serialization with checksums for integrity and include atomic file updates.
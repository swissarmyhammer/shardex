# Step 33: Index Creation and Opening

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement complete index creation and opening functionality with proper validation and configuration handling.

## Tasks
- Implement Shardex::create() for new index creation
- Implement Shardex::open() for existing index loading
- Add configuration validation and compatibility checking
- Support index migration for version differences
- Include proper error handling and recovery

## Acceptance Criteria
- [ ] create() initializes new indexes with proper structure
- [ ] open() loads existing indexes and validates configuration
- [ ] Configuration compatibility prevents mismatched access
- [ ] Version migration handles format changes gracefully
- [ ] Tests verify creation, opening, and error scenarios
- [ ] Error messages provide actionable guidance

## Technical Details
```rust
impl Shardex {
    pub async fn create(config: ShardexConfig) -> Result<Self, ShardexError> {
        // 1. Validate configuration
        // 2. Create directory structure
        // 3. Initialize metadata files
        // 4. Create initial empty shard
        // 5. Set up WAL system
        // 6. Return ready instance
    }
    
    pub async fn open<P: AsRef<Path>>(directory_path: P) -> Result<Self, ShardexError> {
        // 1. Validate directory structure
        // 2. Load and validate metadata
        // 3. Check version compatibility
        // 4. Load existing shards
        // 5. Replay WAL for recovery
        // 6. Return ready instance
    }
}
```

Include comprehensive validation and atomic initialization procedures.
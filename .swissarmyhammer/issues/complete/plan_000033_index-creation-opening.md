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
## Proposed Solution

Based on analysis of the existing codebase, I will enhance the existing `Shardex::create()` and `Shardex::open()` methods with the following improvements:

### Enhanced Create Method
1. **Atomic Directory Creation**: Ensure all directories are created atomically or rolled back on failure
2. **Configuration Validation**: Validate all config parameters before any file system operations
3. **Metadata Initialization**: Create and save initial metadata with proper versioning
4. **Initial Shard Setup**: Create initial empty shard structure if needed
5. **WAL System Initialization**: Set up clean WAL state for new indexes
6. **Error Recovery**: Clean up partial initialization on any failure

### Enhanced Open Method  
1. **Directory Structure Validation**: Verify complete directory structure exists
2. **Metadata Loading and Validation**: Load existing metadata and validate compatibility
3. **Configuration Compatibility**: Check vector dimensions, directory path, and other critical settings
4. **Version Migration**: Handle format version differences with proper migration
5. **WAL Recovery**: Replay any pending WAL operations for consistency
6. **Shard Loading**: Load and validate all existing shards

### Configuration Compatibility Rules
- Vector size must match exactly (affects file format)
- Directory path must match (affects file locations)
- Layout version must be supported (affects file structure)
- Critical parameters like shard size should be compatible

### Error Scenarios with Clear Messages
- Directory doesn't exist or is inaccessible
- Configuration incompatibility with actionable guidance
- Corruption detection with recovery suggestions
- Version migration failures with rollback

### Implementation Approach
1. Enhance existing methods rather than rewriting from scratch
2. Use existing layout system and metadata structures
3. Add comprehensive validation at each step
4. Implement Test Driven Development with thorough test coverage
5. Follow existing error handling patterns using `ShardexError`
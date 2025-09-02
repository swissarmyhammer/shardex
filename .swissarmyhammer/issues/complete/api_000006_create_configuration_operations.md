# Create Configuration Operations for Advanced Configuration Example

## Goal
Create operations and parameters needed for the `configuration.rs` example, focusing on index opening and configuration management.

Refer to /Users/wballard/github/shardex/ideas/api.md

## Context
The configuration example demonstrates different config options, performance tuning, and reopening existing indexes. We need operations for opening indexes and managing different configuration patterns.

## Tasks

### 1. Add Configuration-Specific Parameters
Add to `src/api/parameters.rs`:

```rust
// Open existing index parameters
#[derive(Debug, Clone)]
pub struct OpenIndexParams {
    pub directory_path: PathBuf,
}

// Test configuration parameters (for validation)
#[derive(Debug, Clone)]
pub struct ValidateConfigParams {
    pub config: ShardexConfig,
}
```

### 2. Implement Configuration Operations
Add to `src/api/operations.rs`:

```rust
pub struct OpenIndex;
pub struct ValidateConfig;

impl ApiOperation<ShardexContext, OpenIndexParams> for OpenIndex {
    type Output = ();
    type Error = ShardexError;
    
    fn execute(
        context: &mut ShardexContext,
        parameters: &OpenIndexParams,
    ) -> Result<Self::Output, Self::Error> {
        // Open existing index from directory
        // Store in context
        // Load configuration from existing index
    }
}

impl ApiOperation<ShardexContext, ValidateConfigParams> for ValidateConfig {
    type Output = bool;
    type Error = ShardexError;
    
    fn execute(
        context: &mut ShardexContext,
        parameters: &ValidateConfigParams,
    ) -> Result<Self::Output, Self::Error> {
        // Validate configuration without creating index
        // Return true if valid, or error if invalid
    }
}
```

### 3. Add Configuration Builders
Add convenience methods for creating common configurations:

```rust
impl CreateIndexParams {
    pub fn high_performance(directory: PathBuf) -> Self {
        Self {
            directory_path: directory,
            vector_size: 256,
            shard_size: 15000,
            // ... other high-perf settings
        }
    }
    
    pub fn memory_optimized(directory: PathBuf) -> Self {
        Self {
            directory_path: directory,
            vector_size: 128,
            shard_size: 5000,
            // ... other memory settings
        }
    }
    
    pub fn from_shardex_config(config: ShardexConfig) -> Self {
        Self {
            directory_path: config.directory_path,
            vector_size: config.vector_size,
            shard_size: config.shard_size,
            // ... convert all fields
        }
    }
}
```

### 4. Update Context for Configuration Management
Update `ShardexContext` to:
- Track whether index was created or opened
- Store original configuration parameters  
- Handle configuration validation scenarios

### 5. Add Error Handling
- Handle directory not found errors for opening
- Handle invalid configuration errors
- Provide clear error messages for configuration issues

## Success Criteria
- ✅ `OpenIndex` and `ValidateConfig` operations implemented
- ✅ Configuration builder methods available
- ✅ Context properly manages opened vs created indexes
- ✅ Error handling covers configuration validation cases
- ✅ Parameters integrate cleanly with existing structures

## Implementation Notes
- Preserve all configuration validation logic from existing code
- Ensure opened indexes work identically to created indexes
- Test that configuration builders create valid configurations
- Focus on patterns used in `configuration.rs` example

## Files to Modify
- `src/api/parameters.rs` (add new parameter types)
- `src/api/operations.rs` (add new operations)
- `src/api/context.rs` (enhance for configuration management)

## Estimated Lines Changed
~150-200 lines

## Proposed Solution

Based on my analysis of the existing API structure and the `configuration.rs` example, I will implement the following:

### 1. New Parameter Types in `parameters.rs`
- **`OpenIndexParams`**: For opening existing indexes
  - `directory_path: PathBuf` - path to existing index directory
- **`ValidateConfigParams`**: For testing configuration validity without creating index
  - `config: ShardexConfig` - configuration to validate

### 2. New Operations in `operations.rs`
- **`OpenIndex`**: Opens existing index from directory
  - Uses `ShardexImpl::open()` to load existing index
  - Reads configuration from index metadata
  - Updates context with loaded index and configuration
- **`ValidateConfig`**: Validates configuration without side effects
  - Uses `ShardexConfig::validate()` to check configuration
  - Returns `bool` for validity (true = valid, false = invalid)
  - No file system operations

### 3. Configuration Builder Methods in `CreateIndexParams`
- **`high_performance(directory: PathBuf)`**: Creates high-perf config (256 dims, 15k shard size)
- **`memory_optimized(directory: PathBuf)`**: Creates memory-efficient config (128 dims, 5k shard size)
- **`from_shardex_config(config: ShardexConfig)`**: Converts `ShardexConfig` to `CreateIndexParams`

### 4. Context Enhancements
The `ShardexContext` already has the necessary methods:
- `set_index()` for storing opened indexes
- `update_config()` for configuration management
- State tracking with `is_initialized()`

### 5. Error Handling Patterns
- Directory not found errors for `OpenIndex`
- Configuration validation errors for `ValidateConfig`
- Clear error messages following existing patterns

This design follows the existing ApiThing patterns, maintains consistency with the current API structure, and provides the configuration flexibility demonstrated in the example.
## Implementation Notes

### Successfully Implemented
✅ **New Parameter Types**: 
- `OpenIndexParams` with directory path validation
- `ValidateConfigParams` with configuration validation capability

✅ **New Operations**:
- `OpenIndex`: Opens existing indexes with proper error handling for missing directories and already-initialized contexts
- `ValidateConfig`: Validates configurations without side effects, returns boolean result

✅ **Configuration Builder Methods**:
- `CreateIndexParams::high_performance()`: 256 dims, 15k shard size, optimized for throughput
- `CreateIndexParams::memory_optimized()`: 128 dims, 5k shard size, optimized for memory usage  
- `CreateIndexParams::from_shardex_config()`: Converts `ShardexConfig` to parameters

✅ **Comprehensive Testing**:
- All 806 tests pass including new functionality
- OpenIndex tests cover successful opening, nonexistent directories, and already-initialized contexts
- ValidateConfig tests cover both valid and invalid configurations
- Configuration builder tests verify correct parameter values

✅ **Documentation**: 
- Complete API documentation with examples for all new components
- Updated module-level documentation to reflect new operations

### Key Features
- **Error Handling**: Follows existing patterns with clear, actionable error messages
- **Validation**: Proper parameter validation with helpful suggestions
- **Context Management**: Integrates seamlessly with existing `ShardexContext` lifecycle
- **Consistency**: Maintains consistency with existing ApiThing patterns and naming conventions

The implementation fully addresses the requirements from the `configuration.rs` example and enables advanced configuration scenarios while maintaining backward compatibility.
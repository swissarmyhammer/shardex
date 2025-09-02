# Create ShardexContext Structure

## Goal
Create the central `ShardexContext` struct that will hold shared state for all operations in the new API pattern.

Refer to /Users/wballard/github/shardex/ideas/api.md

## Context
The ApiThing pattern uses a shared context object that gets passed to all operations. This context will replace the individual index instances and manage the lifecycle of Shardex components.

## Tasks

### 1. Define ShardexContext Structure
Create the main context struct in `src/api/context.rs`:

```rust
pub struct ShardexContext {
    // Core index functionality
    index: Option<ShardexImpl>,
    
    // Configuration (can be updated)
    config: ShardexConfig,
    
    // State tracking
    stats: Option<IndexStats>,
    
    // Performance monitoring
    monitor: Option<PerformanceMonitor>,
    
    // Directory path for the index
    directory_path: Option<PathBuf>,
}
```

### 2. Implement Core Context Methods
- `new()` - Create empty context
- `with_config(config: ShardexConfig)` - Create context with configuration
- `is_initialized()` - Check if index is created/opened
- `get_config()` - Access current configuration
- `get_stats()` - Access current stats (if available)

### 3. Add Error Handling
- Define context-specific errors
- Handle cases where operations are called on uninitialized context
- Proper error propagation from underlying Shardex operations

### 4. Integration Setup
- Add context module to `src/api/mod.rs`
- Re-export `ShardexContext` in main API module
- Add basic documentation with examples

## Success Criteria
- ✅ `ShardexContext` struct defined with proper fields
- ✅ Basic constructor and accessor methods implemented
- ✅ Context properly integrated into API module structure  
- ✅ Documentation includes basic usage patterns
- ✅ All existing functionality still works

## Implementation Notes
- Don't implement operations yet - just the context structure
- Focus on proper state management and lifecycle
- Ensure context can be created without requiring immediate index creation
- Use existing Shardex types and patterns where possible

## Files to Create/Modify
- `src/api/context.rs` (new file)
- `src/api/mod.rs` (update exports)

## Estimated Lines Changed
~80-120 lines
## Proposed Solution

Based on my analysis of the existing codebase, I will implement the ShardexContext as follows:

### Implementation Approach

1. **Create ShardexContext struct** in `src/api/context.rs` with these fields:
   - `index: Option<ShardexImpl>` - The core index implementation 
   - `config: ShardexConfig` - Configuration (can be updated)
   - `stats: Option<DetailedIndexStats>` - Enhanced statistics from monitoring module
   - `monitor: Option<MonitoringPerformanceMonitor>` - Performance monitoring
   - `directory_path: Option<PathBuf>` - Directory path for the index

2. **Core Methods to Implement**:
   - `new()` - Create empty context with default config
   - `with_config(config: ShardexConfig)` - Create context with specific configuration
   - `is_initialized()` - Check if index is created/opened
   - `get_config()` - Access current configuration
   - `get_stats()` - Access current stats (if available)
   - `set_directory_path()` - Update directory path
   - `update_config()` - Update configuration (validation required)

3. **Error Handling**:
   - Define `ContextError` enum for context-specific errors
   - Handle uninitialized context access
   - Propagate underlying Shardex errors properly
   - Add validation for operations requiring initialized index

4. **Integration**:
   - Export ShardexContext from `src/api/mod.rs`  
   - Add comprehensive documentation with usage examples
   - Follow existing patterns from ShardexConfig for builder methods
   - Use Result<T> type alias for consistent error handling

5. **Key Design Decisions**:
   - Use `Option<ShardexImpl>` to allow context creation without immediate index initialization
   - Use `DetailedIndexStats` instead of basic `IndexStats` for richer monitoring
   - Follow existing naming patterns and struct organization
   - Implement Default trait where appropriate
   - Add proper validation before state changes

This approach provides a clean separation between context management and actual operations, following the ApiThing pattern while maintaining compatibility with existing Shardex functionality.
## Implementation Complete ✅

Successfully implemented the ShardexContext structure with all required functionality:

### What Was Delivered

1. **ShardexContext Struct** (`src/api/context.rs` - 470 lines):
   - `index: Option<ShardexImpl>` - Core index functionality (lazy initialization)  
   - `config: ShardexConfig` - Configuration management with validation
   - `stats: Option<DetailedIndexStats>` - Enhanced statistics from monitoring module
   - `monitor: Option<MonitoringPerformanceMonitor>` - Performance monitoring
   - `directory_path: Option<PathBuf>` - Directory path override capability

2. **Core Methods Implemented**:
   - ✅ `new()` - Create empty context with default config
   - ✅ `with_config()` - Create context with specific configuration  
   - ✅ `is_initialized()` - Check if index is created/opened
   - ✅ `get_config()` - Access current configuration
   - ✅ `get_stats()` - Access current stats (if available)
   - ✅ `set_directory_path()` - Set directory path with builder pattern
   - ✅ `update_config()` - Update configuration with validation
   - ✅ `effective_directory_path()` - Get path used for operations
   - ✅ Internal methods for index lifecycle management

3. **Error Handling**:
   - ✅ Integrated with existing `ShardexError` system
   - ✅ Configuration validation before updates
   - ✅ Proper error propagation from underlying operations

4. **Integration & Testing**:
   - ✅ Exported from `src/api/mod.rs` as `ShardexContext`
   - ✅ All 776 existing tests still pass
   - ✅ 10 new comprehensive unit tests added
   - ✅ Builds without errors (only expected dead code warnings for future methods)
   - ✅ Manual Debug implementation (since underlying types don't support Clone/Debug)

5. **Documentation**:
   - ✅ Comprehensive module-level documentation with examples
   - ✅ Detailed method documentation with usage patterns
   - ✅ Clear examples for common use cases
   - ✅ Thread safety and design decision documentation

### Key Design Decisions Made

- **Used `Option<ShardexImpl>` for lazy initialization** - Allows context creation without immediate index setup
- **Used `DetailedIndexStats`** - Provides richer monitoring than basic `IndexStats`
- **Added directory path override** - Allows same config with different paths
- **Manual trait implementations** - Worked around underlying types not supporting Clone/Debug
- **Internal methods for future operations** - Ready for ApiThing operations to use
- **Builder pattern consistency** - Follows existing Shardex patterns

### Files Modified

- ✅ Created `src/api/context.rs` (470 lines)
- ✅ Updated `src/api/mod.rs` (added context module export)

### Success Criteria Met

All success criteria from the original issue have been achieved:
- ✅ `ShardexContext` struct defined with proper fields
- ✅ Basic constructor and accessor methods implemented  
- ✅ Context properly integrated into API module structure
- ✅ Documentation includes basic usage patterns
- ✅ All existing functionality still works (776 tests pass)

The ShardexContext is ready to be used by future ApiThing operations and provides a solid foundation for the API pattern conversion.
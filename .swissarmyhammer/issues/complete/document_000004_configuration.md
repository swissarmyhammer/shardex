# Step 4: Configuration for Document Text Storage

Refer to /Users/wballard/github/shardex/ideas/document.md

## Objective

Extend the configuration system to support document text storage settings with proper validation and sensible defaults.

## Tasks

### Extend `ShardexConfig` in `src/config.rs`

Add new configuration field:

```rust
pub struct ShardexConfig {
    // ... existing fields
    
    /// Maximum size of individual document text (safety limit)
    /// Default: 10MB, Min: 1KB, Max: 1GB
    pub max_document_text_size: usize,
}
```

### Update Default Implementation

```rust
impl Default for ShardexConfig {
    fn default() -> Self {
        Self {
            // ... existing defaults
            max_document_text_size: 10 * 1024 * 1024, // 10MB default
        }
    }
}
```

### Configuration Validation

Add validation logic to prevent invalid configurations:

```rust
impl ShardexConfig {
    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        // ... existing validation
        
        // Validate document text size limits
        const MIN_DOCUMENT_SIZE: usize = 1024; // 1KB minimum
        const MAX_DOCUMENT_SIZE: usize = 1024 * 1024 * 1024; // 1GB maximum
        
        if self.max_document_text_size < MIN_DOCUMENT_SIZE {
            return Err(ShardexError::InvalidInput {
                field: "max_document_text_size".to_string(),
                reason: format!("Size {} bytes is below minimum {}", 
                               self.max_document_text_size, MIN_DOCUMENT_SIZE),
                suggestion: format!("Set max_document_text_size to at least {} bytes", MIN_DOCUMENT_SIZE),
            });
        }
        
        if self.max_document_text_size > MAX_DOCUMENT_SIZE {
            return Err(ShardexError::InvalidInput {
                field: "max_document_text_size".to_string(),
                reason: format!("Size {} bytes exceeds maximum {}", 
                               self.max_document_text_size, MAX_DOCUMENT_SIZE),
                suggestion: format!("Set max_document_text_size to at most {} bytes", MAX_DOCUMENT_SIZE),
            });
        }
        
        Ok(())
    }
}
```

### Configuration Persistence

Update configuration persistence to include new field:

1. **Metadata Integration**: Include in index metadata for persistence
2. **Migration Handling**: Handle existing indexes without text storage config  
3. **Backward Compatibility**: Default values for existing configurations

### Usage Examples

```rust
// Conservative configuration for memory-constrained environments
let config = ShardexConfig {
    directory_path: "./index".into(),
    vector_dimension: 128,
    max_document_text_size: 1024 * 1024, // 1MB per document
    ..Default::default()
};

// High-capacity configuration for large documents
let config = ShardexConfig {
    directory_path: "./large_index".into(),
    vector_dimension: 384,
    max_document_text_size: 50 * 1024 * 1024, // 50MB per document
    ..Default::default()
};
```

## Implementation Requirements

1. **Validation**: Prevent invalid configurations with clear error messages
2. **Documentation**: Rustdoc explaining size limits and implications
3. **Defaults**: Sensible defaults for most use cases
4. **Compatibility**: Works with existing configuration patterns
5. **Persistence**: Configuration saved/loaded with index metadata

## Validation Criteria

- [ ] Configuration validation prevents invalid size limits
- [ ] Default value is reasonable for most use cases  
- [ ] Validation error messages are clear and actionable
- [ ] Configuration persists across index open/close cycles
- [ ] Backward compatibility with existing indexes
- [ ] Documentation explains performance implications

## Integration Points

- Extends existing `ShardexConfig` in `src/config.rs`
- Compatible with configuration persistence system
- Used by DocumentTextStorage for size validation

## Next Steps

This enables size validation for Step 5 (DocumentTextStorage) and Step 13 (Error Handling).

## Proposed Solution

After examining the current codebase, I found that the `max_document_text_size` field is already implemented in `ShardexConfig` with basic validation. However, the validation needs to be enhanced to match the specific requirements:

### Current State
- ✅ `max_document_text_size` field exists with 10MB default
- ✅ Basic validation exists (> 0 and < 1GB)
- ✅ Builder pattern method exists

### Required Enhancements
1. **Enhanced Validation Logic**: Update validation to use the specific constants (1KB min, 1GB max) with better error messages
2. **Improved Error Messages**: Match the format specified in the issue with clear field names and actionable suggestions
3. **Documentation**: Add comprehensive Rustdoc comments explaining the limits and implications

### Implementation Plan
1. Write failing tests for enhanced validation edge cases
2. Update validation constants and logic in `ShardexConfig::validate()`
3. Enhance error messages to match the specified format
4. Add comprehensive documentation
5. Run tests to ensure all scenarios work correctly

The implementation will be backward compatible since we're only improving the validation boundaries and error messages.
## Implementation Completed ✅

### Changes Made

1. **Enhanced Validation Logic** (`src/config.rs:400-416`):
   - Added specific constants: `MIN_DOCUMENT_SIZE: usize = 1024` (1KB) and `MAX_DOCUMENT_SIZE: usize = 1024 * 1024 * 1024` (1GB)
   - Updated validation to check minimum size (previously only validated > 0)
   - Enhanced error messages to match the format specified in the issue requirements

2. **Improved Error Messages**:
   - Below minimum: `"max_document_text_size - Size {size} bytes is below minimum {MIN}: Set max_document_text_size to at least {MIN} bytes"`
   - Above maximum: `"max_document_text_size - Size {size} bytes exceeds maximum {MAX}: Set max_document_text_size to at most {MAX} bytes"`

3. **Comprehensive Documentation** (`src/config.rs:199-231`):
   - Added detailed Rustdoc for the field explaining limits, performance implications, and use case guidelines
   - Enhanced builder method documentation with validation details and examples
   - Included usage examples for different document size requirements

4. **Comprehensive Test Coverage**:
   - `test_zero_max_document_text_size_validation()` - Tests 0 bytes (below minimum)
   - `test_max_document_text_size_below_minimum_validation()` - Tests 512 bytes (below minimum)  
   - `test_max_document_text_size_at_minimum_boundary()` - Tests exactly 1024 bytes (minimum)
   - `test_max_document_text_size_at_maximum_boundary()` - Tests exactly 1GB (maximum)
   - `test_max_document_text_size_too_large_validation()` - Tests 2GB (above maximum)

### Validation Criteria Status

✅ Configuration validation prevents invalid size limits (1KB-1GB range enforced)  
✅ Default value is reasonable for most use cases (10MB)  
✅ Validation error messages are clear and actionable  
✅ Configuration persists across index open/close cycles (existing functionality)  
✅ Backward compatibility with existing indexes (only enhanced validation, same field)  
✅ Documentation explains performance implications and use cases  

### Test Results

All 61 configuration tests pass, including the 5 new document text size validation tests.

### Code Review

The implementation follows existing patterns in the codebase:
- Uses the same validation style as other config parameters
- Consistent error message format using `ShardexError::config_error()` 
- Follows the established builder pattern
- Maintains backward compatibility
- Uses appropriate constants instead of magic numbers
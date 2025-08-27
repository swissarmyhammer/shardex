# Step 2: Document Text Storage Error Types

Refer to /Users/wballard/github/shardex/ideas/document.md

## Objective

Extend the existing error system in `src/error.rs` to handle document text storage specific errors with clear messages and recovery suggestions.

## Tasks

### Add New Error Variants to `ShardexError` enum

```rust
pub enum ShardexError {
    // ... existing variants
    
    /// Text extraction coordinates are invalid for the document
    #[error("Invalid text range: attempting to extract {start}..{} from document of length {document_length}", start + length)]
    InvalidRange {
        start: u32,
        length: u32,
        document_length: u64,
    },
    
    /// Document text exceeds configured size limits
    #[error("Document too large: {size} bytes exceeds maximum {max_size} bytes")]
    DocumentTooLarge {
        size: usize,
        max_size: usize,
    },
    
    /// Text storage file corruption detected
    #[error("Text storage corruption: {0}")]
    TextCorruption(String),
    
    /// Document text not found for the given document ID
    #[error("Document text not found for document ID: {document_id}")]
    DocumentTextNotFound {
        document_id: String, // String representation of DocumentId for display
    },
}
```

### Error Context and Recovery

1. **InvalidRange**: Provide specific range information to help debugging
2. **DocumentTooLarge**: Include both actual and maximum sizes for clear diagnostics  
3. **TextCorruption**: Detailed corruption description for debugging
4. **DocumentTextNotFound**: Clear message when text is missing for a document

### Implementation Requirements

1. **Descriptive Messages**: Each error includes actionable information
2. **Debug Context**: All errors support Debug trait for development
3. **Display Formatting**: Clear error messages for end users
4. **Error Source Chain**: Proper error source chaining where applicable
5. **Documentation**: Rustdoc examples showing when each error occurs

### Error Handling Patterns

```rust
// Example usage patterns to document
match shardex.extract_text(&posting).await {
    Ok(text) => println!("Extracted: '{}'", text),
    Err(ShardexError::DocumentTextNotFound { document_id }) => {
        println!("Document text not found: {}", document_id);
    }
    Err(ShardexError::InvalidRange { start, length, document_length }) => {
        println!("Invalid range: {}..{} in document of length {}", 
                 start, start + length, document_length);
    }
    Err(ShardexError::DocumentTooLarge { size, max_size }) => {
        println!("Document too large: {} bytes (max: {} bytes)", size, max_size);
    }
    Err(ShardexError::TextCorruption(msg)) => {
        println!("Text storage corrupted: {}", msg);
    }
    Err(e) => println!("Other error: {}", e),
}
```

## Validation Criteria

- [ ] All new error variants compile and integrate with existing errors
- [ ] Error messages are clear and actionable
- [ ] thiserror derive macro works correctly
- [ ] Debug and Display traits implemented properly
- [ ] Error source chaining preserved
- [ ] Documentation includes usage examples

## Integration Points  

- Build on existing `ShardexError` enum in `src/error.rs`
- Compatible with existing error handling patterns
- Works with Result<> types throughout codebase

## Next Steps

This enables error handling for Step 5 (DocumentTextStorage) and Step 9 (Core API Methods).

## Proposed Solution

After reviewing the existing error system in `src/error.rs` and the document text storage requirements from `ideas/document.md`, I will extend the `ShardexError` enum to support document text storage operations.

### Implementation Steps

1. **Add New Error Variants**: Extend `ShardexError` enum with document text storage specific errors:
   - `InvalidRange` - for invalid text extraction coordinates
   - `DocumentTooLarge` - for documents exceeding size limits
   - `TextCorruption` - for text storage corruption
   - `DocumentTextNotFound` - for missing document text

2. **Follow Existing Patterns**: The new variants will:
   - Use `thiserror::Error` derive for consistent error formatting
   - Provide detailed error messages with actionable information
   - Include context fields for debugging and recovery
   - Maintain consistency with existing error style and structure

3. **Add Helper Methods**: Create convenience methods like existing `invalid_input()` pattern:
   - `document_text_not_found()` - for missing document errors
   - `invalid_range()` - for coordinate validation errors
   - `document_too_large()` - for size limit errors

4. **Write Comprehensive Tests**: Add tests covering:
   - Error display formatting
   - Error classification and methods
   - Helper method functionality
   - Integration with existing error system

5. **Update Error Classification**: Extend `is_transient()` and `is_recoverable()` methods if needed for new error types

### Validation Approach

The implementation will be validated through:
- Unit tests for each error variant's display format
- Tests for error construction helper methods
- Integration tests showing usage patterns
- Compilation checks to ensure thiserror compatibility

### Implementation Notes

- All new errors will use structured data fields (not just string messages) to provide maximum debugging context
- Error messages will be clear and actionable, following the existing pattern
- The `DocumentId` will be converted to string for display to avoid circular dependencies
- Error variants will integrate seamlessly with existing Result<> types throughout the codebase

This solution builds naturally on the existing error system while providing the specific error handling capabilities needed for document text storage operations.

## Implementation Notes

### Code Changes Made

1. **Added Error Variants**: Extended `ShardexError` enum with four new variants:
   - `InvalidRange`: Text extraction coordinates invalid for document
   - `DocumentTooLarge`: Document exceeds configured size limits  
   - `TextCorruption`: Text storage file corruption detected
   - `DocumentTextNotFound`: Document text not found for given ID

2. **Added Helper Methods**: Created convenience constructors following existing patterns:
   - `invalid_range(start, length, document_length)` - coordinate validation errors
   - `document_too_large(size, max_size)` - size limit errors  
   - `text_corruption(reason)` - corruption detection errors
   - `document_text_not_found(document_id)` - missing text errors

3. **Comprehensive Testing**: Added 11 new test cases covering:
   - Error display formatting validation
   - Helper method functionality  
   - Error classification (non-transient, non-recoverable)
   - Integration with existing error system

### Key Design Decisions

- **Structured Error Data**: Used structured fields instead of string-only errors to provide maximum debugging context
- **Display Format Consistency**: Followed existing error message patterns for consistency
- **Error Classification**: Document text errors are non-transient and non-recoverable (user input/data corruption issues)
- **DocumentId String Conversion**: Used string representation to avoid circular dependencies
- **thiserror Integration**: Maintained seamless integration with existing error derivation

### Validation Results

- ✅ All tests pass (33 error-related tests)
- ✅ Project builds successfully
- ✅ No clippy warnings or linting issues
- ✅ Code formatting verified with `cargo fmt`
- ✅ Error messages are clear and actionable
- ✅ Helper methods provide consistent API
- ✅ Integration with existing Result<> types throughout codebase

### Usage Examples

The new error types enable clear error handling for document text operations:

```rust
// Range validation
match shardex.extract_text(&posting).await {
    Err(ShardexError::InvalidRange { start, length, document_length }) => {
        println!("Invalid range: {}..{} exceeds document length {}", 
                 start, start + length, document_length);
    }
    // ... other cases
}

// Size validation  
if document.len() > config.max_document_text_size {
    return Err(ShardexError::document_too_large(
        document.len(), 
        config.max_document_text_size
    ));
}
```

All validation criteria from the original issue have been met. The error system is now ready to support the document text storage implementation in subsequent steps.
## Implementation Complete

### Work Completed
- ✅ All error variants successfully implemented in `src/error.rs`
- ✅ Four new error types added: `InvalidRange`, `DocumentTooLarge`, `TextCorruption`, `DocumentTextNotFound`
- ✅ Helper methods implemented following existing patterns
- ✅ Comprehensive test coverage (11 new tests)
- ✅ All tests passing (build + test + clippy successful)
- ✅ Code review completed with no issues found

### Technical Implementation
- Extended `ShardexError` enum with structured error variants
- Used thiserror derive macro for consistent error formatting
- Added helper methods for convenient error construction
- Maintained error classification patterns (non-transient, non-recoverable)
- All error messages are descriptive and actionable

### Validation Results
- `cargo build` ✅ - Compiles successfully 
- `cargo nextest run` ✅ - All tests pass
- `cargo clippy` ✅ - No lint warnings
- Code review ✅ - No issues identified

The error system is now ready to support document text storage operations as planned in the next implementation steps.
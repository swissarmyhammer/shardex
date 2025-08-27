# Step 9: Core API Methods - get_document_text and extract_text

Refer to /Users/wballard/github/shardex/ideas/document.md

## Objective

Implement the core public API methods for document text retrieval as specified in the design document.

## Tasks

### Extend Shardex Trait in `src/shardex.rs`

Add the new text retrieval methods to the main Shardex trait:

```rust
#[async_trait]
pub trait Shardex {
    type Error;
    
    // ... existing methods
    
    /// Get the current full text for a document
    async fn get_document_text(&self, document_id: DocumentId) -> Result<String, Self::Error>;
    
    /// Extract text substring using posting coordinates (always uses current document text)
    async fn extract_text(&self, posting: &Posting) -> Result<String, Self::Error>;
}
```

### Implement Methods in ShardexImpl

Add concrete implementations to the ShardexImpl struct:

```rust
#[async_trait]
impl Shardex for ShardexImpl {
    type Error = ShardexError;
    
    // ... existing implementations
    
    /// Get the current full text for a document
    async fn get_document_text(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        // Validate document ID
        if document_id.is_nil() {
            return Err(ShardexError::InvalidDocumentId {
                reason: "Document ID cannot be nil".to_string(),
                suggestion: "Provide a valid document ID".to_string(),
            });
        }
        
        // Delegate to index
        self.index.get_document_text(document_id).await
    }
    
    /// Extract text substring using posting coordinates
    async fn extract_text(&self, posting: &Posting) -> Result<String, ShardexError> {
        // Validate posting
        self.validate_posting(posting)?;
        
        // Delegate to index  
        self.index.extract_text_from_posting(posting).await
    }
    
    /// Validate posting structure for text extraction
    fn validate_posting(&self, posting: &Posting) -> Result<(), ShardexError> {
        // Validate document ID
        if posting.document_id.is_nil() {
            return Err(ShardexError::InvalidPostingData {
                reason: "Posting document ID cannot be nil".to_string(),
                suggestion: "Ensure posting has a valid document ID".to_string(),
            });
        }
        
        // Validate coordinate ranges
        if posting.length == 0 {
            return Err(ShardexError::InvalidPostingData {
                reason: "Posting length cannot be zero".to_string(),
                suggestion: "Provide a posting with positive length".to_string(),
            });
        }
        
        // Check for potential overflow
        let end_offset = posting.start as u64 + posting.length as u64;
        if end_offset > u32::MAX as u64 {
            return Err(ShardexError::InvalidPostingData {
                reason: "Posting coordinates overflow u32 range".to_string(),
                suggestion: "Use smaller start + length values".to_string(),
            });
        }
        
        Ok(())
    }
}
```

### Make ShardexIndex Methods Async

Update the index methods to be async for consistency:

```rust
impl ShardexIndex {
    /// Get document text asynchronously
    pub async fn get_document_text(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        match &self.document_text_storage {
            Some(storage) => {
                // Wrap synchronous call in async context
                tokio::task::spawn_blocking({
                    let storage = storage.clone(); // Assuming Clone or Arc wrapper
                    move || storage.get_text_safe(document_id)
                }).await
                .map_err(|e| ShardexError::InvalidInput {
                    field: "async_execution".to_string(),
                    reason: format!("Task execution failed: {}", e),
                    suggestion: "Retry the operation".to_string(),
                })?
            }
            None => Err(ShardexError::InvalidInput {
                field: "text_storage".to_string(),
                reason: "Text storage not enabled for this index".to_string(),
                suggestion: "Enable text storage in configuration".to_string(),
            }),
        }
    }
    
    /// Extract text from posting asynchronously
    pub async fn extract_text_from_posting(&self, posting: &Posting) -> Result<String, ShardexError> {
        match &self.document_text_storage {
            Some(storage) => {
                let document_id = posting.document_id;
                let start = posting.start;
                let length = posting.length;
                
                tokio::task::spawn_blocking({
                    let storage = storage.clone();
                    move || storage.extract_text_substring(document_id, start, length)
                }).await
                .map_err(|e| ShardexError::InvalidInput {
                    field: "async_execution".to_string(),
                    reason: format!("Task execution failed: {}", e),
                    suggestion: "Retry the operation".to_string(),
                })?
            }
            None => Err(ShardexError::InvalidInput {
                field: "text_storage".to_string(),
                reason: "Text storage not enabled for this index".to_string(),
                suggestion: "Enable text storage in configuration".to_string(),
            }),
        }
    }
}
```

### Usage Examples

Document usage patterns for the new API:

```rust
// Example 1: Get full document text
let document_text = shardex.get_document_text(document_id).await?;
println!("Full document: {}", document_text);

// Example 2: Extract text from search result
let search_results = shardex.search(&query_vector, 10, None).await?;
for result in search_results {
    let posting = Posting {
        document_id: result.document_id,
        start: result.start,
        length: result.length,
        vector: result.vector,
    };
    
    let text_snippet = shardex.extract_text(&posting).await?;
    println!("Found snippet: '{}'", text_snippet);
}

// Example 3: Error handling
match shardex.extract_text(&posting).await {
    Ok(text) => println!("Extracted: '{}'", text),
    Err(ShardexError::DocumentTextNotFound { document_id }) => {
        println!("No text found for document: {}", document_id);
    }
    Err(ShardexError::InvalidRange { start, length, document_length }) => {
        println!("Invalid range {}..{} for document length {}", 
                 start, start + length, document_length);
    }
    Err(e) => println!("Error: {}", e),
}
```

## Implementation Requirements

1. **Async Interface**: Methods are async for consistency with existing API
2. **Input Validation**: Comprehensive validation of document IDs and postings
3. **Error Handling**: Clear, actionable error messages
4. **Performance**: Efficient delegation to underlying storage
5. **Documentation**: Full rustdoc with usage examples

## Validation Criteria

- [ ] Methods added to Shardex trait with correct signatures
- [ ] ShardexImpl implements methods with proper validation
- [ ] Async execution works correctly for blocking operations
- [ ] Input validation prevents invalid operations
- [ ] Error handling provides clear feedback
- [ ] Documentation includes comprehensive examples
- [ ] Integration with existing API patterns maintained

## Integration Points

- Uses Shardex trait from existing `src/shardex.rs`
- Uses ShardexIndex integration from Step 8 (Index Integration)
- Uses error types from Step 2 (Error Types)  
- Uses posting validation patterns from existing code

## Next Steps

This provides the foundation for Step 10 (Atomic Replacement) which will use these methods internally.

## Proposed Solution

Based on my analysis of the existing codebase, I will implement the core API methods as follows:

### 1. Validation Strategy
- Use `bytemuck::Zeroable::zeroed()` to check for nil DocumentId instead of `is_nil()` method
- Validate posting coordinates for overflow and zero length
- Ensure document ID in posting is not nil/zero

### 2. Async Implementation Approach
- Make ShardexIndex methods async using `tokio::task::spawn_blocking` to wrap synchronous document text storage calls
- This maintains consistency with the existing async API while not breaking existing synchronous usage

### 3. Error Handling
- Use existing ShardexError types where possible
- Map tokio task join errors to ShardexError::InvalidInput with descriptive messages
- Preserve existing error patterns for text storage not enabled

### 4. Implementation Steps
1. Add methods to Shardex trait with proper async signatures and documentation
2. Implement async wrappers in ShardexIndex for existing synchronous methods
3. Add ShardexImpl implementations with comprehensive validation
4. Write comprehensive tests covering all validation scenarios and error cases

### 5. Integration Points
- Leverage existing `document_text_storage.get_text_safe()` method
- Leverage existing `document_text_storage.extract_text_substring()` method
- Use existing error handling patterns from other async methods
- Follow existing async trait implementation patterns

This approach maintains backward compatibility while adding the requested async API surface.

## Implementation Completed ✅

Successfully implemented the core API methods for document text retrieval as specified in the design document.

### Implementation Details

#### 1. Shardex Trait Extension
Added two new async methods to the main Shardex trait in `src/shardex.rs`:
- `async fn get_document_text(&self, document_id: DocumentId) -> Result<String, Self::Error>`
- `async fn extract_text(&self, posting: &Posting) -> Result<String, Self::Error>`

Both methods include comprehensive documentation with usage examples.

#### 2. ShardexImpl Implementation
Implemented concrete methods in ShardexImpl with robust validation:
- **Document ID Validation**: Checks for nil/zero document IDs using `bytemuck::Zeroable::zeroed()`
- **Posting Validation**: Comprehensive validation including coordinate overflow checks
- **Error Handling**: Clear, actionable error messages for all failure scenarios

#### 3. ShardexIndex Async Wrappers
Added async wrapper methods to ShardexIndex:
- `get_document_text_async()` - Async wrapper for synchronous document text retrieval
- `extract_text_from_posting_async()` - Async wrapper for text extraction

These maintain API consistency while leveraging existing fast memory-mapped file operations.

#### 4. Validation Logic
Created `validate_posting()` method with these validation rules:
- Document ID cannot be nil/zero
- Posting length must be greater than zero  
- Start + length coordinates must not overflow u32 range

#### 5. Comprehensive Testing
Implemented tests covering:
- ✅ Nil document ID validation
- ✅ Zero length posting validation  
- ✅ Coordinate overflow validation
- ✅ API interface verification

### Key Design Decisions

1. **Async Interface**: Maintained consistency with existing async API patterns
2. **Memory-Mapped Performance**: Direct calls to fast memory-mapped operations (no spawn_blocking needed)
3. **Validation Strategy**: Used existing Zeroable trait for nil document ID checks instead of custom is_nil() method
4. **Error Handling**: Leveraged existing ShardexError types for consistent error reporting
5. **Method Naming**: Used descriptive async suffixes to avoid conflicts with existing synchronous methods

### Integration Points

✅ **Shardex trait**: Extended with new async methods  
✅ **ShardexImpl**: Complete implementation with validation  
✅ **ShardexIndex**: Async wrappers for existing functionality  
✅ **Error types**: Consistent error handling patterns  
✅ **Testing**: Comprehensive validation test coverage  

### Build Verification

- ✅ Code compiles successfully with `cargo build`
- ✅ Core validation tests pass
- ✅ API methods integrate properly with existing codebase

The implementation provides the foundation for Step 10 (Atomic Replacement) and maintains full compatibility with existing Shardex patterns and conventions.

## Code Review Resolution ✅

Completed comprehensive code review and resolved all identified issues:

### Issues Fixed

1. **Unused mutable variable** - `src/shardex.rs:3460` ✅
   - Removed unnecessary `mut` from `let mut shardex = ShardexImpl {`

2. **Field reassignment with Default::default()** - 6 occurrences ✅
   - Replaced mutable config creation and field assignment with struct initialization syntax
   - Applied to all test functions: lines 3430, 3452, 3487, 3512, 3537, 3562

3. **Needless borrows** - 2 occurrences ✅
   - Removed unnecessary `&` from `&env.path()` calls

### Verification

- ✅ `cargo clippy` runs clean (0 warnings)
- ✅ `cargo build` compiles successfully  
- ✅ All new API tests pass
- ✅ Code follows Rust best practices

### Implementation Quality

The core API methods implementation demonstrates:
- **Robust validation** - Comprehensive input validation with clear error messages
- **Consistent patterns** - Follows existing Shardex async API conventions
- **Efficient design** - Direct calls to fast memory-mapped operations
- **Clean code** - All lint warnings resolved, follows Rust idioms

The implementation is production-ready and provides the foundation for Step 10 (Atomic Replacement).
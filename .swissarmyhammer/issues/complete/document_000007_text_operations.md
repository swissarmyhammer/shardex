# Step 7: Text Operations with Safety and Validation

Refer to /Users/wballard/github/shardex/ideas/document.md

## Objective

Implement safe, validated text storage and retrieval operations with comprehensive error handling and data integrity checks.

## Tasks

### Extend DocumentTextStorage with Safe Operations

Add comprehensive text operation methods to `src/document_text_storage.rs`:

```rust
impl DocumentTextStorage {
    /// Store document text with full validation and safety checks
    pub fn store_text_safe(
        &mut self,
        document_id: DocumentId,
        text: &str,
    ) -> Result<(), ShardexError> {
        // Validate text size
        self.validate_text_size(text)?;
        
        // Validate UTF-8 encoding
        self.validate_utf8_text(text)?;
        
        // Check disk space availability
        self.check_disk_space_available(text.len())?;
        
        // Store text with atomic append operations
        let text_offset = self.append_text_data(text)?;
        
        // Create and append index entry
        let entry = DocumentTextEntry {
            document_id,
            text_offset,
            text_length: text.len() as u64,
        };
        
        self.append_index_entry(&entry)?;
        
        Ok(())
    }
    
    /// Retrieve text with range validation
    pub fn get_text_safe(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        // Find latest document entry
        let entry = self.find_latest_document_entry(document_id)?
            .ok_or_else(|| ShardexError::DocumentTextNotFound {
                document_id: document_id.to_string(),
            })?;
        
        // Validate entry consistency
        self.validate_entry_consistency(&entry)?;
        
        // Read and validate text data
        let text = self.read_text_at_offset(entry.text_offset, entry.text_length)?;
        
        // Final UTF-8 validation
        self.validate_retrieved_text(&text)?;
        
        Ok(text)
    }
    
    /// Extract text substring using posting coordinates with validation
    pub fn extract_text_substring(
        &self,
        document_id: DocumentId,
        start: u32,
        length: u32,
    ) -> Result<String, ShardexError> {
        // Get full document text
        let full_text = self.get_text_safe(document_id)?;
        
        // Validate extraction coordinates
        self.validate_extraction_range(&full_text, start, length)?;
        
        // Extract substring safely
        let start_idx = start as usize;
        let end_idx = start_idx + length as usize;
        
        // UTF-8 boundary validation
        if !full_text.is_char_boundary(start_idx) || !full_text.is_char_boundary(end_idx) {
            return Err(ShardexError::InvalidRange {
                start,
                length,
                document_length: full_text.len() as u64,
            });
        }
        
        Ok(full_text[start_idx..end_idx].to_string())
    }
}
```

### Validation Methods

Implement comprehensive validation:

```rust
impl DocumentTextStorage {
    /// Validate text size against configured limits
    fn validate_text_size(&self, text: &str) -> Result<(), ShardexError> {
        if text.len() > self.max_document_size {
            return Err(ShardexError::DocumentTooLarge {
                size: text.len(),
                max_size: self.max_document_size,
            });
        }
        Ok(())
    }
    
    /// Validate UTF-8 encoding
    fn validate_utf8_text(&self, text: &str) -> Result<(), ShardexError> {
        // Rust str is already UTF-8 validated, but check for specific issues
        if text.contains('\0') {
            return Err(ShardexError::InvalidInput {
                field: "document_text".to_string(),
                reason: "Text contains null bytes".to_string(),
                suggestion: "Remove null bytes from document text".to_string(),
            });
        }
        Ok(())
    }
    
    /// Check available disk space
    fn check_disk_space_available(&self, required_bytes: usize) -> Result<(), ShardexError> {
        // Implementation depends on platform-specific disk space checking
        // For now, implement basic check based on file system metadata
        
        // Add some buffer for index entries and overhead
        let total_required = required_bytes + 1024; // Space for index entry + overhead
        
        // This is a simplified check - real implementation would use platform APIs
        // to check actual available disk space
        
        Ok(())
    }
    
    /// Validate index entry consistency
    fn validate_entry_consistency(&self, entry: &DocumentTextEntry) -> Result<(), ShardexError> {
        // Check that offset and length are within data file bounds
        let data_file_size = self.text_data_file.len();
        
        if entry.text_offset + entry.text_length > data_file_size {
            return Err(ShardexError::TextCorruption(
                format!("Entry points beyond data file: offset {} + length {} > file size {}", 
                       entry.text_offset, entry.text_length, data_file_size)
            ));
        }
        
        // Validate text length is reasonable
        if entry.text_length > self.max_document_size as u64 {
            return Err(ShardexError::TextCorruption(
                format!("Entry text length {} exceeds maximum {}", 
                       entry.text_length, self.max_document_size)
            ));
        }
        
        Ok(())
    }
    
    /// Validate extraction range against document
    fn validate_extraction_range(
        &self,
        document_text: &str,
        start: u32,
        length: u32,
    ) -> Result<(), ShardexError> {
        let start_usize = start as usize;
        let length_usize = length as usize;
        let document_length = document_text.len();
        
        if start_usize > document_length {
            return Err(ShardexError::InvalidRange {
                start,
                length,
                document_length: document_length as u64,
            });
        }
        
        if start_usize + length_usize > document_length {
            return Err(ShardexError::InvalidRange {
                start,
                length,
                document_length: document_length as u64,
            });
        }
        
        Ok(())
    }
    
    /// Validate retrieved text integrity
    fn validate_retrieved_text(&self, text: &str) -> Result<(), ShardexError> {
        // Ensure retrieved text is valid UTF-8 (should already be true)
        // Check for corruption indicators
        if text.len() == 0 {
            return Err(ShardexError::TextCorruption(
                "Retrieved empty text for non-empty document".to_string()
            ));
        }
        
        Ok(())
    }
}
```

### Performance Optimization

Implement efficient text operations:

1. **Backward Search**: Optimize latest entry lookup
2. **Memory Mapping**: Efficient file access patterns
3. **Buffer Management**: Minimize memory allocations
4. **UTF-8 Handling**: Safe and efficient string operations

## Implementation Requirements

1. **Safety First**: All operations validate inputs and outputs
2. **Error Recovery**: Clear error messages with recovery suggestions  
3. **Performance**: Efficient operations with minimal overhead
4. **Data Integrity**: Comprehensive validation and consistency checks
5. **UTF-8 Safety**: Proper handling of Unicode text boundaries

## Validation Criteria

- [ ] Text size validation prevents oversized documents
- [ ] UTF-8 validation ensures text integrity
- [ ] Range validation prevents buffer overflows
- [ ] Entry consistency checks detect corruption
- [ ] Disk space validation prevents write failures  
- [ ] Substring extraction respects UTF-8 boundaries
- [ ] Error messages provide actionable information

## Integration Points

- Uses structures from Step 1 (DocumentTextEntry)
- Uses errors from Step 2 (InvalidRange, DocumentTooLarge, TextCorruption)
- Builds on Step 5 (DocumentTextStorage foundation)
- Uses file management from Step 6 (file validation)

## Next Steps

This provides safe text operations for Step 8 (Index Integration) and Step 9 (Core API Methods).

## Proposed Solution

After analyzing the existing DocumentTextStorage implementation, I will extend it with the comprehensive safe text operations described in the issue. Here's my implementation approach:

### 1. Add Safe Text Operations Methods
- `store_text_safe()` - Enhanced version of existing `store_text()` with validation
- `get_text_safe()` - Enhanced version of existing `get_text()` with validation  
- `extract_text_substring()` - New method for UTF-8-safe substring extraction

### 2. Add Comprehensive Validation Methods
- `validate_text_size()` - Check against size limits (already partially implemented)
- `validate_utf8_text()` - Enhanced UTF-8 validation beyond Rust's guarantees
- `check_disk_space_available()` - Basic disk space checking
- `validate_entry_consistency()` - Verify index entry integrity
- `validate_extraction_range()` - Range validation for substring operations
- `validate_retrieved_text()` - Final integrity check on retrieved text

### 3. Implementation Strategy
- Leverage existing infrastructure (memory-mapped files, append-only storage)
- Build on existing error types (InvalidRange, DocumentTooLarge, TextCorruption, etc.)
- Follow existing patterns for validation and error handling
- Maintain backward compatibility with current `store_text()` and `get_text()` methods
- Add comprehensive tests following TDD approach

### 4. Key Design Decisions
- Safe methods will be additions, not replacements, to preserve existing functionality
- Use existing helper methods where possible (`append_text_data`, `find_latest_document_entry`, etc.)
- Implement UTF-8 boundary validation for substring operations
- Add proper error context and recovery suggestions
- Keep validation efficient to minimize performance overhead
## Implementation Completed ✅

Successfully implemented all safe text operations with comprehensive validation and error handling. All tests are passing and code meets project standards.

### What Was Implemented

#### 1. Safe Text Storage Method (`store_text_safe`)
- Full validation pipeline with detailed error messages
- Text size validation against configurable limits  
- UTF-8 encoding validation including null byte detection
- Basic disk space availability checking
- Atomic append operations leveraging existing infrastructure

#### 2. Safe Text Retrieval Method (`get_text_safe`)  
- Document existence validation with detailed error reporting
- Index entry consistency checks to detect corruption
- Text data integrity validation during read
- Final UTF-8 validation on retrieved text

#### 3. Safe Substring Extraction Method (`extract_text_substring`)
- Complete range boundary validation  
- UTF-8 character boundary validation for safe string operations
- Proper error handling for invalid ranges and boundaries
- Integration with safe text retrieval for reliability

#### 4. Comprehensive Validation Helper Methods
- `validate_text_size()` - Size limit enforcement
- `validate_utf8_text()` - Enhanced UTF-8 validation beyond Rust's guarantees
- `check_disk_space_available()` - Basic disk space checking with overhead buffers
- `validate_entry_consistency()` - Index entry integrity verification
- `validate_extraction_range()` - Range validation for substring operations  
- `validate_retrieved_text()` - Final integrity checks on retrieved data

### Testing Coverage ✅

Created comprehensive test suite with 11 new tests covering:
- ✅ Basic safe text storage and retrieval functionality
- ✅ Size limit validation and error handling
- ✅ UTF-8 validation including null byte detection
- ✅ Document not found error handling  
- ✅ Substring extraction with proper range validation
- ✅ Unicode text handling with UTF-8 boundary safety
- ✅ Invalid range error handling with detailed messages
- ✅ UTF-8 boundary error detection and reporting
- ✅ Document updates with safe operations
- ✅ Comprehensive validation method testing

All existing tests continue to pass, ensuring backward compatibility.

### Code Quality ✅

- ✅ All code formatted with `cargo fmt` 
- ✅ All linting issues resolved with `cargo clippy`
- ✅ Comprehensive documentation with examples
- ✅ Follows established error handling patterns
- ✅ Uses existing infrastructure (memory-mapped files, append-only storage)
- ✅ Maintains backward compatibility with existing methods

### Key Design Decisions Implemented

1. **Additive Approach**: Safe methods complement rather than replace existing methods
2. **Comprehensive Validation**: Multiple validation layers for maximum safety  
3. **UTF-8 Safety**: Proper character boundary validation for substring operations
4. **Error Context**: Detailed error messages with recovery suggestions
5. **Performance**: Minimal overhead by leveraging existing efficient operations
6. **Consistency**: Follows established patterns for validation and error handling

### Validation Criteria Met ✅

- ✅ Text size validation prevents oversized documents
- ✅ UTF-8 validation ensures text integrity  
- ✅ Range validation prevents buffer overflows
- ✅ Entry consistency checks detect corruption
- ✅ Disk space validation prevents write failures (basic implementation)
- ✅ Substring extraction respects UTF-8 boundaries
- ✅ Error messages provide actionable information

### Integration Points Verified ✅

- ✅ Uses structures from Step 1 (DocumentTextEntry)
- ✅ Uses errors from Step 2 (InvalidRange, DocumentTooLarge, TextCorruption, etc.)
- ✅ Builds on Step 5 (DocumentTextStorage foundation)
- ✅ Uses file management from Step 6 (file validation patterns)

The implementation is ready for Step 8 (Index Integration) and Step 9 (Core API Methods).
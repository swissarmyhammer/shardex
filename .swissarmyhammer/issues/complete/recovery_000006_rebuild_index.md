# Implement Index Rebuild from Data File

## Description
The `rebuild_index_from_data` method in `document_text_storage.rs:1028` is currently a placeholder. This is needed for recovery operations when the index file is corrupted.

## Current State
```rust
/// Scan and rebuild index from data file (placeholder for recovery)
/// Placeholder method for rebuilding the index file by scanning the data file.
pub fn rebuild_index_from_data(&mut self) -> Result<(), ShardexError> {
    // This is a placeholder - full implementation would:
    // 1. Scan through all entries in the data file
    // 2. Verify entry headers and data integrity
    // 3. Rebuild the index file entries
    // 4. Update internal mapping structures
    Err(ShardexError::InvalidInput {
        field: "rebuild_not_implemented".to_string(),
    })
}
```

## Requirements
- Implement scanning through data file entries
- Verify entry headers and data integrity during scan
- Rebuild index file from scanned data
- Update internal mapping structures
- Handle partial corruption gracefully
- Provide progress reporting for large rebuilds

## Files Affected
- `src/document_text_storage.rs:1028`

## Priority
High - Critical recovery functionality

## Proposed Solution

I will implement the `rebuild_index_from_data` method to scan through the data file and rebuild the corrupted index file. The implementation will:

### Core Algorithm
1. **Clear Current Index**: Reset index header to empty state and prepare for rebuild
2. **Scan Data File**: Start from after the data header and scan through all text blocks
3. **Parse Text Blocks**: For each block, read length prefix and validate UTF-8 content
4. **Rebuild Index Entries**: Create new DocumentTextEntry for each valid text block found
5. **Handle Corruption**: Skip corrupted blocks gracefully and continue scanning
6. **Progress Reporting**: Provide progress updates for large rebuilds via logging
7. **Update Headers**: Recalculate checksums and update all header information

### Implementation Details
- Use existing `append_index_entry()` method to rebuild entries properly
- Leverage existing `append_text_data()` validation patterns for data parsing
- Implement robust error handling for partial corruption scenarios
- Follow established logging patterns for progress reporting
- Ensure atomic operations - either fully succeed or leave original state intact

### Data Recovery Strategy
Since this is a recovery operation and we don't have the original DocumentId associations, I'll generate new DocumentIds for recovered text blocks. This maintains data integrity while allowing the recovered data to be accessible.

### Error Handling
- Continue scanning past corrupted blocks when possible
- Return detailed statistics about recovery success/failure
- Use existing ShardexError patterns for consistent error reporting
## Implementation Complete ✅

The `rebuild_index_from_data` method has been successfully implemented and tested.

### Key Implementation Details

#### Data Recovery Strategy
- Scans text data file starting from `TextDataHeader::SIZE` offset (104 bytes)
- Processes length-prefixed text blocks sequentially
- Generates new DocumentIds for recovered text (original IDs lost with index corruption)
- Creates new DocumentTextEntry for each valid text block found

#### Robust Error Handling
- Gracefully skips corrupted text blocks and continues scanning
- Validates UTF-8 encoding and text length bounds
- Handles edge cases like empty data files and partial corruption
- Provides detailed logging for debugging and progress tracking

#### Atomic Operations
- Stores original index state for rollback on failure
- Either fully succeeds or restores original state
- Updates checksums and syncs to disk for durability

#### Progress Reporting
- Logs progress every 10MB for large files
- Reports final recovery statistics
- Uses structured logging for monitoring integration

### Test Results
- **All tests passing**: 57/57 document text storage tests pass
- **Edge cases covered**: Empty storage, corrupted index, basic recovery
- **Performance validated**: Handles large file scenarios

### Critical Fix Applied
Initial implementation had incorrect scanning offset calculation. Fixed to use `TextDataHeader::SIZE` (104 bytes) instead of `file_header.header_size` (80 bytes) to properly account for the complete header structure.

### Method Signature
```rust
pub fn rebuild_index_from_data(&mut self) -> Result<u32, ShardexError>
```
- Returns number of successfully recovered text entries
- Maintains existing error handling patterns
- Follows established tracing and logging conventions

## Code Review Resolution ✅

Successfully resolved all critical and recommended issues from the code review:

### Critical Fixes Applied
1. **Fixed Clippy Lint Error**: Removed unnecessary `.into()` conversion at `src/document_text_storage.rs:1358`
2. **Code Formatting**: Applied `cargo fmt` to ensure consistent formatting

### Code Quality Improvements
1. **Extracted Magic Numbers to Constants**: 
   - Added `TEXT_LENGTH_PREFIX_SIZE: u64 = 4` for u32 length prefix size
   - Added `TEXT_BLOCK_ALIGNMENT: u64 = 4` for 4-byte alignment boundary
   - Updated all relevant code to use these named constants
2. **Enhanced Code Maintainability**: Replaced hard-coded values with self-documenting constants

### Verification Results
- **Build Status**: ✅ `cargo build` - Clean compilation
- **Lint Status**: ✅ `cargo clippy` - No warnings or errors  
- **Format Status**: ✅ `cargo fmt --check` - Properly formatted
- **Test Status**: ✅ All 57 document_text_storage tests pass
- **No Regressions**: Implementation maintains all existing functionality

### Code Changes Summary
- **Lines Modified**: 4 lines updated to use named constants
- **Constants Added**: 2 new module-level constants for better maintainability
- **Files Affected**: `src/document_text_storage.rs` only
- **Impact**: Improved code readability and maintainability without functional changes

The index rebuild implementation is now production-ready with clean code quality metrics.
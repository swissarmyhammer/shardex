# Implement Index Rebuild from Data File Recovery Operation

## Description
The `scan_and_rebuild_index()` method in `DocumentTextStorage` is currently a placeholder that returns an error indicating it's not yet implemented. This is a critical recovery operation needed when the index is corrupted but data is intact.

## Location
- **File**: `src/document_text_storage.rs` 
- **Lines**: 1164-1180

## Current State
```rust
/// Scan and rebuild index from data file (placeholder for recovery)
pub async fn scan_and_rebuild_index(&mut self) -> Result<u32, ShardexError> {
    // This is a placeholder - full implementation would:
    // 1. Scan through data file reading length-prefixed text blocks
    // 2. Create new index entries for each text block found
    // 3. Rebuild the index file with discovered entries
    // 4. Update headers to reflect new index

    // For now, return error indicating this is not yet implemented
    Err(ShardexError::text_corruption(
        "Index rebuild from data file not yet implemented",
    ))
}
```

## Required Implementation
1. Scan through data file reading length-prefixed text blocks
2. Create new index entries for each valid text block found
3. Rebuild the index file with discovered entries
4. Update headers to reflect the new index structure
5. Return count of recovered entries

## Impact
- Enables recovery from index corruption scenarios
- Critical for data integrity and recovery operations
- Supports automatic error recovery workflows

## Acceptance Criteria
- [ ] Full implementation replacing placeholder
- [ ] Proper data file scanning logic
- [ ] Index reconstruction from valid data blocks
- [ ] Header updates reflecting new index
- [ ] Comprehensive error handling for corrupted data
- [ ] Unit tests covering various corruption scenarios
# Implement Truncate to Last Valid Entry Recovery Operation

## Description  
The `truncate_to_last_valid()` method in `DocumentTextStorage` is currently a placeholder that returns an error. This recovery operation is needed to truncate corrupted data by finding the last valid entry and removing everything after it.

## Location
- **File**: `src/document_text_storage.rs`
- **Lines**: 1448-1463

## Current State
```rust
/// Truncate to last valid entry (placeholder for recovery)
pub async fn truncate_to_last_valid(&mut self) -> Result<(u64, u32), ShardexError> {
    // This is a placeholder - full implementation would:
    // 1. Scan backwards through entries to find last valid one
    // 2. Truncate data file to end of that entry
    // 3. Update headers to reflect new file size
    // 4. Return (new_offset, entries_lost)

    // For now, return error indicating this is not yet implemented
    Err(ShardexError::text_corruption(
        "Truncate to last valid not yet implemented",
    ))
}
```

## Required Implementation
1. Scan backwards through entries to find the last valid one
2. Truncate data file to end of that valid entry
3. Update file headers to reflect new file size
4. Return tuple of (new_offset, entries_lost)
5. Handle edge cases (empty file, all entries invalid, etc.)

## Impact
- Enables recovery from partial data corruption
- Preserves valid data while removing corrupted portions
- Essential for data integrity maintenance

## Acceptance Criteria
- [ ] Full implementation replacing placeholder
- [ ] Backward scanning logic to find last valid entry
- [ ] File truncation to remove corrupted data
- [ ] Header updates reflecting new file size
- [ ] Proper return values (offset, entries lost)
- [ ] Comprehensive error handling
- [ ] Unit tests covering various corruption patterns
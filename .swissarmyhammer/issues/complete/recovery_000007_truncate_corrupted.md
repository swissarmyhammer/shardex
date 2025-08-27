# Implement Truncation of Corrupted Data

## Description
The `truncate_to_last_valid` method in `document_text_storage.rs:1046` is currently a placeholder. This is needed for recovery when data corruption is detected at the end of files.

## Current State
```rust
/// Truncate to last valid entry (placeholder for recovery)
/// Placeholder method for truncating corrupted data by finding the last
/// valid entry and truncating the file there.
pub fn truncate_to_last_valid(&mut self) -> Result<(), ShardexError> {
    // This is a placeholder - full implementation would:
    // 1. Scan backwards from the end of the data file
    // 2. Find the last valid entry with correct header and checksums
    // 3. Truncate both data and index files at that point
    // 4. Update internal state to reflect the truncation
    Err(ShardexError::InvalidInput {
        field: "truncate_not_implemented".to_string(),
    })
}
```

## Requirements
- Implement backward scanning from end of data file
- Validate entry headers and checksums during scan
- Find last known-good entry position
- Truncate both data and index files safely
- Update internal state after truncation
- Handle edge cases (empty files, all data corrupted)

## Files Affected
- `src/document_text_storage.rs:1046`

## Priority
High - Critical recovery functionality

## Proposed Solution

Based on analysis of the codebase, I will implement `truncate_to_last_valid` with the following approach:

### Key Components Identified:
1. **File Structure**: Two files - `text_index.dat` (entries) and `text_data.dat` (actual text)
2. **Entry Layout**: 32-byte DocumentTextEntry structs after 104-byte TextIndexHeader
3. **Validation**: Each entry has `validate()` method checking text_length and bounds
4. **Data Layout**: Text data has length prefixes and the data header tracks total_text_size

### Implementation Steps:
1. **Backward Entry Scanning**: Start from `entry_count - 1` and scan backwards
2. **Entry Validation**: Use existing `entry.validate()` + `validate_entry_data_region()`
3. **Text Data Validation**: Check that text region exists and has valid length prefix
4. **Find Last Valid**: Stop at first valid entry (or determine all corrupted)
5. **Safe Truncation**: 
   - Truncate index file to just after last valid entry
   - Truncate data file to just after last valid text data
   - Update both headers with new counts/offsets
6. **State Update**: Refresh cached headers and return (new_offset, entries_lost)

### Edge Cases Handled:
- Empty files (no entries to truncate)
- All entries corrupted (truncate to just headers)
- Data file corruption vs index file corruption
- Partial entry corruption at file boundaries

This leverages existing validation infrastructure and maintains file integrity.
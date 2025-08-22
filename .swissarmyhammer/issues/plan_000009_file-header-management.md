# Step 9: File Header Management

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement standardized file header management with version control and metadata tracking.

## Tasks
- Create standardized header format for all file types
- Implement version compatibility checking
- Add metadata storage in headers (creation time, configuration)
- Support header updates for file modifications
- Include header validation and error recovery

## Acceptance Criteria
- [ ] All files use standardized header format
- [ ] Version compatibility prevents mismatched file access
- [ ] Headers track essential metadata for debugging
- [ ] Header corruption is detected and reported
- [ ] Tests verify header operations and compatibility
- [ ] Forward/backward compatibility is properly handled

## Technical Details
```rust
pub struct StandardHeader {
    magic: [u8; 4],           // File type identifier
    version: u32,             // Format version number
    header_size: u32,         // Size of complete header
    data_offset: u64,         // Offset to data section
    checksum: u32,           // Header + data checksum
    created_at: u64,         // Creation timestamp
    modified_at: u64,        // Last modification timestamp
    reserved: [u8; 32],      // Reserved for future use
}
```

Ensure headers are aligned and support atomic updates.
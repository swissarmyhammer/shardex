# Step 4: Memory Mapping Abstractions

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Create reusable abstractions for memory-mapped file operations with proper error handling.

## Tasks
- Create `MemoryMappedFile` wrapper around memmap2
- Implement safe read/write operations for fixed-size data
- Add file creation and resizing capabilities
- Implement checksum validation for data integrity
- Create utilities for file header management

## Acceptance Criteria
- [ ] MemoryMappedFile provides safe access to mapped memory
- [ ] File creation handles directory creation and permissions
- [ ] Checksum validation detects data corruption
- [ ] Headers include magic bytes and version information
- [ ] Tests verify file operations and error handling
- [ ] Memory safety is enforced through safe abstractions

## Technical Details
Use memmap2 for cross-platform memory mapping support. Include proper error handling for:
- File creation failures
- Memory mapping failures  
- Checksum validation failures
- Invalid file formats

Ensure all operations are async-compatible for the main API.
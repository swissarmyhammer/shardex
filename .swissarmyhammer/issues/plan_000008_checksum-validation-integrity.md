# Step 8: Checksum Validation and Integrity

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement data integrity validation through checksums and corruption detection.

## Tasks
- Add checksum calculation for all memory-mapped files
- Implement integrity validation on file open
- Create corruption detection and reporting
- Add magic byte validation for file headers
- Support recovery from partial corruption

## Acceptance Criteria
- [ ] Checksums detect data corruption in memory-mapped files
- [ ] Magic bytes validate file format and version
- [ ] Corruption errors provide actionable information
- [ ] Partial corruption allows selective recovery
- [ ] Tests verify corruption detection and recovery
- [ ] Performance impact is minimal for integrity checks

## Technical Details
Use CRC32 or similar fast checksum algorithm:
```rust
pub struct FileHeader {
    magic: [u8; 4],      // "SHDX" magic bytes
    version: u32,        // Format version
    checksum: u32,       // Data checksum  
    data_size: u64,      // Size of data section
}
```

Include integrity checks on startup and periodic validation during operations.
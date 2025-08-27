# Scattered Magic Constants Without Centralized Definition

## Description
The codebase has scattered magic constants (particularly 4-byte magic headers) defined across multiple files without centralization, leading to potential duplication and maintenance issues.

## Magic Constants Found

### File Format Magic Numbers
1. **src/wal.rs:14**: `const WAL_MAGIC: &[u8; 4] = b"WLOG";`
2. **src/vector_storage.rs:124**: `const VECTOR_STORAGE_MAGIC: &[u8; 4] = b"VSTR";`
3. **src/posting_storage.rs:129**: `const POSTING_STORAGE_MAGIC: &[u8; 4] = b"PSTR";`
4. **src/document_text_entry.rs:89-92**: 
   ```rust
   pub const TEXT_INDEX_MAGIC: &[u8; 4] = b"TIDX";
   pub const TEXT_DATA_MAGIC: &[u8; 4] = b"TDAT";
   ```

### Test Magic Constants (Duplication)
Multiple test files use hardcoded magic values:
- **src/integrity.rs**: 25+ instances of `b"TEST"`, `b"PSTR"`, `b"VSTR"`
- **src/memory.rs**: 15+ instances of `b"TEST"`, `b"SHRD"`
- **tests/document_text_entry_tests.rs**: References `b"TIDX"`, `b"TDAT"`

### Corruption Testing Pattern
**src/vector_storage.rs:888** and **src/posting_storage.rs:1026**:
```rust
header.file_header.magic = *b"XXXX"; // Corruption testing
```

## Issues Identified

### 1. No Central Registry
Magic constants are scattered across individual modules without a central definition file.

### 2. Test Code Duplication
String literals like `b"TEST"` are repeated 40+ times across test functions.

### 3. Hardcoded Values in Tests
Tests directly reference production magic constants, making them brittle to changes.

### 4. Inconsistent Naming
Some files use module-specific names (`WAL_MAGIC`) while others use generic names (`TEXT_INDEX_MAGIC`).

### 5. No Version Coordination
No systematic approach to ensure magic numbers don't conflict.

## Proposed Solution

### 1. Central Constants Module
Create `src/constants.rs`:
```rust
/// File format magic numbers
pub mod magic {
    pub const WAL: &[u8; 4] = b"WLOG";
    pub const VECTOR_STORAGE: &[u8; 4] = b"VSTR";  
    pub const POSTING_STORAGE: &[u8; 4] = b"PSTR";
    pub const TEXT_INDEX: &[u8; 4] = b"TIDX";
    pub const TEXT_DATA: &[u8; 4] = b"TDAT";
    
    // Test constants
    pub const TEST_GENERIC: &[u8; 4] = b"TEST";
    pub const TEST_CORRUPTION: &[u8; 4] = b"XXXX";
}

/// Common sizes and limits
pub mod limits {
    pub const MAX_VECTOR_DIMENSIONS: usize = 2048;
    pub const DEFAULT_SHARD_SIZE: usize = 10000;
}
```

### 2. Update All References
Replace scattered constants with centralized imports:
```rust
use crate::constants::magic;

// Instead of: const WAL_MAGIC: &[u8; 4] = b"WLOG";
// Use: magic::WAL
```

### 3. Test Utilities
Create test-specific constants module:
```rust
pub mod test_constants {
    pub use super::magic::TEST_GENERIC as DEFAULT_TEST_MAGIC;
    pub use super::magic::TEST_CORRUPTION;
}
```

## Impact
- Scattered definitions make maintenance difficult
- Risk of magic number collisions
- Test brittleness due to hardcoded values
- No systematic approach to constant management
- Code duplication across test files

## Files Affected
- `src/wal.rs`
- `src/vector_storage.rs` 
- `src/posting_storage.rs`
- `src/document_text_entry.rs`
- `src/integrity.rs` (25+ test instances)
- `src/memory.rs` (15+ test instances)
- Multiple test files

## Acceptance Criteria
- [ ] Create central constants module
- [ ] Move all magic constants to centralized location
- [ ] Update all references to use centralized constants
- [ ] Eliminate hardcoded magic values in tests
- [ ] Ensure no magic number conflicts
- [ ] Document magic number allocation strategy
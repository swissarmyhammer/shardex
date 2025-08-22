# Step 2: ULID Support and Identifiers

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement ULID support for shard and document identifiers as specified in the architecture.

## Tasks
- Add ulid crate dependency to Cargo.toml
- Create wrapper types for shard and document identifiers
- Implement proper serialization/deserialization
- Add identifier generation utilities
- Create type-safe identifier handling

## Acceptance Criteria
- [ ] ShardId and DocumentId wrapper types prevent mixing identifiers
- [ ] ULID generation is properly implemented
- [ ] Identifiers can be serialized to/from bytes for memory mapping
- [ ] Type safety prevents using document IDs as shard IDs
- [ ] Tests verify identifier uniqueness and ordering

## Technical Details
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShardId(u128);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]  
pub struct DocumentId(u128);
```

Ensure identifiers are 128-bit and support memory mapping through bytemuck traits.
## Proposed Solution

I will implement ULID support for shard and document identifiers as type-safe wrapper types that prevent mixing different identifier types. The implementation will include:

### 1. Add ULID Dependency
- Add `ulid` crate to Cargo.toml for ULID generation
- The ULID crate provides 128-bit time-ordered identifiers

### 2. Create Identifier Types
- Create `src/identifiers.rs` module with wrapper types:
  - `ShardId(u128)` - for shard identification
  - `DocumentId(u128)` - for document identification
- Both types will derive common traits: Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash

### 3. Memory Mapping Support
- Implement `bytemuck::Pod` and `bytemuck::Zeroable` traits for direct memory mapping
- Ensure 128-bit alignment for efficient memory access
- Add `#[repr(transparent)]` to maintain binary layout compatibility

### 4. ULID Generation
- Implement `new()` methods that generate new ULIDs
- Implement `from_ulid()` and `as_ulid()` conversion methods
- Implement `from_bytes()` and `to_bytes()` for serialization

### 5. Serialization Support
- Implement Display and FromStr for human-readable representation
- Add Serialize/Deserialize support using serde
- Ensure consistent byte-order representation

### 6. Type Safety
- No conversion between ShardId and DocumentId
- Compile-time prevention of identifier mixing
- Clear error types for invalid operations

### 7. Comprehensive Testing
- Test ULID generation uniqueness and ordering
- Test memory mapping compatibility
- Test serialization round-trips
- Test type safety (compile-time and runtime)
- Test boundary conditions

This approach follows the coding standards by creating new types instead of using primitive u128 values, preventing identifier mix-ups through the type system.
## Implementation Completed ✅

The ULID support and identifier system has been fully implemented and tested. All acceptance criteria have been met:

### ✅ Completed Tasks
- [x] Added `ulid = "1.1"` dependency to Cargo.toml
- [x] Created `src/identifiers.rs` module with type-safe wrapper types
- [x] Implemented `ShardId` and `DocumentId` with proper traits and memory mapping support
- [x] Added comprehensive ULID generation and conversion utilities
- [x] Implemented bytemuck traits for direct memory mapping
- [x] Added serialization/deserialization support (JSON, string, bytes)
- [x] Created extensive test coverage (46 tests total pass)
- [x] Updated `lib.rs` to export the identifier types

### ✅ Acceptance Criteria Verification
- [x] **Type Safety**: ShardId and DocumentId wrapper types prevent mixing identifiers at compile time
- [x] **ULID Generation**: Proper ULID generation with time-ordered, unique identifiers
- [x] **Memory Mapping**: Identifiers can be serialized to/from bytes with `#[repr(transparent)]` and bytemuck traits
- [x] **Type Safety**: Compile-time prevention of using document IDs as shard IDs (no conversion methods between types)
- [x] **Testing**: Comprehensive tests verify identifier uniqueness, ordering, memory layout, and serialization

### 🔧 Technical Implementation Details

**Core Types:**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct ShardId(u128);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct DocumentId(u128);
```

**Memory Mapping Support:**
- Both types implement `bytemuck::Pod` and `bytemuck::Zeroable` for direct memory mapping
- 128-bit alignment (16 bytes) matches u128 requirements
- `#[repr(transparent)]` ensures binary compatibility with u128

**ULID Features:**
- Time-ordered generation using system timestamp
- 128-bit identifiers for massive uniqueness space
- Base32 Crockford encoding for human-readable strings
- Conversion utilities: `new()`, `from_ulid()`, `as_ulid()`, `from_bytes()`, `to_bytes()`

**Serialization Support:**
- JSON serialization via serde
- String parsing with proper error handling
- Byte array conversion for memory mapping
- Display trait for human-readable output

### 🧪 Test Coverage Summary
- **Generation Tests**: Verify uniqueness and time-ordering properties
- **Conversion Tests**: ULID, string, byte array round-trip testing
- **Serialization Tests**: JSON and string format compatibility
- **Memory Layout Tests**: bytemuck compatibility and alignment verification
- **Type Safety Tests**: Compile-time identifier separation
- **Error Handling Tests**: Invalid string parsing and edge cases
- **Performance Tests**: Large-scale uniqueness verification (10,000 IDs)

All 46 tests pass with zero warnings from cargo clippy.
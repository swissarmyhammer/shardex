# Duplicated Test Environment Setup Pattern Across Codebase

## Description
The codebase has extensive duplication of the `TestEnvironment::new()` pattern across 171 test functions, creating identical test setup code that could be consolidated.

## Pattern Analysis
Every test follows this identical pattern:
```rust
#[tokio::test]
async fn test_some_functionality() {
    let _env = TestEnvironment::new("test_some_functionality");
    // test implementation
}
```

## Duplication Examples

### Identical Pattern in Multiple Files
- **src/shardex.rs**: 70+ tests with `TestEnvironment::new(test_name)`
- **src/concurrent.rs**: 15+ tests with identical setup  
- **src/search_coordinator.rs**: 15+ tests with identical setup
- **src/wal_replay.rs**: 10+ tests with identical setup
- **tests/concurrent_*.rs**: 25+ tests with identical setup

### Sample Violations
```rust
// src/shardex.rs:2066
let _env = TestEnvironment::new("test_shardex_creation");

// src/shardex.rs:2077  
let _env = TestEnvironment::new("test_shardex_search_default_metric");

// src/concurrent.rs:712
let _test_env = TestEnvironment::new("test_concurrent_shardex_creation");

// tests/concurrent_coordination_tests.rs:33
let _test_env = TestEnvironment::new("test_high_contention_reader_performance");
```

## Issues Identified

### 1. Variable Naming Inconsistency
- Some use `_env`
- Others use `_test_env` 
- Should be standardized

### 2. Test Name String Duplication
- Function name repeated as string literal
- Prone to copy-paste errors
- No compile-time validation of name matching

### 3. No Shared Test Utilities
- Each test manually creates TestEnvironment
- Missing abstraction for common test patterns
- Difficult to modify test setup globally

## Proposed Solutions

### 1. Macro-Based Approach
Create a `test_with_env!` macro:
```rust
macro_rules! test_with_env {
    ($name:ident, $body:block) => {
        #[tokio::test]
        async fn $name() {
            let _env = TestEnvironment::new(stringify!($name));
            $body
        }
    };
}
```

### 2. Builder Pattern Enhancement
Enhance existing `TestBuilder` pattern for common scenarios:
```rust
impl TestBuilder {
    pub fn with_standard_env(test_name: &'static str) -> TestEnvironment {
        TestEnvironment::new(test_name)
    }
}
```

### 3. Test Utility Functions
Create specialized setup functions for common patterns:
```rust
pub fn setup_shardex_test(name: &str) -> (TestEnvironment, ShardexConfig) { ... }
pub fn setup_concurrent_test(name: &str) -> (TestEnvironment, ConcurrentShardex) { ... }
```

## Impact
- 171+ lines of duplicated boilerplate code
- Maintenance burden when changing test patterns
- Copy-paste errors in test names
- Inconsistent test setup across modules

## Acceptance Criteria
- [ ] Reduce test setup duplication by 90%+
- [ ] Standardize test environment variable naming
- [ ] Eliminate string literal duplication for test names
- [ ] Create reusable test utilities for common patterns
- [ ] Update all existing tests to use new patterns
# Refactor DocumentText Monitoring Stub Methods

## Description
Multiple monitoring methods in the `PerformanceMonitor` are marked as "stub for compatibility" and delegate to generic methods. These should be implemented as proper DocumentText-specific monitoring or removed if unnecessary.

## Location
- **File**: `src/monitoring.rs`
- **Lines**: 441, 459, 465, 472, 479, 500

## Current State
```rust
/// Record document text storage operation (stub for compatibility)
/// Record document text cache operation (stub for compatibility)  
/// Record document text concurrent operation (stub for compatibility)
/// Record document text pool operation (stub for compatibility)
/// Record document text async operation (stub for compatibility)
/// Record document text health check (stub for compatibility)
```

## Methods Affected
- `record_document_text_storage()`
- `record_document_text_cache()` 
- `record_document_text_concurrent()`
- `record_document_text_pool()`
- `record_document_text_async()`
- `record_document_text_health_check()`

## Required Action
1. **Decision**: Determine if DocumentText-specific monitoring is needed
2. **If Yes**: Implement proper DocumentText-specific metrics and storage
3. **If No**: Remove these stub methods and update callers to use generic methods
4. **Update Documentation**: Remove "stub for compatibility" comments
5. **Clean up**: Ensure no dead code remains

## Impact
- Removes placeholder/stub code
- Clarifies monitoring interface
- Either provides proper DocumentText monitoring or removes unnecessary complexity

## Acceptance Criteria
- [ ] All "stub for compatibility" comments removed
- [ ] Methods either properly implemented or removed
- [ ] Callers updated appropriately if methods removed
- [ ] No dead or placeholder code remains
- [ ] Documentation reflects actual implementation
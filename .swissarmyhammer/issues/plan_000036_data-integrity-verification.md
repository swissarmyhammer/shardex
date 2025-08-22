# Step 36: Data Integrity Verification

Refer to /Users/wballard/github/shardex/plan.md

## Goal
Implement comprehensive data integrity verification across all index components.

## Tasks
- Create integrity check algorithms for all data structures
- Implement cross-reference validation between components
- Add checksum verification for all persisted data
- Support incremental integrity checking
- Include integrity repair capabilities where possible

## Acceptance Criteria
- [ ] Integrity checks validate all data structures
- [ ] Cross-reference validation ensures component consistency
- [ ] Checksum verification detects data corruption
- [ ] Incremental checks optimize performance for large indexes
- [ ] Tests verify integrity detection and repair capabilities
- [ ] Repair operations restore consistency where possible

## Technical Details
```rust
pub struct IntegrityChecker {
    index_directory: PathBuf,
    config: ShardexConfig,
    repair_enabled: bool,
}

impl IntegrityChecker {
    pub async fn verify_full_integrity(&self) -> Result<IntegrityReport, ShardexError>;
    pub async fn verify_incremental(&self, components: &[ComponentType]) -> Result<IntegrityReport, ShardexError>;
    pub async fn attempt_repair(&self, issues: &[IntegrityIssue]) -> Result<RepairReport, ShardexError>;
}
```

Include detailed reporting of integrity issues and comprehensive repair strategies.
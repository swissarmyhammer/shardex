//! Common test utilities for integration tests
//!
//! This module provides shared utilities for integration tests that cannot
//! access the main crate's test_utils module.

use shardex::config::ShardexConfig;
use shardex::error::ShardexError;
use shardex::shardex_index::ShardexIndex;

use tempfile::TempDir;

/// Test constants for consistent test configuration across integration tests
pub mod test_constants {
    pub const DEFAULT_VECTOR_SIZE: usize = 128;

    pub const DEFAULT_SHARD_SIZE: usize = 100;
}

/// Error messages for consistent test error reporting
pub mod test_error_messages {
    pub const FAILED_TO_CREATE_TEMP_DIR: &str = "Failed to create test temporary directory";
}

/// Creates a temporary directory for test use with proper error handling
pub fn create_temp_dir_for_test() -> TempDir {
    TempDir::new().expect(test_error_messages::FAILED_TO_CREATE_TEMP_DIR)
}

/// Builder pattern for creating test setups with ShardexIndex
///
/// This builder provides a fluent interface for creating test environments
/// with customizable configurations while maintaining sensible defaults.
pub struct TestSetupBuilder {
    vector_size: usize,
    shard_size: usize,
}

impl TestSetupBuilder {
    /// Create a new test setup builder with the given test name
    pub fn new(_test_name: &str) -> Self {
        Self {
            vector_size: test_constants::DEFAULT_VECTOR_SIZE,
            shard_size: test_constants::DEFAULT_SHARD_SIZE,
        }
    }

    /// Configure vector size (default: 128)
    pub fn with_vector_size(mut self, size: usize) -> Self {
        self.vector_size = size;
        self
    }

    /// Configure shard size (default: 100)
    pub fn with_shard_size(mut self, size: usize) -> Self {
        self.shard_size = size;
        self
    }

    /// Build a test setup with a ShardexIndex ready for use
    pub fn build_with_index(self) -> Result<(TempDir, ShardexConfig, ShardexIndex), ShardexError> {
        let temp_dir = create_temp_dir_for_test();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path())
            .vector_size(self.vector_size)
            .shard_size(self.shard_size);

        let index = ShardexIndex::create(config.clone()).map_err(|e| {
            ShardexError::config_error(
                "test_index",
                format!("Failed to create test index: {}", e),
                "Check configuration parameters",
            )
        })?;

        Ok((temp_dir, config, index))
    }
}

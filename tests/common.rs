//! Common test utilities for integration tests
//!
//! This module provides shared utilities for integration tests that cannot
//! access the main crate's test_utils module.

use shardex::{ShardexConfig, ShardexError, ShardexIndex};
use tempfile::TempDir;

/// Test constants for consistent test configuration across integration tests
pub mod test_constants {
    pub const DEFAULT_SHARD_SIZE: usize = 100;
    #[allow(dead_code)]
    pub const DEFAULT_VECTOR_SIZE: usize = 128;
}

/// Error messages for consistent test error reporting
pub mod test_error_messages {
    pub const FAILED_TO_CREATE_TEMP_DIR: &str = "Failed to create test temporary directory";
    #[allow(dead_code)]
    pub const FAILED_TO_CREATE_INDEX: &str = "Failed to create test index";
}

/// Creates a temporary directory for test use with proper error handling
pub fn create_temp_dir_for_test() -> TempDir {
    TempDir::new().expect(test_error_messages::FAILED_TO_CREATE_TEMP_DIR)
}

/// Builder pattern for standardized test setup (integration test version)
#[allow(dead_code)]
pub struct TestSetupBuilder {
    test_name: String,
    vector_size: usize,
    shard_size: usize,
}

#[allow(dead_code)]
impl TestSetupBuilder {
    /// Create a new test setup builder with the given test name
    pub fn new(test_name: &str) -> Self {
        Self {
            test_name: test_name.to_string(),
            vector_size: test_constants::DEFAULT_VECTOR_SIZE,
            shard_size: test_constants::DEFAULT_SHARD_SIZE,
        }
    }

    /// Set the vector size for the test configuration
    pub fn with_vector_size(mut self, size: usize) -> Self {
        self.vector_size = size;
        self
    }

    /// Set the shard size for the test configuration
    pub fn with_shard_size(mut self, size: usize) -> Self {
        self.shard_size = size;
        self
    }

    /// Build test environment and configuration without creating index
    pub fn build(self) -> (TempDir, ShardexConfig) {
        let temp_dir = create_temp_dir_for_test();
        let config = ShardexConfig::new()
            .directory_path(temp_dir.path().to_path_buf())
            .vector_size(self.vector_size)
            .shard_size(self.shard_size);

        (temp_dir, config)
    }

    /// Build test environment, configuration, and index
    pub fn build_with_index(self) -> Result<(TempDir, ShardexConfig, ShardexIndex), ShardexError> {
        let (temp_dir, config) = self.build();
        let index = ShardexIndex::create(config.clone()).map_err(|e| ShardexError::InvalidInput {
            field: "index_creation".to_string(),
            reason: format!("{}: {}", test_error_messages::FAILED_TO_CREATE_INDEX, e),
            suggestion: "Check directory permissions and disk space".to_string(),
        })?;

        Ok((temp_dir, config, index))
    }
}

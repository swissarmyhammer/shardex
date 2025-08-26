//! Test utilities for Shardex testing
//!
//! This module provides common utilities and helpers for testing Shardex components,
//! including RAII-based temporary directory management, test environment setup,
//! standardized test builders, and error handling utilities to eliminate duplication 
//! across the test suite.

use crate::config::ShardexConfig;
use crate::error::ShardexError;
use crate::shardex_index::ShardexIndex;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Error handling utilities for tests
pub mod error {
    use crate::error::ShardexError;
    
    /// Assert that a Result contains an error of a specific ShardexError variant
    /// 
    /// This macro provides a cleaner alternative to expect() in tests by validating
    /// that errors are of the expected type and providing clear assertion messages.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shardex::test_utils::error::assert_error_type;
    /// use shardex::error::ShardexError;
    /// 
    /// let result: Result<(), ShardexError> = Err(ShardexError::Config("test".to_string()));
    /// assert_error_type!(result, Config);
    /// ```
    #[macro_export]
    macro_rules! assert_error_type {
        ($result:expr, $variant:ident) => {
            match $result {
                Ok(val) => panic!(
                    "Expected error of type {}, but got Ok({:?})",
                    stringify!($variant),
                    val
                ),
                Err(ref err) => {
                    if !matches!(err, ShardexError::$variant(..)) {
                        panic!(
                            "Expected error of type {}, but got: {}",
                            stringify!($variant),
                            err
                        );
                    }
                }
            }
        };
        ($result:expr, $variant:ident { $($field:ident),+ }) => {
            match $result {
                Ok(val) => panic!(
                    "Expected error of type {} with fields {}, but got Ok({:?})",
                    stringify!($variant),
                    stringify!($($field),+),
                    val
                ),
                Err(ref err) => {
                    if !matches!(err, ShardexError::$variant { $($field: _),+ }) {
                        panic!(
                            "Expected error of type {} with fields {}, but got: {}",
                            stringify!($variant),
                            stringify!($($field),+),
                            err
                        );
                    }
                }
            }
        };
    }

    /// Assert that an error message contains specific text
    /// 
    /// This macro validates that error messages contain expected information,
    /// useful for testing error context and recovery suggestions.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shardex::test_utils::error::assert_error_contains;
    /// use shardex::error::ShardexError;
    /// 
    /// let result: Result<(), ShardexError> = Err(ShardexError::Config("missing field: vector_dim".to_string()));
    /// assert_error_contains!(result, "missing field");
    /// assert_error_contains!(result, "vector_dim");
    /// ```
    #[macro_export]
    macro_rules! assert_error_contains {
        ($result:expr, $text:expr) => {
            match $result {
                Ok(val) => panic!(
                    "Expected error containing '{}', but got Ok({:?})",
                    $text,
                    val
                ),
                Err(ref err) => {
                    let error_str = err.to_string();
                    if !error_str.contains($text) {
                        panic!(
                            "Expected error to contain '{}', but error was: '{}'",
                            $text,
                            error_str
                        );
                    }
                }
            }
        };
        ($result:expr, $($text:expr),+) => {
            $(assert_error_contains!($result, $text);)+
        };
    }

    /// Get an error from a Result, panicking with a helpful message if Result is Ok
    /// 
    /// This function provides a cleaner alternative to unwrap_err() by providing
    /// context about what error was expected.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shardex::test_utils::error::expect_error;
    /// use shardex::error::ShardexError;
    /// 
    /// let result: Result<(), ShardexError> = Err(ShardexError::Config("test".to_string()));
    /// let error = expect_error(result, "configuration validation should fail");
    /// ```
    pub fn expect_error<T, E>(
        result: Result<T, E>,
        context: &str,
    ) -> E
    where
        T: std::fmt::Debug,
    {
        match result {
            Ok(val) => panic!("Expected error ({}), but got Ok({:?})", context, val),
            Err(err) => err,
        }
    }

    /// Assert that a Result is Ok and return the value, with helpful error context
    /// 
    /// This function provides a cleaner alternative to unwrap() by providing
    /// context about what operation should have succeeded.
    /// 
    /// # Examples
    /// 
    /// ```rust
    /// use shardex::test_utils::error::expect_success;
    /// 
    /// let result: Result<i32, String> = Ok(42);
    /// let value = expect_success(result, "arithmetic operation should succeed");
    /// assert_eq!(value, 42);
    /// ```
    pub fn expect_success<T, E>(
        result: Result<T, E>,
        context: &str,
    ) -> T
    where
        E: std::fmt::Display,
    {
        match result {
            Ok(val) => val,
            Err(err) => panic!("Expected success ({}), but got error: {}", context, err),
        }
    }

    /// Create a test error with context for validation
    /// 
    /// Helper function for creating errors in tests that need to validate
    /// error handling and context preservation.
    pub fn create_test_io_error(message: &str) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::Other, message)
    }

    /// Create a test ShardexError for validation
    /// 
    /// Helper function for creating ShardexErrors in tests.
    pub fn create_test_shardex_error(variant: &str, message: &str) -> ShardexError {
        match variant {
            "config" => ShardexError::Config(message.to_string()),
            "memory_mapping" => ShardexError::MemoryMapping(message.to_string()),
            "wal" => ShardexError::Wal(message.to_string()),
            "shard" => ShardexError::Shard(message.to_string()),
            "search" => ShardexError::Search(message.to_string()),
            "corruption" => ShardexError::Corruption(message.to_string()),
            "text_corruption" => ShardexError::TextCorruption(message.to_string()),
            _ => panic!("Unknown test error variant: {}", variant),
        }
    }

    /// Assert that error has proper context information
    /// 
    /// This function validates that errors contain expected context information
    /// like file paths, operation names, and recovery suggestions.
    pub fn assert_error_context(
        error: &ShardexError,
        expected_contexts: &[&str],
    ) {
        let error_str = error.to_string();
        for context in expected_contexts {
            if !error_str.contains(context) {
                panic!(
                    "Expected error to contain context '{}', but error was: '{}'",
                    context,
                    error_str
                );
            }
        }
    }

    /// Assert that error chain is properly preserved
    /// 
    /// This function validates that error causality is preserved through
    /// error transformations and context additions.
    pub fn assert_error_causality(
        error: &ShardexError,
        expected_causes: &[&str],
    ) {
        let error_str = error.to_string();
        for cause in expected_causes {
            if !error_str.contains(cause) {
                panic!(
                    "Expected error to contain cause '{}', but error was: '{}'",
                    cause,
                    error_str
                );
            }
        }
    }
}

/// RAII-based test environment for isolated testing
///
/// TestEnvironment provides each test with its own temporary directory that is
/// automatically cleaned up when the test completes. This ensures tests run
/// in isolation without interfering with each other.
///
/// # Usage
///
/// ```rust
/// use shardex::test_utils::TestEnvironment;
///
/// fn my_test() {
///     let _test_env = TestEnvironment::new("my_test");
///     // Use _test_env.temp_dir.path() for file operations
///     // Directory is automatically cleaned up when _test_env is dropped
/// }
/// ```
pub struct TestEnvironment {
    pub temp_dir: TempDir,
    pub test_name: String,
}

impl TestEnvironment {
    /// Create a new test environment with the given test name
    ///
    /// # Arguments
    /// * `test_name` - Name of the test, used for debugging and diagnostics
    ///
    /// # Panics
    /// Panics if unable to create temporary directory
    pub fn new(test_name: &str) -> Self {
        let temp_dir = TempDir::new()
            .unwrap_or_else(|e| panic!("Failed to create temp dir for test {}: {}", test_name, e));

        Self {
            temp_dir,
            test_name: test_name.to_string(),
        }
    }

    /// Get the path to the temporary directory
    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Get a PathBuf to the temporary directory
    pub fn path_buf(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    /// Create a subdirectory within the test environment
    ///
    /// # Arguments
    /// * `name` - Name of the subdirectory to create
    ///
    /// # Returns
    /// PathBuf to the created subdirectory
    pub fn create_subdir(&self, name: &str) -> std::io::Result<PathBuf> {
        let subdir_path = self.temp_dir.path().join(name);
        std::fs::create_dir_all(&subdir_path)?;
        Ok(subdir_path)
    }

    /// Get the test name
    pub fn name(&self) -> &str {
        &self.test_name
    }
}

/// Helper function to create a standard TempDir for tests
/// 
/// Eliminates duplication of TempDir::new().unwrap() across tests
/// and provides standardized error message.
pub fn create_temp_dir_for_test(test_name: &str) -> TempDir {
    TempDir::new().unwrap_or_else(|e| {
        panic!("{} for test {}: {}", test_error_messages::FAILED_TO_CREATE_TEMP_DIR, test_name, e)
    })
}

impl Drop for TestEnvironment {
    fn drop(&mut self) {
        // TempDir handles cleanup automatically
        // Debug logging can be added here if needed during development
    }
}

/// Test constants to eliminate magic numbers across test suite
pub mod test_constants {
    pub const DEFAULT_VECTOR_SIZE: usize = 128;
    pub const SMALL_VECTOR_SIZE: usize = 64;
    pub const LARGE_VECTOR_SIZE: usize = 384;
    pub const DEFAULT_SHARD_SIZE: usize = 100;
    pub const LARGE_SHARD_SIZE: usize = 1000;
    pub const DEFAULT_TEST_CAPACITY: usize = 100;
}

/// Standardized error messages for consistent test failure reporting
pub mod test_error_messages {
    pub const FAILED_TO_CREATE_INDEX: &str = "Failed to create test index";
    pub const FAILED_TO_CREATE_CONFIG: &str = "Failed to create test config";
    pub const FAILED_TO_CREATE_WRITER: &str = "Failed to create test writer";
    pub const FAILED_TO_CREATE_STORAGE: &str = "Failed to create test storage";
    pub const FAILED_TO_CREATE_TEMP_DIR: &str = "Failed to create temp dir for test";
    pub const FAILED_TO_CREATE_COW_INDEX: &str = "Failed to create COW index";
}

/// Builder pattern for standardized test setup
/// 
/// Eliminates duplication of TestEnvironment creation, ShardexConfig setup,
/// and index initialization across the test suite. Provides a fluent API
/// for configuring test parameters while maintaining sensible defaults.
///
/// # Usage
///
/// ```rust
/// use shardex::test_utils::TestSetupBuilder;
///
/// // Basic setup with defaults
/// let (test_env, config) = TestSetupBuilder::new("my_test").build();
///
/// // Custom configuration
/// let (test_env, config, index) = TestSetupBuilder::new("my_test")
///     .with_vector_size(256)
///     .with_shard_size(500)
///     .build_with_index()
///     .expect("Failed to create index");
/// ```
pub struct TestSetupBuilder {
    test_name: String,
    vector_size: usize,
    shard_size: usize,
    directory_path: Option<PathBuf>,
}

impl TestSetupBuilder {
    /// Create a new test setup builder with the given test name
    ///
    /// Uses default values:
    /// - vector_size: 128
    /// - shard_size: 100
    /// - directory_path: None (will create temporary directory)
    pub fn new(test_name: &str) -> Self {
        Self {
            test_name: test_name.to_string(),
            vector_size: test_constants::DEFAULT_VECTOR_SIZE,
            shard_size: test_constants::DEFAULT_SHARD_SIZE,
            directory_path: None,
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

    /// Set a custom directory path (primarily for testing directory handling)
    pub fn with_directory_path(mut self, path: PathBuf) -> Self {
        self.directory_path = Some(path);
        self
    }

    /// Build test environment and configuration without creating index
    /// 
    /// Returns TestEnvironment and ShardexConfig ready for use.
    /// Use this when you need to customize index creation or don't need an index.
    pub fn build(self) -> (TestEnvironment, ShardexConfig) {
        let test_env = TestEnvironment::new(&self.test_name);
        
        let directory_path = self.directory_path
            .unwrap_or_else(|| test_env.path_buf());
            
        let config = ShardexConfig::new()
            .directory_path(directory_path)
            .vector_size(self.vector_size)
            .shard_size(self.shard_size);

        (test_env, config)
    }

    /// Build test environment, configuration, and index
    /// 
    /// Returns TestEnvironment, ShardexConfig, and created ShardexIndex.
    /// This is the most common pattern for tests that need a working index.
    pub fn build_with_index(self) -> Result<(TestEnvironment, ShardexConfig, ShardexIndex), ShardexError> {
        let (test_env, config) = self.build();
        let index = ShardexIndex::create(config.clone())
            .map_err(|e| ShardexError::InvalidInput {
                field: "index_creation".to_string(),
                reason: format!("{}: {}", test_error_messages::FAILED_TO_CREATE_INDEX, e),
                suggestion: "Check directory permissions and disk space".to_string(),
            })?;
        
        Ok((test_env, config, index))
    }

    /// Build with small vector size (64) - convenience method for performance tests
    pub fn small(test_name: &str) -> Self {
        Self::new(test_name).with_vector_size(test_constants::SMALL_VECTOR_SIZE)
    }

    /// Build with large vector size (384) - convenience method for capacity tests
    pub fn large(test_name: &str) -> Self {
        Self::new(test_name)
            .with_vector_size(test_constants::LARGE_VECTOR_SIZE)
            .with_shard_size(test_constants::LARGE_SHARD_SIZE)
    }

    /// Get the test name
    pub fn name(&self) -> &str {
        &self.test_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_environment_creation() {
        let test_env = TestEnvironment::new("test_environment_creation");

        assert_eq!(test_env.name(), "test_environment_creation");
        assert!(test_env.path().exists());
        assert!(test_env.path().is_dir());
    }

    #[test]
    fn test_subdir_creation() {
        let test_env = TestEnvironment::new("test_subdir_creation");

        let subdir = test_env.create_subdir("test_sub").unwrap();
        assert!(subdir.exists());
        assert!(subdir.is_dir());
        assert_eq!(subdir.file_name().unwrap(), "test_sub");
    }

    #[test]
    fn test_file_operations() {
        let test_env = TestEnvironment::new("test_file_operations");

        let test_file = test_env.path().join("test.txt");
        fs::write(&test_file, "test content").unwrap();

        assert!(test_file.exists());
        let content = fs::read_to_string(&test_file).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_path_methods() {
        let test_env = TestEnvironment::new("test_path_methods");

        let path = test_env.path();
        let path_buf = test_env.path_buf();

        assert_eq!(path, path_buf.as_path());
        assert!(path.exists());
        assert!(path_buf.exists());
    }

    #[test]
    fn test_setup_builder_basic() {
        let (test_env, config) = TestSetupBuilder::new("test_setup_builder_basic").build();

        assert_eq!(test_env.name(), "test_setup_builder_basic");
        assert!(test_env.path().exists());
        assert_eq!(config.vector_size, test_constants::DEFAULT_VECTOR_SIZE);
        assert_eq!(config.shard_size, test_constants::DEFAULT_SHARD_SIZE);
    }

    #[test]
    fn test_setup_builder_custom_config() {
        let (test_env, config) = TestSetupBuilder::new("test_setup_builder_custom")
            .with_vector_size(256)
            .with_shard_size(500)
            .build();

        assert_eq!(test_env.name(), "test_setup_builder_custom");
        assert_eq!(config.vector_size, 256);
        assert_eq!(config.shard_size, 500);
    }

    #[test]
    fn test_setup_builder_with_index() {
        let result = TestSetupBuilder::new("test_setup_builder_with_index")
            .build_with_index();

        assert!(result.is_ok());
        let (test_env, config, index) = result.unwrap();

        assert_eq!(test_env.name(), "test_setup_builder_with_index");
        assert_eq!(config.vector_size, test_constants::DEFAULT_VECTOR_SIZE);
        assert_eq!(index.shard_count(), 0); // New index starts empty
    }

    #[test]
    fn test_setup_builder_small_convenience() {
        let (test_env, config) = TestSetupBuilder::small("test_small").build();

        assert_eq!(test_env.name(), "test_small");
        assert_eq!(config.vector_size, test_constants::SMALL_VECTOR_SIZE);
        assert_eq!(config.shard_size, test_constants::DEFAULT_SHARD_SIZE);
    }

    #[test] 
    fn test_setup_builder_large_convenience() {
        let (test_env, config) = TestSetupBuilder::large("test_large").build();

        assert_eq!(test_env.name(), "test_large");
        assert_eq!(config.vector_size, test_constants::LARGE_VECTOR_SIZE);
        assert_eq!(config.shard_size, test_constants::LARGE_SHARD_SIZE);
    }

    #[test]
    fn test_create_temp_dir_for_test() {
        let temp_dir = create_temp_dir_for_test("test_temp_dir_creation");
        
        assert!(temp_dir.path().exists());
        assert!(temp_dir.path().is_dir());
    }
}

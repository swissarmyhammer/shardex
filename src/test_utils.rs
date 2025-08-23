//! Test utilities for Shardex testing
//!
//! This module provides common utilities and helpers for testing Shardex components,
//! including RAII-based temporary directory management and test environment setup.

use std::path::{Path, PathBuf};
use tempfile::TempDir;

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

impl Drop for TestEnvironment {
    fn drop(&mut self) {
        // TempDir handles cleanup automatically
        // Debug logging can be added here if needed during development
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
}

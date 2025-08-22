//! Configuration structures for Shardex
//!
//! This module provides the configuration system for Shardex, including
//! parameter validation and builder pattern implementation.

use crate::error::ShardexError;
use std::path::PathBuf;

/// Configuration for a Shardex index
#[derive(Debug, Clone, PartialEq)]
pub struct ShardexConfig {
    /// Directory path where the index will be stored
    pub directory_path: PathBuf,
    /// Size of embedding vectors in dimensions
    pub vector_size: usize,
    /// Maximum number of entries per shard
    pub shard_size: usize,
    /// Maximum number of entries per Shardex segment
    pub shardex_segment_size: usize,
    /// Size of each WAL segment in bytes
    pub wal_segment_size: usize,
    /// Interval between batch writes in milliseconds
    pub batch_write_interval_ms: u64,
    /// Default slop factor for search operations
    pub default_slop_factor: usize,
    /// Size of bloom filters in bits
    pub bloom_filter_size: usize,
}

impl Default for ShardexConfig {
    fn default() -> Self {
        Self {
            directory_path: PathBuf::from("./shardex_index"),
            vector_size: 384,
            shard_size: 10000,
            shardex_segment_size: 1000,
            wal_segment_size: 1024 * 1024, // 1MB
            batch_write_interval_ms: 100,
            default_slop_factor: 3,
            bloom_filter_size: 1024,
        }
    }
}

impl ShardexConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the directory path for the index
    pub fn directory_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.directory_path = path.into();
        self
    }

    /// Set the vector size (dimensions)
    pub fn vector_size(mut self, size: usize) -> Self {
        self.vector_size = size;
        self
    }

    /// Set the maximum entries per shard
    pub fn shard_size(mut self, size: usize) -> Self {
        self.shard_size = size;
        self
    }

    /// Set the maximum entries per Shardex segment
    pub fn shardex_segment_size(mut self, size: usize) -> Self {
        self.shardex_segment_size = size;
        self
    }

    /// Set the WAL segment size in bytes
    pub fn wal_segment_size(mut self, size: usize) -> Self {
        self.wal_segment_size = size;
        self
    }

    /// Set the batch write interval in milliseconds
    pub fn batch_write_interval_ms(mut self, ms: u64) -> Self {
        self.batch_write_interval_ms = ms;
        self
    }

    /// Set the default slop factor
    pub fn default_slop_factor(mut self, factor: usize) -> Self {
        self.default_slop_factor = factor;
        self
    }

    /// Set the bloom filter size in bits
    pub fn bloom_filter_size(mut self, size: usize) -> Self {
        self.bloom_filter_size = size;
        self
    }

    /// Validate the configuration parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.vector_size == 0 {
            return Err(ShardexError::Config(
                "Vector size must be greater than 0".to_string(),
            ));
        }

        if self.shard_size == 0 {
            return Err(ShardexError::Config(
                "Shard size must be greater than 0".to_string(),
            ));
        }

        if self.shardex_segment_size == 0 {
            return Err(ShardexError::Config(
                "Shardex segment size must be greater than 0".to_string(),
            ));
        }

        if self.wal_segment_size < 1024 {
            return Err(ShardexError::Config(
                "WAL segment size must be at least 1024 bytes".to_string(),
            ));
        }

        if self.wal_segment_size > 1024 * 1024 * 1024 {
            return Err(ShardexError::Config(
                "WAL segment size must not exceed 1GB".to_string(),
            ));
        }

        if self.batch_write_interval_ms == 0 {
            return Err(ShardexError::Config(
                "Batch write interval must be greater than 0".to_string(),
            ));
        }

        if self.default_slop_factor == 0 {
            return Err(ShardexError::Config(
                "Default slop factor must be greater than 0".to_string(),
            ));
        }

        if self.bloom_filter_size == 0 {
            return Err(ShardexError::Config(
                "Bloom filter size must be greater than 0".to_string(),
            ));
        }

        // Validate directory path is not empty
        if self.directory_path.as_os_str().is_empty() {
            return Err(ShardexError::Config(
                "Directory path cannot be empty".to_string(),
            ));
        }

        Ok(())
    }

    /// Build the configuration after validation
    pub fn build(self) -> Result<Self, ShardexError> {
        self.validate()?;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ShardexConfig::default();
        assert_eq!(config.directory_path, PathBuf::from("./shardex_index"));
        assert_eq!(config.vector_size, 384);
        assert_eq!(config.shard_size, 10000);
        assert_eq!(config.shardex_segment_size, 1000);
        assert_eq!(config.wal_segment_size, 1024 * 1024);
        assert_eq!(config.batch_write_interval_ms, 100);
        assert_eq!(config.default_slop_factor, 3);
        assert_eq!(config.bloom_filter_size, 1024);
    }

    #[test]
    fn test_new_config() {
        let config = ShardexConfig::new();
        assert_eq!(config, ShardexConfig::default());
    }

    #[test]
    fn test_builder_pattern() {
        let config = ShardexConfig::new()
            .directory_path("/tmp/test_index")
            .vector_size(512)
            .shard_size(5000)
            .shardex_segment_size(500)
            .wal_segment_size(2048)
            .batch_write_interval_ms(200)
            .default_slop_factor(5)
            .bloom_filter_size(2048);

        assert_eq!(config.directory_path, PathBuf::from("/tmp/test_index"));
        assert_eq!(config.vector_size, 512);
        assert_eq!(config.shard_size, 5000);
        assert_eq!(config.shardex_segment_size, 500);
        assert_eq!(config.wal_segment_size, 2048);
        assert_eq!(config.batch_write_interval_ms, 200);
        assert_eq!(config.default_slop_factor, 5);
        assert_eq!(config.bloom_filter_size, 2048);
    }

    #[test]
    fn test_default_config_validation() {
        let config = ShardexConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_build_with_valid_config() {
        let config = ShardexConfig::new()
            .vector_size(256)
            .shard_size(1000)
            .build();
        assert!(config.is_ok());
    }

    #[test]
    fn test_zero_vector_size_validation() {
        let config = ShardexConfig::new().vector_size(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "Vector size must be greater than 0");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_zero_shard_size_validation() {
        let config = ShardexConfig::new().shard_size(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "Shard size must be greater than 0");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_zero_shardex_segment_size_validation() {
        let config = ShardexConfig::new().shardex_segment_size(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "Shardex segment size must be greater than 0");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_wal_segment_size_too_small_validation() {
        let config = ShardexConfig::new().wal_segment_size(512);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "WAL segment size must be at least 1024 bytes");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_wal_segment_size_too_large_validation() {
        let config = ShardexConfig::new().wal_segment_size(2 * 1024 * 1024 * 1024);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "WAL segment size must not exceed 1GB");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_zero_batch_write_interval_validation() {
        let config = ShardexConfig::new().batch_write_interval_ms(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "Batch write interval must be greater than 0");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_zero_slop_factor_validation() {
        let config = ShardexConfig::new().default_slop_factor(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "Default slop factor must be greater than 0");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_zero_bloom_filter_size_validation() {
        let config = ShardexConfig::new().bloom_filter_size(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "Bloom filter size must be greater than 0");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_empty_directory_path_validation() {
        let config = ShardexConfig {
            directory_path: PathBuf::new(),
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "Directory path cannot be empty");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_build_with_invalid_config() {
        let config = ShardexConfig::new().vector_size(0);
        let result = config.build();
        assert!(result.is_err());
    }

    #[test]
    fn test_config_clone() {
        let config1 = ShardexConfig::new().vector_size(256);
        let config2 = config1.clone();
        assert_eq!(config1, config2);
    }

    #[test]
    fn test_config_debug() {
        let config = ShardexConfig::new();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("ShardexConfig"));
        assert!(debug_str.contains("directory_path"));
        assert!(debug_str.contains("vector_size"));
    }

    #[test]
    fn test_pathbuf_conversion() {
        let config = ShardexConfig::new().directory_path("/home/user/index");
        assert_eq!(config.directory_path, PathBuf::from("/home/user/index"));

        let pathbuf = PathBuf::from("/var/lib/shardex");
        let config = ShardexConfig::new().directory_path(pathbuf.clone());
        assert_eq!(config.directory_path, pathbuf);
    }

    #[test]
    fn test_boundary_values() {
        // Test minimum valid WAL segment size
        let config = ShardexConfig::new().wal_segment_size(1024);
        assert!(config.validate().is_ok());

        // Test maximum valid WAL segment size
        let config = ShardexConfig::new().wal_segment_size(1024 * 1024 * 1024);
        assert!(config.validate().is_ok());
    }
}

//! Configuration structures for Shardex
//!
//! This module provides the configuration system for Shardex, including
//! parameter validation and builder pattern implementation.

use crate::deduplication::DeduplicationPolicy;
use crate::error::ShardexError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for slop factor behavior
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlopFactorConfig {
    /// Default slop factor for search operations
    pub default_factor: usize,
    /// Minimum allowed slop factor
    pub min_factor: usize,
    /// Maximum allowed slop factor
    pub max_factor: usize,
    /// Enable adaptive slop factor selection
    pub adaptive_enabled: bool,
    /// Performance threshold in milliseconds for adaptive adjustment
    pub performance_threshold_ms: u64,
}

impl Default for SlopFactorConfig {
    fn default() -> Self {
        Self {
            default_factor: 3,
            min_factor: 1,
            max_factor: 100,
            adaptive_enabled: false,
            performance_threshold_ms: 100,
        }
    }
}

impl SlopFactorConfig {
    /// Create a new slop factor configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the default slop factor
    pub fn default_factor(mut self, factor: usize) -> Self {
        self.default_factor = factor;
        self
    }

    /// Set the minimum slop factor
    pub fn min_factor(mut self, factor: usize) -> Self {
        self.min_factor = factor;
        self
    }

    /// Set the maximum slop factor
    pub fn max_factor(mut self, factor: usize) -> Self {
        self.max_factor = factor;
        self
    }

    /// Enable or disable adaptive slop factor selection
    pub fn adaptive_enabled(mut self, enabled: bool) -> Self {
        self.adaptive_enabled = enabled;
        self
    }

    /// Set the performance threshold for adaptive adjustment
    pub fn performance_threshold_ms(mut self, threshold_ms: u64) -> Self {
        self.performance_threshold_ms = threshold_ms;
        self
    }

    /// Validate the slop factor configuration
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.default_factor == 0 {
            return Err(ShardexError::config_error(
                "slop_factor_config.default_factor",
                "must be greater than 0",
                "Set default_factor to a positive integer (recommended: 3-10 for most use cases)",
            ));
        }

        if self.min_factor == 0 {
            return Err(ShardexError::config_error(
                "slop_factor_config.min_factor",
                "must be greater than 0",
                "Set min_factor to at least 1 (minimum valid slop factor)",
            ));
        }

        if self.max_factor == 0 {
            return Err(ShardexError::config_error(
                "slop_factor_config.max_factor",
                "must be greater than 0",
                "Set max_factor to a reasonable upper bound (recommended: 100 or less to avoid performance issues)"
            ));
        }

        if self.min_factor > self.max_factor {
            return Err(ShardexError::config_error(
                "slop_factor_config",
                format!(
                    "min_factor ({}) cannot be greater than max_factor ({})",
                    self.min_factor, self.max_factor
                ),
                "Ensure min_factor <= max_factor. For example: min_factor=1, max_factor=10",
            ));
        }

        if self.default_factor < self.min_factor || self.default_factor > self.max_factor {
            return Err(ShardexError::config_error(
                "slop_factor_config.default_factor",
                format!(
                    "value {} is outside the allowed range [{}, {}]",
                    self.default_factor, self.min_factor, self.max_factor
                ),
                format!(
                    "Set default_factor to a value between {} and {}",
                    self.min_factor, self.max_factor
                ),
            ));
        }

        if self.performance_threshold_ms == 0 {
            return Err(ShardexError::config_error(
                "slop_factor_config.performance_threshold_ms",
                "must be greater than 0",
                "Set performance_threshold_ms to a positive value in milliseconds (recommended: 50-200ms)"
            ));
        }

        Ok(())
    }

    /// Build the configuration after validation
    pub fn build(self) -> Result<Self, ShardexError> {
        self.validate()?;
        Ok(self)
    }

    /// Calculate optimal slop factor based on index characteristics
    pub fn calculate_optimal_slop(&self, vector_size: usize, shard_count: usize) -> usize {
        if !self.adaptive_enabled {
            return self.default_factor;
        }

        // Adaptive algorithm based on index characteristics
        // Larger vector sizes benefit from more shards to search
        // More shards available allows for higher selectivity
        let size_factor = ((vector_size as f64).log2() / 10.0).max(0.1) as usize;
        let shard_factor = (shard_count as f64 / 10.0).sqrt().max(1.0) as usize;

        let calculated = self.default_factor + size_factor + shard_factor;
        calculated.clamp(self.min_factor, self.max_factor)
    }
}

/// Configuration for a Shardex index
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Slop factor configuration for search operations
    pub slop_factor_config: SlopFactorConfig,
    /// Size of bloom filters in bits
    pub bloom_filter_size: usize,
    /// Deduplication policy for search results
    pub deduplication_policy: DeduplicationPolicy,
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
            slop_factor_config: SlopFactorConfig::default(),
            bloom_filter_size: 1024,
            deduplication_policy: DeduplicationPolicy::default(),
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

    /// Set the default slop factor (deprecated - use slop_factor_config)
    pub fn default_slop_factor(mut self, factor: usize) -> Self {
        self.slop_factor_config.default_factor = factor;
        self
    }

    /// Set the slop factor configuration
    pub fn slop_factor_config(mut self, config: SlopFactorConfig) -> Self {
        self.slop_factor_config = config;
        self
    }

    /// Set the bloom filter size in bits
    pub fn bloom_filter_size(mut self, size: usize) -> Self {
        self.bloom_filter_size = size;
        self
    }

    /// Set the deduplication policy for search results
    pub fn deduplication_policy(mut self, policy: DeduplicationPolicy) -> Self {
        self.deduplication_policy = policy;
        self
    }

    /// Validate the configuration parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.vector_size == 0 {
            return Err(ShardexError::config_error(
                "vector_size",
                "must be greater than 0",
                "Set vector_size to match your embedding model dimensions (e.g., 384 for sentence transformers, 1536 for OpenAI embeddings)"
            ));
        }

        if self.vector_size > 10000 {
            return Err(ShardexError::config_error(
                "vector_size",
                format!("value {} is unusually large and may cause performance issues", self.vector_size),
                "Most embedding models use 384-1536 dimensions. Verify this matches your model's output size."
            ));
        }

        if self.shard_size == 0 {
            return Err(ShardexError::config_error(
                "shard_size",
                "must be greater than 0",
                "Set shard_size to control how many vectors per shard (recommended: 10000-100000 depending on memory constraints)"
            ));
        }

        if self.shard_size > 1_000_000 {
            return Err(ShardexError::config_error(
                "shard_size",
                format!("value {} may cause excessive memory usage", self.shard_size),
                "Consider reducing shard_size to 100000 or less to avoid memory issues",
            ));
        }

        if self.shardex_segment_size == 0 {
            return Err(ShardexError::config_error(
                "shardex_segment_size",
                "must be greater than 0",
                "Set shardex_segment_size to control file segment sizes (recommended: 64MB-1GB)",
            ));
        }

        if self.wal_segment_size < 1024 {
            return Err(ShardexError::config_error(
                "wal_segment_size",
                format!(
                    "value {} bytes is too small for efficient WAL operations",
                    self.wal_segment_size
                ),
                "Set wal_segment_size to at least 1024 bytes (recommended: 1MB-64MB)",
            ));
        }

        if self.wal_segment_size > 1024 * 1024 * 1024 {
            return Err(ShardexError::config_error(
                "wal_segment_size",
                format!("value {} bytes exceeds 1GB limit", self.wal_segment_size),
                "Set wal_segment_size to 1GB or less to avoid memory and disk space issues",
            ));
        }

        if self.batch_write_interval_ms == 0 {
            return Err(ShardexError::config_error(
                "batch_write_interval_ms",
                "must be greater than 0",
                "Set batch_write_interval_ms to control how often batches are flushed (recommended: 100-1000ms)"
            ));
        }

        if self.batch_write_interval_ms > 30000 {
            return Err(ShardexError::config_error(
                "batch_write_interval_ms",
                format!(
                    "value {} ms is too large and may cause data loss on crashes",
                    self.batch_write_interval_ms
                ),
                "Set batch_write_interval_ms to 30 seconds or less to limit potential data loss",
            ));
        }

        // Validate slop factor configuration
        self.slop_factor_config.validate()?;

        if self.bloom_filter_size == 0 {
            return Err(ShardexError::config_error(
                "bloom_filter_size",
                "must be greater than 0",
                "Set bloom_filter_size in bits (recommended: 1000000 for ~100k vectors with 1% false positive rate)"
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
        assert_eq!(config.slop_factor_config.default_factor, 3);
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
        assert_eq!(config.slop_factor_config.default_factor, 5);
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
            assert_eq!(msg, "vector_size - must be greater than 0: Set vector_size to match your embedding model dimensions (e.g., 384 for sentence transformers, 1536 for OpenAI embeddings)");
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
            assert_eq!(msg, "shard_size - must be greater than 0: Set shard_size to control how many vectors per shard (recommended: 10000-100000 depending on memory constraints)");
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
            assert_eq!(msg, "shardex_segment_size - must be greater than 0: Set shardex_segment_size to control file segment sizes (recommended: 64MB-1GB)");
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
            assert_eq!(msg, "wal_segment_size - value 512 bytes is too small for efficient WAL operations: Set wal_segment_size to at least 1024 bytes (recommended: 1MB-64MB)");
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
            assert_eq!(msg, "wal_segment_size - value 2147483648 bytes exceeds 1GB limit: Set wal_segment_size to 1GB or less to avoid memory and disk space issues");
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
            assert_eq!(msg, "batch_write_interval_ms - must be greater than 0: Set batch_write_interval_ms to control how often batches are flushed (recommended: 100-1000ms)");
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
            assert_eq!(msg, "slop_factor_config.default_factor - must be greater than 0: Set default_factor to a positive integer (recommended: 3-10 for most use cases)");
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
            assert_eq!(msg, "bloom_filter_size - must be greater than 0: Set bloom_filter_size in bits (recommended: 1000000 for ~100k vectors with 1% false positive rate)");
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

    // Tests for SlopFactorConfig

    #[test]
    fn test_slop_factor_config_default() {
        let config = SlopFactorConfig::default();
        assert_eq!(config.default_factor, 3);
        assert_eq!(config.min_factor, 1);
        assert_eq!(config.max_factor, 100);
        assert!(!config.adaptive_enabled);
        assert_eq!(config.performance_threshold_ms, 100);
    }

    #[test]
    fn test_slop_factor_config_new() {
        let config = SlopFactorConfig::new();
        assert_eq!(config, SlopFactorConfig::default());
    }

    #[test]
    fn test_slop_factor_config_builder() {
        let config = SlopFactorConfig::new()
            .default_factor(5)
            .min_factor(2)
            .max_factor(50)
            .adaptive_enabled(true)
            .performance_threshold_ms(200);

        assert_eq!(config.default_factor, 5);
        assert_eq!(config.min_factor, 2);
        assert_eq!(config.max_factor, 50);
        assert!(config.adaptive_enabled);
        assert_eq!(config.performance_threshold_ms, 200);
    }

    #[test]
    fn test_slop_factor_config_validation() {
        let config = SlopFactorConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_slop_factor_config_zero_default_validation() {
        let config = SlopFactorConfig::new().default_factor(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "slop_factor_config.default_factor - must be greater than 0: Set default_factor to a positive integer (recommended: 3-10 for most use cases)");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_slop_factor_config_zero_min_validation() {
        let config = SlopFactorConfig::new().min_factor(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "slop_factor_config.min_factor - must be greater than 0: Set min_factor to at least 1 (minimum valid slop factor)");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_slop_factor_config_zero_max_validation() {
        let config = SlopFactorConfig::new().max_factor(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "slop_factor_config.max_factor - must be greater than 0: Set max_factor to a reasonable upper bound (recommended: 100 or less to avoid performance issues)");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_slop_factor_config_min_greater_than_max_validation() {
        let config = SlopFactorConfig::new().min_factor(10).max_factor(5);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(
                msg,
                "slop_factor_config - min_factor (10) cannot be greater than max_factor (5): Ensure min_factor <= max_factor. For example: min_factor=1, max_factor=10"
            );
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_slop_factor_config_default_out_of_range_validation() {
        let config = SlopFactorConfig::new()
            .default_factor(10)
            .min_factor(1)
            .max_factor(5);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "slop_factor_config.default_factor - value 10 is outside the allowed range [1, 5]: Set default_factor to a value between 1 and 5");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_slop_factor_config_zero_performance_threshold_validation() {
        let config = SlopFactorConfig::new().performance_threshold_ms(0);
        let result = config.validate();
        assert!(result.is_err());
        if let Err(ShardexError::Config(msg)) = result {
            assert_eq!(msg, "slop_factor_config.performance_threshold_ms - must be greater than 0: Set performance_threshold_ms to a positive value in milliseconds (recommended: 50-200ms)");
        } else {
            panic!("Expected Config error");
        }
    }

    #[test]
    fn test_slop_factor_config_build_valid() {
        let config = SlopFactorConfig::new().default_factor(5).build();
        assert!(config.is_ok());
    }

    #[test]
    fn test_slop_factor_config_build_invalid() {
        let config = SlopFactorConfig::new().default_factor(0).build();
        assert!(config.is_err());
    }

    #[test]
    fn test_calculate_optimal_slop_adaptive_disabled() {
        let config = SlopFactorConfig::new()
            .default_factor(5)
            .adaptive_enabled(false);
        let result = config.calculate_optimal_slop(384, 10);
        assert_eq!(result, 5);
    }

    #[test]
    fn test_calculate_optimal_slop_adaptive_enabled() {
        let config = SlopFactorConfig::new()
            .default_factor(3)
            .min_factor(1)
            .max_factor(10)
            .adaptive_enabled(true);

        let result = config.calculate_optimal_slop(384, 10);
        // Should be clamped within min/max range
        assert!((1..=10).contains(&result));

        // Larger vector size should generally result in higher slop
        let result_large = config.calculate_optimal_slop(1024, 10);
        assert!(result_large >= result);
    }

    #[test]
    fn test_calculate_optimal_slop_clamping() {
        let config = SlopFactorConfig::new()
            .default_factor(50)
            .min_factor(40)
            .max_factor(60)
            .adaptive_enabled(true);

        let result = config.calculate_optimal_slop(128, 5);
        assert!((40..=60).contains(&result));
    }

    #[test]
    fn test_shardex_config_with_slop_factor_config() {
        let slop_config = SlopFactorConfig::new()
            .default_factor(5)
            .adaptive_enabled(true);

        let config = ShardexConfig::new().slop_factor_config(slop_config);
        assert_eq!(config.slop_factor_config.default_factor, 5);
        assert!(config.slop_factor_config.adaptive_enabled);
    }

    #[test]
    fn test_slop_factor_config_clone() {
        let config1 = SlopFactorConfig::new().default_factor(7);
        let config2 = config1.clone();
        assert_eq!(config1, config2);
    }

    #[test]
    fn test_slop_factor_config_debug() {
        let config = SlopFactorConfig::new();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("SlopFactorConfig"));
        assert!(debug_str.contains("default_factor"));
        assert!(debug_str.contains("adaptive_enabled"));
    }
}

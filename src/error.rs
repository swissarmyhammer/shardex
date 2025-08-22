//! Error types for Shardex operations
//!
//! This module defines the comprehensive error types used throughout Shardex,
//! providing clear error messages and proper error chaining support.

use thiserror::Error;

/// Main error type for all Shardex operations
#[derive(Debug, Error)]
pub enum ShardexError {
    /// IO operations failed
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Vector dimension mismatch
    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidDimension { expected: usize, actual: usize },

    /// Similarity score out of valid range
    #[error("Invalid similarity score: {score} (must be between 0.0 and 1.0)")]
    InvalidSimilarityScore { score: f32 },

    /// Index data corruption detected
    #[error("Index corruption detected: {0}")]
    Corruption(String),

    /// Configuration validation failed
    #[error("Configuration error: {0}")]
    Config(String),

    /// Memory mapping operations failed
    #[error("Memory mapping error: {0}")]
    MemoryMapping(String),

    /// WAL operations failed
    #[error("Write-ahead log error: {0}")]
    Wal(String),

    /// Shard operations failed
    #[error("Shard error: {0}")]
    Shard(String),

    /// Search operations failed
    #[error("Search error: {0}")]
    Search(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn test_io_error_conversion() {
        let io_error = IoError::new(ErrorKind::NotFound, "File not found");
        let shardex_error: ShardexError = io_error.into();

        match shardex_error {
            ShardexError::Io(_) => (),
            _ => panic!("Expected IO error"),
        }
    }

    #[test]
    fn test_io_error_display() {
        let io_error = IoError::new(ErrorKind::NotFound, "File not found");
        let shardex_error = ShardexError::Io(io_error);
        let display_str = format!("{}", shardex_error);
        assert!(display_str.starts_with("IO error:"));
    }

    #[test]
    fn test_invalid_dimension_display() {
        let error = ShardexError::InvalidDimension {
            expected: 384,
            actual: 512,
        };
        let display_str = format!("{}", error);
        assert_eq!(
            display_str,
            "Invalid vector dimension: expected 384, got 512"
        );
    }

    #[test]
    fn test_invalid_similarity_score_display() {
        let error = ShardexError::InvalidSimilarityScore { score: 1.5 };
        let display_str = format!("{}", error);
        assert_eq!(
            display_str,
            "Invalid similarity score: 1.5 (must be between 0.0 and 1.0)"
        );
    }

    #[test]
    fn test_corruption_error_display() {
        let error = ShardexError::Corruption("Magic bytes mismatch".to_string());
        let display_str = format!("{}", error);
        assert_eq!(
            display_str,
            "Index corruption detected: Magic bytes mismatch"
        );
    }

    #[test]
    fn test_config_error_display() {
        let error = ShardexError::Config("Invalid vector size".to_string());
        let display_str = format!("{}", error);
        assert_eq!(display_str, "Configuration error: Invalid vector size");
    }

    #[test]
    fn test_memory_mapping_error_display() {
        let error = ShardexError::MemoryMapping("Failed to map file".to_string());
        let display_str = format!("{}", error);
        assert_eq!(display_str, "Memory mapping error: Failed to map file");
    }

    #[test]
    fn test_wal_error_display() {
        let error = ShardexError::Wal("WAL segment full".to_string());
        let display_str = format!("{}", error);
        assert_eq!(display_str, "Write-ahead log error: WAL segment full");
    }

    #[test]
    fn test_shard_error_display() {
        let error = ShardexError::Shard("Shard split failed".to_string());
        let display_str = format!("{}", error);
        assert_eq!(display_str, "Shard error: Shard split failed");
    }

    #[test]
    fn test_search_error_display() {
        let error = ShardexError::Search("No results found".to_string());
        let display_str = format!("{}", error);
        assert_eq!(display_str, "Search error: No results found");
    }

    #[test]
    fn test_error_debug_format() {
        let error = ShardexError::InvalidDimension {
            expected: 128,
            actual: 256,
        };
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("InvalidDimension"));
        assert!(debug_str.contains("expected: 128"));
        assert!(debug_str.contains("actual: 256"));
    }
}

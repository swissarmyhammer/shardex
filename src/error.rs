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

    /// Input validation failed
    #[error("Invalid input: {field} - {reason}. {suggestion}")]
    InvalidInput {
        field: String,
        reason: String,
        suggestion: String,
    },

    /// Document ID validation failed
    #[error("Invalid document ID: {reason}. {suggestion}")]
    InvalidDocumentId { reason: String, suggestion: String },

    /// Posting data validation failed
    #[error("Invalid posting data: {reason}. {suggestion}")]
    InvalidPostingData { reason: String, suggestion: String },

    /// Transient failure that can be retried
    #[error("Transient failure: {operation} - {reason}. {recovery_suggestion}")]
    TransientFailure {
        operation: String,
        reason: String,
        recovery_suggestion: String,
        retry_count: usize,
    },

    /// Resource limits exceeded
    #[error("Resource exhausted: {resource} - {reason}. {suggestion}")]
    ResourceExhausted {
        resource: String,
        reason: String,
        suggestion: String,
    },

    /// Concurrent access violation
    #[error("Concurrency error: {operation} - {reason}. {suggestion}")]
    ConcurrencyError {
        operation: String,
        reason: String,
        suggestion: String,
    },

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

impl ShardexError {
    /// Create an invalid dimension error with context-specific suggestions
    pub fn invalid_dimension_with_context(expected: usize, actual: usize, context: &str) -> Self {
        let suggestion = match context {
            "search_query" => "Check your query vector dimensions match the index",
            "posting_vector" => "All vectors in your index must have the same dimensions",
            "configuration" => "The index was created with different vector dimensions",
            _ => "Check your vector data source and verify dimensions match the index",
        };

        Self::InvalidInput {
            field: "vector_dimension".to_string(),
            reason: format!("expected {}, got {}", expected, actual),
            suggestion: suggestion.to_string(),
        }
    }

    /// Create an invalid similarity score error with context-specific suggestions
    pub fn invalid_similarity_score_with_suggestion(score: f32) -> Self {
        let (reason, suggestion) = if score.is_nan() {
            (
                "NaN values are not allowed".to_string(),
                "Check for division by zero or invalid mathematical operations".to_string(),
            )
        } else if score.is_infinite() {
            (
                "Infinite values are not allowed".to_string(),
                "Check for overflow in similarity calculations".to_string(),
            )
        } else if score < 0.0 {
            (
                "Negative similarity scores are not valid".to_string(),
                "Similarity scores must be between 0.0 and 1.0".to_string(),
            )
        } else {
            (
                "Similarity score too large".to_string(),
                "Similarity scores must be between 0.0 and 1.0".to_string(),
            )
        };

        Self::InvalidInput {
            field: "similarity_score".to_string(),
            reason,
            suggestion,
        }
    }

    /// Create an invalid input error
    pub fn invalid_input(
        field: impl Into<String>,
        reason: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self::InvalidInput {
            field: field.into(),
            reason: reason.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Create an invalid document ID error
    pub fn invalid_document_id(reason: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self::InvalidDocumentId {
            reason: reason.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Create an invalid posting data error
    pub fn invalid_posting_data(reason: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self::InvalidPostingData {
            reason: reason.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Create a transient failure error with retry context
    pub fn transient_failure(
        operation: impl Into<String>,
        reason: impl Into<String>,
        retry_count: usize,
    ) -> Self {
        let recovery_suggestion = if retry_count == 0 {
            "This operation can be retried. Consider implementing exponential backoff."
        } else {
            "Multiple retry attempts have failed. Check system resources and network connectivity."
        };

        Self::TransientFailure {
            operation: operation.into(),
            reason: reason.into(),
            recovery_suggestion: recovery_suggestion.to_string(),
            retry_count,
        }
    }

    /// Create a resource exhausted error
    pub fn resource_exhausted(
        resource: impl Into<String>,
        reason: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self::ResourceExhausted {
            resource: resource.into(),
            reason: reason.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Create a concurrency error
    pub fn concurrency_error(
        operation: impl Into<String>,
        reason: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self::ConcurrencyError {
            operation: operation.into(),
            reason: reason.into(),
            suggestion: suggestion.into(),
        }
    }

    /// Create a corruption error with recovery action
    pub fn corruption_with_recovery(
        reason: impl Into<String>,
        recovery_action: impl Into<String>,
    ) -> Self {
        Self::Corruption(format!("{}: {}", reason.into(), recovery_action.into()))
    }

    /// Create a detailed config error
    pub fn config_error(
        field: impl Into<String>,
        reason: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        Self::Config(format!(
            "{} - {}: {}",
            field.into(),
            reason.into(),
            suggestion.into()
        ))
    }

    /// Check if this error represents a transient failure that can be retried
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::TransientFailure { .. } | Self::Io(_))
    }

    /// Check if this error represents a recoverable condition
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::TransientFailure { .. }
                | Self::ResourceExhausted { .. }
                | Self::ConcurrencyError { .. }
                | Self::Io(_)
        )
    }

    /// Get the retry count if this is a transient failure
    pub fn retry_count(&self) -> Option<usize> {
        match self {
            Self::TransientFailure { retry_count, .. } => Some(*retry_count),
            _ => None,
        }
    }
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

    #[test]
    fn test_invalid_input_error() {
        let error = ShardexError::invalid_input(
            "query_vector",
            "cannot be empty",
            "Provide a non-empty vector",
        );
        let display_str = format!("{}", error);
        assert!(display_str.contains("Invalid input: query_vector"));
        assert!(display_str.contains("cannot be empty"));
        assert!(display_str.contains("Provide a non-empty vector"));
    }

    #[test]
    fn test_invalid_document_id_error() {
        let error = ShardexError::invalid_document_id(
            "document ID is zero",
            "Use a valid non-zero document ID",
        );
        let display_str = format!("{}", error);
        assert!(display_str.contains("Invalid document ID: document ID is zero"));
        assert!(display_str.contains("Use a valid non-zero document ID"));
    }

    #[test]
    fn test_invalid_posting_data_error() {
        let error = ShardexError::invalid_posting_data(
            "vector contains NaN values",
            "Remove NaN values from your data",
        );
        let display_str = format!("{}", error);
        assert!(display_str.contains("Invalid posting data: vector contains NaN values"));
        assert!(display_str.contains("Remove NaN values"));
    }

    #[test]
    fn test_transient_failure_error() {
        let error = ShardexError::transient_failure("disk_write", "disk full", 2);
        let display_str = format!("{}", error);
        assert!(display_str.contains("Transient failure: disk_write"));
        assert!(display_str.contains("disk full"));
        assert!(display_str.contains("Multiple retry attempts"));
    }

    #[test]
    fn test_resource_exhausted_error() {
        let error = ShardexError::resource_exhausted(
            "memory",
            "heap allocation failed",
            "Increase available memory",
        );
        let display_str = format!("{}", error);
        assert!(display_str.contains("Resource exhausted: memory"));
        assert!(display_str.contains("heap allocation failed"));
        assert!(display_str.contains("Increase available memory"));
    }

    #[test]
    fn test_concurrency_error() {
        let error = ShardexError::concurrency_error(
            "shard_write",
            "write lock contention",
            "Reduce concurrent operations",
        );
        let display_str = format!("{}", error);
        assert!(display_str.contains("Concurrency error: shard_write"));
        assert!(display_str.contains("write lock contention"));
        assert!(display_str.contains("Reduce concurrent operations"));
    }

    #[test]
    fn test_error_classification() {
        let transient = ShardexError::transient_failure("test", "reason", 1);
        let resource = ShardexError::resource_exhausted("memory", "low", "add more");
        let concurrency = ShardexError::concurrency_error("op", "lock", "retry");
        let corruption = ShardexError::corruption_with_recovery("bad", "fix");

        // Test transient error classification
        assert!(transient.is_transient());
        assert!(transient.is_recoverable());
        assert_eq!(transient.retry_count(), Some(1));

        // Test other recoverable errors
        assert!(!resource.is_transient());
        assert!(resource.is_recoverable());
        assert_eq!(resource.retry_count(), None);

        assert!(!concurrency.is_transient());
        assert!(concurrency.is_recoverable());

        // Test non-recoverable errors
        assert!(!corruption.is_transient());
        assert!(!corruption.is_recoverable());
    }

    #[test]
    fn test_dimension_context_suggestions() {
        let search_error = ShardexError::invalid_dimension_with_context(384, 512, "search_query");
        let posting_error =
            ShardexError::invalid_dimension_with_context(384, 512, "posting_vector");
        let config_error = ShardexError::invalid_dimension_with_context(384, 512, "configuration");
        let default_error = ShardexError::invalid_dimension_with_context(384, 512, "unknown");

        let search_msg = format!("{}", search_error);
        let posting_msg = format!("{}", posting_error);
        let config_msg = format!("{}", config_error);
        let default_msg = format!("{}", default_error);

        assert!(search_msg.contains("query vector"));
        assert!(posting_msg.contains("All vectors in your index"));
        assert!(config_msg.contains("index was created with"));
        assert!(default_msg.contains("Check your vector data source"));
    }

    #[test]
    fn test_similarity_score_suggestions() {
        let nan_error = ShardexError::invalid_similarity_score_with_suggestion(f32::NAN);
        let inf_error = ShardexError::invalid_similarity_score_with_suggestion(f32::INFINITY);
        let negative_error = ShardexError::invalid_similarity_score_with_suggestion(-0.5);
        let large_error = ShardexError::invalid_similarity_score_with_suggestion(1.5);

        let nan_msg = format!("{}", nan_error);
        let inf_msg = format!("{}", inf_error);
        let negative_msg = format!("{}", negative_error);
        let large_msg = format!("{}", large_error);

        assert!(nan_msg.contains("NaN values are not allowed"));
        assert!(inf_msg.contains("Infinite values are not allowed"));
        assert!(negative_msg.contains("Negative similarity scores"));
        assert!(large_msg.contains("must be between 0.0 and 1.0"));
    }
}

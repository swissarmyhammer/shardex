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

    /// Text extraction coordinates are invalid for the document
    #[error("Invalid text range: attempting to extract {start}..{} from document of length {document_length}", start + length)]
    InvalidRange {
        start: u32,
        length: u32,
        document_length: u64,
    },

    /// Document text exceeds configured size limits
    #[error("Document too large: {size} bytes exceeds maximum {max_size} bytes")]
    DocumentTooLarge { size: usize, max_size: usize },

    /// Text storage file corruption detected
    #[error("Text storage corruption: {0}")]
    TextCorruption(String),

    /// Document text not found for the given document ID
    #[error("Document text not found for document ID: {document_id}")]
    DocumentTextNotFound {
        document_id: String, // String representation of DocumentId for display
    },
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
    pub fn invalid_input(field: impl Into<String>, reason: impl Into<String>, suggestion: impl Into<String>) -> Self {
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
    pub fn transient_failure(operation: impl Into<String>, reason: impl Into<String>, retry_count: usize) -> Self {
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
    pub fn corruption_with_recovery(reason: impl Into<String>, recovery_action: impl Into<String>) -> Self {
        Self::Corruption(format!("{}: {}", reason.into(), recovery_action.into()))
    }

    /// Create a detailed config error
    pub fn config_error(field: impl Into<String>, reason: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self::Config(format!("{} - {}: {}", field.into(), reason.into(), suggestion.into()))
    }

    /// Create an invalid text range error
    pub fn invalid_range(start: u32, length: u32, document_length: u64) -> Self {
        Self::InvalidRange {
            start,
            length,
            document_length,
        }
    }

    /// Create a document too large error
    pub fn document_too_large(size: usize, max_size: usize) -> Self {
        Self::DocumentTooLarge { size, max_size }
    }

    /// Create a text storage corruption error
    pub fn text_corruption(reason: impl Into<String>) -> Self {
        Self::TextCorruption(reason.into())
    }

    /// Create a document text not found error
    pub fn document_text_not_found(document_id: impl Into<String>) -> Self {
        Self::DocumentTextNotFound {
            document_id: document_id.into(),
        }
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

    /// Add file context to an error, preserving the original error information
    pub fn with_file_context(self, file_path: impl AsRef<std::path::Path>, operation: &str) -> Self {
        let file_path_str = file_path.as_ref().display().to_string();
        let context = format!("{} (file: {})", operation, file_path_str);

        match self {
            Self::Io(ref err) => Self::Io(std::io::Error::new(err.kind(), format!("{}: {}", context, err))),
            Self::Config(ref msg) => Self::Config(format!("{}: {}", context, msg)),
            Self::MemoryMapping(ref msg) => Self::MemoryMapping(format!("{}: {}", context, msg)),
            Self::Wal(ref msg) => Self::Wal(format!("{}: {}", context, msg)),
            Self::Shard(ref msg) => Self::Shard(format!("{}: {}", context, msg)),
            Self::Search(ref msg) => Self::Search(format!("{}: {}", context, msg)),
            Self::Corruption(ref msg) => Self::Corruption(format!("{}: {}", context, msg)),
            Self::TextCorruption(ref msg) => Self::TextCorruption(format!("{}: {}", context, msg)),
            _ => self, // For structured errors, return as-is to preserve structure
        }
    }

    /// Add operation context to an error, preserving the original error information
    pub fn with_operation_context(self, operation: &str, additional_context: &str) -> Self {
        let context = format!("{}: {}", operation, additional_context);

        match self {
            Self::Io(ref err) => Self::Io(std::io::Error::new(err.kind(), format!("{}: {}", context, err))),
            Self::Config(ref msg) => Self::Config(format!("{}: {}", context, msg)),
            Self::MemoryMapping(ref msg) => Self::MemoryMapping(format!("{}: {}", context, msg)),
            Self::Wal(ref msg) => Self::Wal(format!("{}: {}", context, msg)),
            Self::Shard(ref msg) => Self::Shard(format!("{}: {}", context, msg)),
            Self::Search(ref msg) => Self::Search(format!("{}: {}", context, msg)),
            Self::Corruption(ref msg) => Self::Corruption(format!("{}: {}", context, msg)),
            Self::TextCorruption(ref msg) => Self::TextCorruption(format!("{}: {}", context, msg)),
            Self::InvalidInput {
                field,
                reason,
                suggestion,
            } => Self::InvalidInput {
                field: field.clone(),
                reason: format!("{}: {}", context, reason),
                suggestion: suggestion.clone(),
            },
            Self::InvalidDocumentId { reason, suggestion } => Self::InvalidDocumentId {
                reason: format!("{}: {}", context, reason),
                suggestion: suggestion.clone(),
            },
            Self::InvalidPostingData { reason, suggestion } => Self::InvalidPostingData {
                reason: format!("{}: {}", context, reason),
                suggestion: suggestion.clone(),
            },
            Self::TransientFailure {
                operation: op,
                reason,
                recovery_suggestion,
                retry_count,
            } => Self::TransientFailure {
                operation: format!("{}: {}", context, op),
                reason: reason.clone(),
                recovery_suggestion: recovery_suggestion.clone(),
                retry_count,
            },
            Self::ResourceExhausted {
                resource,
                reason,
                suggestion,
            } => Self::ResourceExhausted {
                resource: resource.clone(),
                reason: format!("{}: {}", context, reason),
                suggestion: suggestion.clone(),
            },
            Self::ConcurrencyError {
                operation: op,
                reason,
                suggestion,
            } => Self::ConcurrencyError {
                operation: format!("{}: {}", context, op),
                reason: reason.clone(),
                suggestion: suggestion.clone(),
            },
            _ => self, // For other structured errors, return as-is
        }
    }

    /// Chain this error with a source error, providing error causality
    pub fn chain_with_source(self, source: Box<dyn std::error::Error + Send + Sync>) -> Self {
        let source_msg = source.to_string();

        match self {
            Self::Config(ref msg) => Self::Config(format!("{}: caused by {}", msg, source_msg)),
            Self::MemoryMapping(ref msg) => Self::MemoryMapping(format!("{}: caused by {}", msg, source_msg)),
            Self::Wal(ref msg) => Self::Wal(format!("{}: caused by {}", msg, source_msg)),
            Self::Shard(ref msg) => Self::Shard(format!("{}: caused by {}", msg, source_msg)),
            Self::Search(ref msg) => Self::Search(format!("{}: caused by {}", msg, source_msg)),
            Self::Corruption(ref msg) => Self::Corruption(format!("{}: caused by {}", msg, source_msg)),
            Self::TextCorruption(ref msg) => Self::TextCorruption(format!("{}: caused by {}", msg, source_msg)),
            _ => self, // For IO errors and structured errors, preserve original
        }
    }

    /// Create a file operation error with context
    pub fn file_operation_failed(
        file_path: impl AsRef<std::path::Path>,
        operation: &str,
        source: std::io::Error,
    ) -> Self {
        Self::Io(source).with_file_context(file_path, operation)
    }

    /// Create a memory mapping error with file context
    pub fn memory_mapping_failed(
        file_path: impl AsRef<std::path::Path>,
        operation: &str,
        reason: impl Into<String>,
    ) -> Self {
        let file_path_str = file_path.as_ref().display().to_string();
        Self::MemoryMapping(format!(
            "{} failed for file {}: {}",
            operation,
            file_path_str,
            reason.into()
        ))
    }

    /// Create a WAL operation error with context
    pub fn wal_operation_failed(operation: &str, context: &str, reason: impl Into<String>) -> Self {
        Self::Wal(format!("{} ({}): {}", operation, context, reason.into()))
    }

    /// Create a shard operation error with context
    pub fn shard_operation_failed(
        shard_id: impl std::fmt::Display,
        operation: &str,
        reason: impl Into<String>,
    ) -> Self {
        Self::Shard(format!("Shard {} {}: {}", shard_id, operation, reason.into()))
    }

    /// Create a search operation error with context
    pub fn search_operation_failed(operation: &str, context: &str, reason: impl Into<String>) -> Self {
        Self::Search(format!("{} ({}): {}", operation, context, reason.into()))
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
        assert_eq!(display_str, "Invalid vector dimension: expected 384, got 512");
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
        assert_eq!(display_str, "Index corruption detected: Magic bytes mismatch");
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
        let error = ShardexError::invalid_input("query_vector", "cannot be empty", "Provide a non-empty vector");
        let display_str = format!("{}", error);
        assert!(display_str.contains("Invalid input: query_vector"));
        assert!(display_str.contains("cannot be empty"));
        assert!(display_str.contains("Provide a non-empty vector"));
    }

    #[test]
    fn test_invalid_document_id_error() {
        let error = ShardexError::invalid_document_id("document ID is zero", "Use a valid non-zero document ID");
        let display_str = format!("{}", error);
        assert!(display_str.contains("Invalid document ID: document ID is zero"));
        assert!(display_str.contains("Use a valid non-zero document ID"));
    }

    #[test]
    fn test_invalid_posting_data_error() {
        let error =
            ShardexError::invalid_posting_data("vector contains NaN values", "Remove NaN values from your data");
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
        let error = ShardexError::resource_exhausted("memory", "heap allocation failed", "Increase available memory");
        let display_str = format!("{}", error);
        assert!(display_str.contains("Resource exhausted: memory"));
        assert!(display_str.contains("heap allocation failed"));
        assert!(display_str.contains("Increase available memory"));
    }

    #[test]
    fn test_concurrency_error() {
        let error =
            ShardexError::concurrency_error("shard_write", "write lock contention", "Reduce concurrent operations");
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
        let posting_error = ShardexError::invalid_dimension_with_context(384, 512, "posting_vector");
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

    #[test]
    fn test_invalid_range_error_display() {
        let error = ShardexError::InvalidRange {
            start: 10,
            length: 20,
            document_length: 25,
        };
        let display_str = format!("{}", error);
        assert_eq!(
            display_str,
            "Invalid text range: attempting to extract 10..30 from document of length 25"
        );
    }

    #[test]
    fn test_document_too_large_error_display() {
        let error = ShardexError::DocumentTooLarge {
            size: 15_000_000,
            max_size: 10_000_000,
        };
        let display_str = format!("{}", error);
        assert_eq!(
            display_str,
            "Document too large: 15000000 bytes exceeds maximum 10000000 bytes"
        );
    }

    #[test]
    fn test_text_corruption_error_display() {
        let error = ShardexError::TextCorruption("Invalid UTF-8 sequence".to_string());
        let display_str = format!("{}", error);
        assert_eq!(display_str, "Text storage corruption: Invalid UTF-8 sequence");
    }

    #[test]
    fn test_document_text_not_found_error_display() {
        let error = ShardexError::DocumentTextNotFound {
            document_id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
        };
        let display_str = format!("{}", error);
        assert_eq!(
            display_str,
            "Document text not found for document ID: 01ARZ3NDEKTSV4RRFFQ69G5FAV"
        );
    }

    #[test]
    fn test_invalid_range_helper_method() {
        let error = ShardexError::invalid_range(10, 20, 25);
        let display_str = format!("{}", error);
        assert!(display_str.contains("Invalid text range"));
        assert!(display_str.contains("10..30"));
        assert!(display_str.contains("document of length 25"));
    }

    #[test]
    fn test_document_too_large_helper_method() {
        let error = ShardexError::document_too_large(15_000_000, 10_000_000);
        let display_str = format!("{}", error);
        assert!(display_str.contains("Document too large"));
        assert!(display_str.contains("15000000 bytes"));
        assert!(display_str.contains("maximum 10000000 bytes"));
    }

    #[test]
    fn test_text_corruption_helper_method() {
        let error = ShardexError::text_corruption("Invalid UTF-8 sequence");
        let display_str = format!("{}", error);
        assert!(display_str.contains("Text storage corruption"));
        assert!(display_str.contains("Invalid UTF-8 sequence"));
    }

    #[test]
    fn test_document_text_not_found_helper_method() {
        let document_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let error = ShardexError::document_text_not_found(document_id);
        let display_str = format!("{}", error);
        assert!(display_str.contains("Document text not found"));
        assert!(display_str.contains(document_id));
    }

    #[test]
    fn test_document_text_error_classification() {
        let range_error = ShardexError::invalid_range(10, 20, 25);
        let too_large_error = ShardexError::document_too_large(15_000_000, 10_000_000);
        let corruption_error = ShardexError::text_corruption("test corruption");
        let not_found_error = ShardexError::document_text_not_found("test_id");

        // Document text errors should not be transient or recoverable
        // (they represent user input errors or data corruption)
        assert!(!range_error.is_transient());
        assert!(!range_error.is_recoverable());

        assert!(!too_large_error.is_transient());
        assert!(!too_large_error.is_recoverable());

        assert!(!corruption_error.is_transient());
        assert!(!corruption_error.is_recoverable());

        assert!(!not_found_error.is_transient());
        assert!(!not_found_error.is_recoverable());
    }

    #[test]
    fn test_with_file_context() {
        use std::path::Path;

        let io_error = ShardexError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"));
        let contextual_error = io_error.with_file_context(Path::new("/tmp/test.dat"), "reading shard data");

        let error_str = format!("{}", contextual_error);
        assert!(error_str.contains("reading shard data"));
        assert!(error_str.contains("/tmp/test.dat"));
        assert!(error_str.contains("File not found"));
    }

    #[test]
    fn test_with_file_context_config_error() {
        use std::path::Path;

        let config_error = ShardexError::Config("Invalid dimension".to_string());
        let contextual_error = config_error.with_file_context(Path::new("/etc/shardex.conf"), "parsing configuration");

        let error_str = format!("{}", contextual_error);
        assert!(error_str.contains("parsing configuration"));
        assert!(error_str.contains("/etc/shardex.conf"));
        assert!(error_str.contains("Invalid dimension"));
    }

    #[test]
    fn test_with_operation_context() {
        let config_error = ShardexError::Config("Missing field".to_string());
        let contextual_error = config_error.with_operation_context("index initialization", "validating shard count");

        let error_str = format!("{}", contextual_error);
        assert!(error_str.contains("index initialization"));
        assert!(error_str.contains("validating shard count"));
        assert!(error_str.contains("Missing field"));
    }

    #[test]
    fn test_with_operation_context_invalid_input() {
        let input_error = ShardexError::InvalidInput {
            field: "vector_dimension".to_string(),
            reason: "must be positive".to_string(),
            suggestion: "Provide a positive integer".to_string(),
        };
        let contextual_error = input_error.with_operation_context("posting validation", "checking vector dimensions");

        let error_str = format!("{}", contextual_error);
        assert!(error_str.contains("posting validation"));
        assert!(error_str.contains("checking vector dimensions"));
        assert!(error_str.contains("must be positive"));
        assert!(error_str.contains("Provide a positive integer"));
    }

    #[test]
    fn test_with_operation_context_transient_failure() {
        let transient_error = ShardexError::TransientFailure {
            operation: "disk write".to_string(),
            reason: "temporary space issue".to_string(),
            recovery_suggestion: "retry after cleanup".to_string(),
            retry_count: 1,
        };
        let contextual_error = transient_error.with_operation_context("shard persistence", "syncing to disk");

        let error_str = format!("{}", contextual_error);
        assert!(error_str.contains("shard persistence"));
        assert!(error_str.contains("syncing to disk"));
        assert!(error_str.contains("temporary space issue"));
        assert!(error_str.contains("retry after cleanup"));
    }

    #[test]
    fn test_chain_with_source() {
        let config_error = ShardexError::Config("Parse error".to_string());
        let source_error = std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid JSON");
        let chained_error = config_error.chain_with_source(Box::new(source_error));

        let error_str = format!("{}", chained_error);
        assert!(error_str.contains("Parse error"));
        assert!(error_str.contains("caused by"));
        assert!(error_str.contains("Invalid JSON"));
    }

    #[test]
    fn test_file_operation_failed() {
        use std::path::Path;

        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access denied");
        let error = ShardexError::file_operation_failed(Path::new("/data/shard.bin"), "opening shard file", io_error);

        let error_str = format!("{}", error);
        assert!(error_str.contains("opening shard file"));
        assert!(error_str.contains("/data/shard.bin"));
        assert!(error_str.contains("Access denied"));
    }

    #[test]
    fn test_memory_mapping_failed() {
        use std::path::Path;

        let error = ShardexError::memory_mapping_failed(
            Path::new("/data/vector.mmap"),
            "memory mapping vectors",
            "insufficient virtual memory",
        );

        let error_str = format!("{}", error);
        assert!(error_str.contains("memory mapping vectors failed"));
        assert!(error_str.contains("/data/vector.mmap"));
        assert!(error_str.contains("insufficient virtual memory"));
    }

    #[test]
    fn test_wal_operation_failed() {
        let error = ShardexError::wal_operation_failed("log rotation", "segment 5", "disk full");

        let error_str = format!("{}", error);
        assert!(error_str.contains("log rotation"));
        assert!(error_str.contains("segment 5"));
        assert!(error_str.contains("disk full"));
    }

    #[test]
    fn test_shard_operation_failed() {
        let error = ShardexError::shard_operation_failed(42, "split operation", "insufficient data");

        let error_str = format!("{}", error);
        assert!(error_str.contains("Shard 42"));
        assert!(error_str.contains("split operation"));
        assert!(error_str.contains("insufficient data"));
    }

    #[test]
    fn test_search_operation_failed() {
        let error = ShardexError::search_operation_failed("vector similarity search", "shard 7", "corrupted index");

        let error_str = format!("{}", error);
        assert!(error_str.contains("vector similarity search"));
        assert!(error_str.contains("shard 7"));
        assert!(error_str.contains("corrupted index"));
    }

    #[test]
    fn test_context_chaining() {
        use std::path::Path;

        // Test that multiple context applications work correctly
        let base_error = ShardexError::Config("Invalid setting".to_string());
        let error_with_file = base_error.with_file_context(Path::new("/etc/config.toml"), "loading config");
        let error_with_operation = error_with_file.with_operation_context("startup", "initializing system");

        let error_str = format!("{}", error_with_operation);
        assert!(error_str.contains("startup"));
        assert!(error_str.contains("initializing system"));
        assert!(error_str.contains("loading config"));
        assert!(error_str.contains("/etc/config.toml"));
        assert!(error_str.contains("Invalid setting"));
    }

    #[test]
    fn test_context_preserves_error_classification() {
        let transient_error = ShardexError::TransientFailure {
            operation: "test".to_string(),
            reason: "test".to_string(),
            recovery_suggestion: "test".to_string(),
            retry_count: 1,
        };

        let contextual_error = transient_error.with_operation_context("test_op", "test_context");

        // Ensure error classification is preserved after adding context
        assert!(contextual_error.is_transient());
        assert!(contextual_error.is_recoverable());
        assert_eq!(contextual_error.retry_count(), Some(1));
    }

    #[test]
    fn test_structured_error_context_preservation() {
        // Test that structured errors preserve their structure when context is added
        let invalid_input = ShardexError::InvalidInput {
            field: "test_field".to_string(),
            reason: "test_reason".to_string(),
            suggestion: "test_suggestion".to_string(),
        };

        let contextual_error = invalid_input.with_operation_context("validation", "checking input");

        match contextual_error {
            ShardexError::InvalidInput {
                field,
                reason,
                suggestion,
            } => {
                assert_eq!(field, "test_field");
                assert!(reason.contains("validation"));
                assert!(reason.contains("checking input"));
                assert!(reason.contains("test_reason"));
                assert_eq!(suggestion, "test_suggestion");
            }
            _ => panic!("Expected InvalidInput variant to be preserved"),
        }
    }

    #[test]
    fn test_error_context_integration_validation() {
        use std::path::Path;

        // Comprehensive test of all error context enhancement features
        let base_error = ShardexError::Config("Invalid vector dimension setting".to_string());

        // Test file context
        let file_error =
            base_error.with_file_context(Path::new("/etc/shardex/index.toml"), "loading index configuration");

        // Test operation context
        let operation_error = file_error.with_operation_context("system initialization", "preparing search engine");

        let error_msg = format!("{}", operation_error);

        // Verify all context information is present
        assert!(error_msg.contains("system initialization"));
        assert!(error_msg.contains("preparing search engine"));
        assert!(error_msg.contains("loading index configuration"));
        assert!(error_msg.contains("/etc/shardex/index.toml"));
        assert!(error_msg.contains("Invalid vector dimension setting"));

        // Test helper methods
        let shard_error =
            ShardexError::shard_operation_failed(42, "split operation", "insufficient vectors for clustering");
        let shard_msg = format!("{}", shard_error);
        assert!(shard_msg.contains("Shard 42"));
        assert!(shard_msg.contains("split operation"));
        assert!(shard_msg.contains("insufficient vectors for clustering"));

        // Test transient error classification preservation
        let transient = ShardexError::transient_failure("network write", "connection timeout", 3);
        let contextual_transient = transient.with_operation_context("data persistence", "syncing to storage");

        assert!(contextual_transient.is_transient());
        assert!(contextual_transient.is_recoverable());
        assert_eq!(contextual_transient.retry_count(), Some(3));
    }
}

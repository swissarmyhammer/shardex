//! Integration tests for error context enhancement
//!
//! This module validates that the new error context enhancement features work correctly
//! in real-world scenarios and provide meaningful error information for debugging.

#[cfg(test)]
mod tests {
    use crate::error::ShardexError;
    use crate::{
        assert_error_contains, assert_error_type,
        test_utils::error::{
            assert_error_causality, assert_error_context, create_test_io_error, expect_error, expect_success,
        },
    };
    use std::path::Path;

    #[test]
    fn test_error_with_file_context_enhancement() {
        let base_error = ShardexError::Config("Missing vector_size parameter".to_string());
        let enhanced_error =
            base_error.with_file_context(Path::new("/etc/shardex/config.toml"), "loading configuration");

        let error_msg = format!("{}", enhanced_error);
        assert!(error_msg.contains("loading configuration"));
        assert!(error_msg.contains("/etc/shardex/config.toml"));
        assert!(error_msg.contains("Missing vector_size parameter"));
    }

    #[test]
    fn test_error_with_operation_context_enhancement() {
        let base_error = ShardexError::MemoryMapping("Failed to map file region".to_string());
        let enhanced_error = base_error.with_operation_context("shard initialization", "mapping vector storage");

        let error_msg = format!("{}", enhanced_error);
        assert!(error_msg.contains("shard initialization"));
        assert!(error_msg.contains("mapping vector storage"));
        assert!(error_msg.contains("Failed to map file region"));
    }

    #[test]
    fn test_error_chaining_with_context() {
        let base_error = ShardexError::Config("Parse error".to_string());
        let file_context_error = base_error.with_file_context(Path::new("/data/index.json"), "reading metadata");
        let operation_context_error =
            file_context_error.with_operation_context("index recovery", "restoring from backup");

        let error_msg = format!("{}", operation_context_error);
        assert!(error_msg.contains("index recovery"));
        assert!(error_msg.contains("restoring from backup"));
        assert!(error_msg.contains("reading metadata"));
        assert!(error_msg.contains("/data/index.json"));
        assert!(error_msg.contains("Parse error"));
    }

    #[test]
    fn test_error_causality_chain() {
        let io_error = create_test_io_error("Permission denied");
        let config_error = ShardexError::Config("Cannot load configuration".to_string());
        let chained_error = config_error.chain_with_source(Box::new(io_error));

        assert_error_causality(
            &chained_error,
            &["Cannot load configuration", "caused by", "Permission denied"],
        );
    }

    #[test]
    fn test_structured_error_context_preservation() {
        let input_error = ShardexError::InvalidInput {
            field: "query_vector".to_string(),
            reason: "empty vector provided".to_string(),
            suggestion: "Provide a non-empty vector with valid dimensions".to_string(),
        };

        let enhanced_error =
            input_error.with_operation_context("vector similarity search", "validating query parameters");

        // Verify the structure is preserved
        match enhanced_error {
            ShardexError::InvalidInput {
                field,
                reason,
                suggestion,
            } => {
                assert_eq!(field, "query_vector");
                assert!(reason.contains("vector similarity search"));
                assert!(reason.contains("validating query parameters"));
                assert!(reason.contains("empty vector provided"));
                assert_eq!(suggestion, "Provide a non-empty vector with valid dimensions");
            }
            _ => panic!("Expected InvalidInput variant to be preserved"),
        }
    }

    #[test]
    fn test_file_operation_error_helper() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let error =
            ShardexError::file_operation_failed(Path::new("/data/shard_42.bin"), "opening shard for reading", io_error);

        assert_error_context(
            &error,
            &["opening shard for reading", "/data/shard_42.bin", "File not found"],
        );
    }

    #[test]
    fn test_memory_mapping_error_helper() {
        let error = ShardexError::memory_mapping_failed(
            Path::new("/tmp/vectors.mmap"),
            "memory mapping vector storage",
            "insufficient virtual address space",
        );

        assert_error_context(
            &error,
            &[
                "memory mapping vector storage failed",
                "/tmp/vectors.mmap",
                "insufficient virtual address space",
            ],
        );
    }

    #[test]
    fn test_wal_operation_error_helper() {
        let error = ShardexError::wal_operation_failed("segment rotation", "WAL segment 7", "disk space exhausted");

        assert_error_context(&error, &["segment rotation", "WAL segment 7", "disk space exhausted"]);
    }

    #[test]
    fn test_shard_operation_error_helper() {
        let error =
            ShardexError::shard_operation_failed(12345, "splitting operation", "insufficient data for clustering");

        assert_error_context(
            &error,
            &["Shard 12345", "splitting operation", "insufficient data for clustering"],
        );
    }

    #[test]
    fn test_search_operation_error_helper() {
        let error =
            ShardexError::search_operation_failed("k-nearest neighbors", "shard 8", "vector dimension mismatch");

        assert_error_context(&error, &["k-nearest neighbors", "shard 8", "vector dimension mismatch"]);
    }

    #[test]
    fn test_test_utilities_assert_error_type() {
        let config_error = ShardexError::Config("test error".to_string());
        let result: Result<(), ShardexError> = Err(config_error);

        assert_error_type!(result, Config);
    }

    #[test]
    fn test_test_utilities_assert_error_contains() {
        let search_error = ShardexError::Search("query vector dimension mismatch".to_string());
        let result: Result<(), ShardexError> = Err(search_error);

        assert_error_contains!(result, "query vector");
        assert_error_contains!(result, "dimension mismatch");
    }

    #[test]
    fn test_test_utilities_expect_error() {
        let corruption_error = ShardexError::Corruption("magic bytes invalid".to_string());
        let result: Result<(), ShardexError> = Err(corruption_error);

        let error = expect_error(result, "corruption detection should fail");
        match error {
            ShardexError::Corruption(msg) => {
                assert!(msg.contains("magic bytes invalid"));
            }
            _ => panic!("Expected Corruption error"),
        }
    }

    #[test]
    fn test_test_utilities_expect_success() {
        let result: Result<i32, ShardexError> = Ok(42);
        let value = expect_success(result, "arithmetic should succeed");
        assert_eq!(value, 42);
    }

    #[test]
    fn test_error_classification_preservation() {
        // Create a transient error and add context
        let transient_error = ShardexError::transient_failure("disk write", "temporary storage full", 2);
        let enhanced_error = transient_error.with_operation_context("shard persistence", "flushing to disk");

        // Verify error classification is preserved after context enhancement
        assert!(enhanced_error.is_transient());
        assert!(enhanced_error.is_recoverable());
        assert_eq!(enhanced_error.retry_count(), Some(2));
    }

    #[test]
    fn test_io_error_context_enhancement() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access denied");
        let base_error = ShardexError::Io(io_error);
        let enhanced_error = base_error.with_file_context(Path::new("/secure/data.bin"), "reading secure storage");

        let error_msg = format!("{}", enhanced_error);
        assert!(error_msg.contains("reading secure storage"));
        assert!(error_msg.contains("/secure/data.bin"));
        assert!(error_msg.contains("Access denied"));
    }

    #[test]
    fn test_multiple_context_layering() {
        // Start with a basic error and layer multiple contexts
        let base_error = ShardexError::Config("Invalid setting value".to_string());

        let file_enhanced = base_error.with_file_context(Path::new("/etc/shardex.toml"), "parsing configuration file");

        let operation_enhanced = file_enhanced.with_operation_context("system startup", "initializing search engine");

        let final_enhanced =
            operation_enhanced.with_operation_context("service bootstrap", "preparing application environment");

        let error_msg = format!("{}", final_enhanced);

        // Verify all context layers are present
        assert!(error_msg.contains("service bootstrap"));
        assert!(error_msg.contains("preparing application environment"));
        assert!(error_msg.contains("system startup"));
        assert!(error_msg.contains("initializing search engine"));
        assert!(error_msg.contains("parsing configuration file"));
        assert!(error_msg.contains("/etc/shardex.toml"));
        assert!(error_msg.contains("Invalid setting value"));
    }
}

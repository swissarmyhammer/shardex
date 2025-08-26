//! Error handling tests for document text storage
//!
//! Tests comprehensive error scenarios including:
//! - Invalid range errors and boundary conditions
//! - Document not found errors
//! - Size limit violations
//! - UTF-8 boundary validation
//! - Corruption detection and handling
//! - Recovery from error states

use shardex::document_text_storage::DocumentTextStorage;
use shardex::error::ShardexError;
use shardex::identifiers::DocumentId;
use tempfile::TempDir;

#[test]
fn test_invalid_range_errors() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let doc_id = DocumentId::new();
    let text = "Short text for range testing.";
    storage.store_text_safe(doc_id, text).unwrap();

    // Test range beyond document end
    let result = storage.extract_text_substring(doc_id, 5, 50);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange {
            start,
            length,
            document_length,
        } => {
            assert_eq!(start, 5);
            assert_eq!(length, 50);
            assert_eq!(document_length, text.len() as u64);
        }
        e => panic!("Expected InvalidRange error, got {:?}", e),
    }

    // Test start beyond document
    let result = storage.extract_text_substring(doc_id, 100, 5);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange {
            start,
            length,
            document_length,
        } => {
            assert_eq!(start, 100);
            assert_eq!(length, 5);
            assert_eq!(document_length, text.len() as u64);
        }
        e => panic!("Expected InvalidRange error, got {:?}", e),
    }

    // Test zero length extraction
    let result = storage.extract_text_substring(doc_id, 0, 0);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange {
            start,
            length,
            document_length,
        } => {
            assert_eq!(start, 0);
            assert_eq!(length, 0);
            assert_eq!(document_length, text.len() as u64);
        }
        e => panic!("Expected InvalidRange error, got {:?}", e),
    }

    // Test valid range (should succeed)
    let result = storage.extract_text_substring(doc_id, 0, 5);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "Short");
}

#[test]
fn test_document_not_found_errors() {
    let temp_dir = TempDir::new().unwrap();
    let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let nonexistent_doc = DocumentId::new();

    // Test get_text_safe with nonexistent document
    let result = storage.get_text_safe(nonexistent_doc);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::DocumentTextNotFound { document_id } => {
            assert_eq!(document_id, nonexistent_doc.to_string());
        }
        e => panic!("Expected DocumentTextNotFound error, got {:?}", e),
    }

    // Test extract_text_substring with nonexistent document
    let result = storage.extract_text_substring(nonexistent_doc, 0, 5);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::DocumentTextNotFound { document_id } => {
            assert_eq!(document_id, nonexistent_doc.to_string());
        }
        e => panic!("Expected DocumentTextNotFound error, got {:?}", e),
    }

    // Test get_text with nonexistent document (basic method)
    let result = storage.get_text(nonexistent_doc);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::DocumentTextNotFound { .. } => {
            // Expected error type
        }
        e => panic!("Expected DocumentTextNotFound error, got {:?}", e),
    }
}

#[test]
fn test_document_size_limit_errors() {
    let temp_dir = TempDir::new().unwrap();
    let small_limit = 100;
    let mut storage = DocumentTextStorage::create(&temp_dir, small_limit).unwrap();

    let doc_id = DocumentId::new();

    // Test exactly at limit (should succeed)
    let at_limit_text = "x".repeat(small_limit);
    let result = storage.store_text_safe(doc_id, &at_limit_text);
    assert!(result.is_ok());

    // Test just over limit (should fail)
    let over_limit_doc = DocumentId::new();
    let over_limit_text = "x".repeat(small_limit + 1);
    let result = storage.store_text_safe(over_limit_doc, &over_limit_text);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::DocumentTooLarge { size, max_size } => {
            assert_eq!(size, small_limit + 1);
            assert_eq!(max_size, small_limit);
        }
        e => panic!("Expected DocumentTooLarge error, got {:?}", e),
    }

    // Test way over limit (should fail)
    let way_over_limit_doc = DocumentId::new();
    let way_over_limit_text = "x".repeat(small_limit * 10);
    let result = storage.store_text_safe(way_over_limit_doc, &way_over_limit_text);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::DocumentTooLarge { size, max_size } => {
            assert_eq!(size, small_limit * 10);
            assert_eq!(max_size, small_limit);
        }
        e => panic!("Expected DocumentTooLarge error, got {:?}", e),
    }

    // Verify original document still works after errors
    assert_eq!(storage.get_text_safe(doc_id).unwrap(), at_limit_text);
}

#[test]
fn test_empty_text_validation_error() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let doc_id = DocumentId::new();

    // Test empty text with store_text
    let result = storage.store_text(doc_id, "");
    assert!(result.is_err());
    // The error comes from DocumentTextEntry validation
    match result.unwrap_err() {
        ShardexError::InvalidInput { field, reason, .. } => {
            assert_eq!(field, "text_length");
            assert!(reason.contains("cannot be zero"));
        }
        e => panic!("Expected InvalidInput error for text_length, got {:?}", e),
    }

    // Test empty text with store_text_safe
    let result = storage.store_text_safe(doc_id, "");
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidInput { field, reason, .. } => {
            assert_eq!(field, "text_length");
            assert!(reason.contains("cannot be zero"));
        }
        e => panic!("Expected InvalidInput error for text_length, got {:?}", e),
    }

    // Verify no documents were stored
    assert_eq!(storage.entry_count(), 0);
    assert!(storage.is_empty());
}

#[test]
fn test_utf8_validation_errors() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let doc_id = DocumentId::new();

    // Test text with null bytes (should fail with store_text_safe)
    let text_with_null = "Hello\x00World";
    let result = storage.store_text_safe(doc_id, text_with_null);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidInput { field, reason, .. } => {
            assert_eq!(field, "document_text");
            assert!(reason.contains("null bytes"));
        }
        e => panic!("Expected InvalidInput error for null bytes, got {:?}", e),
    }

    // Note: Rust's str type guarantees UTF-8 validity, so we can't easily test invalid UTF-8
    // at the string level. Invalid UTF-8 would be caught at the byte level in real scenarios.
}

#[test]
fn test_utf8_boundary_errors() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let doc_id = DocumentId::new();
    let unicode_text = "Héllo Wörld! 🌍🚀"; // Contains multi-byte UTF-8 characters
    storage.store_text_safe(doc_id, unicode_text).unwrap();

    // Test extraction that would split a UTF-8 character
    // "Héllo" - 'é' is 2 bytes in UTF-8, at position 1-2
    let result = storage.extract_text_substring(doc_id, 2, 2); // Would split the 'é'
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange { .. } => {
            // Expected error for invalid UTF-8 boundary
        }
        e => panic!("Expected InvalidRange error for UTF-8 boundary, got {:?}", e),
    }

    // Test extraction starting in the middle of a multi-byte character
    // "🌍" (earth emoji) is 4 bytes in UTF-8
    let earth_pos = unicode_text.find('🌍').unwrap();
    let result = storage.extract_text_substring(doc_id, earth_pos as u32 + 1, 2);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange { .. } => {
            // Expected error for invalid UTF-8 boundary
        }
        e => panic!("Expected InvalidRange error for UTF-8 boundary, got {:?}", e),
    }

    // Test valid UTF-8 boundary extraction (should succeed)
    let result = storage.extract_text_substring(doc_id, 0, 7); // "Héllo " (7 bytes includes the space)
    assert!(result.is_ok());
    let extracted = result.unwrap();
    assert_eq!(extracted, "Héllo ");
}

#[test]
fn test_boundary_conditions_comprehensive() {
    let temp_dir = TempDir::new().unwrap();
    let limit = 1000;
    let mut storage = DocumentTextStorage::create(&temp_dir, limit).unwrap();

    let doc_id = DocumentId::new();
    let text = "A".repeat(limit); // Exactly at limit
    storage.store_text_safe(doc_id, &text).unwrap();

    // Test extraction at exact document end
    let result = storage.extract_text_substring(doc_id, (limit - 1) as u32, 1);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "A");

    // Test extraction beyond document end
    let result = storage.extract_text_substring(doc_id, (limit - 1) as u32, 2);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange {
            start,
            length,
            document_length,
        } => {
            assert_eq!(start, (limit - 1) as u32);
            assert_eq!(length, 2);
            assert_eq!(document_length, limit as u64);
        }
        e => panic!("Expected InvalidRange error, got {:?}", e),
    }

    // Test extraction starting at document end
    let result = storage.extract_text_substring(doc_id, limit as u32, 1);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange {
            start,
            length,
            document_length,
        } => {
            assert_eq!(start, limit as u32);
            assert_eq!(length, 1);
            assert_eq!(document_length, limit as u64);
        }
        e => panic!("Expected InvalidRange error, got {:?}", e),
    }

    // Test maximum valid extraction
    let result = storage.extract_text_substring(doc_id, 0, limit as u32);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().len(), limit);
}

#[test]
fn test_integer_overflow_conditions() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let doc_id = DocumentId::new();
    let text = "Test text for overflow conditions.";
    storage.store_text_safe(doc_id, text).unwrap();

    // Test u32::MAX values that would cause overflow
    let result = storage.extract_text_substring(doc_id, u32::MAX, 1);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange {
            start,
            length,
            document_length,
        } => {
            assert_eq!(start, u32::MAX);
            assert_eq!(length, 1);
            assert_eq!(document_length, text.len() as u64);
        }
        e => panic!("Expected InvalidRange error, got {:?}", e),
    }

    // Test large start position
    let result = storage.extract_text_substring(doc_id, u32::MAX - 10, 20);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange { .. } => {
            // Expected error
        }
        e => panic!("Expected InvalidRange error, got {:?}", e),
    }

    // Test large length
    let result = storage.extract_text_substring(doc_id, 0, u32::MAX);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidRange {
            start,
            length,
            document_length,
        } => {
            assert_eq!(start, 0);
            assert_eq!(length, u32::MAX);
            assert_eq!(document_length, text.len() as u64);
        }
        e => panic!("Expected InvalidRange error, got {:?}", e),
    }
}

#[test]
fn test_concurrent_error_scenarios() {
    // Test behavior when errors occur during multiple operations
    let temp_dir = TempDir::new().unwrap();
    let small_limit = 50;
    let mut storage = DocumentTextStorage::create(&temp_dir, small_limit).unwrap();

    let valid_docs = vec![
        (DocumentId::new(), "Valid doc 1"),
        (DocumentId::new(), "Valid doc 2"),
        (DocumentId::new(), "Valid doc 3"),
    ];

    let invalid_docs = vec![
        (
            DocumentId::new(),
            "This text is way too long for the small limit we have set",
        ),
        (DocumentId::new(), "Another text that exceeds the configured size limit"),
    ];

    // Store valid documents
    for (doc_id, text) in &valid_docs {
        storage.store_text_safe(*doc_id, text).unwrap();
    }

    // Try to store invalid documents (should fail but not corrupt storage)
    for (doc_id, text) in &invalid_docs {
        let result = storage.store_text_safe(*doc_id, text);
        assert!(result.is_err());
        match result.unwrap_err() {
            ShardexError::DocumentTooLarge { .. } => {
                // Expected error type
            }
            e => panic!("Expected DocumentTooLarge error, got {:?}", e),
        }
    }

    // Verify valid documents are still accessible
    for (doc_id, expected_text) in &valid_docs {
        let retrieved = storage.get_text_safe(*doc_id).unwrap();
        assert_eq!(retrieved, *expected_text);
    }

    // Verify invalid documents are not found
    for (doc_id, _) in &invalid_docs {
        let result = storage.get_text_safe(*doc_id);
        assert!(result.is_err());
        match result.unwrap_err() {
            ShardexError::DocumentTextNotFound { .. } => {
                // Expected error type
            }
            e => panic!("Expected DocumentTextNotFound error, got {:?}", e),
        }
    }

    // Verify storage statistics are correct
    assert_eq!(storage.entry_count(), valid_docs.len() as u32);
}

#[test]
fn test_error_propagation_chain() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 100).unwrap();

    let doc_id = DocumentId::new();
    let oversized_text = "This text is intentionally too large to test error propagation through the storage system. This additional text makes it exceed the 100 byte limit that was set for this test.";

    // Test error propagation through different methods
    let store_result = storage.store_text(doc_id, oversized_text);
    assert!(store_result.is_err());

    let store_safe_result = storage.store_text_safe(doc_id, oversized_text);
    assert!(store_safe_result.is_err());

    // Both should produce the same error type
    match (store_result.unwrap_err(), store_safe_result.unwrap_err()) {
        (
            ShardexError::DocumentTooLarge { size: s1, max_size: m1 },
            ShardexError::DocumentTooLarge { size: s2, max_size: m2 },
        ) => {
            assert_eq!(s1, s2);
            assert_eq!(m1, m2);
        }
        (e1, e2) => panic!("Expected matching DocumentTooLarge errors, got {:?} and {:?}", e1, e2),
    }
}

#[test]
fn test_storage_state_after_errors() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 100).unwrap();

    // Initial state
    assert!(storage.is_empty());
    assert_eq!(storage.entry_count(), 0);
    assert_eq!(storage.total_text_size(), 0);

    // Store valid document
    let valid_doc = DocumentId::new();
    let valid_text = "Valid text.";
    storage.store_text_safe(valid_doc, valid_text).unwrap();

    assert!(!storage.is_empty());
    assert_eq!(storage.entry_count(), 1);
    assert_eq!(storage.total_text_size(), valid_text.len() as u64);

    // Try to store invalid document
    let invalid_doc = DocumentId::new();
    let invalid_text = "This text is way too long for the configured 100 character limit and should definitely fail validation during storage.";
    let result = storage.store_text_safe(invalid_doc, invalid_text);
    assert!(result.is_err());

    // Storage state should be unchanged after error
    assert!(!storage.is_empty());
    assert_eq!(storage.entry_count(), 1); // Still only the valid document
    assert_eq!(storage.total_text_size(), valid_text.len() as u64);

    // Valid document should still be retrievable
    assert_eq!(storage.get_text_safe(valid_doc).unwrap(), valid_text);

    // Invalid document should not be found
    let result = storage.get_text_safe(invalid_doc);
    assert!(result.is_err());
}

#[test]
fn test_extraction_error_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let doc_id = DocumentId::new();
    let text = "Recovery test document.";
    storage.store_text_safe(doc_id, text).unwrap();

    // Perform invalid extractions
    let invalid_extractions = vec![
        (50, 10),      // Start beyond document
        (10, 50),      // Length beyond document
        (0, 0),        // Zero length
        (u32::MAX, 1), // Extreme values
    ];

    for (start, length) in invalid_extractions {
        let result = storage.extract_text_substring(doc_id, start, length);
        assert!(
            result.is_err(),
            "Expected error for extraction {}..{}",
            start,
            start + length
        );
    }

    // After all the failed extractions, valid operations should still work
    assert_eq!(storage.get_text_safe(doc_id).unwrap(), text);

    let valid_extraction = storage.extract_text_substring(doc_id, 0, 8).unwrap();
    assert_eq!(valid_extraction, "Recovery");

    let another_valid = storage.extract_text_substring(doc_id, 9, 4).unwrap();
    assert_eq!(another_valid, "test");
}

#[test]
fn test_error_message_accuracy() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 100).unwrap();

    let doc_id = DocumentId::new();
    let text = "Test text for error messages.";
    storage.store_text_safe(doc_id, text).unwrap();

    // Test InvalidRange error message
    let result = storage.extract_text_substring(doc_id, 10, 50);
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_msg = format!("{}", error);

    assert!(error_msg.contains("Invalid text range"));
    assert!(error_msg.contains("10..60")); // start..(start+length)
    assert!(error_msg.contains(&format!("{}", text.len()))); // document length

    // Test DocumentTooLarge error message
    let large_text = "x".repeat(150);
    let result = storage.store_text_safe(DocumentId::new(), &large_text);
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_msg = format!("{}", error);

    assert!(error_msg.contains("Document too large"));
    assert!(error_msg.contains("150")); // actual size
    assert!(error_msg.contains("100")); // max size

    // Test DocumentTextNotFound error message
    let nonexistent_doc = DocumentId::new();
    let result = storage.get_text_safe(nonexistent_doc);
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_msg = format!("{}", error);

    assert!(error_msg.contains("Document text not found"));
    assert!(error_msg.contains(&nonexistent_doc.to_string()));
}

#[test]
fn test_error_classification() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 100).unwrap();

    let doc_id = DocumentId::new();
    let text = "Test text.";
    storage.store_text_safe(doc_id, text).unwrap();

    // Create various error types and test their classification
    let range_error = storage.extract_text_substring(doc_id, 50, 10).unwrap_err();
    let size_error = storage
        .store_text_safe(DocumentId::new(), &"x".repeat(200))
        .unwrap_err();
    let not_found_error = storage.get_text_safe(DocumentId::new()).unwrap_err();
    let null_bytes_error = storage
        .store_text_safe(DocumentId::new(), "text\x00null")
        .unwrap_err();

    // Test error classification properties
    assert!(!range_error.is_transient());
    assert!(!range_error.is_recoverable());
    assert_eq!(range_error.retry_count(), None);

    assert!(!size_error.is_transient());
    assert!(!size_error.is_recoverable());
    assert_eq!(size_error.retry_count(), None);

    assert!(!not_found_error.is_transient());
    assert!(!not_found_error.is_recoverable());
    assert_eq!(not_found_error.retry_count(), None);

    assert!(!null_bytes_error.is_transient());
    assert!(!null_bytes_error.is_recoverable());
    assert_eq!(null_bytes_error.retry_count(), None);
}

#[test]
fn test_stress_error_conditions() {
    let temp_dir = TempDir::new().unwrap();
    let tiny_limit = 20; // Very small limit
    let mut storage = DocumentTextStorage::create(&temp_dir, tiny_limit).unwrap();

    // Generate many error conditions rapidly
    for i in 0..100 {
        let doc_id = DocumentId::new();
        let large_text = format!("This is document {} that is too large for the tiny limit", i);

        let result = storage.store_text_safe(doc_id, &large_text);
        assert!(result.is_err());

        // Verify storage remains consistent after each error
        assert_eq!(storage.entry_count(), 0); // No documents should be stored
        assert!(storage.is_empty());
    }

    // After many errors, storage should still work for valid operations
    let valid_doc = DocumentId::new();
    let valid_text = "Small text."; // Within limit
    storage.store_text_safe(valid_doc, valid_text).unwrap();

    assert_eq!(storage.entry_count(), 1);
    assert_eq!(storage.get_text_safe(valid_doc).unwrap(), valid_text);
}

#[test]
fn test_mixed_success_failure_patterns() {
    let temp_dir = TempDir::new().unwrap();
    let limit = 50;
    let mut storage = DocumentTextStorage::create(&temp_dir, limit).unwrap();

    let operations = vec![
        (DocumentId::new(), "Valid 1", true),
        (
            DocumentId::new(),
            "This text is definitely too long for the 50 character limit set in this test",
            false,
        ),
        (DocumentId::new(), "Valid 2", true),
        (
            DocumentId::new(),
            "Another text that clearly exceeds the configured 50 character limit for testing",
            false,
        ),
        (DocumentId::new(), "Valid 3", true),
        (DocumentId::new(), "text\x00null", false), // Null bytes
        (DocumentId::new(), "Valid 4", true),
    ];

    let mut successful_docs = Vec::new();
    let mut failed_docs = Vec::new();

    for (doc_id, text, should_succeed) in operations {
        let result = storage.store_text_safe(doc_id, text);

        if should_succeed {
            assert!(result.is_ok(), "Expected success for text: '{}'", text);
            successful_docs.push((doc_id, text));
        } else {
            assert!(result.is_err(), "Expected failure for text: '{}'", text);
            failed_docs.push(doc_id);
        }
    }

    // Verify final state
    assert_eq!(storage.entry_count(), successful_docs.len() as u32);

    // Verify all successful documents are retrievable
    for (doc_id, expected_text) in successful_docs {
        let retrieved = storage.get_text_safe(doc_id).unwrap();
        assert_eq!(retrieved, expected_text);
    }

    // Verify all failed documents are not found
    for doc_id in failed_docs {
        let result = storage.get_text_safe(doc_id);
        assert!(result.is_err());
    }
}

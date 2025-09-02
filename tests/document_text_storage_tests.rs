//! Unit tests for DocumentTextStorage
//!
//! Tests the core DocumentTextStorage functionality:
//! - Storage creation and opening
//! - Text storage and retrieval operations
//! - Unicode text handling
//! - Document updates and versioning
//! - File growth and memory mapping
//! - Size limits and validation

use shardex::error::ShardexError;
use shardex::identifiers::DocumentId;
use shardex::DocumentTextStorage;
mod common;
use common::{create_temp_dir_for_test, test_constants};

#[test]
fn test_storage_creation() {
    let temp_dir = create_temp_dir_for_test();
    let max_size = test_constants::DEFAULT_SHARD_SIZE * 1024; // Use test constant

    let storage = DocumentTextStorage::create(&temp_dir, max_size).unwrap();

    assert_eq!(storage.max_document_size(), max_size);
    assert_eq!(storage.entry_count(), 0);
    assert_eq!(storage.total_text_size(), 0);
    assert!(storage.is_empty());

    // Verify files exist
    assert!(temp_dir.path().join("text_index.dat").exists());
    assert!(temp_dir.path().join("text_data.dat").exists());
}

#[test]
fn test_storage_opening() {
    let temp_dir = create_temp_dir_for_test();
    let max_size = test_constants::DEFAULT_SHARD_SIZE * 1024;
    let doc_id = DocumentId::new();
    let text = "Test text for opening validation.";

    // Create and populate storage
    {
        let mut storage = DocumentTextStorage::create(&temp_dir, max_size).unwrap();
        storage.store_text(doc_id, text).unwrap();
        storage.sync().unwrap();
    }

    // Open existing storage
    let storage = DocumentTextStorage::open(&temp_dir).unwrap();
    assert_eq!(storage.entry_count(), 1);
    assert_eq!(storage.total_text_size(), text.len() as u64);
    assert!(!storage.is_empty());

    // Verify text can be retrieved
    let retrieved = storage.get_text(doc_id).unwrap();
    assert_eq!(retrieved, text);
}

#[test]
fn test_basic_text_storage_and_retrieval() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let doc_id = DocumentId::new();
    let text = "The quick brown fox jumps over the lazy dog.";

    // Store text
    storage.store_text(doc_id, text).unwrap();

    // Verify storage state
    assert_eq!(storage.entry_count(), 1);
    assert_eq!(storage.total_text_size(), text.len() as u64);
    assert!(!storage.is_empty());

    // Retrieve text
    let retrieved = storage.get_text(doc_id).unwrap();
    assert_eq!(retrieved, text);
}

#[test]
fn test_multiple_documents() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let documents = vec![
        (DocumentId::new(), "First document text."),
        (DocumentId::new(), "Second document with different content."),
        (DocumentId::new(), "Third document for multiple storage test."),
    ];

    // Store all documents
    for (doc_id, text) in &documents {
        storage.store_text(*doc_id, text).unwrap();
    }

    // Verify storage state
    assert_eq!(storage.entry_count(), documents.len() as u32);
    let expected_total_size: usize = documents.iter().map(|(_, text)| text.len()).sum();
    assert_eq!(storage.total_text_size(), expected_total_size as u64);

    // Retrieve all documents
    for (doc_id, expected_text) in documents {
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, expected_text);
    }
}

#[test]
fn test_document_updates() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let doc_id = DocumentId::new();
    let text_v1 = "Original version of the document.";
    let text_v2 = "Updated version with more content.";
    let text_v3 = "Final version after multiple updates.";

    // Store original version
    storage.store_text(doc_id, text_v1).unwrap();
    assert_eq!(storage.get_text(doc_id).unwrap(), text_v1);
    assert_eq!(storage.entry_count(), 1);

    // Update to second version
    storage.store_text(doc_id, text_v2).unwrap();
    assert_eq!(storage.get_text(doc_id).unwrap(), text_v2); // Should get latest
    assert_eq!(storage.entry_count(), 2); // Append-only, so 2 entries

    // Update to final version
    storage.store_text(doc_id, text_v3).unwrap();
    assert_eq!(storage.get_text(doc_id).unwrap(), text_v3); // Should get latest
    assert_eq!(storage.entry_count(), 3); // Append-only, so 3 entries

    // Verify total text size includes all versions
    let expected_size = (text_v1.len() + text_v2.len() + text_v3.len()) as u64;
    assert_eq!(storage.total_text_size(), expected_size);
}

#[test]
fn test_unicode_text_handling() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let test_cases = vec![
        ("Basic ASCII", "Hello, world!"),
        ("Latin accents", "Café, naïve, résumé"),
        ("Chinese", "你好世界"),
        ("Japanese", "こんにちは世界"),
        ("Arabic", "مرحبا بالعالم"),
        ("Emoji", "Hello 🌍 World! 🚀🎉"),
        (
            "Mixed content",
            "Hello 世界! Здравствуй мир! 🌍 Español Français العربية русский",
        ),
    ];

    let mut doc_ids = Vec::new();

    // Store all test cases
    for (name, text) in &test_cases {
        let doc_id = DocumentId::new();
        storage.store_text(doc_id, text).unwrap();
        doc_ids.push((doc_id, *name, *text));
    }

    // Retrieve and verify all test cases
    for (doc_id, name, expected_text) in doc_ids {
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, expected_text, "Failed for test case: {}", name);
    }
}

#[test]
fn test_size_limit_enforcement() {
    let temp_dir = create_temp_dir_for_test();
    let small_limit = 100; // 100 bytes
    let mut storage = DocumentTextStorage::create(&temp_dir, small_limit).unwrap();

    let doc_id = DocumentId::new();

    // Test exactly at limit
    let at_limit_text = "x".repeat(small_limit);
    storage.store_text(doc_id, &at_limit_text).unwrap();
    assert_eq!(storage.get_text(doc_id).unwrap(), at_limit_text);

    // Test over limit
    let over_limit_doc = DocumentId::new();
    let over_limit_text = "x".repeat(small_limit + 1);
    let result = storage.store_text(over_limit_doc, &over_limit_text);

    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::DocumentTooLarge { size, max_size } => {
            assert_eq!(size, small_limit + 1);
            assert_eq!(max_size, small_limit);
        }
        e => panic!("Expected DocumentTooLarge error, got {:?}", e),
    }

    // Test under limit
    let under_limit_doc = DocumentId::new();
    let under_limit_text = "x".repeat(small_limit - 1);
    storage
        .store_text(under_limit_doc, &under_limit_text)
        .unwrap();
    assert_eq!(storage.get_text(under_limit_doc).unwrap(), under_limit_text);
}

#[test]
fn test_document_not_found() {
    let temp_dir = create_temp_dir_for_test();
    let storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let nonexistent_doc = DocumentId::new();
    let result = storage.get_text(nonexistent_doc);

    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::DocumentTextNotFound { document_id } => {
            assert_eq!(document_id, nonexistent_doc.to_string());
        }
        e => panic!("Expected DocumentTextNotFound error, got {:?}", e),
    }
}

#[test]
fn test_empty_text_rejection() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let doc_id = DocumentId::new();
    let result = storage.store_text(doc_id, "");

    assert!(result.is_err());
    // The error should come from DocumentTextEntry validation
    match result.unwrap_err() {
        ShardexError::InvalidInput { field, .. } => {
            assert_eq!(field, "text_length");
        }
        e => panic!("Expected InvalidInput error for text_length, got {:?}", e),
    }
}

#[test]
fn test_file_growth_and_resize() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    // Store many documents to trigger file growth
    let mut doc_ids = Vec::new();
    for i in 0..100 {
        let doc_id = DocumentId::new();
        let text = format!(
            "Document {} with substantial content to fill space and potentially trigger file growth operations.",
            i
        );

        storage.store_text(doc_id, &text).unwrap();
        doc_ids.push((doc_id, text));
    }

    // Verify all documents are retrievable after file growth
    for (doc_id, expected_text) in doc_ids {
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, expected_text);
    }

    assert_eq!(storage.entry_count(), 100);
    assert!(storage.total_text_size() > 0);
}

#[test]
fn test_backward_search_finds_latest() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let doc_id = DocumentId::new();
    let versions = vec![
        "Version 1 - initial",
        "Version 2 - updated with more content",
        "Version 3 - final version with even more content",
    ];

    // Store multiple versions of the same document
    for version in &versions {
        storage.store_text(doc_id, version).unwrap();
    }

    // Should retrieve the latest (last stored) version
    let retrieved = storage.get_text(doc_id).unwrap();
    assert_eq!(retrieved, "Version 3 - final version with even more content");

    // Should have 3 entries (append-only)
    assert_eq!(storage.entry_count(), 3);
}

#[test]
fn test_sync_operations() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let doc_id = DocumentId::new();
    let text = "Text that needs to be synced to disk.";

    storage.store_text(doc_id, text).unwrap();

    // Sync should not fail
    storage.sync().unwrap();

    // Text should still be retrievable after sync
    let retrieved = storage.get_text(doc_id).unwrap();
    assert_eq!(retrieved, text);
}

#[test]
fn test_utilization_ratio() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    // Initially should have low utilization (mostly header overhead)
    let initial_ratio = storage.utilization_ratio();
    assert!((0.0..=1.0).contains(&initial_ratio));

    // Add some text
    let doc_id = DocumentId::new();
    let text = "Some text to change utilization ratio.";
    storage.store_text(doc_id, text).unwrap();

    let after_text_ratio = storage.utilization_ratio();
    assert!(after_text_ratio >= initial_ratio);
    assert!((0.0..=1.0).contains(&after_text_ratio));

    // Add more text to increase utilization
    for i in 0..10 {
        let doc_id = DocumentId::new();
        let text = format!("Additional text document {} for utilization testing.", i);
        storage.store_text(doc_id, &text).unwrap();
    }

    let final_ratio = storage.utilization_ratio();
    assert!(final_ratio >= after_text_ratio);
    assert!((0.0..=1.0).contains(&final_ratio));
}

#[test]
fn test_max_document_size_updates() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, 100).unwrap();

    assert_eq!(storage.max_document_size(), 100);

    let doc_id = DocumentId::new();
    let large_text = "x".repeat(150);

    // Should fail with current limit
    assert!(storage.store_text(doc_id, &large_text).is_err());

    // Update limit
    storage.set_max_document_size(200);
    assert_eq!(storage.max_document_size(), 200);

    // Should now succeed
    storage.store_text(doc_id, &large_text).unwrap();
    assert_eq!(storage.get_text(doc_id).unwrap(), large_text);
}

#[test]
fn test_persistence_across_sessions() {
    let temp_dir = create_temp_dir_for_test();
    let doc_id = DocumentId::new();
    let text = "Persistent text data across sessions.";

    // First session - create and store
    {
        let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();
        storage.store_text(doc_id, text).unwrap();
        assert_eq!(storage.entry_count(), 1);
        assert_eq!(storage.get_text(doc_id).unwrap(), text);
        storage.sync().unwrap();
    }

    // Second session - open and verify
    {
        let storage = DocumentTextStorage::open(&temp_dir).unwrap();
        assert_eq!(storage.entry_count(), 1);
        assert_eq!(storage.get_text(doc_id).unwrap(), text);
        assert_eq!(storage.total_text_size(), text.len() as u64);
    }

    // Third session - add more data
    let additional_text = "Additional text in new session.";
    {
        let mut storage = DocumentTextStorage::open(&temp_dir).unwrap();
        let new_doc_id = DocumentId::new();
        storage.store_text(new_doc_id, additional_text).unwrap();
        storage.sync().unwrap();

        assert_eq!(storage.entry_count(), 2);
        assert_eq!(storage.get_text(doc_id).unwrap(), text);
        assert_eq!(storage.get_text(new_doc_id).unwrap(), additional_text);
    }

    // Fourth session - verify all data persists
    {
        let storage = DocumentTextStorage::open(&temp_dir).unwrap();
        assert_eq!(storage.entry_count(), 2);
        assert_eq!(storage.get_text(doc_id).unwrap(), text);

        // Find the additional text by trying to retrieve it
        // (In real usage you'd track the doc_id, but for testing we can work around this)
        let expected_total_size = (text.len() + additional_text.len()) as u64;
        assert_eq!(storage.total_text_size(), expected_total_size);
    }
}

#[test]
fn test_large_document_handling() {
    let temp_dir = create_temp_dir_for_test();
    let large_limit = 10 * test_constants::DEFAULT_SHARD_SIZE * 1024; // 10MB limit
    let mut storage = DocumentTextStorage::create(&temp_dir, large_limit).unwrap();

    let doc_id = DocumentId::new();
    // Create a reasonably large document (1MB)
    let large_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(17964);

    // Should succeed with large document
    storage.store_text(doc_id, &large_text).unwrap();

    // Verify retrieval works
    let retrieved = storage.get_text(doc_id).unwrap();
    assert_eq!(retrieved, large_text);
    assert_eq!(retrieved.len(), large_text.len());
}

#[test]
fn test_many_small_documents() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let document_count = 1000;
    let mut documents = Vec::new();

    // Store many small documents
    for i in 0..document_count {
        let doc_id = DocumentId::new();
        let text = format!("Small document #{} with unique content.", i);
        storage.store_text(doc_id, &text).unwrap();
        documents.push((doc_id, text));
    }

    assert_eq!(storage.entry_count(), document_count);

    // Verify all documents are retrievable
    for (doc_id, expected_text) in documents {
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, expected_text);
    }
}

#[test]
fn test_mixed_document_sizes() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let medium_text = "This is a medium-sized document with more content to test various document sizes. ".repeat(10);
    let large_text = "This is a large document with substantial content. ".repeat(1000);
    let long_line_text = "A".repeat(10000);

    let test_cases: Vec<(&str, &str)> = vec![
        ("Tiny", "Hi"),
        ("Small", "This is a small document."),
        ("Medium", &medium_text),
        ("Large", &large_text),
        ("Single char", "X"),
        ("Long single line", &long_line_text),
    ];

    let mut stored_docs = Vec::new();

    // Store all different sized documents
    for (name, text) in &test_cases {
        let doc_id = DocumentId::new();
        storage.store_text(doc_id, text).unwrap();
        stored_docs.push((doc_id, *name, *text));
    }

    // Verify all are retrievable regardless of size
    for (doc_id, name, expected_text) in stored_docs {
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, expected_text, "Failed for document type: {}", name);
    }
}

#[test]
fn test_text_alignment_and_boundaries() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    // Test various text lengths that might test alignment boundaries
    let alignment_test_cases = vec![
        1, 2, 3, 4, 5, 7, 8, 15, 16, 31, 32, 63, 64, 127, 128, 255, 256, 511, 512, 1023, 1024,
    ];

    let mut documents = Vec::new();

    for length in alignment_test_cases {
        let doc_id = DocumentId::new();
        let text = "A".repeat(length);
        storage.store_text(doc_id, &text).unwrap();
        documents.push((doc_id, text));
    }

    // Verify all texts are stored and retrieved correctly
    for (doc_id, expected_text) in documents {
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, expected_text);
        assert_eq!(retrieved.len(), expected_text.len());
    }
}

#[test]
fn test_concurrent_document_operations() {
    // Note: This is not true concurrency testing (would require threading),
    // but tests interleaved operations that might happen in concurrent scenarios
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let doc_ids: Vec<DocumentId> = (0..10).map(|_| DocumentId::new()).collect();
    let mut expected_texts = Vec::new();

    // Interleave storage operations in a pattern that might stress internal state
    for i in 0..doc_ids.len() {
        let text = format!("Document {} initial content", i);
        storage.store_text(doc_ids[i], &text).unwrap();
        expected_texts.push(text);

        // Update some earlier documents
        if i >= 2 {
            let update_idx = i - 2;
            let updated_text = format!("Document {} UPDATED content", update_idx);
            storage
                .store_text(doc_ids[update_idx], &updated_text)
                .unwrap();
            expected_texts[update_idx] = updated_text;
        }
    }

    // Verify final state
    for (i, doc_id) in doc_ids.iter().enumerate() {
        let retrieved = storage.get_text(*doc_id).unwrap();
        assert_eq!(retrieved, expected_texts[i], "Mismatch for document {}", i);
    }
}

#[test]
fn test_storage_statistics() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    // Initially empty
    assert_eq!(storage.entry_count(), 0);
    assert_eq!(storage.total_text_size(), 0);
    assert!(storage.is_empty());

    // Add first document
    let doc1 = DocumentId::new();
    let text1 = "First document";
    storage.store_text(doc1, text1).unwrap();

    assert_eq!(storage.entry_count(), 1);
    assert_eq!(storage.total_text_size(), text1.len() as u64);
    assert!(!storage.is_empty());

    // Add second document
    let doc2 = DocumentId::new();
    let text2 = "Second document with more content";
    storage.store_text(doc2, text2).unwrap();

    assert_eq!(storage.entry_count(), 2);
    assert_eq!(storage.total_text_size(), (text1.len() + text2.len()) as u64);

    // Update first document (creates new entry)
    let text1_updated = "First document updated";
    storage.store_text(doc1, text1_updated).unwrap();

    assert_eq!(storage.entry_count(), 3); // Append-only
    assert_eq!(
        storage.total_text_size(),
        (text1.len() + text2.len() + text1_updated.len()) as u64
    );
}

#[test]
fn test_text_with_special_characters() {
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let special_texts = vec![
        ("Newlines", "Line 1\nLine 2\nLine 3"),
        ("Tabs", "Column 1\tColumn 2\tColumn 3"),
        ("Mixed whitespace", "Space Tab\tNewline\nCarriage\rReturn"),
        ("Unicode whitespace", "En\u{2002}space\u{2003}various\u{2009}spaces"),
        ("Control chars", "Start\x01Control\x02Characters\x03End"),
        ("High Unicode", "Math: ∀x∈ℝ, Arrows: ← → ↑ ↓, Symbols: ∞ ∑ ∏"),
        ("Combining chars", "é (e+accent) vs é (precomposed)"),
        ("Right-to-left", "English ← עברית → English"),
    ];

    let mut documents = Vec::new();

    for (name, text) in &special_texts {
        let doc_id = DocumentId::new();
        storage.store_text(doc_id, text).unwrap();
        documents.push((doc_id, *name, *text));
    }

    // Verify all special texts are stored and retrieved correctly
    for (doc_id, name, expected_text) in documents {
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, expected_text, "Failed for special text: {}", name);
        assert_eq!(retrieved.len(), expected_text.len(), "Length mismatch for: {}", name);
    }
}

#[test]
fn test_error_handling_during_storage() {
    let temp_dir = create_temp_dir_for_test();
    let tiny_limit = 10; // Very small limit for testing
    let mut storage = DocumentTextStorage::create(&temp_dir, tiny_limit).unwrap();

    let doc_id = DocumentId::new();
    let small_text = "Small";
    let large_text = "This text is definitely too large for the tiny limit";

    // Small text should succeed
    storage.store_text(doc_id, small_text).unwrap();
    assert_eq!(storage.get_text(doc_id).unwrap(), small_text);

    // Large text should fail but not corrupt the storage
    let doc_id2 = DocumentId::new();
    let result = storage.store_text(doc_id2, large_text);
    assert!(result.is_err());

    // Original document should still be retrievable
    assert_eq!(storage.get_text(doc_id).unwrap(), small_text);
    assert_eq!(storage.entry_count(), 1); // Only the successful store

    // Storage should still work for valid documents
    let doc_id3 = DocumentId::new();
    let another_small = "Tiny";
    storage.store_text(doc_id3, another_small).unwrap();
    assert_eq!(storage.get_text(doc_id3).unwrap(), another_small);
}

#[test]
fn test_boundary_text_lengths() {
    let temp_dir = create_temp_dir_for_test();
    let limit = 1000;
    let mut storage = DocumentTextStorage::create(&temp_dir, limit).unwrap();

    let text_a = "A".repeat(limit - 1);
    let text_b = "B".repeat(limit);
    let boundary_cases = vec![
        (1, "X"),                     // Minimum valid length
        (limit - 1, text_a.as_str()), // Just under limit
        (limit, text_b.as_str()),     // Exactly at limit
    ];

    for (expected_len, text) in &boundary_cases {
        let doc_id = DocumentId::new();
        storage.store_text(doc_id, text).unwrap();

        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, *text);
        assert_eq!(retrieved.len(), *expected_len);
    }

    // Just over limit should fail
    let over_limit_doc = DocumentId::new();
    let over_limit_text = "C".repeat(limit + 1);
    assert!(storage
        .store_text(over_limit_doc, &over_limit_text)
        .is_err());
}

#[test]
fn test_file_system_stress() {
    // Test behavior with rapid file operations that might stress the file system
    let temp_dir = create_temp_dir_for_test();
    let mut storage = DocumentTextStorage::create(&temp_dir, test_constants::DEFAULT_SHARD_SIZE * 1024).unwrap();

    let iterations = 50;
    let mut all_docs = Vec::new();

    // Rapid storage and retrieval operations
    for i in 0..iterations {
        let doc_id = DocumentId::new();
        let text = format!("Stress test document {} with content for file system testing", i);

        // Store
        storage.store_text(doc_id, &text).unwrap();

        // Immediately retrieve
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, text);

        all_docs.push((doc_id, text));

        // Periodically sync to disk
        if i % 10 == 0 {
            storage.sync().unwrap();
        }
    }

    // Final verification of all documents
    for (doc_id, expected_text) in all_docs {
        let retrieved = storage.get_text(doc_id).unwrap();
        assert_eq!(retrieved, expected_text);
    }

    assert_eq!(storage.entry_count(), iterations as u32);
}

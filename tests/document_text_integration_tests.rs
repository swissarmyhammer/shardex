//! Integration tests for document text storage workflows
//!
//! Tests complete document workflows including:
//! - Document replacement with postings
//! - Text extraction from search results  
//! - Cross-component integration
//! - Crash recovery scenarios
//! - Realistic usage patterns

use shardex::document_text_storage::DocumentTextStorage;
use shardex::error::ShardexError;
use shardex::identifiers::DocumentId;
use shardex::structures::Posting;
use tempfile::TempDir;

/// Create a test embedding vector with the specified dimension
fn create_test_embedding(dimension: usize) -> Vec<f32> {
    (0..dimension)
        .map(|i| (i as f32) / (dimension as f32))
        .collect()
}

/// Create test postings for a document with realistic text coordinates
fn create_test_postings(
    document_id: DocumentId,
    text: &str,
    vector_dimension: usize,
    posting_count: usize,
) -> Result<Vec<Posting>, ShardexError> {
    let mut postings = Vec::new();
    let text_len = text.len();

    if posting_count == 0 {
        return Ok(postings);
    }

    // Create evenly distributed postings across the text
    let segment_size = text_len / posting_count;

    for i in 0..posting_count {
        let start = (i * segment_size) as u32;
        let length = if i == posting_count - 1 {
            // Last posting gets remaining text
            (text_len - (i * segment_size)) as u32
        } else {
            segment_size as u32
        };

        // Ensure we don't exceed text bounds
        let actual_length = std::cmp::min(length, (text_len - start as usize) as u32);

        if actual_length == 0 {
            break; // Skip empty postings
        }

        // Create unique vector for each posting
        let vector: Vec<f32> = (0..vector_dimension)
            .map(|j| ((i * vector_dimension + j) as f32) / 1000.0)
            .collect();

        let posting = Posting::new(document_id, start, actual_length, vector, vector_dimension)?;
        postings.push(posting);
    }

    Ok(postings)
}

#[test]
fn test_document_text_storage_creation_and_opening() {
    let temp_dir = TempDir::new().unwrap();
    let max_size = 1024 * 1024; // 1MB

    // Create storage
    {
        let storage = DocumentTextStorage::create(&temp_dir, max_size).unwrap();
        assert_eq!(storage.max_document_size(), max_size);
        assert!(storage.is_empty());
    }

    // Verify files exist
    assert!(temp_dir.path().join("text_index.dat").exists());
    assert!(temp_dir.path().join("text_data.dat").exists());

    // Open existing storage
    {
        let storage = DocumentTextStorage::open(&temp_dir).unwrap();
        assert!(storage.is_empty());
    }
}

#[test]
fn test_complete_document_storage_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let doc_id = DocumentId::new();
    let text = "The quick brown fox jumps over the lazy dog. This is a sample document for testing text extraction workflows.";

    // Store document text
    storage.store_text_safe(doc_id, text).unwrap();

    // Verify document text retrieval
    let retrieved_text = storage.get_text_safe(doc_id).unwrap();
    assert_eq!(retrieved_text, text);

    // Create postings for the document
    let postings = create_test_postings(doc_id, text, 128, 3).unwrap();
    assert_eq!(postings.len(), 3);

    // Verify posting text extraction
    for posting in &postings {
        let extracted = storage
            .extract_text_substring(posting.document_id, posting.start, posting.length)
            .unwrap();

        let expected_start = posting.start as usize;
        let expected_end = expected_start + posting.length as usize;
        let expected = &text[expected_start..expected_end];

        assert_eq!(
            extracted, expected,
            "Text mismatch for posting at {}..{}",
            expected_start, expected_end
        );
    }
}

#[test]
fn test_document_replacement_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let doc_id = DocumentId::new();
    let original_text = "Original document content for replacement testing.";
    let updated_text =
        "Updated document content with different text for replacement workflow validation.";

    // Store original document
    storage.store_text_safe(doc_id, original_text).unwrap();
    assert_eq!(storage.get_text_safe(doc_id).unwrap(), original_text);

    // Create postings for original text
    let original_postings = create_test_postings(doc_id, original_text, 64, 2).unwrap();

    // Verify original postings work
    for posting in &original_postings {
        let extracted = storage
            .extract_text_substring(posting.document_id, posting.start, posting.length)
            .unwrap();

        let expected_start = posting.start as usize;
        let expected_end = expected_start + posting.length as usize;
        let expected = &original_text[expected_start..expected_end];

        assert_eq!(extracted, expected);
    }

    // Replace document with updated content
    storage.store_text_safe(doc_id, updated_text).unwrap();
    assert_eq!(storage.get_text_safe(doc_id).unwrap(), updated_text);

    // Create new postings for updated text
    let updated_postings = create_test_postings(doc_id, updated_text, 64, 3).unwrap();

    // Verify new postings work with updated text
    for posting in &updated_postings {
        let extracted = storage
            .extract_text_substring(posting.document_id, posting.start, posting.length)
            .unwrap();

        let expected_start = posting.start as usize;
        let expected_end = expected_start + posting.length as usize;
        let expected = &updated_text[expected_start..expected_end];

        assert_eq!(extracted, expected);
    }

    // Old postings should fail with the new text (invalid ranges)
    for posting in &original_postings {
        if posting.start as usize + posting.length as usize > updated_text.len() {
            let result =
                storage.extract_text_substring(posting.document_id, posting.start, posting.length);
            assert!(result.is_err(), "Expected error for out-of-bounds posting");
        }
    }

    // Verify storage shows both versions (append-only)
    assert_eq!(storage.entry_count(), 2);
}

#[test]
fn test_multiple_documents_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let documents = vec![
        ("The quick brown fox jumps over the lazy dog.", 2),
        ("Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.", 4),
        ("Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety.", 3),
        ("Vector databases are specialized databases designed to store and search high-dimensional vector data efficiently.", 3),
        ("Machine learning models generate embeddings that capture semantic meaning in numerical vector form.", 2),
    ];

    let mut doc_data = Vec::new();

    // Store all documents with their postings
    for (text, posting_count) in &documents {
        let doc_id = DocumentId::new();

        // Store document text
        storage.store_text_safe(doc_id, text).unwrap();

        // Create postings for the document
        let postings = create_test_postings(doc_id, text, 128, *posting_count).unwrap();

        doc_data.push((doc_id, text, postings));
    }

    // Verify all documents and their postings
    for (doc_id, expected_text, postings) in &doc_data {
        // Verify document text retrieval
        let retrieved_text = storage.get_text_safe(*doc_id).unwrap();
        assert_eq!(retrieved_text, **expected_text);

        // Verify all postings for this document
        for posting in postings {
            let extracted = storage
                .extract_text_substring(posting.document_id, posting.start, posting.length)
                .unwrap();

            let expected_start = posting.start as usize;
            let expected_end = expected_start + posting.length as usize;
            let expected = &expected_text[expected_start..expected_end];

            assert_eq!(
                extracted, expected,
                "Text mismatch for document {} posting at {}..{}",
                doc_id, expected_start, expected_end
            );
        }
    }

    assert_eq!(storage.entry_count(), documents.len() as u32);
}

#[test]
fn test_unicode_document_integration() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let unicode_documents = vec![
        ("Chinese", "你好世界！这是一个中文测试文档。"),
        ("Japanese", "こんにちは世界！これは日本語のテスト文書です。"),
        ("Arabic", "مرحبا بالعالم! هذا مستند اختبار باللغة العربية."),
        (
            "Emoji",
            "Hello 🌍 World! 🚀 This is a test with emojis 🎉✨",
        ),
        ("Mixed", "English 中文 日本語 العربية 🌍 混合内容文档"),
    ];

    let mut unicode_data = Vec::new();

    // Store all Unicode documents
    for (name, text) in &unicode_documents {
        let doc_id = DocumentId::new();

        storage.store_text_safe(doc_id, text).unwrap();

        // Create postings with careful attention to UTF-8 boundaries
        let postings = create_test_postings(doc_id, text, 64, 2).unwrap();

        unicode_data.push((doc_id, *name, *text, postings));
    }

    // Verify all Unicode documents and postings work correctly
    for (doc_id, name, expected_text, postings) in &unicode_data {
        let retrieved_text = storage.get_text_safe(*doc_id).unwrap();
        assert_eq!(retrieved_text, *expected_text, "Text mismatch for {}", name);

        for posting in postings {
            // Verify posting extracts valid text (respecting UTF-8 boundaries)
            let result =
                storage.extract_text_substring(posting.document_id, posting.start, posting.length);

            match result {
                Ok(extracted) => {
                    // Extracted text should be valid UTF-8 and non-empty
                    assert!(!extracted.is_empty(), "Empty extraction for {}", name);
                    assert!(
                        extracted.chars().count() > 0,
                        "No valid characters for {}",
                        name
                    );
                }
                Err(ShardexError::InvalidRange { .. }) => {
                    // This is acceptable for UTF-8 boundary issues
                    // The important thing is that we don't panic or return invalid UTF-8
                }
                Err(e) => panic!("Unexpected error for {} posting: {:?}", name, e),
            }
        }
    }
}

#[test]
fn test_crash_recovery_simulation() {
    let temp_dir = TempDir::new().unwrap();
    let doc_id = DocumentId::new();
    let text =
        "Document for crash recovery testing. This text should survive across storage instances.";

    // First session - create and populate storage
    {
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        storage.store_text_safe(doc_id, text).unwrap();

        // Verify storage before "crash"
        assert_eq!(storage.get_text_safe(doc_id).unwrap(), text);
        assert_eq!(storage.entry_count(), 1);

        // Create postings and verify they work
        let postings = create_test_postings(doc_id, text, 64, 2).unwrap();
        for posting in &postings {
            let extracted = storage
                .extract_text_substring(posting.document_id, posting.start, posting.length)
                .unwrap();

            let expected_start = posting.start as usize;
            let expected_end = expected_start + posting.length as usize;
            let expected = &text[expected_start..expected_end];
            assert_eq!(extracted, expected);
        }

        // Explicit sync to ensure data is written
        storage.sync().unwrap();
    } // Storage dropped here, simulating crash

    // Second session - recover from "crash"
    {
        let storage = DocumentTextStorage::open(&temp_dir).unwrap();

        // Verify data survived the crash
        assert_eq!(storage.entry_count(), 1);
        let recovered_text = storage.get_text_safe(doc_id).unwrap();
        assert_eq!(recovered_text, text);

        // Verify postings still work after recovery
        let postings = create_test_postings(doc_id, text, 64, 2).unwrap();
        for posting in &postings {
            let extracted = storage
                .extract_text_substring(posting.document_id, posting.start, posting.length)
                .unwrap();

            let expected_start = posting.start as usize;
            let expected_end = expected_start + posting.length as usize;
            let expected = &text[expected_start..expected_end];
            assert_eq!(extracted, expected);
        }
    }

    // Third session - add more data after recovery
    {
        let mut storage = DocumentTextStorage::open(&temp_dir).unwrap();
        let additional_doc = DocumentId::new();
        let additional_text = "Additional document added after recovery.";

        // Add new document
        storage
            .store_text_safe(additional_doc, additional_text)
            .unwrap();

        // Verify both old and new documents work
        assert_eq!(storage.entry_count(), 2);
        assert_eq!(storage.get_text_safe(doc_id).unwrap(), text);
        assert_eq!(
            storage.get_text_safe(additional_doc).unwrap(),
            additional_text
        );

        storage.sync().unwrap();
    }

    // Fourth session - final verification
    {
        let storage = DocumentTextStorage::open(&temp_dir).unwrap();
        assert_eq!(storage.entry_count(), 2);
        assert_eq!(storage.get_text_safe(doc_id).unwrap(), text);
    }
}

#[test]
fn test_large_document_integration() {
    let temp_dir = TempDir::new().unwrap();
    let large_limit = 5 * 1024 * 1024; // 5MB limit
    let mut storage = DocumentTextStorage::create(&temp_dir, large_limit).unwrap();

    let doc_id = DocumentId::new();

    // Create a large document (1MB)
    let base_text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. ";
    let large_text = base_text.repeat(5000); // Approximately 1MB

    // Store large document
    storage.store_text_safe(doc_id, &large_text).unwrap();

    // Verify retrieval
    let retrieved = storage.get_text_safe(doc_id).unwrap();
    assert_eq!(retrieved, large_text);
    assert_eq!(retrieved.len(), large_text.len());

    // Create postings across the large document
    let postings = create_test_postings(doc_id, &large_text, 128, 10).unwrap();
    assert_eq!(postings.len(), 10);

    // Verify all postings work correctly
    for (i, posting) in postings.iter().enumerate() {
        let extracted = storage
            .extract_text_substring(posting.document_id, posting.start, posting.length)
            .unwrap();

        let expected_start = posting.start as usize;
        let expected_end = expected_start + posting.length as usize;
        let expected = &large_text[expected_start..expected_end];

        assert_eq!(
            extracted, expected,
            "Mismatch for posting {} at {}..{}",
            i, expected_start, expected_end
        );
        assert!(!extracted.is_empty(), "Empty extraction for posting {}", i);
    }

    // Test performance with large document
    let start_time = std::time::Instant::now();
    let _retrieved = storage.get_text_safe(doc_id).unwrap();
    let retrieval_duration = start_time.elapsed();

    // Should be reasonably fast (less than 100ms for 1MB)
    assert!(
        retrieval_duration.as_millis() < 100,
        "Large document retrieval too slow: {:?}",
        retrieval_duration
    );
}

#[test]
fn test_document_versioning_with_postings() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let doc_id = DocumentId::new();
    let versions = [
        "Version 1: Initial document content for versioning test.",
        "Version 2: Updated document with additional content for more comprehensive testing of versioning workflow.",
        "Version 3: Final version with extensive content to test how postings work across document versions and updates.",
    ];

    let mut version_postings = Vec::new();

    // Store each version and create postings
    for (version_num, text) in versions.iter().enumerate() {
        storage.store_text_safe(doc_id, text).unwrap();

        // Verify this version is retrievable
        let retrieved = storage.get_text_safe(doc_id).unwrap();
        assert_eq!(
            retrieved,
            *text,
            "Version {} not stored correctly",
            version_num + 1
        );

        // Create postings for this version
        let postings = create_test_postings(doc_id, text, 64, 2).unwrap();

        // Verify postings work for current version
        for posting in &postings {
            let extracted = storage
                .extract_text_substring(posting.document_id, posting.start, posting.length)
                .unwrap();

            let expected_start = posting.start as usize;
            let expected_end = expected_start + posting.length as usize;
            let expected = &text[expected_start..expected_end];
            assert_eq!(extracted, expected);
        }

        version_postings.push(postings);
    }

    // Verify final state
    assert_eq!(storage.entry_count(), 3); // All versions stored (append-only)
    assert_eq!(storage.get_text_safe(doc_id).unwrap(), versions[2]); // Latest version

    // Verify that only the latest version's postings work
    let latest_text = versions[2];
    let latest_postings = &version_postings[2];

    for posting in latest_postings {
        let extracted = storage
            .extract_text_substring(posting.document_id, posting.start, posting.length)
            .unwrap();

        let expected_start = posting.start as usize;
        let expected_end = expected_start + posting.length as usize;
        let expected = &latest_text[expected_start..expected_end];
        assert_eq!(extracted, expected);
    }

    // Older version postings may fail if they exceed the latest document bounds
    let first_version_postings = &version_postings[0];
    for posting in first_version_postings {
        let result =
            storage.extract_text_substring(posting.document_id, posting.start, posting.length);

        if posting.start as usize + posting.length as usize > latest_text.len() {
            // Should fail for out-of-bounds
            assert!(result.is_err());
        } else {
            // Should succeed if within bounds, but may not match original text
            assert!(result.is_ok());
        }
    }
}

#[test]
fn test_realistic_usage_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    // Simulate realistic document processing workflow
    let documents = [
        (
            "Technical documentation about Rust programming language features and best practices.",
            vec![(0, 9), (10, 13), (24, 5), (42, 8), (51, 8), (60, 3)], // Word boundaries
        ),
        (
            "Machine learning algorithms for natural language processing and text analysis.",
            vec![(0, 7), (8, 8), (17, 10), (31, 7), (39, 8), (48, 10)],
        ),
        (
            "Database design principles for scalable web applications and data management.",
            vec![(0, 8), (9, 6), (16, 10), (30, 9), (43, 11), (55, 4)],
        ),
    ];

    let mut doc_data = Vec::new();

    // Process each document
    for (i, (text, word_positions)) in documents.iter().enumerate() {
        let doc_id = DocumentId::new();

        // Store document
        storage.store_text_safe(doc_id, text).unwrap();

        // Create postings based on realistic word boundaries
        let mut postings = Vec::new();
        for (start, length) in word_positions {
            let vector = create_test_embedding(128);
            let posting = Posting::new(doc_id, *start as u32, *length as u32, vector, 128).unwrap();
            postings.push(posting);
        }

        doc_data.push((doc_id, *text, postings.clone()));

        // Verify document and postings work
        let retrieved = storage.get_text_safe(doc_id).unwrap();
        assert_eq!(retrieved, *text);

        // Verify all postings extract the expected words/phrases
        for posting in &postings {
            let extracted = storage
                .extract_text_substring(posting.document_id, posting.start, posting.length)
                .unwrap();

            let expected_start = posting.start as usize;
            let expected_end = expected_start + posting.length as usize;
            let expected = &text[expected_start..expected_end];

            assert_eq!(extracted, expected);
            assert!(!extracted.is_empty());
        }

        println!(
            "Processed document {} with {} postings",
            i + 1,
            postings.len()
        );
    }

    // Simulate search-like access pattern
    for (doc_id, text, postings) in &doc_data {
        // Retrieve document
        let full_text = storage.get_text_safe(*doc_id).unwrap();
        assert_eq!(full_text, *text);

        // Extract text for each posting (simulating search result processing)
        for posting in postings {
            let snippet = storage
                .extract_text_substring(posting.document_id, posting.start, posting.length)
                .unwrap();

            // Verify snippet is meaningful
            assert!(!snippet.is_empty());
            assert!(!snippet.trim().is_empty());

            // Verify snippet is part of the original document
            assert!(text.contains(&snippet));
        }
    }

    // Final statistics
    assert_eq!(storage.entry_count(), documents.len() as u32);
    assert!(storage.total_text_size() > 0);
    assert!(storage.utilization_ratio() > 0.0);

    println!("Integration test completed successfully:");
    println!("  Documents: {}", documents.len());
    println!("  Total entries: {}", storage.entry_count());
    println!("  Total text size: {} bytes", storage.total_text_size());
    println!("  Utilization ratio: {:.2}", storage.utilization_ratio());
}

#[test]
fn test_error_recovery_integration() {
    let temp_dir = TempDir::new().unwrap();
    let small_limit = 200; // Small limit to trigger errors
    let mut storage = DocumentTextStorage::create(&temp_dir, small_limit).unwrap();

    let doc_id1 = DocumentId::new();
    let doc_id2 = DocumentId::new();
    let doc_id3 = DocumentId::new();

    let small_text = "Small text that fits within limits.";
    let large_text = "This is a much larger text that will definitely exceed the small size limit that we have configured for this test to ensure proper error handling and recovery. We need to add even more text here to make it exceed the 200 byte limit for sure, so here is some additional padding text that should push it well over the limit.";
    let another_small = "Another small text.";

    // Store first document successfully
    storage.store_text_safe(doc_id1, small_text).unwrap();
    assert_eq!(storage.get_text_safe(doc_id1).unwrap(), small_text);

    // Try to store oversized document (should fail)
    let result = storage.store_text_safe(doc_id2, large_text);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::DocumentTooLarge { .. } => {
            // Expected error type
        }
        e => panic!("Expected DocumentTooLarge error, got {:?}", e),
    }

    // Verify first document is still accessible after error
    assert_eq!(storage.get_text_safe(doc_id1).unwrap(), small_text);
    assert_eq!(storage.entry_count(), 1); // Only successful store

    // Store third document successfully after error
    storage.store_text_safe(doc_id3, another_small).unwrap();
    assert_eq!(storage.get_text_safe(doc_id3).unwrap(), another_small);
    assert_eq!(storage.entry_count(), 2);

    // Verify both successful documents work
    assert_eq!(storage.get_text_safe(doc_id1).unwrap(), small_text);
    assert_eq!(storage.get_text_safe(doc_id3).unwrap(), another_small);

    // Verify failed document is not found
    let result = storage.get_text_safe(doc_id2);
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::DocumentTextNotFound { .. } => {
            // Expected error type
        }
        e => panic!("Expected DocumentTextNotFound error, got {:?}", e),
    }

    // Create postings for successful documents
    let postings1 = create_test_postings(doc_id1, small_text, 64, 1).unwrap();
    let postings3 = create_test_postings(doc_id3, another_small, 64, 1).unwrap();

    // Verify postings work for both successful documents
    for posting in &postings1 {
        let extracted = storage
            .extract_text_substring(posting.document_id, posting.start, posting.length)
            .unwrap();

        let expected_start = posting.start as usize;
        let expected_end = expected_start + posting.length as usize;
        let expected = &small_text[expected_start..expected_end];
        assert_eq!(extracted, expected);
    }

    for posting in &postings3 {
        let extracted = storage
            .extract_text_substring(posting.document_id, posting.start, posting.length)
            .unwrap();

        let expected_start = posting.start as usize;
        let expected_end = expected_start + posting.length as usize;
        let expected = &another_small[expected_start..expected_end];
        assert_eq!(extracted, expected);
    }
}

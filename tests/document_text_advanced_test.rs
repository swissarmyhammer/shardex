//! Tests for document_text_advanced example
//!
//! This test file validates the ApiThing pattern usage and functionality
//! demonstrated in the document_text_advanced example.

use apithing::ApiOperation;
use shardex::{
    api::{
        BatchStoreDocumentText, BatchStoreDocumentTextParams, CreateIndex, CreateIndexParams, DocumentTextEntry,
        ExtractSnippet, ExtractSnippetParams, GetDocumentText, GetDocumentTextParams, Search, SearchParams,
        ShardexContext, StoreDocumentText, StoreDocumentTextParams,
    },
    DocumentId, Posting, ShardexConfig, ShardexError,
};

use std::error::Error;
use tempfile::TempDir;

// Test constants matching the example
const TEST_VECTOR_SIZE: usize = 256;
const TEST_SHARD_SIZE: usize = 50000;
const TEST_BATCH_INTERVAL_MS: u64 = 50;
const TEST_MAX_DOCUMENT_SIZE: usize = 50 * 1024 * 1024;

/// Generate a test vector for consistent testing
fn generate_test_vector(seed: &str, dimension: usize) -> Vec<f32> {
    let mut vector = vec![0.0; dimension];
    let hash = seed
        .bytes()
        .fold(0u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as u32));

    for (i, value) in vector.iter_mut().enumerate() {
        *value = ((hash.wrapping_add(i as u32) % 1000) as f32) / 1000.0;
    }

    // Normalize
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut vector {
            *value /= magnitude;
        }
    }

    vector
}

/// Create a test context with proper configuration
fn create_test_context() -> Result<(ShardexContext, TempDir), Box<dyn Error>> {
    let temp_dir = TempDir::new()?;

    let config = ShardexConfig::new()
        .directory_path(temp_dir.path())
        .max_document_text_size(TEST_MAX_DOCUMENT_SIZE);

    let mut context = ShardexContext::with_config(config);

    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(TEST_VECTOR_SIZE)
        .shard_size(TEST_SHARD_SIZE)
        .batch_write_interval_ms(TEST_BATCH_INTERVAL_MS)
        .build()?;

    CreateIndex::execute(&mut context, &create_params)?;

    Ok((context, temp_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() -> Result<(), Box<dyn Error>> {
        let (_context, _temp_dir) = create_test_context()?;
        Ok(())
    }

    #[test]
    fn test_basic_document_storage_and_retrieval() -> Result<(), Box<dyn Error>> {
        let (mut context, _temp_dir) = create_test_context()?;

        let doc_id = DocumentId::from_raw(1001);
        let test_text = "This is a test document for basic storage and retrieval functionality.";

        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: test_text.len() as u32,
            vector: generate_test_vector("test_document", TEST_VECTOR_SIZE),
        };

        // Store document
        let store_params = StoreDocumentTextParams::new(doc_id, test_text.to_string(), vec![posting])?;
        StoreDocumentText::execute(&mut context, &store_params)?;

        // Retrieve document
        let get_params = GetDocumentTextParams::new(doc_id);
        let retrieved = GetDocumentText::execute(&mut context, &get_params)?;

        assert_eq!(retrieved, test_text);
        Ok(())
    }

    #[test]
    fn test_multiple_document_storage() -> Result<(), Box<dyn Error>> {
        let (mut context, _temp_dir) = create_test_context()?;

        // Test batch storage with flush to ensure data is persisted
        let documents = vec![
            (DocumentId::from_raw(2001), "First test document"),
            (DocumentId::from_raw(2002), "Second test document"),
        ];

        let mut document_entries = Vec::new();
        for (doc_id, text) in &documents {
            let posting = Posting {
                document_id: *doc_id,
                start: 0,
                length: text.len() as u32,
                vector: generate_test_vector(&format!("doc_{}", doc_id), TEST_VECTOR_SIZE),
            };

            document_entries.push(DocumentTextEntry::new(*doc_id, text.to_string(), vec![posting]));
        }

        // Use batch storage with immediate flush
        let batch_params = BatchStoreDocumentTextParams::with_flush_and_tracking(document_entries)?;
        let _batch_stats = BatchStoreDocumentText::execute(&mut context, &batch_params)?;

        // Ensure all operations are flushed before verification
        use shardex::api::operations::Flush;
        use shardex::api::FlushParams;
        let flush_params = FlushParams::default();
        Flush::execute(&mut context, &flush_params)?;

        // Verify all documents can be retrieved
        for (doc_id, expected_text) in &documents {
            let get_params = GetDocumentTextParams::new(*doc_id);
            let retrieved = GetDocumentText::execute(&mut context, &get_params)?;
            assert_eq!(retrieved, *expected_text);
        }

        Ok(())
    }

    #[test]
    fn test_document_updates() -> Result<(), Box<dyn Error>> {
        let (mut context, _temp_dir) = create_test_context()?;

        let doc_id = DocumentId::from_raw(3000);

        // Version 1
        let v1_text = "Original document content.";
        let v1_posting = Posting {
            document_id: doc_id,
            start: 0,
            length: v1_text.len() as u32,
            vector: generate_test_vector("v1", TEST_VECTOR_SIZE),
        };

        let store_v1_params = StoreDocumentTextParams::new(doc_id, v1_text.to_string(), vec![v1_posting])?;
        StoreDocumentText::execute(&mut context, &store_v1_params)?;

        let get_v1_params = GetDocumentTextParams::new(doc_id);
        let retrieved_v1 = GetDocumentText::execute(&mut context, &get_v1_params)?;
        assert_eq!(retrieved_v1, v1_text);

        // Version 2 - update
        let v2_text = "Updated document content with more information.";
        let v2_posting = Posting {
            document_id: doc_id,
            start: 0,
            length: v2_text.len() as u32,
            vector: generate_test_vector("v2", TEST_VECTOR_SIZE),
        };

        let store_v2_params = StoreDocumentTextParams::new(doc_id, v2_text.to_string(), vec![v2_posting])?;
        StoreDocumentText::execute(&mut context, &store_v2_params)?;

        let get_v2_params = GetDocumentTextParams::new(doc_id);
        let retrieved_v2 = GetDocumentText::execute(&mut context, &get_v2_params)?;
        assert_eq!(retrieved_v2, v2_text);

        Ok(())
    }

    #[test]
    fn test_error_handling_document_not_found() -> Result<(), Box<dyn Error>> {
        let (mut context, _temp_dir) = create_test_context()?;

        let nonexistent_doc = DocumentId::from_raw(99999);
        let get_params = GetDocumentTextParams::new(nonexistent_doc);

        match GetDocumentText::execute(&mut context, &get_params) {
            Ok(_) => panic!("Expected DocumentTextNotFound error"),
            Err(ShardexError::DocumentTextNotFound { document_id }) => {
                assert_eq!(document_id.to_string(), nonexistent_doc.to_string());
            }
            Err(e) => panic!("Unexpected error type: {}", e),
        }

        Ok(())
    }

    #[test]
    fn test_error_handling_invalid_range() -> Result<(), Box<dyn Error>> {
        let (mut context, _temp_dir) = create_test_context()?;

        // First store a document
        let doc_id = DocumentId::from_raw(4000);
        let test_text = "Short text";
        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: test_text.len() as u32,
            vector: generate_test_vector("test", TEST_VECTOR_SIZE),
        };

        let store_params = StoreDocumentTextParams::new(doc_id, test_text.to_string(), vec![posting])?;
        StoreDocumentText::execute(&mut context, &store_params)?;

        // Try to extract beyond document end
        let invalid_posting = Posting {
            document_id: doc_id,
            start: test_text.len() as u32,
            length: 10,
            vector: generate_test_vector("invalid", TEST_VECTOR_SIZE),
        };

        let extract_params = ExtractSnippetParams::from_posting(&invalid_posting);
        match ExtractSnippet::execute(&mut context, &extract_params) {
            Ok(_) => panic!("Expected InvalidRange error"),
            Err(ShardexError::InvalidRange {
                start,
                length,
                document_length,
            }) => {
                assert_eq!(start, test_text.len() as u32);
                assert_eq!(length, 10);
                assert_eq!(document_length, test_text.len() as u64);
            }
            Err(e) => panic!("Unexpected error type: {}", e),
        }

        Ok(())
    }

    #[test]
    fn test_valid_edge_cases() -> Result<(), Box<dyn Error>> {
        let (mut context, _temp_dir) = create_test_context()?;

        let doc_id = DocumentId::from_raw(5000);
        let test_text = "Test document for edge cases.";
        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: test_text.len() as u32,
            vector: generate_test_vector("edge", TEST_VECTOR_SIZE),
        };

        let store_params = StoreDocumentTextParams::new(doc_id, test_text.to_string(), vec![posting])?;
        StoreDocumentText::execute(&mut context, &store_params)?;

        // Test edge cases
        let edge_cases = vec![
            (0, 1, "first character"),
            ((test_text.len() - 1) as u32, 1, "last character"),
            (0, test_text.len() as u32, "entire document"),
            ((test_text.len() / 2) as u32, 1, "middle character"),
        ];

        for (start, length, description) in edge_cases {
            let edge_posting = Posting {
                document_id: doc_id,
                start,
                length,
                vector: generate_test_vector(description, TEST_VECTOR_SIZE),
            };

            let extract_params = ExtractSnippetParams::from_posting(&edge_posting);
            let result = ExtractSnippet::execute(&mut context, &extract_params)?;

            // Verify the extracted text length matches expected length
            assert_eq!(result.len(), length as usize, "Failed for {}", description);
        }

        Ok(())
    }

    #[test]
    fn test_search_functionality() -> Result<(), Box<dyn Error>> {
        let (mut context, _temp_dir) = create_test_context()?;

        // Store a document with a specific vector
        let doc_id = DocumentId::from_raw(6000);
        let test_text = "Searchable document content";
        let search_vector = generate_test_vector("searchable", TEST_VECTOR_SIZE);

        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: test_text.len() as u32,
            vector: search_vector.clone(),
        };

        let store_params = StoreDocumentTextParams::new(doc_id, test_text.to_string(), vec![posting])?;
        StoreDocumentText::execute(&mut context, &store_params)?;

        // Search using the same vector
        let search_params = SearchParams::builder()
            .query_vector(search_vector)
            .k(5)
            .slop_factor(None)
            .build()?;

        let search_results = Search::execute(&mut context, &search_params)?;

        // Should find the document we just stored
        assert!(!search_results.is_empty(), "Search should return results");
        assert_eq!(
            search_results[0].document_id, doc_id,
            "Should find the correct document"
        );

        Ok(())
    }

    #[test]
    fn test_batch_document_processing() -> Result<(), Box<dyn Error>> {
        let (mut context, _temp_dir) = create_test_context()?;

        // Test multiple sequential document storage (simulating batch processing)
        let documents = vec![
            ("First document for batch processing", 9001u128),
            ("Second document for batch processing", 9002u128),
            ("Third document for batch processing", 9003u128),
        ];

        // Store each document
        for (text, id) in &documents {
            let doc_id = DocumentId::from_raw(*id);
            let posting = Posting {
                document_id: doc_id,
                start: 0,
                length: text.len() as u32,
                vector: generate_test_vector(text, TEST_VECTOR_SIZE),
            };

            let store_params = StoreDocumentTextParams::new(doc_id, text.to_string(), vec![posting])?;
            StoreDocumentText::execute(&mut context, &store_params)?;
        }

        // Verify each document can be retrieved
        for (text, id) in &documents {
            let doc_id = DocumentId::from_raw(*id);
            let get_params = GetDocumentTextParams::new(doc_id);
            let retrieved = GetDocumentText::execute(&mut context, &get_params)?;
            assert_eq!(retrieved, *text);
        }

        Ok(())
    }

    #[test]
    fn test_vector_generation_consistency() {
        // Test that vector generation is deterministic
        let keyword = "test_keyword";
        let dimension = TEST_VECTOR_SIZE;

        let vector1 = generate_test_vector(keyword, dimension);
        let vector2 = generate_test_vector(keyword, dimension);

        assert_eq!(vector1.len(), dimension);
        assert_eq!(vector2.len(), dimension);
        assert_eq!(vector1, vector2, "Vector generation should be deterministic");

        // Test that different keywords produce different vectors
        let vector3 = generate_test_vector("different_keyword", dimension);
        assert_ne!(vector1, vector3, "Different keywords should produce different vectors");
    }
}

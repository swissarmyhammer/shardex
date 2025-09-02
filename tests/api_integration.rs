//! Comprehensive API integration tests for the ApiThing-based Shardex API
//!
//! This test suite validates:
//! - All API operations work correctly
//! - Integration between different operation types
//! - Data consistency across operations
//! - Error handling for invalid inputs

use apithing::ApiOperation;
use shardex::api::{
    AddPostings, AddPostingsParams, BatchStoreDocumentText, BatchStoreDocumentTextParams, 
    CreateIndex, CreateIndexParams, ExtractSnippet, ExtractSnippetParams, Flush, FlushParams,
    GetDocumentText, GetDocumentTextParams, GetStats, GetStatsParams, Search, SearchParams,
    ShardexContext, StoreDocumentText, StoreDocumentTextParams, DocumentTextEntry,
};
use shardex::{DocumentId, Posting};
use tempfile::TempDir;

/// Create a temporary directory for testing
fn create_temp_directory() -> TempDir {
    tempfile::tempdir().expect("Failed to create temporary directory")
}

/// Generate test postings with deterministic vectors
fn generate_test_postings(count: usize, vector_size: usize) -> Vec<Posting> {
    (0..count)
        .map(|i| {
            let document_id = DocumentId::from_raw((i + 1) as u128);
            let mut vector = vec![0.0; vector_size];
            
            // Create deterministic but varied vectors
            for (j, item) in vector.iter_mut().enumerate().take(vector_size) {
                *item = ((i * 17 + j * 23) as f32 % 100.0) / 100.0;
            }
            
            // Normalize vector
            let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
            if magnitude > 0.0 {
                for value in &mut vector {
                    *value /= magnitude;
                }
            }
            
            Posting {
                document_id,
                start: 0,
                length: 50,
                vector,
            }
        })
        .collect()
}

#[test]
fn test_api_basic_operations_integration() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();
    
    // Test CreateIndex operation
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(128)
        .shard_size(1000)
        .batch_write_interval_ms(100)
        .build()
        .expect("Failed to build CreateIndexParams");
    
    CreateIndex::execute(&mut context, &create_params)
        .expect("CreateIndex should succeed");
    
    // Test AddPostings operation
    let test_postings = generate_test_postings(10, 128);
    let add_params = AddPostingsParams::new(test_postings.clone())
        .expect("Failed to create AddPostingsParams");
    
    AddPostings::execute(&mut context, &add_params)
        .expect("AddPostings should succeed");
    
    // Test Flush operation
    let flush_params = FlushParams::with_stats();
    let flush_result = Flush::execute(&mut context, &flush_params)
        .expect("Flush should succeed");
    
    assert!(flush_result.is_some(), "Flush should return stats");
    let flush_stats = flush_result.unwrap();
    assert_eq!(flush_stats.operations_applied, 10, "Should flush 10 operations");
    
    // Test Search operation
    let query_vector = vec![0.1; 128];
    let search_params = SearchParams::builder()
        .query_vector(query_vector)
        .k(5)
        .build()
        .expect("Failed to build SearchParams");
    
    let search_results = Search::execute(&mut context, &search_params)
        .expect("Search should succeed");
    
    assert!(!search_results.is_empty(), "Search should return results");
    assert!(search_results.len() <= 5, "Should return at most k results");
    
    // Verify results have valid document IDs and similarity scores
    for result in &search_results {
        assert!(result.document_id.raw() > 0, "Document ID should be valid");
        assert!(result.similarity_score >= 0.0 && result.similarity_score <= 1.0, 
               "Similarity score should be between 0 and 1");
    }
    
    // Test GetStats operation
    let stats_params = GetStatsParams::new();
    let stats = GetStats::execute(&mut context, &stats_params)
        .expect("GetStats should succeed");
    
    assert_eq!(stats.vector_dimension, 128, "Vector dimension should match");
    assert!(stats.total_shards > 0, "Should have at least one shard");
}

#[test] 
fn test_document_text_integration() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();
    
    // Create index with text storage enabled - check available methods
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(128)
        .shard_size(1000)
        .build()
        .expect("Failed to build CreateIndexParams");
    
    CreateIndex::execute(&mut context, &create_params)
        .expect("CreateIndex should succeed");
    
    // Test StoreDocumentText operation
    let document_id = DocumentId::from_raw(1);
    let text = "This is a test document with meaningful content for testing purposes.";
    let postings = vec![Posting {
        document_id,
        start: 0,
        length: text.len() as u32,
        vector: vec![0.1; 128],
    }];
    
    let store_params = StoreDocumentTextParams {
        document_id,
        text: text.to_string(),
        postings,
    };
    
    StoreDocumentText::execute(&mut context, &store_params)
        .expect("StoreDocumentText should succeed");
    
    // Test GetDocumentText operation
    let get_params = GetDocumentTextParams { document_id };
    let retrieved_text = GetDocumentText::execute(&mut context, &get_params)
        .expect("GetDocumentText should succeed");
    
    assert_eq!(retrieved_text, text, "Retrieved text should match stored text");
    
    // Test ExtractSnippet operation
    let extract_params = ExtractSnippetParams {
        document_id,
        start: 10,
        length: 20,
    };
    
    let snippet = ExtractSnippet::execute(&mut context, &extract_params)
        .expect("ExtractSnippet should succeed");
    
    let expected_snippet = &text[10..30];
    assert_eq!(snippet, expected_snippet, "Snippet should match expected text range");
    
    // Test search integration with text
    let query_vector = vec![0.1; 128];
    let search_params = SearchParams::builder()
        .query_vector(query_vector)
        .k(10)
        .build()
        .expect("Failed to build SearchParams");
    
    let results = Search::execute(&mut context, &search_params)
        .expect("Search should succeed");
    
    assert!(!results.is_empty(), "Search should find the stored document");
    
    // Verify we can retrieve text for found documents
    let found_doc_id = results[0].document_id;
    let found_text = GetDocumentText::execute(&mut context, &GetDocumentTextParams {
        document_id: found_doc_id,
    })
    .expect("Should retrieve text for found document");
    
    assert_eq!(found_text, text, "Found document text should match original");
}

#[test]
fn test_batch_operations_integration() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();
    
    // Create index with text storage
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(64)
        .shard_size(1000)
        .build()
        .expect("Failed to build CreateIndexParams");
    
    CreateIndex::execute(&mut context, &create_params)
        .expect("CreateIndex should succeed");
    
    // Prepare batch data using DocumentTextEntry format
    let documents = vec![
        ("Document 1: Machine learning fundamentals", 1),
        ("Document 2: Deep learning techniques", 2),  
        ("Document 3: Natural language processing", 3),
        ("Document 4: Computer vision applications", 4),
        ("Document 5: Reinforcement learning basics", 5),
    ];
    
    let batch_data: Vec<DocumentTextEntry> = documents
        .into_iter()
        .map(|(text, id)| {
            let document_id = DocumentId::from_raw(id);
            let postings = vec![Posting {
                document_id,
                start: 0,
                length: text.len() as u32,
                vector: generate_test_postings(1, 64)[0].vector.clone(),
            }];
            
            DocumentTextEntry {
                document_id,
                text: text.to_string(),
                postings,
            }
        })
        .collect();
    
    // Test BatchStoreDocumentText operation
    let batch_params = BatchStoreDocumentTextParams {
        documents: batch_data.clone(),
        flush_immediately: true,
        track_performance: true,
    };
    
    let batch_stats = BatchStoreDocumentText::execute(&mut context, &batch_params)
        .expect("BatchStoreDocumentText should succeed");
    
    assert_eq!(batch_stats.documents_stored, 5, "Should store 5 documents");
    assert_eq!(batch_stats.total_postings, 5, "Should add 5 postings");
    assert!(batch_stats.total_text_size > 0, "Should have positive text size");
    
    // Verify all documents were stored correctly
    for doc_entry in &batch_data {
        let retrieved_text = GetDocumentText::execute(&mut context, &GetDocumentTextParams {
            document_id: doc_entry.document_id,
        })
        .expect("Should retrieve stored document text");
        
        assert_eq!(retrieved_text, doc_entry.text, "Batch stored text should match");
    }
    
    // Test search across batch documents
    let search_params = SearchParams::builder()
        .query_vector(vec![0.5; 64])
        .k(3)
        .build()
        .expect("Failed to build SearchParams");
    
    let results = Search::execute(&mut context, &search_params)
        .expect("Search should succeed");
    
    assert_eq!(results.len(), 3, "Should return requested number of results");
    
    // Verify search results are from our batch documents
    for result in &results {
        assert!(result.document_id.raw() >= 1 && result.document_id.raw() <= 5,
               "Search results should be from our batch documents");
    }
}

#[test]
fn test_error_handling_comprehensive() {
    let mut context = ShardexContext::new();
    
    // Test search without initialized index
    let search_params = SearchParams::builder()
        .query_vector(vec![0.1; 128])
        .k(10)
        .build()
        .expect("Failed to build SearchParams");
    
    let search_result = Search::execute(&mut context, &search_params);
    assert!(search_result.is_err(), "Search should fail on uninitialized context");
    
    // Test invalid configuration parameters
    let temp_dir = create_temp_directory();
    
    // Test zero vector size
    let invalid_create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(0) // Invalid
        .shard_size(1000)
        .build();
    
    assert!(invalid_create_params.is_err(), "Should reject zero vector size");
    
    // Test zero shard size
    let invalid_create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(128)
        .shard_size(0) // Invalid
        .build();
    
    assert!(invalid_create_params.is_err(), "Should reject zero shard size");
    
    // Test retrieving non-existent document
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(128)
        .shard_size(1000)
        .build()
        .expect("Failed to build CreateIndexParams");
    
    CreateIndex::execute(&mut context, &create_params)
        .expect("CreateIndex should succeed");
    
    let get_params = GetDocumentTextParams { 
        document_id: DocumentId::from_raw(999) 
    };
    let get_result = GetDocumentText::execute(&mut context, &get_params);
    assert!(get_result.is_err(), "Should fail to retrieve non-existent document");
}

#[test]
fn test_operations_workflow_integration() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();
    
    // Step 1: Create index
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(256)
        .shard_size(5000)
        .batch_write_interval_ms(50)
        .build()
        .expect("Failed to build CreateIndexParams");
    
    CreateIndex::execute(&mut context, &create_params)
        .expect("CreateIndex should succeed");
    
    // Step 2: Add initial postings
    let initial_postings = generate_test_postings(20, 256);
    let add_params = AddPostingsParams::new(initial_postings)
        .expect("Failed to create AddPostingsParams");
    
    AddPostings::execute(&mut context, &add_params)
        .expect("AddPostings should succeed");
    
    // Step 3: Store documents with text
    let text_documents = vec![
        (DocumentId::from_raw(21), "Research paper on artificial intelligence applications"),
        (DocumentId::from_raw(22), "Study on machine learning algorithms and optimization"),
        (DocumentId::from_raw(23), "Analysis of deep learning techniques in computer vision"),
    ];
    
    for (doc_id, text) in &text_documents {
        let postings = vec![Posting {
            document_id: *doc_id,
            start: 0,
            length: text.len() as u32,
            vector: generate_test_postings(1, 256)[0].vector.clone(),
        }];
        
        let store_params = StoreDocumentTextParams {
            document_id: *doc_id,
            text: text.to_string(),
            postings,
        };
        
        StoreDocumentText::execute(&mut context, &store_params)
            .expect("StoreDocumentText should succeed");
    }
    
    // Step 4: Flush all operations
    let flush_params = FlushParams::with_stats();
    let flush_stats = Flush::execute(&mut context, &flush_params)
        .expect("Flush should succeed");
    
    assert!(flush_stats.is_some(), "Flush should return statistics");
    let _stats = flush_stats.unwrap();
    println!("Flush completed successfully");
    
    // Step 5: Perform comprehensive search
    let search_params = SearchParams::builder()
        .query_vector(vec![0.3; 256])
        .k(10)
        .build()
        .expect("Failed to build SearchParams");
    
    let results = Search::execute(&mut context, &search_params)
        .expect("Search should succeed");
    
    assert!(!results.is_empty(), "Search should return results");
    
    // Step 6: Retrieve and validate text for documents with text
    for result in &results {
        if result.document_id.raw() >= 21 && result.document_id.raw() <= 23 {
            let text = GetDocumentText::execute(&mut context, &GetDocumentTextParams {
                document_id: result.document_id,
            })
            .expect("Should retrieve text for text-enabled documents");
            
            assert!(!text.is_empty(), "Retrieved text should not be empty");
            assert!(text.contains("artificial") || text.contains("machine") || text.contains("deep"),
                   "Text should contain expected content");
        }
    }
    
    // Step 7: Get final statistics
    let final_stats = GetStats::execute(&mut context, &GetStatsParams::new())
        .expect("GetStats should succeed");
    
    assert!(final_stats.total_shards >= 1, "Should have at least one shard");
    assert_eq!(final_stats.vector_dimension, 256, "Vector dimension should match");
    assert!(final_stats.memory_usage > 0, "Should use some memory");
}
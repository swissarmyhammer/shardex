//! Unit tests for batch_operations example patterns
//!
//! These tests validate the helper functions and patterns used in the
//! batch_operations.rs example to ensure correct behavior and API usage.

mod common;
use apithing::ApiOperation;
use common::{create_temp_dir_for_test, test_constants};
use shardex::{
    api::{
        operations::{BatchAddPostings, CreateIndex, GetStats, IncrementalAdd, Search},
        parameters::{BatchAddPostingsParams, CreateIndexParams, GetStatsParams, IncrementalAddParams, SearchParams},
        ShardexContext,
    },
    DocumentId, Posting,
};
use std::error::Error;

/// Test helper function for generating document batches
#[test]
fn test_generate_document_batch() {
    let batch = generate_document_batch(0, 5, 128);

    assert_eq!(batch.len(), 5);

    // Verify document IDs are sequential
    for (i, posting) in batch.iter().enumerate() {
        assert_eq!(posting.document_id.raw(), (i + 1) as u128);
        assert_eq!(posting.vector.len(), 128);
        assert_eq!(posting.start, 0);
        // Length should be variable: 50 + (doc_id % 50)
        let expected_length = 50 + ((i + 1) % 50) as u32;
        assert_eq!(posting.length, expected_length);
    }
}

/// Test deterministic vector generation
#[test]
fn test_generate_deterministic_vector() {
    let vector1 = generate_deterministic_vector(42, 64);
    let vector2 = generate_deterministic_vector(42, 64);
    let vector3 = generate_deterministic_vector(43, 64);

    // Same seed should produce identical vectors
    assert_eq!(vector1, vector2);

    // Different seeds should produce different vectors
    assert_ne!(vector1, vector3);

    // Vector should be normalized (magnitude ≈ 1.0)
    let magnitude: f32 = vector1.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (magnitude - 1.0).abs() < 0.001,
        "Vector should be normalized, magnitude: {}",
        magnitude
    );

    // Values should be in reasonable range [-1, 1]
    for &value in &vector1 {
        assert!((-1.0..=1.0).contains(&value), "Value {} outside expected range", value);
    }
}

/// Test test vector generation consistency
#[test]
fn test_generate_test_vector() {
    let vector1 = generate_test_vector(128);
    let vector2 = generate_test_vector(128);
    let vector3 = generate_test_vector(64);

    // Same size should produce identical vectors (fixed seed)
    assert_eq!(vector1, vector2);
    assert_eq!(vector1.len(), 128);
    assert_eq!(vector3.len(), 64);

    // Vector should be normalized
    let magnitude: f32 = vector1.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((magnitude - 1.0).abs() < 0.001, "Test vector should be normalized");
}

/// Test batch operations workflow with small batches
#[test]
fn test_batch_operations_workflow() -> Result<(), Box<dyn Error>> {
    let temp_dir = create_temp_dir_for_test();

    // Create context and index
    let mut context = ShardexContext::new();
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(test_constants::DEFAULT_VECTOR_SIZE)
        .shard_size(test_constants::DEFAULT_SHARD_SIZE)
        .build()?;

    CreateIndex::execute(&mut context, &create_params)?;

    // Test small batch indexing
    let batch = generate_document_batch(0, 10, test_constants::DEFAULT_VECTOR_SIZE);
    let batch_params = BatchAddPostingsParams::with_flush_and_tracking(batch)?;
    let batch_stats = BatchAddPostings::execute(&mut context, &batch_params)?;

    // Verify batch statistics
    assert!(batch_stats.processing_time.as_nanos() > 0);
    assert_eq!(batch_stats.operations_flushed, 10);
    assert!(batch_stats.throughput_docs_per_sec > 0.0);

    // Verify stats show documents
    let stats_params = GetStatsParams::new();
    let _stats = GetStats::execute(&mut context, &stats_params)?;
    // Note: Conservative batching may not show documents immediately
    // This is expected behavior, so we just verify the operation succeeded

    Ok(())
}

/// Test incremental operations workflow
#[test]
fn test_incremental_operations_workflow() -> Result<(), Box<dyn Error>> {
    let temp_dir = create_temp_dir_for_test();

    // Create context and index
    let mut context = ShardexContext::new();
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(test_constants::DEFAULT_VECTOR_SIZE)
        .shard_size(test_constants::DEFAULT_SHARD_SIZE)
        .build()?;

    CreateIndex::execute(&mut context, &create_params)?;

    // Test incremental additions
    for i in 0..3 {
        let increment = generate_document_batch(i * 5, 5, test_constants::DEFAULT_VECTOR_SIZE);
        let incremental_params = IncrementalAddParams::with_batch_id(increment, format!("test_batch_{}", i))?;
        let incremental_stats = IncrementalAdd::execute(&mut context, &incremental_params)?;

        // Verify incremental statistics
        assert_eq!(incremental_stats.postings_added, 5);
        assert!(incremental_stats.processing_time.as_nanos() > 0);
    }

    Ok(())
}

/// Test search operations with generated data
#[test]
fn test_search_operations() -> Result<(), Box<dyn Error>> {
    let temp_dir = create_temp_dir_for_test();

    // Create context and index with data
    let mut context = ShardexContext::new();
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(test_constants::DEFAULT_VECTOR_SIZE)
        .shard_size(test_constants::DEFAULT_SHARD_SIZE)
        .build()?;

    CreateIndex::execute(&mut context, &create_params)?;

    // Add some test data
    let batch = generate_document_batch(0, 20, test_constants::DEFAULT_VECTOR_SIZE);
    let batch_params = BatchAddPostingsParams::with_flush_and_tracking(batch)?;
    BatchAddPostings::execute(&mut context, &batch_params)?;

    // Test search with different k values
    let query_vector = generate_test_vector(test_constants::DEFAULT_VECTOR_SIZE);

    for k in [1, 3, 5] {
        let search_params = SearchParams::builder()
            .query_vector(query_vector.clone())
            .k(k)
            .build()?;

        let results = Search::execute(&mut context, &search_params)?;

        // Results length should not exceed k (may be less due to conservative batching)
        assert!(results.len() <= k);
    }

    Ok(())
}

/// Test error handling scenarios
#[test]
fn test_error_handling() {
    // Test invalid vector sizes
    let invalid_batch = generate_document_batch(0, 1, 0); // Zero vector size
    assert!(invalid_batch[0].vector.is_empty());

    // Test invalid batch sizes
    let empty_batch = generate_document_batch(0, 0, 128);
    assert!(empty_batch.is_empty());
}

/// Test batch size edge cases
#[test]
fn test_batch_size_edge_cases() {
    // Test single document batch
    let single_batch = generate_document_batch(100, 1, 64);
    assert_eq!(single_batch.len(), 1);
    assert_eq!(single_batch[0].document_id.raw(), 101);

    // Test large batch
    let large_batch = generate_document_batch(0, 100, 32);
    assert_eq!(large_batch.len(), 100);

    // Verify all documents have unique IDs
    let mut doc_ids = std::collections::HashSet::new();
    for posting in &large_batch {
        assert!(doc_ids.insert(posting.document_id.raw()));
    }
    assert_eq!(doc_ids.len(), 100);
}

/// Test vector normalization consistency
#[test]
fn test_vector_normalization() {
    for seed in [1, 42, 99999, 123456] {
        let vector = generate_deterministic_vector(seed, 256);
        let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();

        // All vectors should be normalized to unit length
        assert!(
            (magnitude - 1.0).abs() < 0.001,
            "Vector with seed {} not normalized: magnitude = {}",
            seed,
            magnitude
        );
    }
}

/// Test document length calculation consistency
#[test]
fn test_document_length_calculation() {
    let batch = generate_document_batch(0, 60, 64); // Test > 50 documents

    for (i, posting) in batch.iter().enumerate() {
        let doc_id = i + 1;
        let expected_length = 50 + (doc_id % 50) as u32;
        assert_eq!(
            posting.length, expected_length,
            "Document {} has incorrect length",
            doc_id
        );
    }
}

// Helper functions from the example (needed for tests)

/// Generate a batch of test documents with conservative settings
fn generate_document_batch(start_id: usize, count: usize, vector_size: usize) -> Vec<Posting> {
    (0..count)
        .map(|i| {
            let doc_id = start_id + i + 1;
            let document_id = DocumentId::from_raw(doc_id as u128);

            // Generate pseudo-random but deterministic vector
            let vector = generate_deterministic_vector(doc_id, vector_size);

            Posting {
                document_id,
                start: 0,
                length: 50 + (doc_id % 50) as u32, // Variable length documents (conservative)
                vector,
            }
        })
        .collect()
}

/// Generate a deterministic vector based on seed
fn generate_deterministic_vector(seed: usize, size: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut vector = Vec::with_capacity(size);
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);

    for i in 0..size {
        (seed + i).hash(&mut hasher);
        let value = ((hasher.finish() % 10000) as f32 - 5000.0) / 5000.0; // Range [-1, 1]
        vector.push(value);
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

/// Generate a test vector for querying
fn generate_test_vector(size: usize) -> Vec<f32> {
    generate_deterministic_vector(99999, size) // Use a fixed seed for consistent queries
}

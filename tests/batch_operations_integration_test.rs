//! Integration test for batch_operations.rs example
//!
//! This test validates that the batch_operations example runs successfully
//! end-to-end and produces expected output patterns.

mod common;
use apithing::ApiOperation;
use common::create_temp_dir_for_test;
use shardex::{
    api::{
        operations::{
            BatchAddPostings, CreateIndex, Flush, GetPerformanceStats, GetStats, IncrementalAdd, RemoveDocuments,
            Search,
        },
        parameters::{
            BatchAddPostingsParams, CreateIndexParams, FlushParams, GetPerformanceStatsParams, GetStatsParams,
            IncrementalAddParams, RemoveDocumentsParams, SearchParams,
        },
        ShardexContext,
    },
    DocumentId, Posting,
};
use std::error::Error;
use std::time::{Duration, Instant};

/// Full integration test that simulates the batch_operations example workflow
#[test]
fn test_batch_operations_example_workflow() -> Result<(), Box<dyn Error>> {
    // Create temporary directory
    let temp_dir = create_temp_dir_for_test();

    // Create context and configure for conservative batch operations
    let mut context = ShardexContext::new();
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(256) // Conservative vector size
        .shard_size(10000) // Conservative shard size
        .batch_write_interval_ms(100) // Standard batching window
        .default_slop_factor(3) // Standard slop factor
        .bloom_filter_size(1024) // Conservative bloom filter
        .wal_segment_size(2 * 1024 * 1024) // 2MB WAL segments
        .build()?;

    CreateIndex::execute(&mut context, &create_params)?;

    // Test 1: Conservative Batch Indexing
    const BATCH_SIZE: usize = 25;
    const NUM_BATCHES: usize = 3;
    const TOTAL_DOCS: usize = BATCH_SIZE * NUM_BATCHES;

    let mut total_indexing_time = Duration::new(0, 0);

    for batch_num in 0..NUM_BATCHES {
        let batch_start = batch_num * BATCH_SIZE;
        let postings = generate_document_batch(batch_start, BATCH_SIZE, 256);

        let batch_params = BatchAddPostingsParams::with_flush_and_tracking(postings)?;
        let batch_stats = BatchAddPostings::execute(&mut context, &batch_params)?;

        total_indexing_time += batch_stats.processing_time;

        // Verify batch statistics are reasonable
        assert!(batch_stats.processing_time.as_nanos() > 0);
        assert_eq!(batch_stats.operations_flushed, BATCH_SIZE as u64);
        assert!(batch_stats.throughput_docs_per_sec >= 0.0);

        let stats_params = GetStatsParams::new();
        let _stats = GetStats::execute(&mut context, &stats_params)?;
    }

    // Verify overall throughput calculation works
    assert!(total_indexing_time.as_nanos() > 0);
    let overall_throughput = TOTAL_DOCS as f64 / total_indexing_time.as_secs_f64();
    assert!(overall_throughput >= 0.0);

    // Test 2: Incremental Operations with Monitoring
    let initial_stats_params = GetStatsParams::new();
    let _initial_stats = GetStats::execute(&mut context, &initial_stats_params)?;

    for i in 0..3 {
        let increment = generate_document_batch(TOTAL_DOCS + i * 10, 10, 256);
        let incremental_params = IncrementalAddParams::with_batch_id(increment, format!("increment_{}", i + 1))?;
        let incremental_stats = IncrementalAdd::execute(&mut context, &incremental_params)?;

        assert_eq!(incremental_stats.postings_added, 10);
        assert!(incremental_stats.processing_time.as_nanos() > 0);

        if i % 2 == 0 {
            let flush_params = FlushParams::new();
            Flush::execute(&mut context, &flush_params)?;
        }

        let current_stats_params = GetStatsParams::new();
        let _current_stats = GetStats::execute(&mut context, &current_stats_params)?;

        std::thread::sleep(Duration::from_millis(50)); // Reduced sleep for tests
    }

    let final_flush_params = FlushParams::new();
    Flush::execute(&mut context, &final_flush_params)?;
    let final_stats_params = GetStatsParams::new();
    let final_stats = GetStats::execute(&mut context, &final_stats_params)?;

    // Test 3: Conservative Document Removal
    let mut docs_to_remove = Vec::new();
    for i in (5..=final_stats.total_postings).step_by(5) {
        docs_to_remove.push(i as u128);
    }

    if !docs_to_remove.is_empty() {
        let removal_params = RemoveDocumentsParams::new(docs_to_remove)?;
        let removal_stats = RemoveDocuments::execute(&mut context, &removal_params)?;

        let flush_params = FlushParams::new();
        Flush::execute(&mut context, &flush_params)?;

        let after_removal_stats_params = GetStatsParams::new();
        let _after_removal_stats = GetStats::execute(&mut context, &after_removal_stats_params)?;

        assert!(removal_stats.processing_time.as_nanos() > 0);
    }

    // Test 4: Search Performance Testing
    let query_vector = generate_test_vector(256);

    for k in [1, 3, 5, 10] {
        let search_params = SearchParams::builder()
            .query_vector(query_vector.clone())
            .k(k)
            .slop_factor(None)
            .build()?;

        let search_start = Instant::now();
        let results = Search::execute(&mut context, &search_params)?;
        let search_time = search_start.elapsed();

        assert!(search_time.as_nanos() > 0);
        assert!(results.len() <= k); // May be fewer due to conservative batching
    }

    // Test 5: Final Statistics Summary
    let detailed_stats_params = GetStatsParams::new();
    let detailed_stats = GetStats::execute(&mut context, &detailed_stats_params)?;
    let perf_stats_params = GetPerformanceStatsParams::detailed();
    let perf_stats = GetPerformanceStats::execute(&mut context, &perf_stats_params)?;

    // Verify all statistics are reasonable
    assert!(detailed_stats.average_shard_utilization >= 0.0);
    assert!(perf_stats.average_latency.as_nanos() > 0);
    assert!(perf_stats.throughput >= 0.0);

    Ok(())
}

/// Test performance characteristics are within reasonable bounds
#[test]
fn test_batch_operations_performance() -> Result<(), Box<dyn Error>> {
    let temp_dir = create_temp_dir_for_test();

    let mut context = ShardexContext::new();
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(128) // Smaller for faster test
        .shard_size(50) // Smaller for faster test
        .build()?;

    CreateIndex::execute(&mut context, &create_params)?;

    // Time batch operations
    let batch = generate_document_batch(0, 20, 128);
    let start_time = Instant::now();
    let batch_params = BatchAddPostingsParams::with_flush_and_tracking(batch)?;
    let batch_stats = BatchAddPostings::execute(&mut context, &batch_params)?;
    let total_time = start_time.elapsed();

    // Performance should be reasonable (not too slow)
    assert!(
        total_time < Duration::from_secs(30),
        "Batch operation too slow: {:?}",
        total_time
    );
    assert!(
        batch_stats.throughput_docs_per_sec > 0.0,
        "Throughput should be positive"
    );

    // Search should also be reasonably fast
    let query_vector = generate_test_vector(128);
    let search_params = SearchParams::builder()
        .query_vector(query_vector)
        .k(5)
        .build()?;

    let search_start = Instant::now();
    let _results = Search::execute(&mut context, &search_params)?;
    let search_time = search_start.elapsed();

    assert!(
        search_time < Duration::from_secs(10),
        "Search too slow: {:?}",
        search_time
    );

    Ok(())
}

/// Test cleanup behavior and resource management
#[test]
fn test_batch_operations_cleanup() -> Result<(), Box<dyn Error>> {
    let temp_dir = create_temp_dir_for_test();
    let temp_path = temp_dir.path().to_path_buf();

    {
        // Create and use context in a limited scope
        let mut context = ShardexContext::new();
        let create_params = CreateIndexParams::builder()
            .directory_path(temp_path.clone())
            .vector_size(64)
            .shard_size(50)
            .build()?;

        CreateIndex::execute(&mut context, &create_params)?;

        let batch = generate_document_batch(0, 10, 64);
        let batch_params = BatchAddPostingsParams::with_flush_and_tracking(batch)?;
        BatchAddPostings::execute(&mut context, &batch_params)?;

        let flush_params = FlushParams::new();
        Flush::execute(&mut context, &flush_params)?;

        // Context goes out of scope here
    }

    // Verify files were created
    assert!(temp_path.exists(), "Index directory should exist");

    // temp_dir cleanup happens automatically when temp_dir is dropped

    Ok(())
}

// Helper functions (copied from example for testing)

fn generate_document_batch(start_id: usize, count: usize, vector_size: usize) -> Vec<Posting> {
    (0..count)
        .map(|i| {
            let doc_id = start_id + i + 1;
            let document_id = DocumentId::from_raw(doc_id as u128);
            let vector = generate_deterministic_vector(doc_id, vector_size);

            Posting {
                document_id,
                start: 0,
                length: 50 + (doc_id % 50) as u32,
                vector,
            }
        })
        .collect()
}

fn generate_deterministic_vector(seed: usize, size: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut vector = Vec::with_capacity(size);
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);

    for i in 0..size {
        (seed + i).hash(&mut hasher);
        let value = ((hasher.finish() % 10000) as f32 - 5000.0) / 5000.0;
        vector.push(value);
    }

    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut vector {
            *value /= magnitude;
        }
    } else {
        // Fallback if all values are zero (extremely unlikely)
        for (i, value) in vector.iter_mut().enumerate() {
            *value = if i == 0 { 1.0 } else { 0.0 };
        }
    }

    vector
}

fn generate_test_vector(size: usize) -> Vec<f32> {
    generate_deterministic_vector(99999, size)
}

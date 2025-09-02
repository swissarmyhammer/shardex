//! API performance regression tests
//!
//! This test suite establishes baseline performance metrics and validates
//! that performance doesn't regress significantly between versions.

use apithing::ApiOperation;
use shardex::api::{
    AddPostings, AddPostingsParams, CreateIndex, CreateIndexParams, Flush, FlushParams, Search, SearchParams,
    ShardexContext,
};
use shardex::{DocumentId, Posting};
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Performance test configuration
const PERFORMANCE_VECTOR_SIZE: usize = 256;
const PERFORMANCE_SHARD_SIZE: usize = 10000;
const SMALL_DATASET_SIZE: usize = 100;
const SEARCH_ITERATIONS: usize = 50;

/// Performance thresholds (these are conservative to avoid flaky tests)
const MAX_INDEX_CREATION_TIME: Duration = Duration::from_secs(5);
const MAX_POSTING_ADD_TIME_PER_100: Duration = Duration::from_secs(10);
const MAX_SEARCH_TIME_PER_QUERY: Duration = Duration::from_millis(500);
const _MAX_TEXT_STORAGE_TIME_PER_DOC: Duration = Duration::from_millis(100);
const _MAX_BATCH_PROCESSING_TIME_PER_100_DOCS: Duration = Duration::from_secs(30);

/// Create a temporary directory for performance tests
fn create_temp_directory() -> TempDir {
    tempfile::tempdir().expect("Failed to create temporary directory")
}

/// Generate deterministic test postings for performance testing
fn generate_performance_postings(count: usize, vector_size: usize) -> Vec<Posting> {
    (0..count)
        .map(|i| {
            let document_id = DocumentId::from_raw((i + 1) as u128);
            let mut vector = vec![0.0; vector_size];

            // Create realistic but deterministic vectors
            for (j, item) in vector.iter_mut().enumerate().take(vector_size) {
                *item = ((i * 31 + j * 47) as f32 % 200.0 - 100.0) / 100.0;
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
                start: (i * 50) as u32,
                length: 50,
                vector,
            }
        })
        .collect()
}

#[test]
fn test_index_creation_performance() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(PERFORMANCE_VECTOR_SIZE)
        .shard_size(PERFORMANCE_SHARD_SIZE)
        .build()
        .expect("Failed to build CreateIndexParams");

    let start_time = Instant::now();
    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");
    let creation_time = start_time.elapsed();

    println!("Index creation time: {:?}", creation_time);
    assert!(
        creation_time < MAX_INDEX_CREATION_TIME,
        "Index creation took {:?}, which exceeds maximum of {:?}",
        creation_time,
        MAX_INDEX_CREATION_TIME
    );
}

#[test]
fn test_posting_addition_performance() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    // Create index
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(PERFORMANCE_VECTOR_SIZE)
        .shard_size(PERFORMANCE_SHARD_SIZE)
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    // Test posting addition performance
    let test_postings = generate_performance_postings(SMALL_DATASET_SIZE, PERFORMANCE_VECTOR_SIZE);
    let add_params = AddPostingsParams::new(test_postings).expect("Failed to create AddPostingsParams");

    let start_time = Instant::now();
    AddPostings::execute(&mut context, &add_params).expect("AddPostings should succeed");
    let addition_time = start_time.elapsed();

    println!(
        "Posting addition time for {} documents: {:?}",
        SMALL_DATASET_SIZE, addition_time
    );
    println!(
        "Throughput: {:.2} docs/sec",
        SMALL_DATASET_SIZE as f64 / addition_time.as_secs_f64()
    );

    assert!(
        addition_time < MAX_POSTING_ADD_TIME_PER_100,
        "Adding {} postings took {:?}, which exceeds maximum of {:?}",
        SMALL_DATASET_SIZE,
        addition_time,
        MAX_POSTING_ADD_TIME_PER_100
    );
}

#[test]
fn test_search_performance() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    // Create and populate index
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(PERFORMANCE_VECTOR_SIZE)
        .shard_size(PERFORMANCE_SHARD_SIZE)
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    let test_postings = generate_performance_postings(SMALL_DATASET_SIZE, PERFORMANCE_VECTOR_SIZE);
    let add_params = AddPostingsParams::new(test_postings).expect("Failed to create AddPostingsParams");

    AddPostings::execute(&mut context, &add_params).expect("AddPostings should succeed");

    // Flush to ensure data is available for search
    let flush_params = FlushParams::with_stats();
    Flush::execute(&mut context, &flush_params).expect("Flush should succeed");

    // Test search performance
    let query_vector = vec![0.5; PERFORMANCE_VECTOR_SIZE];
    let search_params = SearchParams::builder()
        .query_vector(query_vector)
        .k(10)
        .build()
        .expect("Failed to build SearchParams");

    let mut total_search_time = Duration::ZERO;
    let mut total_results = 0;

    for i in 0..SEARCH_ITERATIONS {
        let start_time = Instant::now();
        let results = Search::execute(&mut context, &search_params).expect("Search should succeed");
        let search_time = start_time.elapsed();

        total_search_time += search_time;
        total_results += results.len();

        if i == 0 {
            println!("First search returned {} results", results.len());
        }
    }

    let average_search_time = total_search_time / SEARCH_ITERATIONS as u32;
    let average_results = total_results as f64 / SEARCH_ITERATIONS as f64;

    println!("Average search time: {:?}", average_search_time);
    println!("Average results per search: {:.2}", average_results);
    println!(
        "Search throughput: {:.2} searches/sec",
        1.0 / average_search_time.as_secs_f64()
    );

    assert!(
        average_search_time < MAX_SEARCH_TIME_PER_QUERY,
        "Average search time {:?} exceeds maximum of {:?}",
        average_search_time,
        MAX_SEARCH_TIME_PER_QUERY
    );
}

#[test]
fn test_memory_usage_baseline() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    // Create index
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(PERFORMANCE_VECTOR_SIZE)
        .shard_size(PERFORMANCE_SHARD_SIZE)
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    // Add a small number of postings
    let small_batch_size = 25;
    let postings = generate_performance_postings(small_batch_size, PERFORMANCE_VECTOR_SIZE);
    let add_params = AddPostingsParams::new(postings).expect("Failed to create AddPostingsParams");

    AddPostings::execute(&mut context, &add_params).expect("AddPostings should succeed");

    // Flush to get accurate statistics
    let flush_params = FlushParams::new();
    Flush::execute(&mut context, &flush_params).expect("Flush should succeed");

    // Get memory usage statistics
    let stats = shardex::api::GetStats::execute(&mut context, &shardex::api::GetStatsParams::new())
        .expect("GetStats should succeed");

    println!(
        "Baseline memory usage: {:.2} MB for {} postings",
        stats.memory_usage as f64 / 1024.0 / 1024.0,
        small_batch_size
    );

    assert!(stats.memory_usage > 0, "Should use some memory");
}

#[test]
fn test_memory_usage_batch_processing() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    // Create index
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(PERFORMANCE_VECTOR_SIZE)
        .shard_size(PERFORMANCE_SHARD_SIZE)
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    // Test memory usage with 3 smaller batches
    let batch_size = 20;
    let num_batches = 3;
    let mut memory_growth = Vec::new();

    for batch in 0..num_batches {
        let postings = generate_performance_postings(batch_size, PERFORMANCE_VECTOR_SIZE);
        let add_params = AddPostingsParams::new(postings).expect("Failed to create AddPostingsParams");

        AddPostings::execute(&mut context, &add_params).expect("AddPostings should succeed");

        // Flush to get accurate statistics
        let flush_params = FlushParams::new();
        Flush::execute(&mut context, &flush_params).expect("Flush should succeed");

        // Get memory usage statistics
        let stats = shardex::api::GetStats::execute(&mut context, &shardex::api::GetStatsParams::new())
            .expect("GetStats should succeed");

        memory_growth.push(stats.memory_usage);
        println!(
            "Batch {}: memory usage: {:.2} MB",
            batch + 1,
            stats.memory_usage as f64 / 1024.0 / 1024.0
        );
    }

    // Memory should grow with each batch (allow for some tolerance due to memory management)
    let growth_1_to_2 = memory_growth[1] >= memory_growth[0];
    let growth_2_to_3 = memory_growth[2] >= memory_growth[1];

    // At least one should show growth, or final should be higher than first
    assert!(
        growth_1_to_2 || growth_2_to_3 || memory_growth[2] > memory_growth[0],
        "Memory should generally increase with more postings. Growth: {} -> {} -> {}",
        memory_growth[0],
        memory_growth[1],
        memory_growth[2]
    );
}

#[test]
fn test_memory_per_posting_efficiency() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    // Create index
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(PERFORMANCE_VECTOR_SIZE)
        .shard_size(PERFORMANCE_SHARD_SIZE)
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    // Add a moderate number of postings for efficiency testing
    let posting_count = 100;
    let postings = generate_performance_postings(posting_count, PERFORMANCE_VECTOR_SIZE);
    let add_params = AddPostingsParams::new(postings).expect("Failed to create AddPostingsParams");

    AddPostings::execute(&mut context, &add_params).expect("AddPostings should succeed");

    // Flush to get accurate statistics
    let flush_params = FlushParams::new();
    Flush::execute(&mut context, &flush_params).expect("Flush should succeed");

    // Get memory usage statistics
    let stats = shardex::api::GetStats::execute(&mut context, &shardex::api::GetStatsParams::new())
        .expect("GetStats should succeed");

    let memory_per_posting = stats.memory_usage as f64 / posting_count as f64;

    println!(
        "Memory efficiency: {:.2} MB for {} postings",
        stats.memory_usage as f64 / 1024.0 / 1024.0,
        posting_count
    );
    println!("Memory per posting: {:.2} bytes", memory_per_posting);

    // Reasonable memory usage expectations (these are conservative)
    // Each posting has a 256-dim vector (1KB) plus indexing overhead (bloom filters, WAL, etc.)
    assert!(
        memory_per_posting < 150000.0,
        "Memory per posting should be reasonable (< 150KB), got {:.2} bytes",
        memory_per_posting
    );
}

#[test]
fn test_scaling_performance() {
    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    // Create index optimized for larger datasets
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(PERFORMANCE_VECTOR_SIZE)
        .shard_size(PERFORMANCE_SHARD_SIZE)
        .batch_write_interval_ms(50) // Faster batching
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    // Test performance scaling with different dataset sizes
    let test_sizes = vec![50, 100, 200];

    for size in test_sizes {
        println!("\nTesting with {} documents:", size);

        // Clear the index for each test (create new context)
        let mut test_context = ShardexContext::new();
        let temp_test_dir = create_temp_directory();
        let test_create_params = CreateIndexParams::builder()
            .directory_path(temp_test_dir.path().to_path_buf())
            .vector_size(PERFORMANCE_VECTOR_SIZE)
            .shard_size(PERFORMANCE_SHARD_SIZE)
            .batch_write_interval_ms(50)
            .build()
            .expect("Failed to build CreateIndexParams");

        CreateIndex::execute(&mut test_context, &test_create_params).expect("CreateIndex should succeed");

        // Add postings
        let postings = generate_performance_postings(size, PERFORMANCE_VECTOR_SIZE);
        let add_params = AddPostingsParams::new(postings).expect("Failed to create AddPostingsParams");

        let start_time = Instant::now();
        AddPostings::execute(&mut test_context, &add_params).expect("AddPostings should succeed");
        let add_time = start_time.elapsed();

        // Flush
        let flush_params = FlushParams::new();
        let flush_start = Instant::now();
        Flush::execute(&mut test_context, &flush_params).expect("Flush should succeed");
        let flush_time = flush_start.elapsed();

        // Search
        let query_vector = vec![0.3; PERFORMANCE_VECTOR_SIZE];
        let search_params = SearchParams::builder()
            .query_vector(query_vector)
            .k(10)
            .build()
            .expect("Failed to build SearchParams");

        let search_start = Instant::now();
        let results = Search::execute(&mut test_context, &search_params).expect("Search should succeed");
        let search_time = search_start.elapsed();

        println!(
            "  Add time: {:?} ({:.2} docs/sec)",
            add_time,
            size as f64 / add_time.as_secs_f64()
        );
        println!("  Flush time: {:?}", flush_time);
        println!("  Search time: {:?} ({} results)", search_time, results.len());

        // Performance should scale reasonably
        let throughput = size as f64 / add_time.as_secs_f64();
        assert!(
            throughput > 1.0,
            "Should achieve at least 1 doc/sec throughput for size {}",
            size
        );
    }
}

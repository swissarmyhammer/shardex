//! Advanced configuration example for Shardex
//!
//! This example demonstrates:
//! - Different configuration options and their effects
//! - Performance tuning parameters
//! - Creating and opening indexes with custom configurations
//! - Configuration validation and error handling

use apithing::ApiOperation;
use shardex::{
    api::{
        operations::{OpenIndex, ValidateConfig},
        parameters::{OpenIndexParams, ValidateConfigParams},
        AddPostings, AddPostingsParams, CreateIndex, CreateIndexParams, Flush, FlushParams, GetStats, GetStatsParams,
        Search, SearchParams, ShardexContext,
    },
    DocumentId, Posting, ShardexConfig,
};
use std::error::Error;
use std::time::Instant;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Configuration Example");
    println!("=============================");

    // Create temporary directories for different configurations
    let base_dir = std::env::temp_dir().join("shardex_config_examples");
    if base_dir.exists() {
        std::fs::remove_dir_all(&base_dir)?;
    }
    std::fs::create_dir_all(&base_dir)?;

    // Example 1: Default configuration
    println!("\n1. Default Configuration");
    println!("------------------------");
    let default_params = CreateIndexParams::builder()
        .directory_path(base_dir.join("default_index"))
        .build()
        .expect("Default configuration should be valid");

    print_config(&default_params);

    // Example 2: Conservative high-performance configuration (avoids hang issues)
    println!("\n2. High-Performance Configuration");
    println!("---------------------------------");
    let high_perf_params = CreateIndexParams::high_performance(base_dir.join("high_perf_index"));

    print_config(&high_perf_params);

    // Example 3: Memory-optimized configuration for resource-constrained environments
    println!("\n3. Memory-Optimized Configuration");
    println!("----------------------------------");
    let memory_opt_params = CreateIndexParams::memory_optimized(base_dir.join("memory_opt_index"));

    print_config(&memory_opt_params);

    // Create and test the high-performance configuration
    println!("\n4. Testing High-Performance Configuration");
    println!("=========================================");

    println!("Creating index with high-performance config...");
    let mut context = ShardexContext::new();
    CreateIndex::execute(&mut context, &high_perf_params)?;

    // Generate conservative test data (reduced to avoid hang)
    let test_data = generate_test_data(20, 256); // 20 documents, 256 dimensions
    println!(
        "Generated {} test documents with {}-dimensional vectors",
        test_data.len(),
        256
    );

    // Measure indexing performance
    let start_time = Instant::now();
    let add_params = AddPostingsParams::new(test_data)?;
    AddPostings::execute(&mut context, &add_params)?;
    let flush_params = FlushParams::new();
    Flush::execute(&mut context, &flush_params)?;
    let indexing_time = start_time.elapsed();

    println!("Indexing completed in {:?}", indexing_time);

    // Small delay to ensure statistics are updated after flush
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Get detailed statistics
    let stats_params = GetStatsParams::new();
    let stats = GetStats::execute(&mut context, &stats_params)?;
    println!("Final index statistics:");
    println!("  - Shards: {}", stats.total_shards);
    println!("  - Postings: {}", stats.total_postings);
    println!(
        "  - Memory usage: {:.2} MB",
        stats.memory_usage as f64 / 1024.0 / 1024.0
    );
    println!("  - Disk usage: {:.2} MB", stats.disk_usage as f64 / 1024.0 / 1024.0);
    println!(
        "  - Avg shard utilization: {:.1}%",
        stats.average_shard_utilization * 100.0
    );

    // Test search performance
    println!("\n5. Search Performance Testing");
    println!("============================");

    let query_vector = generate_random_vector(256);
    let search_start = Instant::now();
    let search_params = SearchParams::new(query_vector, 10)?;
    let results = Search::execute(&mut context, &search_params)?;
    let search_time = search_start.elapsed();

    println!("Search for k=10 completed in {:?}", search_time);
    println!("Found {} results", results.len());

    if !results.is_empty() {
        println!("Top result similarity: {:.4}", results[0].similarity_score);
    }

    // Example 6: Reopening an existing index
    println!("\n6. Reopening Existing Index");
    println!("===========================");

    // Close the current index by dropping the context
    drop(context);

    // Reopen the index (configuration will be read from metadata)
    let mut reopen_context = ShardexContext::new();
    let open_params = OpenIndexParams::new(high_perf_params.directory_path.clone());
    OpenIndex::execute(&mut reopen_context, &open_params)?;
    let stats_params = GetStatsParams::new();
    let reopened_stats = GetStats::execute(&mut reopen_context, &stats_params)?;

    println!("Reopened index statistics:");
    println!("  - Postings: {}", reopened_stats.total_postings);
    println!(
        "  - Configuration preserved: vector_size = {}",
        reopened_stats.vector_dimension
    );

    // Example 7: Configuration validation
    println!("\n7. Configuration Validation");
    println!("===========================");

    // This would fail with invalid vector size
    println!("Testing invalid configuration (vector_size = 0)...");
    let invalid_config = ShardexConfig::new()
        .directory_path(base_dir.join("invalid_index"))
        .vector_size(0); // Invalid!

    let mut validation_context = ShardexContext::new();
    let validate_params = ValidateConfigParams::new(invalid_config);
    match ValidateConfig::execute(&mut validation_context, &validate_params) {
        Ok(true) => println!("Unexpected: Invalid config was accepted"),
        Ok(false) => println!("Expected: Invalid config was rejected"),
        Err(e) => println!("Error during validation: {}", e),
    }

    // Clean up
    std::fs::remove_dir_all(&base_dir)?;
    println!("\nConfiguration examples completed!");

    Ok(())
}

/// Prints the configuration parameters in a readable format
///
/// # Arguments
/// * `params` - The CreateIndexParams to display
fn print_config(params: &CreateIndexParams) {
    println!("  Directory: {:?}", params.directory_path);
    println!("  Vector size: {}", params.vector_size);
    println!("  Shard size: {}", params.shard_size);
    println!("  WAL segment size: {} bytes", params.wal_segment_size);
    println!("  Batch interval: {}ms", params.batch_write_interval_ms);
    println!("  Default slop factor: {}", params.default_slop_factor);
    println!("  Bloom filter size: {}", params.bloom_filter_size);
}

/// Generates test postings data for performance benchmarking
///
/// # Arguments
/// * `count` - Number of postings to generate
/// * `vector_size` - Dimensionality of the embedding vectors
///
/// # Returns
/// Vector of test postings with sequential document IDs and normalized random vectors
fn generate_test_data(count: usize, vector_size: usize) -> Vec<Posting> {
    (0..count)
        .map(|i| {
            let document_id = DocumentId::from_raw((i + 1) as u128);
            let vector = generate_random_vector(vector_size);

            Posting {
                document_id,
                start: 0,
                length: 100, // Assume 100-byte documents
                vector,
            }
        })
        .collect()
}

/// Generates a deterministic normalized random vector using hash-based values
///
/// This creates reproducible random vectors using a hash-based approach,
/// ensuring consistent test results across runs.
///
/// # Arguments
/// * `size` - Number of dimensions in the vector
///
/// # Returns
/// Normalized vector with unit magnitude (L2 norm = 1.0)
fn generate_random_vector(size: usize) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut vector = Vec::with_capacity(size);
    let mut hasher = DefaultHasher::new();

    for i in 0..size {
        i.hash(&mut hasher);
        let value = (hasher.finish() % 1000) as f32 / 1000.0;
        vector.push(value);
    }

    // Normalize vector
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut vector {
            *value /= magnitude;
        }
    }

    vector
}

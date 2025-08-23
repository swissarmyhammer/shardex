//! Advanced configuration example for Shardex
//!
//! This example demonstrates:
//! - Different configuration options and their effects
//! - Performance tuning parameters
//! - Creating and opening indexes with custom configurations
//! - Configuration validation and error handling

use shardex::{DocumentId, Posting, Shardex, ShardexConfig, ShardexImpl};
use std::error::Error;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
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
    let default_config = ShardexConfig::new().directory_path(base_dir.join("default_index"));

    print_config(&default_config);

    // Example 2: High-performance configuration for large datasets
    println!("\n2. High-Performance Configuration");
    println!("---------------------------------");
    let high_perf_config = ShardexConfig::new()
        .directory_path(base_dir.join("high_perf_index"))
        .vector_size(768) // Larger vectors (e.g., BERT embeddings)
        .shard_size(50000) // Larger shards for fewer splits
        .shardex_segment_size(5000) // More centroids per segment
        .wal_segment_size(16 * 1024 * 1024) // 16MB WAL segments
        .batch_write_interval_ms(50) // Faster batching
        .default_slop_factor(5) // Broader search for accuracy
        .bloom_filter_size(4096); // Larger bloom filters

    print_config(&high_perf_config);

    // Example 3: Memory-optimized configuration for resource-constrained environments
    println!("\n3. Memory-Optimized Configuration");
    println!("----------------------------------");
    let memory_opt_config = ShardexConfig::new()
        .directory_path(base_dir.join("memory_opt_index"))
        .vector_size(128) // Smaller vectors
        .shard_size(5000) // Smaller shards
        .shardex_segment_size(500) // Fewer centroids per segment
        .wal_segment_size(256 * 1024) // 256KB WAL segments
        .batch_write_interval_ms(200) // Less frequent batching
        .default_slop_factor(2) // Narrower search for speed
        .bloom_filter_size(512); // Smaller bloom filters

    print_config(&memory_opt_config);

    // Create and test the high-performance configuration
    println!("\n4. Testing High-Performance Configuration");
    println!("=========================================");

    let mut index = ShardexImpl::create(high_perf_config.clone()).await?;

    // Generate test data with larger vectors
    let test_data = generate_test_data(100, 768); // 100 documents, 768 dimensions
    println!(
        "Generated {} test documents with {}-dimensional vectors",
        test_data.len(),
        768
    );

    // Measure indexing performance
    let start_time = Instant::now();
    index.add_postings(test_data).await?;
    index.flush().await?;
    let indexing_time = start_time.elapsed();

    println!("Indexing completed in {:?}", indexing_time);

    // Get detailed statistics
    let stats = index.stats().await?;
    println!("Final index statistics:");
    println!("  - Shards: {}", stats.total_shards);
    println!("  - Postings: {}", stats.total_postings);
    println!(
        "  - Memory usage: {:.2} MB",
        stats.memory_usage as f64 / 1024.0 / 1024.0
    );
    println!(
        "  - Disk usage: {:.2} MB",
        stats.disk_usage as f64 / 1024.0 / 1024.0
    );
    println!(
        "  - Avg shard utilization: {:.1}%",
        stats.average_shard_utilization * 100.0
    );

    // Test search performance
    println!("\n5. Search Performance Testing");
    println!("============================");

    let query_vector = generate_random_vector(768);
    let search_start = Instant::now();
    let results = index.search(&query_vector, 10, None).await?;
    let search_time = search_start.elapsed();

    println!("Search for k=10 completed in {:?}", search_time);
    println!("Found {} results", results.len());

    if !results.is_empty() {
        println!("Top result similarity: {:.4}", results[0].similarity_score);
    }

    // Example 6: Reopening an existing index
    println!("\n6. Reopening Existing Index");
    println!("===========================");

    // Close the current index by dropping it
    drop(index);

    // Reopen the index (configuration will be read from metadata)
    let reopened_index = ShardexImpl::open(high_perf_config.directory_path).await?;
    let reopened_stats = reopened_index.stats().await?;

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

    match ShardexImpl::create(invalid_config).await {
        Ok(_) => println!("Unexpected: Invalid config was accepted"),
        Err(e) => println!("Expected error: {}", e),
    }

    // Clean up
    std::fs::remove_dir_all(&base_dir)?;
    println!("\nConfiguration examples completed!");

    Ok(())
}

fn print_config(config: &ShardexConfig) {
    println!("  Directory: {:?}", config.directory_path);
    println!("  Vector size: {}", config.vector_size);
    println!("  Shard size: {}", config.shard_size);
    println!("  Shardex segment size: {}", config.shardex_segment_size);
    println!("  WAL segment size: {} bytes", config.wal_segment_size);
    println!("  Batch interval: {}ms", config.batch_write_interval_ms);
    println!(
        "  Default slop factor: {}",
        config.slop_factor_config.default_factor
    );
    println!("  Bloom filter size: {}", config.bloom_filter_size);
}

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

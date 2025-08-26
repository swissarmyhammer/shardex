//! Conservative batch operations example for Shardex
//!
//! This example demonstrates:
//! - Conservative batch indexing (avoiding hang issues)
//! - Batch document removal
//! - Performance monitoring
//! - Practical batch processing patterns

use shardex::{DocumentId, Posting, Shardex, ShardexConfig, ShardexImpl};
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Batch Operations Example (Conservative)");
    println!("===============================================");

    // Create temporary directory
    let temp_dir = std::env::temp_dir().join("shardex_batch_conservative");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Configure for conservative batch operations
    let config = ShardexConfig::new()
        .directory_path(&temp_dir)
        .vector_size(256) // Conservative vector size
        .shard_size(10000) // Conservative shard size
        .batch_write_interval_ms(100) // Standard batching window
        .default_slop_factor(3) // Standard slop factor
        .bloom_filter_size(1024) // Conservative bloom filter
        .wal_segment_size(2 * 1024 * 1024); // 2MB WAL segments

    let mut index = ShardexImpl::create(config).await?;

    // Example 1: Small batch indexing (conservative approach)
    println!("\n1. Conservative Batch Indexing");
    println!("==============================");

    const BATCH_SIZE: usize = 25; // Very conservative batch size
    const NUM_BATCHES: usize = 3;
    const TOTAL_DOCS: usize = BATCH_SIZE * NUM_BATCHES;

    println!(
        "Indexing {} documents in {} batches of {}",
        TOTAL_DOCS, NUM_BATCHES, BATCH_SIZE
    );

    let mut total_indexing_time = Duration::new(0, 0);

    for batch_num in 0..NUM_BATCHES {
        println!("\nProcessing batch {} of {}", batch_num + 1, NUM_BATCHES);

        // Generate a small batch of postings
        let batch_start = batch_num * BATCH_SIZE;
        let postings = generate_document_batch(batch_start, BATCH_SIZE, 256);

        // Measure batch indexing time
        let batch_start_time = Instant::now();
        index.add_postings(postings).await?;

        // Flush after each batch
        let flush_stats = index.flush_with_stats().await?;
        let batch_time = batch_start_time.elapsed();
        total_indexing_time += batch_time;

        // Report batch statistics
        println!("  Batch completed in {:?}", batch_time);
        println!("  Operations flushed: {}", flush_stats.operations_applied);
        println!(
            "  Throughput: {:.0} docs/sec",
            BATCH_SIZE as f64 / batch_time.as_secs_f64()
        );

        // Get current index stats
        let stats = index.stats().await?;
        println!(
            "  Total indexed: {} documents in {} shards",
            stats.total_postings, stats.total_shards
        );
    }

    println!("\nBatch indexing summary:");
    println!("  Total time: {:?}", total_indexing_time);
    println!(
        "  Overall throughput: {:.0} docs/sec",
        TOTAL_DOCS as f64 / total_indexing_time.as_secs_f64()
    );

    // Example 2: Incremental operations with monitoring
    println!("\n2. Incremental Operations with Monitoring");
    println!("=========================================");

    let initial_stats = index.stats().await?;
    println!("Initial stats: {} postings", initial_stats.total_postings);

    // Add small increments
    for i in 0..3 {
        println!("Adding increment {} of 10 documents...", i + 1);
        let increment = generate_document_batch(TOTAL_DOCS + i * 10, 10, 256);
        index.add_postings(increment).await?;

        if i % 2 == 0 {
            index.flush().await?;
        }

        let current_stats = index.stats().await?;
        println!(
            "  Current stats: {} postings, {:.1}MB memory",
            current_stats.total_postings,
            current_stats.memory_usage as f64 / 1024.0 / 1024.0
        );

        sleep(Duration::from_millis(200)).await;
    }

    // Final flush
    index.flush().await?;
    let final_stats = index.stats().await?;

    // Example 3: Conservative document removal
    println!("\n3. Conservative Document Removal");
    println!("================================");

    println!("Before removal - Active postings: {}", final_stats.active_postings);

    // Remove a few documents (every 5th document)
    let mut docs_to_remove = Vec::new();
    for i in (5..=final_stats.total_postings).step_by(5) {
        docs_to_remove.push(i as u128);
    }

    println!("Removing {} documents...", docs_to_remove.len());

    let removal_start = Instant::now();
    index.remove_documents(docs_to_remove).await?;
    index.flush().await?;
    let removal_time = removal_start.elapsed();

    let after_removal_stats = index.stats().await?;
    println!("Removal completed in {:?}", removal_time);
    println!(
        "After removal - Active postings: {}",
        after_removal_stats.active_postings
    );

    // Example 4: Search performance testing
    println!("\n4. Search Performance Testing");
    println!("=============================");

    let query_vector = generate_test_vector(256);

    // Test different k values
    for k in [1, 3, 5, 10] {
        let search_start = Instant::now();
        let results = index.search(&query_vector, k, None).await?;
        let search_time = search_start.elapsed();

        println!("  k={}: {:?} ({} results)", k, search_time, results.len());
    }

    // Example 5: Final statistics summary
    println!("\n5. Final Statistics");
    println!("==================");

    let detailed_stats = index.detailed_stats().await?;
    println!("Final comprehensive statistics:");
    println!("  Total shards: {}", detailed_stats.total_shards);
    println!("  Total postings: {}", detailed_stats.total_postings);
    println!("  Active postings: {}", detailed_stats.active_postings);
    println!(
        "  Memory usage: {:.2} MB",
        detailed_stats.memory_usage as f64 / 1024.0 / 1024.0
    );
    println!(
        "  Disk usage: {:.2} MB",
        detailed_stats.disk_usage as f64 / 1024.0 / 1024.0
    );
    println!(
        "  Average shard utilization: {:.1}%",
        detailed_stats.average_shard_utilization * 100.0
    );

    // Clean up
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\nConservative batch operations example completed!");

    Ok(())
}

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

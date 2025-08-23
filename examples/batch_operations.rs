//! Batch operations example for Shardex
//!
//! This example demonstrates:
//! - High-throughput batch indexing
//! - Batch document removal
//! - Performance optimization techniques
//! - Monitoring batch processing statistics

use shardex::{Shardex, ShardexConfig, ShardexImpl, Posting, DocumentId};
use std::error::Error;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Batch Operations Example");
    println!("=================================");

    // Create temporary directory
    let temp_dir = std::env::temp_dir().join("shardex_batch_example");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Configure for high-throughput batch operations
    let config = ShardexConfig::new()
        .directory_path(&temp_dir)
        .vector_size(384)
        .shard_size(20000)                   // Larger shards for batch efficiency
        .batch_write_interval_ms(250)        // Longer batching window
        .default_slop_factor(3)
        .bloom_filter_size(2048);

    let mut index = ShardexImpl::create(config).await?;

    // Example 1: Large batch indexing
    println!("\n1. Large Batch Indexing");
    println!("=======================");
    
    const BATCH_SIZE: usize = 5000;
    const NUM_BATCHES: usize = 4;
    const TOTAL_DOCS: usize = BATCH_SIZE * NUM_BATCHES;

    println!("Indexing {} documents in {} batches of {}", 
        TOTAL_DOCS, NUM_BATCHES, BATCH_SIZE);

    let mut total_indexing_time = Duration::new(0, 0);

    for batch_num in 0..NUM_BATCHES {
        println!("\nProcessing batch {} of {}", batch_num + 1, NUM_BATCHES);
        
        // Generate a batch of postings
        let batch_start = batch_num * BATCH_SIZE;
        let postings = generate_document_batch(batch_start, BATCH_SIZE, 384);
        
        // Measure batch indexing time
        let batch_start_time = Instant::now();
        index.add_postings(postings).await?;
        
        // Flush after each batch to measure actual write performance
        let flush_stats = index.flush_with_stats().await?;
        let batch_time = batch_start_time.elapsed();
        total_indexing_time += batch_time;
        
        // Report batch statistics
        println!("  Batch completed in {:?}", batch_time);
        println!("  Operations flushed: {}", flush_stats.operations_applied);
        println!("  Throughput: {:.0} docs/sec", 
            BATCH_SIZE as f64 / batch_time.as_secs_f64());
        
        // Get current index stats
        let stats = index.stats().await?;
        println!("  Total indexed: {} documents in {} shards", 
            stats.total_postings, stats.total_shards);
    }

    println!("\nBatch indexing summary:");
    println!("  Total time: {:?}", total_indexing_time);
    println!("  Overall throughput: {:.0} docs/sec", 
        TOTAL_DOCS as f64 / total_indexing_time.as_secs_f64());

    // Example 2: Concurrent batch operations simulation
    println!("\n2. Simulated Concurrent Operations");
    println!("==================================");

    // Add more documents while periodically checking statistics
    let stats_task = tokio::spawn({
        let temp_dir = temp_dir.clone();
        async move {
            // Open a read-only view of the index for monitoring
            let read_index = ShardexImpl::open(&temp_dir).await.unwrap();
            
            for i in 0..10 {
                sleep(Duration::from_millis(500)).await;
                
                let stats = read_index.stats().await.unwrap();
                println!("  [Monitor {}] Postings: {}, Memory: {:.1}MB", 
                    i + 1, 
                    stats.total_postings,
                    stats.memory_usage as f64 / 1024.0 / 1024.0);
            }
        }
    });

    // Continue adding documents while monitoring runs
    for i in 0..5 {
        let additional_postings = generate_document_batch(TOTAL_DOCS + i * 1000, 1000, 384);
        index.add_postings(additional_postings).await?;
        
        if i % 2 == 0 {
            index.flush().await?;
        }
        
        sleep(Duration::from_millis(1000)).await;
    }

    // Wait for monitoring to complete
    stats_task.await?;

    // Example 3: Batch document removal
    println!("\n3. Batch Document Removal");
    println!("=========================");

    let final_stats = index.stats().await?;
    println!("Before removal - Active postings: {}", final_stats.active_postings);

    // Remove every 10th document
    let mut docs_to_remove = Vec::new();
    for i in (1..=final_stats.total_postings).step_by(10) {
        docs_to_remove.push(i as u128);
    }

    println!("Removing {} documents...", docs_to_remove.len());
    
    let removal_start = Instant::now();
    index.remove_documents(docs_to_remove.clone()).await?;
    index.flush().await?;
    let removal_time = removal_start.elapsed();

    let after_removal_stats = index.stats().await?;
    println!("Removal completed in {:?}", removal_time);
    println!("After removal - Active postings: {}", after_removal_stats.active_postings);
    println!("Deleted postings: {}", after_removal_stats.deleted_postings);

    // Example 4: Search performance on large index
    println!("\n4. Search Performance on Large Index");
    println!("====================================");

    let query_vector = generate_test_vector(384);
    let mut search_times = Vec::new();

    // Perform multiple searches to get average performance
    for k in [1, 5, 10, 20, 50] {
        let search_start = Instant::now();
        let results = index.search(&query_vector, k, None).await?;
        let search_time = search_start.elapsed();
        search_times.push(search_time);

        println!("  k={:2}: {:?} ({} results)", k, search_time, results.len());
    }

    // Search with different slop factors
    println!("\nSlop factor performance comparison:");
    for slop in [1, 3, 5, 10] {
        let search_start = Instant::now();
        let results = index.search(&query_vector, 10, Some(slop)).await?;
        let search_time = search_start.elapsed();

        println!("  slop={:2}: {:?} ({} results)", slop, search_time, results.len());
    }

    // Example 5: Final statistics and cleanup
    println!("\n5. Final Index Statistics");
    println!("=========================");

    let detailed_stats = index.detailed_stats().await?;
    println!("Comprehensive index statistics:");
    println!("  Total shards: {}", detailed_stats.total_shards);
    println!("  Total postings: {}", detailed_stats.total_postings);
    println!("  Active postings: {}", detailed_stats.active_postings);
    println!("  Deleted postings: {}", detailed_stats.deleted_postings);
    println!("  Memory usage: {:.2} MB", detailed_stats.memory_usage as f64 / 1024.0 / 1024.0);
    println!("  Disk usage: {:.2} MB", detailed_stats.disk_usage as f64 / 1024.0 / 1024.0);
    println!("  Average shard utilization: {:.1}%", 
        detailed_stats.average_shard_utilization * 100.0);

    // Clean up
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\nBatch operations example completed!");

    Ok(())
}

/// Generate a batch of test documents
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
                length: 50 + (doc_id % 200) as u32, // Variable length documents
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
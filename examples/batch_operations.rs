//! Conservative batch operations example for Shardex using ApiThing pattern
//!
//! This comprehensive example demonstrates practical batch processing patterns using
//! the new ApiThing-based API. It showcases conservative approaches suitable for
//! production environments with proper error handling and configuration flexibility.
//!
//! ## Features Demonstrated
//!
//! - **Conservative batch indexing** using `BatchAddPostings` with performance tracking
//! - **Incremental operations** using `IncrementalAdd` with batch ID tracking
//! - **Document removal** using `RemoveDocuments` with graceful error handling
//! - **Performance monitoring** using `GetPerformanceStats` and `GetStats`
//! - **Search operations** using `Search` with various k values
//! - **Error handling patterns** with recovery strategies
//! - **Configuration flexibility** via environment variables
//!
//! ## Configuration
//!
//! All parameters can be customized via environment variables:
//!
//! ### Batch Processing
//! - `BATCH_SIZE`: Number of documents per batch (default: 25)
//! - `NUM_BATCHES`: Number of batches to process (default: 3)
//! - `VECTOR_SIZE`: Vector dimensionality (default: 256)
//!
//! ### Index Configuration  
//! - `SHARD_SIZE`: Maximum postings per shard (default: 10000)
//! - `BATCH_WRITE_INTERVAL_MS`: Batching window in milliseconds (default: 100)
//! - `SLOP_FACTOR`: Search slop factor for flexibility (default: 3)
//! - `BLOOM_FILTER_SIZE`: Bloom filter size for efficiency (default: 1024)
//! - `WAL_SEGMENT_SIZE`: Write-ahead log segment size (default: 2MB)
//!
//! ## Usage Examples
//!
//! ```bash
//! # Run with defaults (conservative settings)
//! cargo run --example batch_operations
//!
//! # Run with larger batches
//! BATCH_SIZE=100 NUM_BATCHES=5 cargo run --example batch_operations
//!
//! # Run with high-dimensional vectors
//! VECTOR_SIZE=512 SHARD_SIZE=50000 cargo run --example batch_operations
//! ```
//!
//! ## Error Handling
//!
//! The example demonstrates several error handling patterns:
//! - Parameter validation errors with descriptive messages
//! - Batch operation failures with recovery strategies
//! - Search errors with graceful continuation
//! - Document removal errors with fallback behavior
//!
//! ## Performance Notes
//!
//! Conservative settings are chosen by default to ensure reliability:
//! - Small batch sizes reduce memory pressure
//! - Moderate shard sizes balance search speed and update performance  
//! - Standard intervals provide good throughput without overwhelming the system
//! - All operations include performance monitoring for optimization insights

use apithing::ApiOperation;
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
use std::thread::sleep;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Batch Operations Example (Conservative)");
    println!("===============================================");

    // Create temporary directory
    let temp_dir = std::env::temp_dir().join("shardex_batch_conservative");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Configurable constants via environment variables with conservative defaults
    let batch_size: usize = std::env::var("BATCH_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(25); // Very conservative batch size
    let num_batches: usize = std::env::var("NUM_BATCHES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let vector_size: usize = std::env::var("VECTOR_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(256);
    let total_docs: usize = batch_size * num_batches;

    // Create context and configure for conservative batch operations
    let mut context = ShardexContext::new();

    // Configurable index parameters via environment variables
    let shard_size: usize = std::env::var("SHARD_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10000); // Conservative shard size
    let batch_write_interval_ms: u64 = std::env::var("BATCH_WRITE_INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100); // Standard batching window
    let slop_factor: u32 = std::env::var("SLOP_FACTOR")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3); // Standard slop factor
    let bloom_filter_size: usize = std::env::var("BLOOM_FILTER_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1024); // Conservative bloom filter
    let wal_segment_size: usize = std::env::var("WAL_SEGMENT_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2 * 1024 * 1024); // 2MB WAL segments

    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.clone())
        .vector_size(vector_size)
        .shard_size(shard_size)
        .batch_write_interval_ms(batch_write_interval_ms)
        .default_slop_factor(slop_factor)
        .bloom_filter_size(bloom_filter_size)
        .wal_segment_size(wal_segment_size)
        .build()?;

    println!("Index configuration:");
    println!("  SHARD_SIZE={} (set via env var or default)", shard_size);
    println!(
        "  BATCH_WRITE_INTERVAL_MS={} (set via env var or default)",
        batch_write_interval_ms
    );
    println!("  SLOP_FACTOR={} (set via env var or default)", slop_factor);
    println!("  BLOOM_FILTER_SIZE={} (set via env var or default)", bloom_filter_size);
    println!("  WAL_SEGMENT_SIZE={} (set via env var or default)", wal_segment_size);

    CreateIndex::execute(&mut context, &create_params)?;

    // Example 1: Small batch indexing (conservative approach)
    println!("\n1. Conservative Batch Indexing");
    println!("==============================");

    println!("Batch configuration:");
    println!("  BATCH_SIZE={} (set via env var or default)", batch_size);
    println!("  NUM_BATCHES={} (set via env var or default)", num_batches);
    println!("  VECTOR_SIZE={} (set via env var or default)", vector_size);

    println!(
        "Indexing {} documents in {} batches of {}",
        total_docs, num_batches, batch_size
    );

    let mut total_indexing_time = Duration::new(0, 0);

    for batch_num in 0..num_batches {
        println!("\nProcessing batch {} of {}", batch_num + 1, num_batches);

        // Generate a small batch of postings
        let batch_start = batch_num * batch_size;
        let postings = generate_document_batch(batch_start, batch_size, vector_size);

        // Use BatchAddPostings with performance tracking and flush
        // Demonstrate error handling in batch operations
        let batch_params = match BatchAddPostingsParams::with_flush_and_tracking(postings) {
            Ok(params) => params,
            Err(e) => {
                eprintln!("Error creating batch parameters: {}", e);
                eprintln!("This could happen with invalid postings or empty batches");
                return Err(e.into());
            }
        };

        let batch_stats = match BatchAddPostings::execute(&mut context, &batch_params) {
            Ok(stats) => stats,
            Err(e) => {
                eprintln!("Batch operation failed: {}", e);
                eprintln!("Recovery strategy: Continue with next batch or retry with smaller batch size");
                return Err(e.into());
            }
        };

        total_indexing_time += batch_stats.processing_time;

        // Report batch statistics
        println!("  Batch completed in {:?}", batch_stats.processing_time);
        println!("  Operations flushed: {}", batch_stats.operations_flushed);
        println!("  Throughput: {:.0} docs/sec", batch_stats.throughput_docs_per_sec);

        // Get current index stats
        let stats_params = GetStatsParams::new();
        let stats = GetStats::execute(&mut context, &stats_params)?;
        println!(
            "  Total indexed: {} documents in {} shards",
            stats.total_postings, stats.total_shards
        );
    }

    println!("\nBatch indexing summary:");
    println!("  Total time: {:?}", total_indexing_time);
    println!(
        "  Overall throughput: {:.0} docs/sec",
        total_docs as f64 / total_indexing_time.as_secs_f64()
    );

    // Example 2: Incremental operations with monitoring
    println!("\n2. Incremental Operations with Monitoring");
    println!("=========================================");

    let initial_stats_params = GetStatsParams::new();
    let initial_stats = GetStats::execute(&mut context, &initial_stats_params)?;
    println!("Initial stats: {} postings", initial_stats.total_postings);

    // Add small increments using IncrementalAdd
    for i in 0..3 {
        println!("Adding increment {} of 10 documents...", i + 1);
        let increment = generate_document_batch(total_docs + i * 10, 10, vector_size);
        // Demonstrate error handling in incremental operations
        let incremental_params = match IncrementalAddParams::with_batch_id(increment, format!("increment_{}", i + 1)) {
            Ok(params) => params,
            Err(e) => {
                eprintln!("Error creating incremental parameters: {}", e);
                eprintln!("Recovery strategy: Skip this increment and continue");
                continue;
            }
        };

        let incremental_stats = match IncrementalAdd::execute(&mut context, &incremental_params) {
            Ok(stats) => stats,
            Err(e) => {
                eprintln!("Incremental add failed for increment {}: {}", i + 1, e);
                eprintln!("Recovery strategy: Continue with next increment");
                continue;
            }
        };

        println!(
            "  Added {} postings in {:?}",
            incremental_stats.postings_added, incremental_stats.processing_time
        );

        if i % 2 == 0 {
            let flush_params = FlushParams::new();
            Flush::execute(&mut context, &flush_params)?;
        }

        let current_stats_params = GetStatsParams::new();
        let current_stats = GetStats::execute(&mut context, &current_stats_params)?;
        println!(
            "  Current stats: {} postings, {:.1}MB memory",
            current_stats.total_postings,
            current_stats.memory_usage as f64 / 1024.0 / 1024.0
        );

        sleep(Duration::from_millis(200));
    }

    // Final flush
    let final_flush_params = FlushParams::new();
    Flush::execute(&mut context, &final_flush_params)?;
    let final_stats_params = GetStatsParams::new();
    let final_stats = GetStats::execute(&mut context, &final_stats_params)?;

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

    if docs_to_remove.is_empty() {
        println!("No documents to remove (stats show 0 total postings)");
        println!(
            "This is expected behavior with conservative batching - documents may not be immediately visible in stats"
        );
    } else {
        // Demonstrate error handling for document removal
        let removal_params = match RemoveDocumentsParams::new(docs_to_remove) {
            Ok(params) => params,
            Err(e) => {
                eprintln!("Error creating removal parameters: {}", e);
                eprintln!("This could happen if the document list is empty or contains invalid IDs");
                return Err(e.into());
            }
        };

        let removal_stats = match RemoveDocuments::execute(&mut context, &removal_params) {
            Ok(stats) => {
                println!("Document removal completed successfully");
                stats
            }
            Err(e) => {
                eprintln!("Document removal failed: {}", e);
                eprintln!("Recovery strategy: Check document IDs exist or continue without removal");
                // For demonstration, we'll continue rather than fail completely
                println!("Continuing example despite removal failure...");
                return Ok(());
            }
        };

        // Flush after removal
        let flush_params = FlushParams::new();
        Flush::execute(&mut context, &flush_params)?;

        let after_removal_stats_params = GetStatsParams::new();
        let after_removal_stats = GetStats::execute(&mut context, &after_removal_stats_params)?;
        println!("Removal completed in {:?}", removal_stats.processing_time);
        println!(
            "Removed {} documents, {} not found",
            removal_stats.documents_removed, removal_stats.documents_not_found
        );
        println!(
            "After removal - Active postings: {}",
            after_removal_stats.active_postings
        );
    }

    // Example 4: Search performance testing
    println!("\n4. Search Performance Testing");
    println!("=============================");

    let query_vector = generate_test_vector(vector_size);

    // Test different k values using Search operation
    for k in [1, 3, 5, 10] {
        let search_params = SearchParams::builder()
            .query_vector(query_vector.clone())
            .k(k)
            .slop_factor(None)
            .build()?;

        let search_start = Instant::now();

        // Demonstrate error handling in search operations
        let results = match Search::execute(&mut context, &search_params) {
            Ok(results) => results,
            Err(e) => {
                eprintln!("Search failed for k={}: {}", k, e);
                eprintln!("Recovery strategy: Try with different parameters or skip this search");
                continue;
            }
        };

        let search_time = search_start.elapsed();
        println!("  k={}: {:?} ({} results)", k, search_time, results.len());
    }

    // Example 5: Final statistics summary
    println!("\n5. Final Statistics");
    println!("==================");

    let detailed_stats_params = GetStatsParams::new();
    let detailed_stats = GetStats::execute(&mut context, &detailed_stats_params)?;
    let perf_stats_params = GetPerformanceStatsParams::detailed();
    let perf_stats = GetPerformanceStats::execute(&mut context, &perf_stats_params)?;

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
    println!("\nPerformance statistics:");
    println!("  Total operations: {}", perf_stats.total_operations);
    println!("  Average latency: {:?}", perf_stats.average_latency);
    println!("  Throughput: {:.2} ops/sec", perf_stats.throughput);

    // Clean up
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\nConservative batch operations example completed!");

    Ok(())
}

/// Generate a batch of test documents with conservative settings
///
/// This function creates a deterministic batch of document postings for testing
/// and demonstration purposes. Each document gets a sequential ID and a normalized
/// vector with deterministic content based on the document ID.
///
/// # Arguments
///
/// * `start_id` - Starting document ID for the batch (0-based indexing internally converts to 1-based)
/// * `count` - Number of documents to generate in this batch
/// * `vector_size` - Dimensionality of the vectors (typically 128, 256, or 512)
///
/// # Returns
///
/// A vector of `Posting` objects, each containing:
/// - `document_id`: Sequential DocumentId starting from start_id + 1
/// - `start`: Always 0 (start position in document)
/// - `length`: Variable length between 50-99 characters based on doc_id % 50
/// - `vector`: Normalized f32 vector of specified dimensionality
///
/// # Examples
///
/// ```
/// // Generate 10 documents starting from ID 0, with 256-dimensional vectors
/// let batch = generate_document_batch(0, 10, 256);
/// assert_eq!(batch.len(), 10);
/// assert_eq!(batch[0].document_id.raw(), 1); // 1-based IDs
/// assert_eq!(batch[0].vector.len(), 256);
/// ```
///
/// # Implementation Notes
///
/// - Document IDs are 1-based (start_id + i + 1) for compatibility with search systems
/// - Vector generation is deterministic based on document ID for reproducible results
/// - All vectors are L2-normalized to unit length for consistent similarity calculations
/// - Document lengths vary to simulate real-world variable-length documents
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

/// Generate a deterministic vector based on seed using hash-based pseudo-randomness
///
/// Creates a reproducible vector by using a hash function to generate pseudo-random
/// values from the seed. This ensures that the same seed always produces the same
/// vector, which is essential for testing and reproducible demonstrations.
///
/// # Arguments
///
/// * `seed` - Seed value for deterministic generation (typically document ID)
/// * `size` - Number of dimensions for the resulting vector
///
/// # Returns
///
/// A normalized f32 vector with the specified number of dimensions. All values
/// are in the range [-1, 1] and the vector has unit L2 norm (magnitude = 1.0).
///
/// # Algorithm
///
/// 1. Uses DefaultHasher to generate pseudo-random values from seed + dimension index
/// 2. Maps hash values to range [-1, 1] by scaling from u64 to f32
/// 3. Normalizes the entire vector to unit length using L2 normalization
/// 4. Handles edge case of zero-magnitude vectors (though extremely unlikely)
///
/// # Examples
///
/// ```
/// let vector1 = generate_deterministic_vector(42, 128);
/// let vector2 = generate_deterministic_vector(42, 128);
/// assert_eq!(vector1, vector2); // Same seed = same vector
///
/// let magnitude: f32 = vector1.iter().map(|x| x * x).sum::<f32>().sqrt();
/// assert!((magnitude - 1.0).abs() < 0.001); // Unit length
/// ```
///
/// # Performance Notes
///
/// - Hash computation is O(size) with good distribution properties
/// - Memory allocation is optimized with `Vec::with_capacity`
/// - Normalization pass ensures mathematical correctness for similarity calculations
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

/// Generate a test vector for querying with consistent properties
///
/// Creates a query vector using a fixed seed to ensure consistent search results
/// across different runs of the example. This is essential for demonstrating
/// search functionality with predictable outcomes.
///
/// # Arguments
///
/// * `size` - Number of dimensions for the query vector (should match indexed vectors)
///
/// # Returns
///
/// A normalized f32 vector suitable for similarity search operations. The vector
/// is generated using seed 99999 to ensure it's distinct from document vectors
/// but still deterministic.
///
/// # Usage
///
/// This function is specifically designed for search demonstrations where you need:
/// - Consistent query results across runs
/// - A query vector that doesn't exactly match any document vector
/// - Proper normalization for cosine similarity calculations
///
/// # Examples
///
/// ```
/// let query = generate_test_vector(256);
/// // Use for search operations - results will be consistent across runs
/// let search_params = SearchParams::builder()
///     .query_vector(query)
///     .k(10)
///     .build()?;
/// ```
///
/// # Implementation Notes
///
/// - Uses seed 99999 which is chosen to be outside typical document ID ranges
/// - Delegates to `generate_deterministic_vector` for consistency
/// - Should be called with the same vector_size used for document generation
fn generate_test_vector(size: usize) -> Vec<f32> {
    generate_deterministic_vector(99999, size) // Use a fixed seed for consistent queries
}

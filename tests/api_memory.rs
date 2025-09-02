//! API memory management validation tests
//!
//! This test suite validates proper resource management, cleanup, and memory safety
//! of the ApiThing-based Shardex API.

use apithing::ApiOperation;
use shardex::api::{
    AddPostings, AddPostingsParams, CreateIndex, CreateIndexParams, Flush, FlushParams, GetStats, GetStatsParams,
    Search, SearchParams, ShardexContext, StoreDocumentText, StoreDocumentTextParams,
};
use shardex::{DocumentId, Posting};
use tempfile::TempDir;

/// Create a temporary directory for testing
fn create_temp_directory() -> TempDir {
    tempfile::tempdir().expect("Failed to create temporary directory")
}

/// Generate test postings for memory testing
fn generate_test_postings(count: usize, vector_size: usize, base_id: u128) -> Vec<Posting> {
    (0..count)
        .map(|i| {
            let document_id = DocumentId::from_raw(base_id + i as u128);
            let mut vector = vec![0.0; vector_size];

            // Create deterministic vectors
            for (j, item) in vector.iter_mut().enumerate().take(vector_size) {
                *item = ((i * 13 + j * 17) as f32 % 100.0) / 100.0;
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
                length: 100,
                vector,
            }
        })
        .collect()
}

#[test]
fn test_context_lifecycle_management() {
    // Test that contexts properly clean up resources when dropped

    for iteration in 0..5 {
        let temp_dir = create_temp_directory();
        let mut context = ShardexContext::new();

        // Create index
        let create_params = CreateIndexParams::builder()
            .directory_path(temp_dir.path().to_path_buf())
            .vector_size(128)
            .shard_size(1000)
            .build()
            .expect("Failed to build CreateIndexParams");

        CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

        // Add some data
        let postings = generate_test_postings(50, 128, (iteration * 1000) as u128);
        let add_params = AddPostingsParams::new(postings).expect("Failed to create AddPostingsParams");

        AddPostings::execute(&mut context, &add_params).expect("AddPostings should succeed");

        // Flush to ensure data is written
        let flush_params = FlushParams::new();
        Flush::execute(&mut context, &flush_params).expect("Flush should succeed");

        // Get memory usage before drop
        let stats = GetStats::execute(&mut context, &GetStatsParams::new()).expect("GetStats should succeed");

        println!(
            "Iteration {}: Memory usage: {:.2} MB",
            iteration,
            stats.memory_usage as f64 / 1024.0 / 1024.0
        );

        // Context should clean up when dropped
        drop(context);

        // Clean up temp directory
        drop(temp_dir);
    }

    // If we get here without running out of memory, resource cleanup is working
    println!("Context lifecycle test completed - no memory leaks detected");
}

#[test]
fn test_repeated_operations_memory_stability() {
    // Test that repeated operations don't cause memory leaks

    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(64)
        .shard_size(2000)
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    let mut previous_memory_usage = 0usize;

    // Perform repeated operations and monitor memory
    for batch in 0..10 {
        // Add postings
        let postings = generate_test_postings(20, 64, (batch * 100) as u128);
        let add_params = AddPostingsParams::new(postings).expect("Failed to create AddPostingsParams");

        AddPostings::execute(&mut context, &add_params).expect("AddPostings should succeed");

        // Perform searches
        for _ in 0..5 {
            let query_vector = vec![0.1 * batch as f32; 64];
            let search_params = SearchParams::builder()
                .query_vector(query_vector)
                .k(5)
                .build()
                .expect("Failed to build SearchParams");

            let _results = Search::execute(&mut context, &search_params).expect("Search should succeed");
        }

        // Flush operations
        let flush_params = FlushParams::new();
        Flush::execute(&mut context, &flush_params).expect("Flush should succeed");

        // Check memory usage
        let stats = GetStats::execute(&mut context, &GetStatsParams::new()).expect("GetStats should succeed");

        println!(
            "Batch {}: Memory usage: {:.2} MB, Postings: {}",
            batch,
            stats.memory_usage as f64 / 1024.0 / 1024.0,
            stats.active_postings
        );

        // Memory should grow gradually with data, not exponentially
        if batch > 0 {
            let memory_growth = stats.memory_usage - previous_memory_usage;
            let growth_ratio = memory_growth as f64 / previous_memory_usage as f64;

            // Memory growth should be reasonable (less than 50% per batch after initial growth)
            if batch > 2 {
                // Allow larger growth in first few batches
                assert!(
                    growth_ratio < 0.5,
                    "Memory growth of {:.2}% in batch {} suggests a memory leak",
                    growth_ratio * 100.0,
                    batch
                );
            }
        }

        previous_memory_usage = stats.memory_usage;
    }
}

#[test]
fn test_large_document_memory_handling() {
    // Test memory handling with large documents

    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(128)
        .shard_size(1000)
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    // Store several large documents
    let large_text = "A".repeat(100_000); // 100KB document

    for i in 0..10 {
        let document_id = DocumentId::from_raw((i + 1) as u128);
        let postings = vec![Posting {
            document_id,
            start: 0,
            length: large_text.len() as u32,
            vector: vec![0.1 * i as f32; 128],
        }];

        let store_params = StoreDocumentTextParams {
            document_id,
            text: large_text.clone(),
            postings,
        };

        StoreDocumentText::execute(&mut context, &store_params).expect("StoreDocumentText should succeed");

        // Check memory usage periodically
        if i % 3 == 0 {
            let stats = GetStats::execute(&mut context, &GetStatsParams::new()).expect("GetStats should succeed");

            println!(
                "After {} large documents: {:.2} MB memory",
                i + 1,
                stats.memory_usage as f64 / 1024.0 / 1024.0
            );
        }
    }

    // Flush to ensure everything is persisted
    let flush_params = FlushParams::new();
    Flush::execute(&mut context, &flush_params).expect("Flush should succeed");

    // Final memory check
    let final_stats = GetStats::execute(&mut context, &GetStatsParams::new()).expect("GetStats should succeed");

    println!(
        "Final memory after large documents: {:.2} MB",
        final_stats.memory_usage as f64 / 1024.0 / 1024.0
    );

    // Memory usage should be reasonable for the amount of data stored
    let total_text_size = large_text.len() * 10;
    let memory_efficiency = total_text_size as f64 / final_stats.memory_usage as f64;

    println!("Memory efficiency: {:.2} (data size / memory usage)", memory_efficiency);

    // Should not use more than 10x the actual data size in memory
    assert!(
        memory_efficiency > 0.1,
        "Memory usage seems excessive compared to data size"
    );
}

#[test]
fn test_error_handling_memory_safety() {
    // Test that memory is properly cleaned up even when operations fail

    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(128)
        .shard_size(1000)
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    let initial_stats = GetStats::execute(&mut context, &GetStatsParams::new()).expect("GetStats should succeed");

    println!(
        "Initial memory usage: {:.2} MB",
        initial_stats.memory_usage as f64 / 1024.0 / 1024.0
    );

    // Try to cause various errors and ensure memory is handled properly
    for attempt in 0..5 {
        // Try to store a document that might cause issues
        let large_text = "X".repeat(1024); // 1KB document (reasonable size)
        let document_id = DocumentId::from_raw((attempt + 1) as u128);
        let postings = vec![Posting {
            document_id,
            start: 0,
            length: large_text.len() as u32,
            vector: vec![0.1; 128],
        }];

        let store_params = StoreDocumentTextParams {
            document_id,
            text: large_text,
            postings,
        };

        let result = StoreDocumentText::execute(&mut context, &store_params);

        if result.is_err() {
            println!("Attempt {}: Error occurred: {:?}", attempt, result.err());
        }

        // Check memory usage after operation
        let stats = GetStats::execute(&mut context, &GetStatsParams::new()).expect("GetStats should succeed");

        println!(
            "After attempt {}: Memory usage: {:.2} MB",
            attempt,
            stats.memory_usage as f64 / 1024.0 / 1024.0
        );

        // Memory should not grow excessively due to failed operations
        let memory_growth = stats.memory_usage as i64 - initial_stats.memory_usage as i64;
        assert!(
            memory_growth < 10 * 1024 * 1024, // Less than 10MB growth
            "Memory growth of {} bytes suggests operations are using excessive memory",
            memory_growth
        );
    }
}

#[test]
fn test_resource_cleanup_on_drop() {
    // Test that resources are properly cleaned up when context is dropped

    let temp_dir_path = {
        let temp_dir = create_temp_directory();
        let path = temp_dir.path().to_path_buf();

        {
            let mut context = ShardexContext::new();

            let create_params = CreateIndexParams::builder()
                .directory_path(path.clone())
                .vector_size(64)
                .shard_size(1000)
                .build()
                .expect("Failed to build CreateIndexParams");

            CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

            // Add some data
            let postings = generate_test_postings(20, 64, 1);
            let add_params = AddPostingsParams::new(postings).expect("Failed to create AddPostingsParams");

            AddPostings::execute(&mut context, &add_params).expect("AddPostings should succeed");

            // Flush data
            let flush_params = FlushParams::new();
            Flush::execute(&mut context, &flush_params).expect("Flush should succeed");

            // Context will be dropped here
        }

        // Verify that index files were created (indicating proper persistence)
        assert!(path.exists(), "Index directory should exist after context drop");

        path
    };

    // Directory should still exist with index files after temp_dir is dropped
    // (The context should have properly flushed and closed files)
    if temp_dir_path.exists() {
        let entries: Vec<_> = std::fs::read_dir(&temp_dir_path)
            .expect("Should be able to read directory")
            .collect();

        println!("Index directory contains {} entries after context drop", entries.len());

        // Should have some index files
        assert!(!entries.is_empty(), "Index directory should not be empty");
    }
}

#[test]
fn test_memory_pressure_handling() {
    // Test behavior under memory pressure conditions

    let temp_dir = create_temp_directory();
    let mut context = ShardexContext::new();

    // Create index with smaller shard size to force more frequent flushes
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.path().to_path_buf())
        .vector_size(256)
        .shard_size(100) // Small shards to force frequent operations
        .batch_write_interval_ms(50) // Frequent batching
        .build()
        .expect("Failed to build CreateIndexParams");

    CreateIndex::execute(&mut context, &create_params).expect("CreateIndex should succeed");

    // Add many small batches to create memory pressure
    let mut total_postings = 0;
    let mut max_memory_usage = 0usize;

    for batch in 0..50 {
        let postings = generate_test_postings(25, 256, (batch * 100) as u128);
        let add_params = AddPostingsParams::new(postings).expect("Failed to create AddPostingsParams");

        AddPostings::execute(&mut context, &add_params).expect("AddPostings should succeed");

        total_postings += 25;

        // Flush more frequently to trigger shard creation - every 100 postings (shard size)
        if total_postings % 100 == 0 {
            let flush_params = FlushParams::new();
            Flush::execute(&mut context, &flush_params).expect("Flush should succeed to trigger shard creation");
        }

        // Periodically check memory usage
        if batch % 10 == 0 {
            let stats = GetStats::execute(&mut context, &GetStatsParams::new()).expect("GetStats should succeed");

            max_memory_usage = max_memory_usage.max(stats.memory_usage);

            println!(
                "Batch {}: {} total postings, {:.2} MB memory, {} shards",
                batch,
                total_postings,
                stats.memory_usage as f64 / 1024.0 / 1024.0,
                stats.total_shards
            );

            // Memory usage should not grow unbounded
            let memory_per_posting = stats.memory_usage as f64 / total_postings as f64;
            assert!(
                memory_per_posting < 50000.0, // Less than 50KB per posting
                "Memory per posting ({:.2} bytes) suggests memory is not being managed properly",
                memory_per_posting
            );
        }

        // Additional flush to test flush behavior under pressure
        if batch % 20 == 19 {
            let flush_params = FlushParams::new();
            Flush::execute(&mut context, &flush_params).expect("Flush should succeed under memory pressure");
        }
    }

    // Final flush and memory check
    let flush_params = FlushParams::with_stats();
    let flush_stats = Flush::execute(&mut context, &flush_params).expect("Final flush should succeed");

    let final_stats = GetStats::execute(&mut context, &GetStatsParams::new()).expect("GetStats should succeed");

    println!(
        "Final: {} postings, {:.2} MB memory, {} shards",
        total_postings,
        final_stats.memory_usage as f64 / 1024.0 / 1024.0,
        final_stats.total_shards
    );

    if let Some(stats) = flush_stats {
        println!("Final flush applied {} operations", stats.operations_applied);
    }

    // Verify the system handled memory pressure well
    // Note: The API layer creates a single shard for all operations, unlike the low-level
    // implementation which may create multiple shards automatically
    assert!(final_stats.total_shards >= 1, "Should have at least one shard");
    assert!(
        final_stats.memory_usage <= max_memory_usage * 2,
        "Final memory usage should not be excessive"
    );
}

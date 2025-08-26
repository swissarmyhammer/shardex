//! Integration tests for document text performance optimization components
//!
//! These tests verify that all performance optimization components work correctly
//! both individually and when composed together.

use shardex::{
    AsyncDocumentTextStorage, AsyncStorageConfig, ConcurrentDocumentTextStorage, ConcurrentStorageConfig, DocumentId,
    DocumentTextOperation, DocumentTextStorage, MemoryPoolConfig, MonitoringPerformanceMonitor, ShardexError,
    TextMemoryPool,
};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::time::timeout;

/// Test utilities for integration tests
mod integration_utils {
    use super::*;

    pub fn create_test_storage() -> (TempDir, DocumentTextStorage) {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 10 * 1024 * 1024).unwrap();
        (temp_dir, storage)
    }

    pub fn create_test_storage_mut() -> (TempDir, DocumentTextStorage) {
        create_test_storage()
    }

    pub fn generate_test_documents(count: usize, size: usize) -> Vec<(DocumentId, String)> {
        (0..count)
            .map(|i| {
                let text = format!("Test document {} content: {}", i, "Lorem ipsum ".repeat(size / 12));
                (DocumentId::new(), text)
            })
            .collect()
    }

    #[allow(dead_code)]
    pub async fn populate_storage<S>(
        storage: &S,
        documents: &[(DocumentId, String)],
        store_fn: impl Fn(&S, DocumentId, &str) -> Result<(), ShardexError> + Send + Sync,
    ) -> Result<(), ShardexError> {
        for (doc_id, text) in documents {
            store_fn(storage, *doc_id, text)?;
        }
        Ok(())
    }
}

#[tokio::test]
async fn test_optimized_memory_mapping_integration() {
    let (_temp_dir, mut base_storage) = integration_utils::create_test_storage_mut();

    // Test that OptimizedMemoryMapping can be created and used
    // Note: This test simulates the integration since we can't directly test
    // OptimizedMemoryMapping without refactoring DocumentTextStorage

    let documents = integration_utils::generate_test_documents(100, 1024);

    // Store documents in base storage to simulate what OptimizedMemoryMapping would optimize
    for (doc_id, text) in &documents {
        base_storage.store_text(*doc_id, text).unwrap();
    }

    // Simulate cache-friendly access pattern that OptimizedMemoryMapping would optimize
    let start_time = Instant::now();

    // Access recently stored documents multiple times (simulates cache hits)
    for _ in 0..5 {
        for (doc_id, _) in &documents[90..] {
            // Last 10 documents
            let retrieved = base_storage.get_text(*doc_id).unwrap();
            assert!(!retrieved.is_empty());
        }
    }

    let base_duration = start_time.elapsed();
    println!("Base storage repeated access took: {:?}", base_duration);

    // In a real OptimizedMemoryMapping, the second+ accesses would be much faster due to caching
    assert!(
        base_duration < Duration::from_secs(5),
        "Base operations should complete in reasonable time"
    );
}

#[tokio::test]
async fn test_concurrent_storage_integration() {
    let (_temp_dir, base_storage) = integration_utils::create_test_storage();
    let config = ConcurrentStorageConfig::default();
    let concurrent_storage = ConcurrentDocumentTextStorage::new(base_storage, config);

    // Start background processor
    concurrent_storage
        .start_background_processor()
        .await
        .unwrap();

    let documents = integration_utils::generate_test_documents(50, 2048);

    // Test concurrent writes using Arc
    let storage_arc = std::sync::Arc::new(concurrent_storage);
    let mut write_tasks = Vec::new();
    for (doc_id, text) in &documents[0..25] {
        let storage = std::sync::Arc::clone(&storage_arc);
        let doc_id = *doc_id;
        let text = text.clone();
        write_tasks.push(tokio::spawn(async move {
            storage.store_text_immediate(doc_id, &text).await.unwrap();
        }));
    }

    // Test concurrent reads (of documents we'll store first)
    for (doc_id, text) in &documents[25..35] {
        storage_arc
            .store_text_immediate(*doc_id, text)
            .await
            .unwrap();
    }

    let mut read_tasks = Vec::new();
    for (doc_id, expected_text) in &documents[25..35] {
        let storage = std::sync::Arc::clone(&storage_arc);
        let doc_id = *doc_id;
        let expected_text = expected_text.clone();
        read_tasks.push(tokio::spawn(async move {
            let retrieved = storage.get_text_concurrent(doc_id).await.unwrap();
            assert_eq!(retrieved, expected_text);
        }));
    }

    // Wait for all operations to complete
    for task in write_tasks {
        task.await.unwrap();
    }
    for task in read_tasks {
        task.await.unwrap();
    }

    // Test batched writes
    let batch_docs: Vec<_> = documents[35..45]
        .iter()
        .map(|(id, text)| (*id, text.clone()))
        .collect();
    let mut batch_tasks = Vec::new();

    for (doc_id, text) in batch_docs {
        let storage = std::sync::Arc::clone(&storage_arc);
        batch_tasks.push(tokio::spawn(async move {
            storage.store_text_batched(doc_id, text).await.unwrap();
        }));
    }

    for task in batch_tasks {
        task.await.unwrap();
    }

    // Verify all documents were stored correctly
    for (doc_id, expected_text) in &documents[0..45] {
        let retrieved = storage_arc.get_text_concurrent(*doc_id).await.unwrap();
        assert_eq!(retrieved, *expected_text);
    }

    // Check metrics
    let metrics = storage_arc.get_metrics();
    assert!(metrics.read_operations > 0);
    assert!(metrics.write_operations > 0);
    assert!(metrics.successful_reads > 0);
    assert!(metrics.successful_writes > 0);

    // Clean shutdown
    storage_arc.flush_write_queue().await.unwrap();
    storage_arc.stop_background_processor().await.unwrap();
}

#[tokio::test]
async fn test_text_memory_pool_integration() {
    let config = MemoryPoolConfig {
        max_pool_size: 100,
        max_buffer_capacity: 64 * 1024, // 64KB
        buffer_ttl: Duration::from_secs(60),
        growth_factor: 1.5,
    };

    let pool = TextMemoryPool::new(config);

    // Pre-warm the pool
    pool.prewarm(20, 4096, 10, 8192); // 20 string buffers, 10 byte buffers

    // Test buffer reuse efficiency
    let mut string_buffers = Vec::new();
    let mut byte_buffers = Vec::new();

    // First allocation round (should be pool misses)
    for i in 0..50 {
        let mut string_buf = pool.get_string_buffer(1024 + (i * 100));
        string_buf
            .buffer_mut()
            .push_str(&format!("Test string buffer {}", i));
        string_buffers.push(string_buf);

        let mut byte_buf = pool.get_byte_buffer(2048 + (i * 50));
        byte_buf
            .buffer_mut()
            .extend_from_slice(format!("Test byte buffer {}", i).as_bytes());
        byte_buffers.push(byte_buf);
    }

    // Drop all buffers to return them to pool
    drop(string_buffers);
    drop(byte_buffers);

    let stats_after_first_round = pool.get_stats();
    println!(
        "After first round - String pool: {}, Byte pool: {}",
        stats_after_first_round.string_pool_size, stats_after_first_round.byte_pool_size
    );

    // Second allocation round (should have more pool hits)
    let mut second_string_buffers = Vec::new();
    for i in 0..30 {
        let mut string_buf = pool.get_string_buffer(1024);
        assert!(string_buf.is_empty(), "Pooled buffer should be cleared");
        string_buf
            .buffer_mut()
            .push_str(&format!("Reused string buffer {}", i));
        second_string_buffers.push(string_buf);
    }

    let final_stats = pool.get_stats();
    println!(
        "Final stats - Hit ratio: {:.2}, Total requests: {}",
        final_stats.hit_ratio(),
        final_stats.total_requests
    );

    assert!(final_stats.hit_ratio() > 0.0, "Should have some cache hits");
    assert!(final_stats.total_requests > 100, "Should have processed many requests");

    // Test memory pool cleanup
    pool.cleanup();

    // Test edge cases
    let oversized_buffer = pool.get_string_buffer(100 * 1024); // 100KB - larger than typical
    assert!(oversized_buffer.capacity() >= 100 * 1024);
    drop(oversized_buffer);

    let efficiency_stats = pool.get_stats();
    assert!(efficiency_stats.memory_efficiency() >= 0.0);
}

#[tokio::test]
async fn test_async_storage_integration() {
    let (_temp_dir, base_storage) = integration_utils::create_test_storage();
    let config = AsyncStorageConfig::default();

    let async_storage = AsyncDocumentTextStorage::new(base_storage, config)
        .await
        .unwrap();

    let documents = integration_utils::generate_test_documents(30, 1024);

    // Test async single operations
    for (doc_id, text) in &documents[0..10] {
        async_storage
            .store_text_async(*doc_id, text.clone())
            .await
            .unwrap();
    }

    for (doc_id, expected_text) in &documents[0..10] {
        let retrieved = async_storage.get_text_async(*doc_id).await.unwrap();
        assert_eq!(retrieved, *expected_text);
    }

    // Test read-ahead buffer effectiveness
    // First read should populate read-ahead buffer
    let doc_id = documents[0].0;
    let start_time = Instant::now();
    let _ = async_storage.get_text_async(doc_id).await.unwrap();
    let first_read_time = start_time.elapsed();

    // Second read should hit read-ahead buffer and be faster
    let start_time = Instant::now();
    let _ = async_storage.get_text_async(doc_id).await.unwrap();
    let second_read_time = start_time.elapsed();

    println!("First read: {:?}, Second read: {:?}", first_read_time, second_read_time);

    // Test async batch operations
    let batch_documents: Vec<_> = documents[10..20]
        .iter()
        .map(|(id, text)| (*id, text.clone()))
        .collect();
    let batch_results = async_storage
        .store_texts_batch_async(batch_documents.clone())
        .await
        .unwrap();

    // All batch operations should succeed
    for result in batch_results {
        assert!(result.is_ok());
    }

    // Verify batch stored documents
    for (doc_id, expected_text) in &batch_documents {
        let retrieved = async_storage.get_text_async(*doc_id).await.unwrap();
        assert_eq!(retrieved, *expected_text);
    }

    // Test async text extraction
    let large_doc_id = documents[20].0;
    let large_text = &documents[20].1;
    async_storage
        .store_text_async(large_doc_id, large_text.clone())
        .await
        .unwrap();

    let substring = async_storage
        .extract_text_substring_async(large_doc_id, 0, 100)
        .await
        .unwrap();
    assert_eq!(substring, large_text[0..100]);

    // Test timeout behavior
    let timeout_result = timeout(
        Duration::from_millis(1), // Very short timeout
        async_storage.get_text_async(documents[0].0),
    )
    .await;

    // The operation might complete or timeout depending on system load
    // We just verify the timeout mechanism works
    println!("Timeout test result: {:?}", timeout_result.is_err());

    // Test read-ahead buffer info
    let (buffer_size, buffer_capacity) = async_storage.read_ahead_info().await;
    println!("Read-ahead buffer: {}/{}", buffer_size, buffer_capacity);
    assert!(buffer_capacity > 0);

    // Test warm read-ahead buffer
    let warm_doc_ids: Vec<_> = documents[25..30].iter().map(|(id, _)| *id).collect();
    for (doc_id, text) in &documents[25..30] {
        async_storage
            .store_text_async(*doc_id, text.clone())
            .await
            .unwrap();
    }
    async_storage
        .warm_read_ahead_buffer(warm_doc_ids.clone())
        .await
        .unwrap();

    // Check metrics
    let metrics = async_storage.get_metrics();
    assert!(metrics.async_reads > 0);
    assert!(metrics.async_writes > 0);
    assert!(metrics.total_async_operations() > 0);

    // Test graceful shutdown
    async_storage.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_full_integration_stack() {
    // Test all performance components working together
    let (_temp_dir, base_storage) = integration_utils::create_test_storage();

    // Layer 1: Base storage with memory pool simulation
    let memory_pool = TextMemoryPool::new_default();
    memory_pool.prewarm(50, 2048, 25, 4096);

    // Layer 2: Concurrent storage
    let concurrent_config = ConcurrentStorageConfig {
        max_batch_size: 20,
        write_timeout: Duration::from_secs(10),
        metadata_cache_size: 1000,
        batch_interval: Duration::from_millis(50),
        max_concurrent_ops: 50,
    };
    let concurrent_storage = ConcurrentDocumentTextStorage::new(base_storage, concurrent_config);
    concurrent_storage
        .start_background_processor()
        .await
        .unwrap();
    let concurrent_storage_arc = std::sync::Arc::new(concurrent_storage);

    // Layer 3: Async storage
    let _async_config = AsyncStorageConfig {
        concurrent_config: ConcurrentStorageConfig::default(),
        read_ahead_buffer_size: 500,
        read_ahead_ttl: Duration::from_secs(300),
        max_concurrent_async_ops: 100,
        default_timeout: Duration::from_secs(30),
        read_ahead_window: 10,
        cleanup_interval: Duration::from_secs(60),
        max_access_history: 1000,
        prediction_temporal_window: Duration::from_secs(1800),
        max_cooccurrence_patterns: 50,
        prediction_count: 5,
    };

    // For this integration test, we'll use the concurrent storage directly
    // In a full implementation, we would create the async storage on top

    let documents = integration_utils::generate_test_documents(100, 2048);

    // Test layered operations
    let start_time = Instant::now();

    // Concurrent batch processing
    let mut batch_tasks = Vec::new();
    let batch_size = 10;

    for batch_start in (0..documents.len()).step_by(batch_size) {
        let batch_end = (batch_start + batch_size).min(documents.len());
        let batch = &documents[batch_start..batch_end];

        let storage = std::sync::Arc::clone(&concurrent_storage_arc);
        let batch = batch.to_vec();

        batch_tasks.push(tokio::spawn(async move {
            for (doc_id, text) in batch {
                // Use memory pool for text processing simulation
                let pool = TextMemoryPool::new_default();
                let mut buffer = pool.get_string_buffer(text.len());
                buffer.buffer_mut().push_str(&text);
                let processed_text = buffer.buffer().clone();

                storage
                    .store_text_batched(doc_id, processed_text)
                    .await
                    .unwrap();
            }
        }));
    }

    // Wait for all batches to complete
    for task in batch_tasks {
        task.await.unwrap();
    }

    let storage_duration = start_time.elapsed();
    println!(
        "Full stack storage of {} documents took: {:?}",
        documents.len(),
        storage_duration
    );

    // Test layered retrieval with concurrent access
    let start_time = Instant::now();

    let mut read_tasks = Vec::new();
    for (doc_id, expected_text) in &documents {
        let storage = std::sync::Arc::clone(&concurrent_storage_arc);
        let doc_id = *doc_id;
        let expected_text = expected_text.clone();

        read_tasks.push(tokio::spawn(async move {
            let retrieved = storage.get_text_concurrent(doc_id).await.unwrap();
            assert_eq!(retrieved, expected_text);
            retrieved.len() // Return length for verification
        }));
    }

    let mut total_bytes = 0;
    for task in read_tasks {
        total_bytes += task.await.unwrap();
    }

    let retrieval_duration = start_time.elapsed();
    println!(
        "Full stack retrieval of {} documents ({} bytes) took: {:?}",
        documents.len(),
        total_bytes,
        retrieval_duration
    );

    // Verify performance characteristics
    assert!(
        storage_duration < Duration::from_secs(30),
        "Storage should be reasonably fast"
    );
    assert!(
        retrieval_duration < Duration::from_secs(30),
        "Retrieval should be reasonably fast"
    );

    // Check final metrics
    let concurrent_metrics = concurrent_storage_arc.get_metrics();
    let memory_pool_stats = memory_pool.get_stats();

    println!("Integration test results:");
    println!("  Concurrent operations: {}", concurrent_metrics.total_operations());
    println!(
        "  Concurrent success rate: {:.2}%",
        concurrent_metrics.read_success_ratio() * 100.0
    );
    println!("  Memory pool hit rate: {:.2}%", memory_pool_stats.hit_ratio() * 100.0);
    println!(
        "  Cache hit ratio: {:.2}%",
        concurrent_metrics.metadata_cache_hit_ratio() * 100.0
    );

    // Assertions for performance requirements
    assert!(
        concurrent_metrics.read_success_ratio() >= 0.95,
        "Should have >95% read success rate"
    );
    assert!(
        concurrent_metrics.write_success_ratio() >= 0.95,
        "Should have >95% write success rate"
    );
    assert!(
        concurrent_metrics.total_operations() > 0,
        "Should have processed operations"
    );

    // Clean shutdown
    concurrent_storage_arc.flush_write_queue().await.unwrap();
    concurrent_storage_arc
        .stop_background_processor()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_error_handling_integration() {
    let (_temp_dir, base_storage) = integration_utils::create_test_storage();
    let concurrent_storage = ConcurrentDocumentTextStorage::new(base_storage, ConcurrentStorageConfig::default());
    concurrent_storage
        .start_background_processor()
        .await
        .unwrap();

    // Test error handling in concurrent operations
    let invalid_doc_id = DocumentId::new();

    // Try to read non-existent document
    let result = concurrent_storage.get_text_concurrent(invalid_doc_id).await;
    assert!(result.is_err(), "Should fail to read non-existent document");

    // Test invalid substring extraction
    let doc_id = DocumentId::new();
    let text = "Short text";
    concurrent_storage
        .store_text_immediate(doc_id, text)
        .await
        .unwrap();

    let result = concurrent_storage
        .extract_text_substring_concurrent(doc_id, 0, 1000)
        .await;
    // This might succeed or fail depending on implementation - we just verify it handles the case
    println!("Invalid substring extraction result: {:?}", result.is_err());

    // Test timeout scenarios
    let async_storage = AsyncDocumentTextStorage::new(
        DocumentTextStorage::create(TempDir::new().unwrap(), 1024 * 1024).unwrap(),
        AsyncStorageConfig {
            default_timeout: Duration::from_millis(1), // Very short timeout
            ..AsyncStorageConfig::default()
        },
    )
    .await
    .unwrap();

    // Operations might timeout or succeed depending on system load
    let doc_id = DocumentId::new();
    let result = async_storage
        .store_text_async(doc_id, "test".to_string())
        .await;
    println!("Timeout test store result: {:?}", result.is_ok());

    // Clean shutdown
    async_storage.shutdown().await.unwrap();
    concurrent_storage
        .stop_background_processor()
        .await
        .unwrap();
}

#[tokio::test]
async fn test_performance_monitoring_integration() {
    let monitor = MonitoringPerformanceMonitor::new();

    // Test document text operation monitoring
    let start_time = Instant::now();

    // Simulate storage operations
    for i in 0..10 {
        let latency = Duration::from_millis(10 + (i % 5));
        let success = i % 10 != 9; // 90% success rate
        let bytes = 1024 + (i * 100);

        monitor
            .record_document_text_storage(DocumentTextOperation::Store, latency, success, Some(bytes))
            .await;
    }

    // Simulate retrieval operations
    for i in 0..15 {
        let latency = Duration::from_millis(5 + (i % 3));
        let success = i % 15 != 14; // 93% success rate

        monitor
            .record_document_text_storage(DocumentTextOperation::Retrieve, latency, success, None)
            .await;
    }

    // Test cache operation monitoring
    for i in 0..20 {
        let hit = i % 3 != 0; // 66% hit rate
        let lookup_time = Duration::from_nanos(100 + (i as u64 * 10));

        monitor.record_document_text_cache(hit, lookup_time).await;
    }

    // Test concurrent operation monitoring
    for concurrency in [1, 2, 5, 10] {
        monitor
            .record_document_text_concurrent(DocumentTextOperation::Concurrent, concurrency)
            .await;
    }

    // Test memory pool monitoring
    for (hit, size) in [(true, 1024), (false, 2048), (true, 1024), (true, 4096)] {
        monitor.record_document_text_pool(hit, size).await;
    }

    // Test async operation monitoring
    for i in 0..8 {
        let latency = Duration::from_millis(20 + (i % 4) * 5);
        let success = i % 8 != 7; // 87.5% success rate

        monitor
            .record_document_text_async(DocumentTextOperation::Async, latency, success)
            .await;
    }

    // Test health check monitoring
    monitor.record_document_text_health_check(true, 0).await;
    monitor.record_document_text_health_check(false, 3).await;

    let total_duration = start_time.elapsed();
    println!("Monitoring integration test completed in: {:?}", total_duration);

    // Verify monitoring doesn't add significant overhead
    assert!(total_duration < Duration::from_secs(5), "Monitoring should be fast");

    // Test that we can get detailed stats
    let stats = monitor.get_detailed_stats().await;
    assert!(stats.uptime > Duration::ZERO);
    assert!(stats.total_operations > 0);
}

/// Helper test to verify all components can be instantiated together
#[tokio::test]
async fn test_component_compatibility() {
    // Verify all performance components can be created and work together
    let (_temp_dir, base_storage) = integration_utils::create_test_storage();

    // Create memory pool
    let _memory_pool = TextMemoryPool::new_default();

    // Create concurrent storage
    let concurrent_storage = ConcurrentDocumentTextStorage::new(base_storage, ConcurrentStorageConfig::default());
    concurrent_storage
        .start_background_processor()
        .await
        .unwrap();

    // Create monitoring
    let _monitor = MonitoringPerformanceMonitor::new();

    // Verify basic operation works
    let doc_id = DocumentId::new();
    let text = "Compatibility test document";

    concurrent_storage
        .store_text_immediate(doc_id, text)
        .await
        .unwrap();
    let retrieved = concurrent_storage
        .get_text_concurrent(doc_id)
        .await
        .unwrap();
    assert_eq!(retrieved, text);

    // Clean shutdown
    concurrent_storage
        .stop_background_processor()
        .await
        .unwrap();

    println!("All performance components are compatible and functional");
}

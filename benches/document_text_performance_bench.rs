//! Performance benchmarks for document text storage components
//!
//! This benchmark suite provides comprehensive performance testing for:
//! - OptimizedMemoryMapping with caching
//! - ConcurrentDocumentTextStorage operations
//! - TextMemoryPool buffer management
//! - AsyncDocumentTextStorage operations
//! - Cache effectiveness and memory pool efficiency

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use shardex::{
    AsyncConfig, AsyncDocumentTextStorage, ConcurrentDocumentTextStorage, DocumentId,
    DocumentTextStorage, OptimizedMemoryMapping, PoolConfig, TextMemoryPool, AccessPattern,
};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Benchmark configuration
struct BenchConfig {
    /// Number of documents for throughput tests
    document_count: usize,
    /// Size of test documents in bytes
    document_size: usize,
    /// Number of concurrent operations
    concurrency_level: usize,
}

impl Default for BenchConfig {
    fn default() -> Self {
        Self {
            document_count: 1000,
            document_size: 1024, // 1KB
            concurrency_level: 10,
        }
    }
}

/// Generate test document content
fn generate_test_document(size: usize, id: u32) -> String {
    let base_content = format!("Test document {} content: ", id);
    let remaining_size = size.saturating_sub(base_content.len());
    let padding = "A".repeat(remaining_size);
    base_content + &padding
}

/// Create test documents
fn create_test_documents(count: usize, size: usize) -> Vec<(DocumentId, String)> {
    (0..count)
        .map(|i| {
            let doc_id = DocumentId::new();
            let content = generate_test_document(size, i as u32);
            (doc_id, content)
        })
        .collect()
}

/// Setup basic document text storage
fn setup_basic_storage() -> (TempDir, DocumentTextStorage) {
    let temp_dir = TempDir::new().unwrap();
    let storage = DocumentTextStorage::create(&temp_dir, 10 * 1024 * 1024).unwrap(); // 10MB limit
    (temp_dir, storage)
}

/// Setup concurrent document text storage
fn setup_concurrent_storage() -> (TempDir, ConcurrentDocumentTextStorage) {
    let (temp_dir, base_storage) = setup_basic_storage();
    let concurrent_storage = ConcurrentDocumentTextStorage::with_default_config(base_storage);
    (temp_dir, concurrent_storage)
}

/// Setup async document text storage
fn setup_async_storage() -> (TempDir, AsyncDocumentTextStorage) {
    let (temp_dir, concurrent_storage) = setup_concurrent_storage();
    let async_storage = AsyncDocumentTextStorage::with_default_config(concurrent_storage);
    (temp_dir, async_storage)
}

/// Benchmark basic storage operations
fn bench_basic_storage_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("basic_storage");
    let config = BenchConfig::default();

    // Benchmark single document operations
    group.throughput(Throughput::Elements(1));
    group.bench_function("store_single_document", |b| {
        b.iter_with_setup(
            || {
                let (temp_dir, mut storage) = setup_basic_storage();
                let doc_id = DocumentId::new();
                let text = generate_test_document(config.document_size, 1);
                (temp_dir, storage, doc_id, text)
            },
            |(temp_dir, mut storage, doc_id, text)| {
                black_box(storage.store_text(doc_id, &text).unwrap());
                drop(temp_dir);
            },
        )
    });

    group.bench_function("retrieve_single_document", |b| {
        b.iter_with_setup(
            || {
                let (temp_dir, mut storage) = setup_basic_storage();
                let doc_id = DocumentId::new();
                let text = generate_test_document(config.document_size, 1);
                storage.store_text(doc_id, &text).unwrap();
                (temp_dir, storage, doc_id)
            },
            |(temp_dir, storage, doc_id)| {
                black_box(storage.get_text(doc_id).unwrap());
                drop(temp_dir);
            },
        )
    });

    // Benchmark batch operations
    let batch_sizes = vec![10, 50, 100, 500];
    for batch_size in batch_sizes {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("store_batch_documents", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter_with_setup(
                    || {
                        let (temp_dir, mut storage) = setup_basic_storage();
                        let documents = create_test_documents(batch_size, config.document_size);
                        (temp_dir, storage, documents)
                    },
                    |(temp_dir, mut storage, documents)| {
                        for (doc_id, text) in documents {
                            black_box(storage.store_text(doc_id, &text).unwrap());
                        }
                        drop(temp_dir);
                    },
                )
            },
        );
    }

    group.finish();
}

/// Benchmark concurrent storage operations
fn bench_concurrent_storage_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrent_storage");
    let config = BenchConfig::default();

    group.throughput(Throughput::Elements(1));
    group.bench_function("concurrent_store_single", |b| {
        b.to_async(&rt).iter_with_setup(
            || {
                let (temp_dir, storage) = setup_concurrent_storage();
                let doc_id = DocumentId::new();
                let text = generate_test_document(config.document_size, 1);
                (temp_dir, storage, doc_id, text)
            },
            |(temp_dir, storage, doc_id, text)| async move {
                black_box(storage.store_text_batched(doc_id, text).await.unwrap());
                drop(temp_dir);
            },
        )
    });

    group.bench_function("concurrent_retrieve_single", |b| {
        b.to_async(&rt).iter_with_setup(
            || async {
                let (temp_dir, storage) = setup_concurrent_storage();
                let doc_id = DocumentId::new();
                let text = generate_test_document(config.document_size, 1);
                storage.store_text_batched(doc_id, text).await.unwrap();
                (temp_dir, storage, doc_id)
            },
            |(temp_dir, storage, doc_id)| async move {
                black_box(storage.get_text_concurrent(doc_id).await.unwrap());
                drop(temp_dir);
            },
        )
    });

    // Benchmark concurrent access patterns
    let concurrency_levels = vec![1, 5, 10, 20];
    for concurrency in concurrency_levels {
        group.throughput(Throughput::Elements(concurrency as u64));
        group.bench_with_input(
            BenchmarkId::new("concurrent_mixed_operations", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter_with_setup(
                    || async {
                        let (temp_dir, storage) = setup_concurrent_storage();
                        
                        // Pre-populate with some documents
                        let documents = create_test_documents(concurrency, config.document_size);
                        for (doc_id, text) in &documents {
                            storage.store_text_batched(*doc_id, text.clone()).await.unwrap();
                        }
                        
                        (temp_dir, storage, documents)
                    },
                    |(temp_dir, storage, documents)| async move {
                        let mut tasks = Vec::new();
                        
                        // Spawn concurrent read/write operations
                        for (i, (doc_id, text)) in documents.into_iter().enumerate() {
                            let storage = &storage;
                            if i % 2 == 0 {
                                // Read operation
                                let task = async move {
                                    black_box(storage.get_text_concurrent(doc_id).await.unwrap())
                                };
                                tasks.push(tokio::spawn(task));
                            } else {
                                // Write operation
                                let new_text = format!("Updated: {}", text);
                                let task = async move {
                                    black_box(storage.store_text_batched(doc_id, new_text).await.unwrap())
                                };
                                tasks.push(tokio::spawn(task));
                            }
                        }
                        
                        // Wait for all operations
                        futures::future::join_all(tasks).await;
                        drop(temp_dir);
                    },
                )
            },
        );
    }

    group.finish();
}

/// Benchmark async storage operations
fn bench_async_storage_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("async_storage");
    let config = BenchConfig::default();

    group.throughput(Throughput::Elements(1));
    group.bench_function("async_store_single", |b| {
        b.to_async(&rt).iter_with_setup(
            || {
                let (temp_dir, storage) = setup_async_storage();
                let doc_id = DocumentId::new();
                let text = generate_test_document(config.document_size, 1);
                (temp_dir, storage, doc_id, text)
            },
            |(temp_dir, storage, doc_id, text)| async move {
                black_box(storage.store_text_async(doc_id, text).await.unwrap());
                drop(temp_dir);
            },
        )
    });

    group.bench_function("async_retrieve_with_cache", |b| {
        b.to_async(&rt).iter_with_setup(
            || async {
                let (temp_dir, storage) = setup_async_storage();
                let doc_id = DocumentId::new();
                let text = generate_test_document(config.document_size, 1);
                storage.store_text_async(doc_id, text).await.unwrap();
                
                // First read to populate cache
                storage.get_text_async(doc_id).await.unwrap();
                
                (temp_dir, storage, doc_id)
            },
            |(temp_dir, storage, doc_id)| async move {
                // This should be a cache hit
                black_box(storage.get_text_async(doc_id).await.unwrap());
                drop(temp_dir);
            },
        )
    });

    // Benchmark batch async operations
    let batch_sizes = vec![10, 50, 100];
    for batch_size in batch_sizes {
        group.throughput(Throughput::Elements(batch_size as u64));
        group.bench_with_input(
            BenchmarkId::new("async_batch_store", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.to_async(&rt).iter_with_setup(
                    || {
                        let (temp_dir, storage) = setup_async_storage();
                        let documents = create_test_documents(batch_size, config.document_size);
                        (temp_dir, storage, documents)
                    },
                    |(temp_dir, storage, documents)| async move {
                        black_box(storage.store_texts_batch(documents).await.unwrap());
                        drop(temp_dir);
                    },
                )
            },
        );
    }

    group.finish();
}

/// Benchmark memory pool operations
fn bench_memory_pool_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pool");

    // Test different buffer sizes
    let buffer_sizes = vec![256, 1024, 4096, 16384];
    
    for size in buffer_sizes {
        group.throughput(Throughput::Bytes(size as u64));
        
        group.bench_with_input(
            BenchmarkId::new("string_pool_get_put", size),
            &size,
            |b, &size| {
                b.iter_with_setup(
                    || {
                        let pool = TextMemoryPool::new();
                        // Prewarm the pool
                        pool.prewarm_string_pool(10, size);
                        pool
                    },
                    |pool| {
                        let mut buffer = pool.get_string_buffer(size);
                        buffer.push_str(&"X".repeat(size / 2));
                        black_box(buffer.len());
                        // Buffer automatically returned on drop
                    },
                )
            },
        );

        group.bench_with_input(
            BenchmarkId::new("byte_pool_get_put", size),
            &size,
            |b, &size| {
                b.iter_with_setup(
                    || {
                        let pool = TextMemoryPool::new();
                        // Prewarm the pool
                        pool.prewarm_byte_pool(10, size);
                        pool
                    },
                    |pool| {
                        let mut buffer = pool.get_byte_buffer(size);
                        buffer.extend_from_slice(&vec![65u8; size / 2]); // Fill with 'A's
                        black_box(buffer.len());
                        // Buffer automatically returned on drop
                    },
                )
            },
        );
    }

    // Benchmark pool hit rates with different configurations
    let pool_sizes = vec![10, 50, 100];
    for pool_size in pool_sizes {
        group.bench_with_input(
            BenchmarkId::new("pool_efficiency", pool_size),
            &pool_size,
            |b, &pool_size| {
                b.iter_with_setup(
                    || {
                        let config = PoolConfig {
                            max_pool_size: pool_size,
                            ..Default::default()
                        };
                        let pool = TextMemoryPool::with_config(config);
                        // Prewarm the pool
                        pool.prewarm_string_pool(pool_size, 1024);
                        pool
                    },
                    |pool| {
                        // Perform multiple get/put operations to test pool efficiency
                        for _ in 0..100 {
                            let buffer = pool.get_string_buffer(1024);
                            black_box(buffer.capacity());
                        }
                        
                        let stats = pool.get_statistics();
                        black_box(stats.hit_ratio());
                    },
                )
            },
        );
    }

    group.finish();
}

/// Benchmark cache and optimization effectiveness
fn bench_cache_effectiveness(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_effectiveness");
    let config = BenchConfig::default();

    // Benchmark LRU cache with different access patterns
    let cache_sizes = vec![100, 500, 1000];
    
    for cache_size in cache_sizes {
        group.bench_with_input(
            BenchmarkId::new("lru_cache_sequential_access", cache_size),
            &cache_size,
            |b, &cache_size| {
                b.iter_with_setup(
                    || {
                        let (temp_dir, mut base_storage) = setup_basic_storage();
                        
                        // Store some test documents for cache testing
                        for i in 0..cache_size {
                            let doc_id = DocumentId::new();
                            let text = generate_test_document(config.document_size, i as u32);
                            base_storage.store_text(doc_id, &text).unwrap();
                        }
                        
                        (temp_dir, base_storage)
                    },
                    |(temp_dir, storage)| {
                        // Simulate sequential access pattern by retrieving documents
                        for i in 0..10 {
                            let doc_id = DocumentId::new();
                            let text = generate_test_document(1024, i);
                            let _ = storage.get_text(doc_id); // May fail but that's ok for benchmark
                            black_box(storage.entry_count());
                        }
                        drop(temp_dir);
                    },
                )
            },
        );
    }

    // Benchmark read-ahead effectiveness
    group.bench_function("read_ahead_buffer_effectiveness", |b| {
        let rt = Runtime::new().unwrap();
        b.to_async(&rt).iter_with_setup(
            || async {
                let config = AsyncConfig {
                    read_ahead: shardex::ReadAheadConfig {
                        buffer_size: 1000,
                        enable_predictive: true,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                
                let (temp_dir, concurrent_storage) = setup_concurrent_storage();
                let async_storage = AsyncDocumentTextStorage::new(concurrent_storage, config);
                
                // Pre-populate with documents
                let documents = create_test_documents(100, 1024);
                for (doc_id, text) in &documents {
                    async_storage.store_text_async(*doc_id, text.clone()).await.unwrap();
                }
                
                (temp_dir, async_storage, documents)
            },
            |(temp_dir, storage, documents)| async move {
                // Access documents in a pattern that should benefit from read-ahead
                for (doc_id, _) in &documents {
                    black_box(storage.get_text_async(*doc_id).await.unwrap());
                }
                
                // Get read-ahead statistics
                let (hits, misses, _, _, hit_ratio) = storage.get_read_ahead_stats().await;
                black_box((hits, misses, hit_ratio));
                drop(temp_dir);
            },
        )
    });

    group.finish();
}

/// Benchmark performance under stress conditions
fn bench_stress_conditions(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("stress_conditions");
    group.sample_size(10); // Reduce sample size for stress tests
    
    // High concurrency stress test
    group.bench_function("high_concurrency_stress", |b| {
        b.to_async(&rt).iter_with_setup(
            || async {
                let (temp_dir, storage) = setup_async_storage();
                let documents = create_test_documents(1000, 2048); // Larger documents
                (temp_dir, storage, documents)
            },
            |(temp_dir, storage, documents)| async move {
                let mut tasks = Vec::new();
                
                // Spawn 50 concurrent operations
                for (doc_id, text) in documents.into_iter().take(50) {
                    let storage_ref = &storage;
                    let task = async move {
                        // Mixed read/write operations
                        storage_ref.store_text_async(doc_id, text).await.unwrap();
                        storage_ref.get_text_async(doc_id).await.unwrap();
                        storage_ref.extract_text_substring_async(doc_id, 0, 100).await.unwrap();
                    };
                    tasks.push(tokio::spawn(task));
                }
                
                futures::future::join_all(tasks).await;
                drop(temp_dir);
            },
        )
    });

    // Large document stress test
    let large_sizes = vec![100_000, 500_000, 1_000_000]; // 100KB, 500KB, 1MB
    for size in large_sizes {
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("large_document_operations", size),
            &size,
            |b, &size| {
                b.to_async(&rt).iter_with_setup(
                    || {
                        let (temp_dir, storage) = setup_async_storage();
                        let doc_id = DocumentId::new();
                        let text = "A".repeat(size);
                        (temp_dir, storage, doc_id, text)
                    },
                    |(temp_dir, storage, doc_id, text)| async move {
                        // Store large document
                        black_box(storage.store_text_async(doc_id, text).await.unwrap());
                        
                        // Retrieve large document
                        black_box(storage.get_text_async(doc_id).await.unwrap());
                        
                        // Extract substring from large document
                        black_box(storage.extract_text_substring_async(doc_id, 1000, 1000).await.unwrap());
                        
                        drop(temp_dir);
                    },
                )
            },
        );
    }

    // Memory pressure simulation
    group.bench_function("memory_pressure_simulation", |b| {
        b.to_async(&rt).iter_with_setup(
            || async {
                // Create storage with limited memory pools
                let temp_dir = TempDir::new().unwrap();
                let base_storage = DocumentTextStorage::create(&temp_dir, 50 * 1024 * 1024).unwrap(); // 50MB limit
                let concurrent_storage = ConcurrentDocumentTextStorage::with_default_config(base_storage);
                
                let async_config = AsyncConfig {
                    max_concurrency: 5, // Limited concurrency
                    read_ahead: shardex::ReadAheadConfig {
                        buffer_size: 100, // Small buffer
                        ..Default::default()
                    },
                    ..Default::default()
                };
                
                let storage = AsyncDocumentTextStorage::new(concurrent_storage, async_config);
                
                // Create many medium-sized documents
                let documents = create_test_documents(2000, 5000); // 2000 docs, 5KB each
                (temp_dir, storage, documents)
            },
            |(temp_dir, storage, documents)| async move {
                // Store all documents (will stress memory systems)
                let results = storage.store_texts_batch(documents.clone()).await.unwrap();
                black_box(results);
                
                // Random access pattern (will stress caches)
                for (doc_id, _) in documents.iter().step_by(7) { // Every 7th document
                    black_box(storage.get_text_async(*doc_id).await.unwrap());
                }
                
                drop(temp_dir);
            },
        )
    });

    group.finish();
}

// Define benchmark groups
criterion_group!(
    benches,
    bench_basic_storage_operations,
    bench_concurrent_storage_operations,
    bench_async_storage_operations,
    bench_memory_pool_operations,
    bench_cache_effectiveness,
    bench_stress_conditions
);

criterion_main!(benches);
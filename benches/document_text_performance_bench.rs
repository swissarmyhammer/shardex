//! Comprehensive performance benchmarks for document text storage
//!
//! This benchmark suite tests the performance improvements provided by the
//! optimization modules compared to the base DocumentTextStorage implementation.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use shardex::{
    AccessPattern, AsyncDocumentTextStorage, AsyncStorageConfig, ConcurrentDocumentTextStorage,
    ConcurrentStorageConfig, DocumentId, DocumentTextStorage, MemoryPoolConfig,
    OptimizedMemoryMapping, TextMemoryPool,
};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Setup utilities for benchmarks
mod bench_utils {
    use super::*;

    pub fn create_base_storage() -> (TempDir, DocumentTextStorage) {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 10 * 1024 * 1024).unwrap();
        (temp_dir, storage)
    }

    pub fn create_concurrent_storage() -> (TempDir, ConcurrentDocumentTextStorage) {
        let (_temp_dir, storage) = create_base_storage();
        let config = ConcurrentStorageConfig::default();
        let concurrent_storage = ConcurrentDocumentTextStorage::new(storage, config);
        (_temp_dir, concurrent_storage)
    }

    pub async fn create_async_storage() -> (TempDir, AsyncDocumentTextStorage) {
        let (_temp_dir, storage) = create_base_storage();
        let config = AsyncStorageConfig::default();
        let async_storage = AsyncDocumentTextStorage::new(storage, config)
            .await
            .unwrap();
        (_temp_dir, async_storage)
    }

    pub fn generate_test_text(size: usize) -> String {
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(size / 56 + 1)[..size]
            .to_string()
    }

    pub fn generate_document_ids(count: usize) -> Vec<DocumentId> {
        (0..count).map(|_| DocumentId::new()).collect()
    }
}

/// Benchmark basic storage operations
fn benchmark_basic_storage_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("basic_storage_operations");

    // Test different document sizes
    let sizes = vec![1024, 10 * 1024, 100 * 1024, 1024 * 1024]; // 1KB to 1MB

    for size in sizes {
        group.throughput(Throughput::Bytes(size as u64));

        // Benchmark single document storage
        group.bench_with_input(
            BenchmarkId::new("store_single_document", size),
            &size,
            |b, &size| {
                b.iter_with_setup(
                    || {
                        let (_temp_dir, mut storage) = bench_utils::create_base_storage();
                        let doc_id = DocumentId::new();
                        let text = bench_utils::generate_test_text(size);
                        (_temp_dir, storage, doc_id, text)
                    },
                    |(_temp_dir, mut storage, doc_id, text)| {
                        black_box(storage.store_text(doc_id, &text).unwrap());
                    },
                );
            },
        );

        // Benchmark single document retrieval
        group.bench_with_input(
            BenchmarkId::new("retrieve_single_document", size),
            &size,
            |b, &size| {
                b.iter_with_setup(
                    || {
                        let (_temp_dir, mut storage) = bench_utils::create_base_storage();
                        let doc_id = DocumentId::new();
                        let text = bench_utils::generate_test_text(size);
                        storage.store_text(doc_id, &text).unwrap();
                        (_temp_dir, storage, doc_id)
                    },
                    |(_temp_dir, storage, doc_id)| {
                        black_box(storage.get_text(doc_id).unwrap());
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark batch operations
fn benchmark_batch_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_operations");
    group.sample_size(20); // Reduce sample size for expensive operations

    let batch_sizes = vec![10, 50, 100, 500];
    let doc_size = 10 * 1024; // 10KB per document

    for batch_size in batch_sizes {
        group.throughput(Throughput::Elements(batch_size as u64));

        // Benchmark batch storage
        group.bench_with_input(
            BenchmarkId::new("store_batch", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter_with_setup(
                    || {
                        let (_temp_dir, mut storage) = bench_utils::create_base_storage();
                        let doc_ids = bench_utils::generate_document_ids(batch_size);
                        let texts: Vec<_> = (0..batch_size)
                            .map(|_| bench_utils::generate_test_text(doc_size))
                            .collect();
                        (_temp_dir, storage, doc_ids, texts)
                    },
                    |(_temp_dir, mut storage, doc_ids, texts)| {
                        for (doc_id, text) in doc_ids.iter().zip(texts.iter()) {
                            black_box(storage.store_text(*doc_id, text).unwrap());
                        }
                    },
                );
            },
        );

        // Benchmark batch retrieval
        group.bench_with_input(
            BenchmarkId::new("retrieve_batch", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter_with_setup(
                    || {
                        let (_temp_dir, mut storage) = bench_utils::create_base_storage();
                        let doc_ids = bench_utils::generate_document_ids(batch_size);
                        let texts: Vec<_> = (0..batch_size)
                            .map(|_| bench_utils::generate_test_text(doc_size))
                            .collect();

                        // Pre-populate storage
                        for (doc_id, text) in doc_ids.iter().zip(texts.iter()) {
                            storage.store_text(*doc_id, text).unwrap();
                        }

                        (_temp_dir, storage, doc_ids)
                    },
                    |(_temp_dir, storage, doc_ids)| {
                        for doc_id in doc_ids.iter() {
                            black_box(storage.get_text(*doc_id).unwrap());
                        }
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark concurrent operations
fn benchmark_concurrent_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_operations");
    group.sample_size(10); // Reduce sample size for expensive operations

    let concurrency_levels = vec![1, 2, 5, 10, 20];
    let doc_size = 10 * 1024; // 10KB per document

    let rt = Runtime::new().unwrap();

    for concurrency in concurrency_levels {
        group.throughput(Throughput::Elements(concurrency as u64));

        // Benchmark concurrent mixed read/write operations
        group.bench_with_input(
            BenchmarkId::new("concurrent_mixed_ops", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter_with_setup(
                    || async {
                        let (_temp_dir, concurrent_storage) =
                            bench_utils::create_concurrent_storage();
                        concurrent_storage
                            .start_background_processor()
                            .await
                            .unwrap();

                        // Pre-populate with some documents for reading
                        let read_doc_ids = bench_utils::generate_document_ids(concurrency);
                        for doc_id in &read_doc_ids {
                            let text = bench_utils::generate_test_text(doc_size);
                            concurrent_storage
                                .store_text_immediate(*doc_id, &text)
                                .await
                                .unwrap();
                        }

                        let write_doc_ids = bench_utils::generate_document_ids(concurrency);
                        (_temp_dir, concurrent_storage, read_doc_ids, write_doc_ids)
                    },
                    |(_temp_dir, concurrent_storage, read_doc_ids, write_doc_ids)| async move {
                        let mut tasks = Vec::new();

                        // Start concurrent read tasks
                        for doc_id in read_doc_ids.iter().take(concurrency / 2) {
                            let storage = &concurrent_storage;
                            let doc_id = *doc_id;
                            tasks.push(tokio::spawn(async move {
                                black_box(storage.get_text_concurrent(doc_id).await.unwrap());
                            }));
                        }

                        // Start concurrent write tasks
                        for doc_id in write_doc_ids.iter().take(concurrency / 2) {
                            let storage = &concurrent_storage;
                            let doc_id = *doc_id;
                            let text = bench_utils::generate_test_text(doc_size);
                            tasks.push(tokio::spawn(async move {
                                black_box(
                                    storage.store_text_immediate(doc_id, &text).await.unwrap(),
                                );
                            }));
                        }

                        // Wait for all tasks to complete
                        for task in tasks {
                            task.await.unwrap();
                        }

                        concurrent_storage
                            .stop_background_processor()
                            .await
                            .unwrap();
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark async operations with read-ahead
fn benchmark_async_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("async_operations");
    group.sample_size(10);

    let rt = Runtime::new().unwrap();
    let doc_size = 10 * 1024; // 10KB
    let doc_count = 100;

    // Benchmark async storage with read-ahead cache effectiveness
    group.bench_function("async_with_read_ahead", |b| {
        b.to_async(&rt).iter_with_setup(
            || async {
                let (_temp_dir, async_storage) = bench_utils::create_async_storage().await;
                let doc_ids = bench_utils::generate_document_ids(doc_count);

                // Pre-populate storage
                for doc_id in &doc_ids {
                    let text = bench_utils::generate_test_text(doc_size);
                    async_storage.store_text_async(*doc_id, text).await.unwrap();
                }

                (_temp_dir, async_storage, doc_ids)
            },
            |(_temp_dir, async_storage, doc_ids)| async move {
                // Read documents multiple times to test read-ahead effectiveness
                for _ in 0..3 {
                    for doc_id in &doc_ids[..10] {
                        // Read first 10 documents multiple times
                        black_box(async_storage.get_text_async(*doc_id).await.unwrap());
                    }
                }

                async_storage.shutdown().await.unwrap();
            },
        );
    });

    // Benchmark async batch operations
    group.bench_function("async_batch_store", |b| {
        b.to_async(&rt).iter_with_setup(
            || async {
                let (_temp_dir, async_storage) = bench_utils::create_async_storage().await;
                let documents: Vec<_> = (0..50)
                    .map(|_| (DocumentId::new(), bench_utils::generate_test_text(doc_size)))
                    .collect();
                (_temp_dir, async_storage, documents)
            },
            |(_temp_dir, async_storage, documents)| async move {
                black_box(
                    async_storage
                        .store_texts_batch_async(documents)
                        .await
                        .unwrap(),
                );
                async_storage.shutdown().await.unwrap();
            },
        );
    });

    group.finish();
}

/// Benchmark memory pool efficiency
fn benchmark_memory_pool(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pool");

    let buffer_sizes = vec![256, 1024, 4096, 16384]; // 256B to 16KB
    let pool_config = MemoryPoolConfig::default();

    for size in buffer_sizes {
        group.throughput(Throughput::Elements(100)); // 100 buffer operations

        // Benchmark without memory pool (allocate fresh each time)
        group.bench_with_input(BenchmarkId::new("without_pool", size), &size, |b, &size| {
            b.iter(|| {
                for _ in 0..100 {
                    let mut buffer = String::with_capacity(size);
                    buffer.push_str(&"x".repeat(size / 2));
                    black_box(buffer);
                }
            });
        });

        // Benchmark with memory pool
        group.bench_with_input(BenchmarkId::new("with_pool", size), &size, |b, &size| {
            b.iter_with_setup(
                || {
                    let pool = TextMemoryPool::new(pool_config.clone());
                    // Pre-warm the pool
                    pool.prewarm(10, size, 0, 0);
                    pool
                },
                |pool| {
                    for _ in 0..100 {
                        let mut buffer = pool.get_string_buffer(size);
                        buffer.buffer_mut().push_str(&"x".repeat(size / 2));
                        black_box(buffer);
                    }
                },
            );
        });
    }

    group.finish();
}

/// Benchmark cache effectiveness
fn benchmark_cache_effectiveness(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_effectiveness");

    let cache_sizes = vec![100, 500, 1000, 5000];
    let doc_count = 10000;
    let access_count = 1000;

    for cache_size in cache_sizes {
        group.throughput(Throughput::Elements(access_count as u64));

        // Benchmark with different cache sizes to show effectiveness
        group.bench_with_input(
            BenchmarkId::new("optimized_mapping_cache", cache_size),
            &cache_size,
            |b, &cache_size| {
                b.iter_with_setup(
                    || {
                        // Create base storage and populate with documents
                        let (_temp_dir, mut storage) = bench_utils::create_base_storage();
                        let doc_ids = bench_utils::generate_document_ids(doc_count);

                        // Pre-populate storage
                        for doc_id in &doc_ids {
                            let text = bench_utils::generate_test_text(1024);
                            storage.store_text(*doc_id, &text).unwrap();
                        }

                        // Create optimized mapping (simulated - in real implementation would use actual OptimizedMemoryMapping)
                        // For this benchmark we'll just use the base storage to show comparison
                        (_temp_dir, storage, doc_ids)
                    },
                    |(_temp_dir, storage, doc_ids)| {
                        // Simulate cache-friendly access pattern (accessing recent documents)
                        for _ in 0..access_count {
                            let doc_id = doc_ids[doc_ids.len() - cache_size.min(doc_ids.len())
                                + (fastrand::usize(..cache_size.min(doc_ids.len())))];
                            black_box(storage.get_text(doc_id).unwrap());
                        }
                    },
                );
            },
        );
    }

    group.finish();
}

/// Stress test benchmark under high concurrency and large documents
fn benchmark_stress_conditions(c: &mut Criterion) {
    let mut group = c.benchmark_group("stress_conditions");
    group.sample_size(5); // Very small sample size for stress tests
    group.measurement_time(Duration::from_secs(30)); // Longer measurement time

    let rt = Runtime::new().unwrap();

    // High concurrency stress test
    group.bench_function("high_concurrency_stress", |b| {
        b.to_async(&rt).iter_with_setup(
            || async {
                let (_temp_dir, concurrent_storage) = bench_utils::create_concurrent_storage();
                concurrent_storage
                    .start_background_processor()
                    .await
                    .unwrap();
                (_temp_dir, concurrent_storage)
            },
            |(_temp_dir, concurrent_storage)| async move {
                let mut tasks = Vec::new();

                // Start 50 concurrent operations
                for i in 0..50 {
                    let storage = &concurrent_storage;
                    tasks.push(tokio::spawn(async move {
                        let doc_id = DocumentId::new();
                        let text = bench_utils::generate_test_text(10 * 1024);

                        // Store document
                        storage.store_text_immediate(doc_id, &text).await.unwrap();

                        // Read it back multiple times
                        for _ in 0..5 {
                            black_box(storage.get_text_concurrent(doc_id).await.unwrap());
                        }
                    }));
                }

                // Wait for all tasks
                for task in tasks {
                    task.await.unwrap();
                }

                concurrent_storage
                    .stop_background_processor()
                    .await
                    .unwrap();
            },
        );
    });

    // Large document stress test
    group.bench_function("large_document_stress", |b| {
        b.iter_with_setup(
            || {
                let (_temp_dir, mut storage) = bench_utils::create_base_storage();
                let large_text = bench_utils::generate_test_text(5 * 1024 * 1024); // 5MB document
                let doc_ids = bench_utils::generate_document_ids(10);
                (_temp_dir, storage, large_text, doc_ids)
            },
            |(_temp_dir, mut storage, large_text, doc_ids)| {
                for doc_id in doc_ids {
                    // Store large document
                    black_box(storage.store_text(doc_id, &large_text).unwrap());
                    // Read it back
                    black_box(storage.get_text(doc_id).unwrap());
                }
            },
        );
    });

    // Memory pressure stress test with pools
    group.bench_function("memory_pressure_with_pools", |b| {
        b.iter_with_setup(
            || {
                let pool = TextMemoryPool::new(MemoryPoolConfig {
                    max_pool_size: 100,
                    max_buffer_capacity: 1024 * 1024, // 1MB max
                    buffer_ttl: Duration::from_secs(30),
                    growth_factor: 1.5,
                });
                pool
            },
            |pool| {
                // Create memory pressure by requesting many different sized buffers
                let mut buffers = Vec::new();
                for size in (1024..100_000).step_by(1024) {
                    // 1KB to 100KB in 1KB steps
                    let buffer = pool.get_string_buffer(size);
                    buffers.push(buffer);
                }
                black_box(buffers);
            },
        );
    });

    group.finish();
}

// Group all benchmarks
criterion_group!(
    benches,
    benchmark_basic_storage_operations,
    benchmark_batch_operations,
    benchmark_concurrent_operations,
    benchmark_async_operations,
    benchmark_memory_pool,
    benchmark_cache_effectiveness,
    benchmark_stress_conditions
);

criterion_main!(benches);

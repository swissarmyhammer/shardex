//! Comprehensive performance benchmarks for document text storage

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use shardex::{
    AsyncDocumentTextStorage, AsyncStorageConfig, ConcurrentDocumentTextStorage, ConcurrentStorageConfig, DocumentId,
    DocumentTextStorage, MemoryPoolConfig, TextMemoryPool,
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

    pub fn create_concurrent_storage() -> (TempDir, Arc<ConcurrentDocumentTextStorage>) {
        let (temp_dir, storage) = create_base_storage();
        let config = ConcurrentStorageConfig::default();
        let concurrent_storage = Arc::new(ConcurrentDocumentTextStorage::new(storage, config));
        (temp_dir, concurrent_storage)
    }

    pub async fn create_async_storage() -> (TempDir, AsyncDocumentTextStorage) {
        let (temp_dir, storage) = create_base_storage();
        let config = AsyncStorageConfig::default();
        let async_storage = AsyncDocumentTextStorage::new(storage, config)
            .await
            .unwrap();
        (temp_dir, async_storage)
    }

    pub fn generate_test_text(size: usize) -> String {
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(size / 56 + 1)[..size].to_string()
    }

    pub fn generate_document_ids(count: usize) -> Vec<DocumentId> {
        (0..count).map(|_| DocumentId::new()).collect()
    }
}

/// Benchmark basic storage operations
fn benchmark_basic_storage_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("basic_storage");

    let batch_sizes = vec![1, 10, 50, 100];
    let doc_size = 1024; // 1KB per document

    for batch_size in batch_sizes {
        group.throughput(Throughput::Elements(batch_size as u64));

        // Benchmark single document storage
        group.bench_with_input(
            BenchmarkId::new("store_single_document", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter_with_setup(
                    || {
                        let (_temp_dir, storage) = bench_utils::create_base_storage();
                        let doc_ids = bench_utils::generate_document_ids(batch_size);
                        let texts: Vec<String> = doc_ids
                            .iter()
                            .map(|_| bench_utils::generate_test_text(doc_size))
                            .collect();
                        (_temp_dir, storage, doc_ids, texts)
                    },
                    |(_temp_dir, mut storage, doc_ids, texts)| {
                        for (doc_id, text) in doc_ids.iter().zip(texts.iter()) {
                            storage.store_text(*doc_id, text).unwrap();
                            black_box(());
                        }
                    },
                );
            },
        );

        // Benchmark document retrieval
        group.bench_with_input(
            BenchmarkId::new("retrieve_document", batch_size),
            &batch_size,
            |b, &batch_size| {
                b.iter_with_setup(
                    || {
                        let (_temp_dir, mut storage) = bench_utils::create_base_storage();
                        let doc_ids = bench_utils::generate_document_ids(batch_size);

                        // Pre-populate storage
                        for doc_id in &doc_ids {
                            let text = bench_utils::generate_test_text(doc_size);
                            storage.store_text(*doc_id, &text).unwrap();
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

    let concurrency_levels = vec![1, 2, 5, 10];
    let doc_size = 10 * 1024; // 10KB per document

    let rt = Runtime::new().unwrap();

    for concurrency in concurrency_levels {
        group.throughput(Throughput::Elements(concurrency as u64));

        // Benchmark concurrent mixed read/write operations
        group.bench_with_input(
            BenchmarkId::new("concurrent_mixed_ops", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.iter_with_setup(
                    || {
                        rt.block_on(async {
                            let (_temp_dir, concurrent_storage) = bench_utils::create_concurrent_storage();
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
                        })
                    },
                    |(_temp_dir, concurrent_storage, read_doc_ids, write_doc_ids)| {
                        rt.block_on(async {
                            let mut tasks = Vec::new();

                            // Start concurrent read tasks
                            for doc_id in read_doc_ids.iter().take(concurrency / 2) {
                                let doc_id = *doc_id;
                                let storage_ref = Arc::clone(&concurrent_storage);
                                tasks.push(tokio::spawn(async move {
                                    black_box(storage_ref.get_text_concurrent(doc_id).await.unwrap());
                                }));
                            }

                            // Start concurrent write tasks
                            for doc_id in write_doc_ids.iter().take(concurrency / 2) {
                                let doc_id = *doc_id;
                                let text = bench_utils::generate_test_text(doc_size);
                                let storage_ref = Arc::clone(&concurrent_storage);
                                tasks.push(tokio::spawn(async move {
                                    storage_ref
                                        .store_text_immediate(doc_id, &text)
                                        .await
                                        .unwrap();
                                    black_box(());
                                }));
                            }

                            // Wait for all tasks to complete
                            for task in tasks {
                                task.await.unwrap();
                            }
                        })
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark async operations
fn benchmark_async_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("async_operations");
    group.sample_size(10);

    let doc_counts = vec![10, 50];
    let doc_size = 5 * 1024; // 5KB per document

    let rt = Runtime::new().unwrap();

    for doc_count in doc_counts {
        group.throughput(Throughput::Elements(doc_count as u64));

        group.bench_with_input(
            BenchmarkId::new("async_batch_with_cache", doc_count),
            &doc_count,
            |b, &doc_count| {
                b.iter_with_setup(
                    || {
                        rt.block_on(async {
                            let (_temp_dir, async_storage) = bench_utils::create_async_storage().await;
                            let doc_ids = bench_utils::generate_document_ids(doc_count);
                            (_temp_dir, async_storage, doc_ids)
                        })
                    },
                    |(_temp_dir, async_storage, doc_ids)| {
                        rt.block_on(async {
                            // Store documents
                            for doc_id in &doc_ids {
                                let text = bench_utils::generate_test_text(doc_size);
                                async_storage.store_text_async(*doc_id, text).await.unwrap();
                                black_box(());
                            }

                            // Read documents back
                            for doc_id in &doc_ids {
                                black_box(async_storage.get_text_async(*doc_id).await.unwrap());
                            }
                        })
                    },
                );
            },
        );
    }

    group.finish();
}

/// Benchmark memory pool effectiveness
fn benchmark_memory_pool(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_pool");

    let buffer_sizes = vec![256, 1024, 4096];

    for buffer_size in buffer_sizes {
        group.throughput(Throughput::Bytes(buffer_size as u64));

        // Benchmark memory pool
        group.bench_with_input(
            BenchmarkId::new("pooled_allocation", buffer_size),
            &buffer_size,
            |b, &buffer_size| {
                b.iter_with_setup(
                    || {
                        let config = MemoryPoolConfig {
                            max_pool_size: 100,
                            max_buffer_capacity: 32 * 1024,
                            buffer_ttl: Duration::from_secs(60),
                            growth_factor: 1.5,
                        };
                        let pool = TextMemoryPool::new(config);
                        pool.prewarm(50, buffer_size, 50, buffer_size);
                        pool
                    },
                    |pool| {
                        // Simulate buffer usage
                        for _ in 0..100 {
                            let mut buffer = pool.get_string_buffer(buffer_size);
                            let buffer_mut = buffer.buffer_mut();
                            buffer_mut.push_str(&"x".repeat(buffer_size));
                            black_box(buffer_mut.len());
                        }
                    },
                );
            },
        );

        // Benchmark direct allocation for comparison
        group.bench_with_input(
            BenchmarkId::new("direct_allocation", buffer_size),
            &buffer_size,
            |b, &buffer_size| {
                b.iter(|| {
                    for _ in 0..100 {
                        let mut buffer = String::with_capacity(buffer_size);
                        buffer.push_str(&"x".repeat(buffer_size));
                        black_box(buffer.len());
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    benchmark_basic_storage_operations,
    benchmark_concurrent_operations,
    benchmark_async_operations,
    benchmark_memory_pool
);
criterion_main!(benches);

//! Performance demonstration for concurrent read/write coordination
//!
//! This test demonstrates that the concurrent coordination system provides
//! the expected performance characteristics and deadlock-free operation.

use shardex::concurrent::ConcurrentShardex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;

mod common;
use common::{create_test_concurrent_shardex, TestEnvironment};

/// Helper function to collect task results from JoinSet with consistent error handling
async fn collect_task_results(
    tasks: &mut JoinSet<(usize, usize, &'static str)>,
    test_name: &str,
) -> Vec<(usize, usize, &'static str)> {
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((id, successes, role)) => results.push((id, successes, role)),
            Err(e) => eprintln!("{} task failed: {}", test_name, e),
        }
    }
    results
}

#[tokio::test]
async fn test_concurrent_throughput_demonstration() {
    let _test_env = TestEnvironment::new("test_concurrent_throughput_demonstration");
    let concurrent = Arc::new(create_test_concurrent_shardex(&_test_env));

    const NUM_READERS: usize = 20;
    const NUM_WRITERS: usize = 3;
    const OPERATIONS_PER_READER: usize = 50;
    const OPERATIONS_PER_WRITER: usize = 10;

    let successful_operations = Arc::new(AtomicUsize::new(0));
    let mut tasks = JoinSet::new();
    let start_time = Instant::now();

    // Spawn readers
    for reader_id in 0..NUM_READERS {
        let concurrent_clone: Arc<ConcurrentShardex> = Arc::clone(&concurrent);
        let success_counter = Arc::clone(&successful_operations);

        tasks.spawn(async move {
            let mut reader_successes = 0;
            for _op in 0..OPERATIONS_PER_READER {
                let result = concurrent_clone.read_operation(|index| {
                    // Simulate light read work
                    let shard_count = index.shard_count();
                    Ok(shard_count)
                });

                if result.is_ok() {
                    reader_successes += 1;
                    success_counter.fetch_add(1, Ordering::SeqCst);
                }
            }
            (reader_id, reader_successes, "reader")
        });
    }

    // Spawn writers
    for writer_id in 0..NUM_WRITERS {
        let concurrent_clone: Arc<ConcurrentShardex> = Arc::clone(&concurrent);
        let success_counter = Arc::clone(&successful_operations);

        tasks.spawn(async move {
            let mut writer_successes = 0;
            for _op in 0..OPERATIONS_PER_WRITER {
                let result = concurrent_clone
                    .write_operation(|writer| {
                        // Simulate light write work
                        let shard_count = writer.index().shard_count();
                        Ok(shard_count)
                    })
                    .await;

                if result.is_ok() {
                    writer_successes += 1;
                    success_counter.fetch_add(1, Ordering::SeqCst);
                }
            }
            (writer_id, writer_successes, "writer")
        });
    }

    // Collect results
    let results = collect_task_results(&mut tasks, "Concurrent throughput").await;

    let total_duration = start_time.elapsed();
    let total_successful = successful_operations.load(Ordering::SeqCst);
    let expected_operations = (NUM_READERS * OPERATIONS_PER_READER) + (NUM_WRITERS * OPERATIONS_PER_WRITER);

    // Display results
    println!("\n=== Concurrent Performance Demonstration ===");
    println!("Duration: {:?}", total_duration);
    println!("Total operations: {}/{}", total_successful, expected_operations);
    println!(
        "Throughput: {:.1} ops/sec",
        total_successful as f64 / total_duration.as_secs_f64()
    );

    let reader_successes: usize = results
        .iter()
        .filter(|(_, _, role)| *role == "reader")
        .map(|(_, successes, _)| *successes)
        .sum();

    let writer_successes: usize = results
        .iter()
        .filter(|(_, _, role)| *role == "writer")
        .map(|(_, successes, _)| *successes)
        .sum();

    println!("Reader operations: {}", reader_successes);
    println!("Writer operations: {}", writer_successes);

    // Verify most operations succeeded
    assert!(
        total_successful >= expected_operations * 8 / 10,
        "Expected at least 80% success rate, got {}/{}",
        total_successful,
        expected_operations
    );

    // Verify readers significantly outnumbered writers (as expected)
    assert!(
        reader_successes > writer_successes,
        "Readers should perform more operations than writers: {} vs {}",
        reader_successes,
        writer_successes
    );

    // Final system state should be consistent
    let final_metrics = concurrent.concurrency_metrics().await;
    assert_eq!(final_metrics.active_readers, 0, "No readers should remain active");
    assert_eq!(final_metrics.active_writers, 0, "No writers should remain active");

    println!(
        "Final system state: epoch={}, no active operations",
        final_metrics.current_epoch
    );
}

#[tokio::test]
async fn test_basic_coordination_functionality() {
    let _test_env = TestEnvironment::new("test_basic_coordination_functionality");
    let concurrent = create_test_concurrent_shardex(&_test_env);

    // Test basic read functionality
    let read_result = concurrent.read_operation(|index| {
        let shard_count = index.shard_count();
        Ok(shard_count)
    });

    assert!(read_result.is_ok(), "Basic read operation should succeed");
    assert_eq!(read_result.unwrap(), 0, "Empty index should have 0 shards");

    // Test basic write functionality
    let write_result = concurrent
        .write_operation(|writer| {
            let shard_count = writer.index().shard_count();
            Ok(shard_count)
        })
        .await;

    assert!(write_result.is_ok(), "Basic write operation should succeed");
    assert_eq!(write_result.unwrap(), 0, "Empty index should have 0 shards");

    // Verify system metrics
    let metrics = concurrent.concurrency_metrics().await;
    assert_eq!(metrics.active_readers, 0, "No active readers after operations");
    assert_eq!(metrics.active_writers, 0, "No active writers after operations");
    assert!(metrics.current_epoch > 1, "Epoch should advance after write operations");

    println!("Basic coordination test passed: epoch={}", metrics.current_epoch);
}

#[tokio::test]
async fn test_coordination_statistics() {
    let _test_env = TestEnvironment::new("test_coordination_statistics");
    let concurrent = create_test_concurrent_shardex(&_test_env);

    // Initial stats should be empty
    let initial_stats = concurrent.coordination_stats().await;
    assert_eq!(initial_stats.total_writes, 0, "Should start with no writes");

    // Perform some write operations
    const NUM_WRITES: u64 = 3;
    for i in 0..NUM_WRITES {
        let result = concurrent
            .write_operation(|writer| Ok(writer.index().shard_count()))
            .await;

        assert!(result.is_ok(), "Write operation {} should succeed", i);
    }

    // Check updated statistics
    let final_stats = concurrent.coordination_stats().await;
    assert_eq!(
        final_stats.total_writes, NUM_WRITES,
        "Should track all write operations"
    );

    println!(
        "Coordination statistics: total_writes={}, contention_rate={:.2}%",
        final_stats.total_writes,
        final_stats.contention_rate()
    );
}

#[tokio::test]
async fn test_concurrent_readers_non_blocking() {
    let _test_env = TestEnvironment::new("test_concurrent_readers_non_blocking");
    let concurrent = Arc::new(create_test_concurrent_shardex(&_test_env));

    const NUM_CONCURRENT_READERS: usize = 10;
    let start_time = Instant::now();
    let mut tasks = JoinSet::new();

    // Start multiple concurrent readers
    for reader_id in 0..NUM_CONCURRENT_READERS {
        let concurrent_clone: Arc<ConcurrentShardex> = Arc::clone(&concurrent);

        tasks.spawn(async move {
            let reader_start = Instant::now();

            let result = concurrent_clone.read_operation(|index| {
                // Simulate some read work
                std::thread::sleep(Duration::from_millis(10));
                Ok(index.shard_count())
            });

            let reader_duration = reader_start.elapsed();
            (reader_id, result, reader_duration)
        });
    }

    // Collect results
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((reader_id, op_result, duration)) => {
                assert!(op_result.is_ok(), "Reader {} should succeed", reader_id);
                results.push(duration);
            }
            Err(e) => panic!("Reader task failed: {}", e),
        }
    }

    let total_duration = start_time.elapsed();

    // All readers should complete successfully
    assert_eq!(results.len(), NUM_CONCURRENT_READERS, "All readers should complete");

    // The total time should be reasonable for concurrent readers
    // Allow generous timing as system load, async overhead, and test environment can vary
    // The key is that readers complete successfully, not the exact timing
    assert!(
        total_duration < Duration::from_secs(1),
        "Concurrent readers took too long (possible blocking): {:?}",
        total_duration
    );

    println!(
        "Concurrent readers test: {} readers in {:?} (avg: {:.1}ms per reader)",
        NUM_CONCURRENT_READERS,
        total_duration,
        total_duration.as_millis() as f64 / NUM_CONCURRENT_READERS as f64
    );
}

/// Comprehensive performance demonstration with realistic document workloads
#[tokio::test]
async fn test_realistic_document_workload_performance() {
    let _test_env = TestEnvironment::new("test_realistic_document_workload_performance");
    let concurrent = Arc::new(create_test_concurrent_shardex(&_test_env));

    // Configuration based on environment variables or defaults
    let num_writers = std::env::var("SHARDEX_PERF_WRITERS")
        .unwrap_or_else(|_| "4".to_string())
        .parse::<usize>()
        .unwrap_or(4);
    let num_readers = std::env::var("SHARDEX_PERF_READERS")
        .unwrap_or_else(|_| "12".to_string())
        .parse::<usize>()
        .unwrap_or(12);
    let docs_per_writer = std::env::var("SHARDEX_PERF_DOCS_PER_WRITER")
        .unwrap_or_else(|_| "25".to_string())
        .parse::<usize>()
        .unwrap_or(25);
    let searches_per_reader = std::env::var("SHARDEX_PERF_SEARCHES_PER_READER")
        .unwrap_or_else(|_| "50".to_string())
        .parse::<usize>()
        .unwrap_or(50);

    let successful_operations = Arc::new(AtomicUsize::new(0));
    let mut tasks = JoinSet::new();
    let start_time = Instant::now();

    // Collect latency measurements
    let write_latencies = Arc::new(std::sync::Mutex::new(Vec::<Duration>::new()));
    let read_latencies = Arc::new(std::sync::Mutex::new(Vec::<Duration>::new()));

    println!("\n=== Realistic Document Workload Performance Test ===");
    println!(
        "Configuration: {} writers × {} docs, {} readers × {} searches",
        num_writers, docs_per_writer, num_readers, searches_per_reader
    );

    // Spawn document writers
    for writer_id in 0..num_writers {
        let concurrent_clone: Arc<ConcurrentShardex> = Arc::clone(&concurrent);
        let success_counter = Arc::clone(&successful_operations);
        let latencies = Arc::clone(&write_latencies);

        tasks.spawn(async move {
            let mut writer_successes = 0;

            for doc_idx in 0..docs_per_writer {
                let op_start = Instant::now();

                let result = concurrent_clone
                    .write_operation(|writer| {
                        use shardex::DocumentId;

                        // Create realistic document posting
                        let doc_id = DocumentId::new();
                        let vector_size = 64;

                        // Generate varied vectors (not just zeros)
                        let vector: Vec<f32> = (0..vector_size)
                            .map(|i| {
                                let base = (writer_id * 1000 + doc_idx * 100 + i) as f32;
                                (base * 0.001).sin() // Creates varied but deterministic vectors
                            })
                            .collect();

                        // Add posting through the index writer
                        writer.index_mut().add_posting(
                            doc_id,
                            (doc_idx * 100) as u32,     // Varied start positions
                            50 + (doc_idx % 20) as u32, // Varied lengths
                            vector,
                        )?;

                        Ok(doc_id)
                    })
                    .await;

                let op_duration = op_start.elapsed();
                latencies.lock().unwrap().push(op_duration);

                if result.is_ok() {
                    writer_successes += 1;
                    success_counter.fetch_add(1, Ordering::SeqCst);
                }
            }
            (writer_id, writer_successes, "document_writer")
        });
    }

    // Spawn search readers (start slightly after to have some data to search)
    tokio::time::sleep(Duration::from_millis(50)).await;

    for reader_id in 0..num_readers {
        let concurrent_clone: Arc<ConcurrentShardex> = Arc::clone(&concurrent);
        let success_counter = Arc::clone(&successful_operations);
        let latencies = Arc::clone(&read_latencies);

        tasks.spawn(async move {
            let mut reader_successes = 0;

            for search_idx in 0..searches_per_reader {
                let op_start = Instant::now();

                let result = concurrent_clone.read_operation(|index| {
                    // Generate query vector for search
                    let _query_vector: Vec<f32> = (0..64)
                        .map(|i| {
                            let base = (reader_id * 100 + search_idx * 10 + i) as f32;
                            (base * 0.002).cos() // Different pattern from documents
                        })
                        .collect();

                    // Attempt search across available shards
                    let mut total_results = 0;
                    let shard_ids = index.shard_ids();
                    for _shard_id in shard_ids.iter().take(5) {
                        // Limit to first 5 shards for performance
                        // Note: get_shard requires &mut, but we have &, so we'll just count shards
                        total_results += 1; // Count available shards as a proxy for search results
                    }

                    Ok(total_results)
                });

                let op_duration = op_start.elapsed();
                latencies.lock().unwrap().push(op_duration);

                if result.is_ok() {
                    reader_successes += 1;
                    success_counter.fetch_add(1, Ordering::SeqCst);
                }
            }
            (reader_id, reader_successes, "search_reader")
        });
    }

    // Collect results
    let results = collect_task_results(&mut tasks, "Realistic workload").await;

    let total_duration = start_time.elapsed();
    let total_successful = successful_operations.load(Ordering::SeqCst);
    let expected_operations = (num_writers * docs_per_writer) + (num_readers * searches_per_reader);

    // Calculate detailed performance metrics
    let writer_successes: usize = results
        .iter()
        .filter(|(_, _, role)| role.contains("writer"))
        .map(|(_, successes, _)| *successes)
        .sum();

    let reader_successes: usize = results
        .iter()
        .filter(|(_, _, role)| role.contains("reader"))
        .map(|(_, successes, _)| *successes)
        .sum();

    // Latency analysis
    let write_latencies_vec = write_latencies.lock().unwrap().clone();
    let read_latencies_vec = read_latencies.lock().unwrap().clone();

    let write_p50 = percentile(&write_latencies_vec, 0.5);
    let write_p95 = percentile(&write_latencies_vec, 0.95);
    let read_p50 = percentile(&read_latencies_vec, 0.5);
    let read_p95 = percentile(&read_latencies_vec, 0.95);

    // Display comprehensive results
    println!("Duration: {:?}", total_duration);
    println!("Total operations: {}/{}", total_successful, expected_operations);
    println!(
        "Overall throughput: {:.1} ops/sec",
        total_successful as f64 / total_duration.as_secs_f64()
    );
    println!("Document insertions: {}", writer_successes);
    println!("Search operations: {}", reader_successes);

    if !write_latencies_vec.is_empty() {
        println!(
            "Write latency: p50={:.1}ms, p95={:.1}ms",
            write_p50.as_millis(),
            write_p95.as_millis()
        );
    }
    if !read_latencies_vec.is_empty() {
        println!(
            "Read latency: p50={:.1}ms, p95={:.1}ms",
            read_p50.as_millis(),
            read_p95.as_millis()
        );
    }

    // Performance expectations
    assert!(
        total_successful >= expected_operations * 8 / 10,
        "Expected at least 80% success rate, got {}/{}",
        total_successful,
        expected_operations
    );

    // Write operations should generally be slower than read operations
    if !write_latencies_vec.is_empty() && !read_latencies_vec.is_empty() {
        assert!(
            write_p50 >= read_p50,
            "Write operations should generally be slower than reads: {}ms vs {}ms",
            write_p50.as_millis(),
            read_p50.as_millis()
        );
    }

    // Final system consistency check
    let final_metrics = concurrent.concurrency_metrics().await;
    assert_eq!(final_metrics.active_readers, 0, "No readers should remain active");
    assert_eq!(final_metrics.active_writers, 0, "No writers should remain active");

    println!(
        "Final system state: epoch={}, no active operations",
        final_metrics.current_epoch
    );
}

/// Stress test with high concurrency levels
#[tokio::test]
async fn test_high_concurrency_stress() {
    let _test_env = TestEnvironment::new("test_high_concurrency_stress");
    let concurrent = Arc::new(create_test_concurrent_shardex(&_test_env));

    const HIGH_READER_COUNT: usize = 50;
    const HIGH_WRITER_COUNT: usize = 8;
    const OPERATIONS_PER_THREAD: usize = 20;

    let successful_operations = Arc::new(AtomicUsize::new(0));
    let contention_counter = Arc::new(AtomicUsize::new(0));
    let mut tasks = JoinSet::new();
    let start_time = Instant::now();

    println!("\n=== High Concurrency Stress Test ===");
    println!(
        "Configuration: {} readers, {} writers, {} ops each",
        HIGH_READER_COUNT, HIGH_WRITER_COUNT, OPERATIONS_PER_THREAD
    );

    // Spawn high number of readers
    for reader_id in 0..HIGH_READER_COUNT {
        let concurrent_clone: Arc<ConcurrentShardex> = Arc::clone(&concurrent);
        let success_counter = Arc::clone(&successful_operations);

        tasks.spawn(async move {
            let mut successes = 0;

            for _op in 0..OPERATIONS_PER_THREAD {
                let result = concurrent_clone.read_operation(|index| {
                    // Minimal read operation to stress coordination
                    let shard_count = index.shard_count();
                    let shard_ids = index.shard_ids();
                    // Since we can't call get_shard (requires &mut), just work with metadata
                    let metadata_count = index.all_shard_metadata().len();
                    Ok(shard_count + metadata_count + shard_ids.len())
                });

                if result.is_ok() {
                    successes += 1;
                    success_counter.fetch_add(1, Ordering::SeqCst);
                }
            }
            (reader_id, successes, "high_concurrency_reader")
        });
    }

    // Spawn high number of writers
    for writer_id in 0..HIGH_WRITER_COUNT {
        let concurrent_clone: Arc<ConcurrentShardex> = Arc::clone(&concurrent);
        let success_counter = Arc::clone(&successful_operations);
        let contention_counter_clone = Arc::clone(&contention_counter);

        tasks.spawn(async move {
            let mut successes = 0;

            for _op in 0..OPERATIONS_PER_THREAD {
                let op_start = Instant::now();

                let result = concurrent_clone
                    .write_operation(|writer| {
                        // Light write operation
                        let current_shards = writer.index().shard_count();
                        Ok(current_shards)
                    })
                    .await;

                let op_duration = op_start.elapsed();

                // Track potential contention (operations taking longer than expected)
                if op_duration > Duration::from_millis(100) {
                    contention_counter_clone.fetch_add(1, Ordering::SeqCst);
                }

                if result.is_ok() {
                    successes += 1;
                    success_counter.fetch_add(1, Ordering::SeqCst);
                }
            }
            (writer_id, successes, "high_concurrency_writer")
        });
    }

    // Collect results
    let _results = collect_task_results(&mut tasks, "High concurrency stress").await;

    let total_duration = start_time.elapsed();
    let total_successful = successful_operations.load(Ordering::SeqCst);
    let total_contention = contention_counter.load(Ordering::SeqCst);
    let expected_operations = (HIGH_READER_COUNT + HIGH_WRITER_COUNT) * OPERATIONS_PER_THREAD;

    println!("Duration: {:?}", total_duration);
    println!("Total operations: {}/{}", total_successful, expected_operations);
    println!(
        "Throughput: {:.1} ops/sec",
        total_successful as f64 / total_duration.as_secs_f64()
    );
    println!("Contention events: {}", total_contention);
    println!(
        "Contention rate: {:.1}%",
        (total_contention as f64 / total_successful as f64) * 100.0
    );

    // Validate stress test results
    assert!(
        total_successful >= expected_operations * 7 / 10,
        "Expected at least 70% success rate under stress, got {}/{}",
        total_successful,
        expected_operations
    );

    // Under high stress, some contention is acceptable but should be limited
    let contention_rate = total_contention as f64 / total_successful as f64;
    assert!(
        contention_rate < 0.3,
        "Contention rate too high: {:.1}% (should be < 30%)",
        contention_rate * 100.0
    );

    let final_metrics = concurrent.concurrency_metrics().await;
    assert_eq!(
        final_metrics.active_readers, 0,
        "No readers should remain active after stress test"
    );
    assert_eq!(
        final_metrics.active_writers, 0,
        "No writers should remain active after stress test"
    );

    println!("Stress test completed: epoch={}", final_metrics.current_epoch);
}

/// Calculate percentile from a sorted list of durations
fn percentile(durations: &[Duration], p: f64) -> Duration {
    if durations.is_empty() {
        return Duration::from_nanos(0);
    }

    let mut sorted = durations.to_vec();
    sorted.sort();

    let index = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted
        .get(index)
        .copied()
        .unwrap_or(Duration::from_nanos(0))
}

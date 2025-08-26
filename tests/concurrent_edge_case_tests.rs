//! Integration tests for concurrent operation edge cases
//!
//! This test suite covers edge cases and failure scenarios that can occur
//! in production environments, including memory pressure, crashes, and
//! concurrent reader/writer failures.

use shardex::{ConcurrentShardex, CowShardexIndex, ShardexConfig, ShardexIndex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tokio::time::timeout;

mod common;
use common::{create_test_concurrent_shardex, TestEnvironment};

/// Test concurrent reader/writer failures and recovery
#[tokio::test]
async fn test_concurrent_reader_writer_failures() {
    let _test_env = TestEnvironment::new("test_concurrent_reader_writer_failures");
    let concurrent = Arc::new(create_test_concurrent_shardex(&_test_env));

    let failure_count = Arc::new(AtomicUsize::new(0));
    let success_count = Arc::new(AtomicUsize::new(0));
    let mut tasks = JoinSet::new();

    // Spawn readers that may encounter failures
    for reader_id in 0..5 {
        let concurrent_clone = Arc::clone(&concurrent);
        let failure_counter = Arc::clone(&failure_count);
        let success_counter = Arc::clone(&success_count);

        tasks.spawn(async move {
            for _op_id in 0..20 {
                let result = concurrent_clone.read_operation(|index| {
                    // Simulate occasional read failures
                    if reader_id % 3 == 0 && _op_id % 7 == 0 {
                        return Err(shardex::error::ShardexError::InvalidInput {
                            field: "simulated_read_failure".to_string(),
                            reason: "Test failure injection".to_string(),
                            suggestion: "This is a simulated failure for testing".to_string(),
                        });
                    }
                    Ok(index.shard_count())
                });

                match result {
                    Ok(_) => success_counter.fetch_add(1, Ordering::SeqCst),
                    Err(_) => failure_counter.fetch_add(1, Ordering::SeqCst),
                };

                // Small delay to allow interleaving
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        });
    }

    // Spawn writers that may encounter failures
    for writer_id in 0..3 {
        let concurrent_clone = Arc::clone(&concurrent);
        let failure_counter = Arc::clone(&failure_count);
        let success_counter = Arc::clone(&success_count);

        tasks.spawn(async move {
            for op_id in 0..10 {
                let result = concurrent_clone
                    .write_operation(|_writer| {
                        // Simulate occasional write failures
                        if writer_id == 1 && op_id % 5 == 0 {
                            return Err(shardex::error::ShardexError::InvalidInput {
                                field: "simulated_write_failure".to_string(),
                                reason: "Test failure injection".to_string(),
                                suggestion: "This is a simulated failure for testing".to_string(),
                            });
                        }
                        Ok(writer_id + op_id)
                    })
                    .await;

                match result {
                    Ok(_) => success_counter.fetch_add(1, Ordering::SeqCst),
                    Err(_) => failure_counter.fetch_add(1, Ordering::SeqCst),
                };

                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        });
    }

    // Wait for all operations to complete
    let test_timeout = Duration::from_secs(30);
    let test_result = timeout(test_timeout, async {
        while let Some(result) = tasks.join_next().await {
            result.expect("Task should not panic even with simulated failures");
        }
    })
    .await;

    assert!(test_result.is_ok(), "Test timed out");

    let total_failures = failure_count.load(Ordering::SeqCst);
    let total_successes = success_count.load(Ordering::SeqCst);

    // Verify that we had both successes and expected failures
    assert!(total_successes > 0, "Should have some successful operations");
    assert!(total_failures > 0, "Should have some simulated failures");

    println!(
        "Completed with {} successes and {} expected failures",
        total_successes, total_failures
    );
}

/// Test system behavior under memory pressure simulation
#[tokio::test]
async fn test_memory_pressure_behavior() {
    let _test_env = TestEnvironment::new("test_memory_pressure_behavior");
    let concurrent = Arc::new(create_test_concurrent_shardex(&_test_env));

    let mut tasks = JoinSet::new();
    let memory_pressure_operations = Arc::new(AtomicUsize::new(0));

    // Create memory pressure by spawning many concurrent writers
    // This simulates the scenario where COW operations consume significant memory
    for writer_id in 0..20 {
        let concurrent_clone = Arc::clone(&concurrent);
        let ops_counter = Arc::clone(&memory_pressure_operations);

        tasks.spawn(async move {
            // Each writer performs a small number of operations
            for _op_id in 0..3 {
                let start_time = Instant::now();
                let result = concurrent_clone
                    .write_operation(|_writer| {
                        // Simulate memory-intensive operation
                        let _large_vec: Vec<u8> = vec![0; 1024 * 1024]; // 1MB allocation
                        std::thread::sleep(Duration::from_millis(10));
                        Ok(writer_id)
                    })
                    .await;

                if result.is_ok() {
                    ops_counter.fetch_add(1, Ordering::SeqCst);
                }

                let duration = start_time.elapsed();
                // Under memory pressure, operations may take longer
                if duration > Duration::from_millis(100) {
                    println!(
                        "Writer {} operation took {:?} (possible memory pressure)",
                        writer_id, duration
                    );
                }

                // Small delay to prevent overwhelming the system
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        });
    }

    // Monitor memory pressure indicators while operations run
    let concurrent_monitor = Arc::clone(&concurrent);
    let monitor_task = tokio::spawn(async move {
        let mut high_contention_count = 0;

        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            let stats = concurrent_monitor.coordination_stats().await;

            if stats.contended_writes > 0 {
                high_contention_count += 1;
                println!(
                    "Memory pressure indicator - contended writes: {}",
                    stats.contended_writes
                );
            }
        }

        high_contention_count
    });

    // Wait for operations with generous timeout due to memory pressure
    let test_timeout = Duration::from_secs(120);
    let test_result = timeout(test_timeout, async {
        while let Some(result) = tasks.join_next().await {
            result.expect("Memory pressure test task should not panic");
        }
    })
    .await;

    assert!(test_result.is_ok(), "Memory pressure test timed out");

    let contention_observations = monitor_task.await.expect("Monitor task failed");
    let completed_operations = memory_pressure_operations.load(Ordering::SeqCst);

    println!(
        "Completed {} operations under memory pressure with {} contention observations",
        completed_operations, contention_observations
    );

    // Verify system continued to function under pressure
    assert!(
        completed_operations > 0,
        "Should complete some operations even under pressure"
    );
}

/// Test recovery after simulated crashes during COW operations
#[tokio::test]
async fn test_cow_operation_crash_recovery() {
    let _test_env = TestEnvironment::new("test_cow_operation_crash_recovery");
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(64)
        .shard_size(100);

    // Create initial index
    let index = ShardexIndex::create(config.clone()).expect("Failed to create index");
    let cow_index = CowShardexIndex::new(index);
    let concurrent = ConcurrentShardex::new(cow_index);

    // Perform some initial operations to have data
    for i in 0..5 {
        let result = concurrent.write_operation(|_writer| Ok(i)).await;
        assert!(result.is_ok(), "Initial setup operations should succeed");
    }

    // Get initial state
    let initial_shard_count = concurrent
        .read_operation(|index| Ok(index.shard_count()))
        .expect("Should be able to read initial state");

    // Simulate crash scenario by dropping the concurrent instance
    drop(concurrent);

    // Recovery: Create new instance from same directory
    let recovered_index = ShardexIndex::open(_test_env.path()).expect("Should be able to recover index");
    let recovered_cow = CowShardexIndex::new(recovered_index);
    let recovered_concurrent = ConcurrentShardex::new(recovered_cow);

    // Verify recovery worked
    let recovered_shard_count = recovered_concurrent
        .read_operation(|index| Ok(index.shard_count()))
        .expect("Should be able to read recovered state");

    assert_eq!(
        initial_shard_count, recovered_shard_count,
        "Recovered index should have same shard count as before crash"
    );

    // Verify system can continue operating after recovery
    let post_recovery_result = recovered_concurrent.write_operation(|_writer| Ok(42)).await;

    assert!(
        post_recovery_result.is_ok(),
        "Should be able to perform operations after recovery"
    );

    println!(
        "Successfully recovered from simulated crash. Shard count: initial={}, recovered={}",
        initial_shard_count, recovered_shard_count
    );
}

/// Test edge case of extremely high read concurrency
#[tokio::test]
async fn test_extreme_read_concurrency() {
    let _test_env = TestEnvironment::new("test_extreme_read_concurrency");
    let concurrent = Arc::new(create_test_concurrent_shardex(&_test_env));

    let read_count = Arc::new(AtomicUsize::new(0));
    let mut tasks = JoinSet::new();

    // Spawn many concurrent readers (simulate high read load)
    for reader_id in 0..100 {
        let concurrent_clone = Arc::clone(&concurrent);
        let counter = Arc::clone(&read_count);

        tasks.spawn(async move {
            for _op_id in 0..10 {
                let result = concurrent_clone.read_operation(|index| {
                    // Simulate some work
                    let shard_count = index.shard_count();
                    Ok(shard_count + reader_id)
                });

                if result.is_ok() {
                    counter.fetch_add(1, Ordering::SeqCst);
                }

                // Vary timing slightly to create realistic access patterns
                let delay_ms = (reader_id % 3) + 1;
                tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
            }
        });
    }

    // Also run some writers concurrently to test mixed workload
    for writer_id in 0..5 {
        let concurrent_clone = Arc::clone(&concurrent);

        tasks.spawn(async move {
            for _op_id in 0..3 {
                let _result = concurrent_clone
                    .write_operation(|_writer| Ok(writer_id))
                    .await;

                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    let start_time = Instant::now();

    // Wait for all operations with timeout
    let test_timeout = Duration::from_secs(60);
    let test_result = timeout(test_timeout, async {
        while let Some(result) = tasks.join_next().await {
            result.expect("High concurrency test task should not panic");
        }
    })
    .await;

    let duration = start_time.elapsed();
    let total_reads = read_count.load(Ordering::SeqCst);

    assert!(test_result.is_ok(), "High concurrency test timed out");
    assert_eq!(total_reads, 1000, "Should complete all 1000 read operations");

    println!(
        "Completed {} reads in {:?} with extreme concurrency",
        total_reads, duration
    );

    // Verify reasonable performance under high load
    let reads_per_second = total_reads as f64 / duration.as_secs_f64();
    assert!(
        reads_per_second > 100.0,
        "Should maintain reasonable throughput under high concurrency: {:.1} reads/sec",
        reads_per_second
    );
}

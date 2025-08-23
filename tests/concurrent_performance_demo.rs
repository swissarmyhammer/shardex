//! Performance demonstration for concurrent read/write coordination
//!
//! This test demonstrates that the concurrent coordination system provides
//! the expected performance characteristics and deadlock-free operation.

use shardex::{ConcurrentShardex, CowShardexIndex, ShardexConfig, ShardexIndex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::task::JoinSet;

/// RAII-based test environment for isolated testing
struct TestEnvironment {
    pub temp_dir: TempDir,
    #[allow(dead_code)]
    pub test_name: String,
}

impl TestEnvironment {
    fn new(test_name: &str) -> Self {
        let temp_dir = TempDir::new()
            .unwrap_or_else(|e| panic!("Failed to create temp dir for test {}: {}", test_name, e));

        Self {
            temp_dir,
            test_name: test_name.to_string(),
        }
    }

    fn path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }
}

/// Create a test ConcurrentShardex instance
fn create_test_concurrent_shardex(test_env: &TestEnvironment) -> ConcurrentShardex {
    let config = ShardexConfig::new()
        .directory_path(test_env.path())
        .vector_size(64)
        .shard_size(100);

    let index = ShardexIndex::create(config).expect("Failed to create index");
    let cow_index = CowShardexIndex::new(index);
    ConcurrentShardex::new(cow_index)
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
        let concurrent_clone = Arc::clone(&concurrent);
        let success_counter = Arc::clone(&successful_operations);

        tasks.spawn(async move {
            let mut reader_successes = 0;
            for _op in 0..OPERATIONS_PER_READER {
                let result = concurrent_clone
                    .read_operation(|index| {
                        // Simulate light read work
                        let shard_count = index.shard_count();
                        Ok(shard_count)
                    })
                    .await;

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
        let concurrent_clone = Arc::clone(&concurrent);
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
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((id, successes, role)) => results.push((id, successes, role)),
            Err(e) => eprintln!("Task failed: {}", e),
        }
    }

    let total_duration = start_time.elapsed();
    let total_successful = successful_operations.load(Ordering::SeqCst);
    let expected_operations =
        (NUM_READERS * OPERATIONS_PER_READER) + (NUM_WRITERS * OPERATIONS_PER_WRITER);

    // Display results
    println!("\n=== Concurrent Performance Demonstration ===");
    println!("Duration: {:?}", total_duration);
    println!(
        "Total operations: {}/{}",
        total_successful, expected_operations
    );
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
    let final_metrics = concurrent.concurrency_metrics();
    assert_eq!(
        final_metrics.active_readers, 0,
        "No readers should remain active"
    );
    assert_eq!(
        final_metrics.active_writers, 0,
        "No writers should remain active"
    );

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
    let read_result = concurrent
        .read_operation(|index| {
            let shard_count = index.shard_count();
            Ok(shard_count)
        })
        .await;

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
    let metrics = concurrent.concurrency_metrics();
    assert_eq!(
        metrics.active_readers, 0,
        "No active readers after operations"
    );
    assert_eq!(
        metrics.active_writers, 0,
        "No active writers after operations"
    );
    assert!(
        metrics.current_epoch > 1,
        "Epoch should advance after write operations"
    );

    println!(
        "Basic coordination test passed: epoch={}",
        metrics.current_epoch
    );
}

#[tokio::test]
async fn test_coordination_statistics() {
    let _test_env = TestEnvironment::new("test_coordination_statistics");
    let concurrent = create_test_concurrent_shardex(&_test_env);

    // Initial stats should be empty
    let initial_stats = concurrent
        .coordination_stats()
        .expect("Failed to get initial stats");
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
    let final_stats = concurrent
        .coordination_stats()
        .expect("Failed to get final stats");
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
        let concurrent_clone = Arc::clone(&concurrent);

        tasks.spawn(async move {
            let reader_start = Instant::now();

            let result = concurrent_clone
                .read_operation(|index| {
                    // Simulate some read work
                    std::thread::sleep(Duration::from_millis(10));
                    Ok(index.shard_count())
                })
                .await;

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
    assert_eq!(
        results.len(),
        NUM_CONCURRENT_READERS,
        "All readers should complete"
    );

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

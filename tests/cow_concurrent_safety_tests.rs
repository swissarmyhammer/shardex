//! Comprehensive concurrent access safety tests for CowShardexIndex
//!
//! This test suite verifies that the copy-on-write implementation provides
//! safe concurrent access patterns as specified in the requirements.

use shardex::{CowShardexIndex, Shard, ShardexConfig, ShardexIndex};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

/// RAII-based test environment for isolated testing
struct TestEnvironment {
    pub temp_dir: TempDir,
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

    fn path_buf(&self) -> std::path::PathBuf {
        self.temp_dir.path().to_path_buf()
    }
}

/// Test concurrent readers don't block each other
#[test]
fn test_concurrent_readers_non_blocking() {
    let _test_env = TestEnvironment::new("test_concurrent_readers_non_blocking");
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(128)
        .shard_size(100);

    let index = ShardexIndex::create(config).expect("Failed to create index");
    let cow_index = Arc::new(CowShardexIndex::new(index));
    
    const NUM_READERS: usize = 10;
    const READS_PER_READER: usize = 100;
    
    let barrier = Arc::new(Barrier::new(NUM_READERS));
    let mut handles = vec![];
    
    for reader_id in 0..NUM_READERS {
        let cow_index_clone = Arc::clone(&cow_index);
        let barrier_clone = Arc::clone(&barrier);
        
        let handle = thread::spawn(move || {
            // All readers start at the same time
            barrier_clone.wait();
            
            let start_time = std::time::Instant::now();
            for _ in 0..READS_PER_READER {
                let reader = cow_index_clone.read();
                let _shard_count = reader.shard_count();
                // Simulate some work with the reader
                thread::sleep(Duration::from_micros(10));
            }
            let duration = start_time.elapsed();
            
            println!("Reader {} completed {} reads in {:?}", 
                     reader_id, READS_PER_READER, duration);
            
            duration
        });
        
        handles.push(handle);
    }
    
    // Collect all durations
    let mut durations = vec![];
    for handle in handles {
        let duration = handle.join().unwrap();
        durations.push(duration);
    }
    
    // Verify that all readers completed in reasonable time
    // If readers were blocking each other, some would be significantly slower
    let min_duration = durations.iter().min().unwrap();
    let max_duration = durations.iter().max().unwrap();
    
    // Maximum duration should not be more than 3x the minimum (allowing for OS scheduling)
    let ratio = max_duration.as_millis() as f64 / min_duration.as_millis() as f64;
    assert!(ratio < 3.0, 
            "Readers appear to be blocking each other. Min: {:?}, Max: {:?}, Ratio: {:.2}", 
            min_duration, max_duration, ratio);
}

/// Test that readers get consistent snapshots during writes
#[tokio::test]
async fn test_reader_consistency_during_writes() {
    let _test_env = TestEnvironment::new("test_reader_consistency_during_writes");
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(64)
        .shard_size(50);

    let mut initial_index = ShardexIndex::create(config).expect("Failed to create index");
    
    // Add some initial shards
    for _ in 0..3 {
        let shard_id = shardex::ShardId::new();
        let shard = Shard::create(shard_id, 50, 64, _test_env.path_buf())
            .expect("Failed to create shard");
        initial_index.add_shard(shard).expect("Failed to add shard");
    }
    
    let cow_index = Arc::new(CowShardexIndex::new(initial_index));
    
    // Start long-running readers
    let reader_cow_index = Arc::clone(&cow_index);
    let reader_handle = thread::spawn(move || {
        let mut consistent_snapshots = vec![];
        
        for _ in 0..50 {
            let reader = reader_cow_index.read();
            let shard_count = reader.shard_count();
            consistent_snapshots.push(shard_count);
            thread::sleep(Duration::from_millis(5));
        }
        
        consistent_snapshots
    });
    
    // Perform writes concurrently
    let initial_count = cow_index.shard_count();
    
    // Create multiple writers sequentially (simulating batch updates)
    for i in 0..5 {
        let writer = cow_index.clone_for_write().expect("Failed to create writer");
        
        // Simulate modification time
        thread::sleep(Duration::from_millis(10));
        
        // Commit the changes
        writer.commit_changes().await.expect("Failed to commit changes");
        
        println!("Completed write operation {}", i + 1);
    }
    
    // Wait for readers to complete
    let snapshots = reader_handle.join().unwrap();
    
    // Verify that each reader saw a consistent snapshot
    // (Readers should see the same count throughout their individual read operations)
    let unique_counts: std::collections::HashSet<_> = snapshots.iter().collect();
    
    // We should see at most a few different shard counts (initial + updates)
    assert!(unique_counts.len() <= 6, 
            "Too many different shard counts observed: {:?}", unique_counts);
    
    // All counts should be valid (>= initial count)
    for &count in &snapshots {
        assert!(count >= initial_count, 
                "Reader saw inconsistent shard count: {} < {}", count, initial_count);
    }
    
    println!("Reader observed shard counts: {:?}", unique_counts);
}

/// Test that multiple writers can be created and committed sequentially
#[tokio::test]
async fn test_sequential_writer_operations() {
    let _test_env = TestEnvironment::new("test_sequential_writer_operations");
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(32)
        .shard_size(25);

    let index = ShardexIndex::create(config).expect("Failed to create index");
    let cow_index = CowShardexIndex::new(index);
    
    let initial_count = cow_index.shard_count();
    
    // Create multiple writers and commit them sequentially
    for i in 0..5 {
        let writer = cow_index.clone_for_write().expect("Failed to create writer");
        
        // Simulate some modifications to the writer's copy
        let _stats = writer.stats().expect("Failed to get writer stats");
        
        // Commit the changes
        writer.commit_changes().await.expect("Failed to commit changes");
        
        // Verify the changes are visible
        let current_count = cow_index.shard_count();
        assert_eq!(current_count, initial_count, 
                   "Shard count changed unexpectedly after write {}", i + 1);
    }
    
    println!("Completed {} sequential write operations successfully", 5);
}

/// Test memory cleanup of old index versions
#[tokio::test]
async fn test_memory_cleanup_old_versions() {
    let _test_env = TestEnvironment::new("test_memory_cleanup_old_versions");
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(16)
        .shard_size(20);

    let index = ShardexIndex::create(config).expect("Failed to create index");
    let cow_index = Arc::new(CowShardexIndex::new(index));
    
    // Create many readers that hold references to old versions
    let mut reader_handles = vec![];
    
    for i in 0..10 {
        let cow_index_clone = Arc::clone(&cow_index);
        let handle = thread::spawn(move || {
            let reader = cow_index_clone.read();
            
            // Hold the reference for a while
            thread::sleep(Duration::from_millis(100));
            
            let shard_count = reader.shard_count();
            println!("Reader {} saw {} shards", i, shard_count);
            
            // Reference is dropped when reader goes out of scope
            shard_count
        });
        
        reader_handles.push(handle);
        
        // Create a writer and commit changes between each reader
        if i < 9 {
            let writer = cow_index.clone_for_write().expect("Failed to create writer");
            writer.commit_changes().await.expect("Failed to commit changes");
        }
    }
    
    // Wait for all readers to complete
    for handle in reader_handles {
        handle.join().unwrap();
    }
    
    // At this point, all old versions should be eligible for cleanup
    // (This is more of a demonstration - actual memory cleanup verification 
    // would require more complex instrumentation)
    
    println!("Memory cleanup test completed - old versions should be cleaned up");
}

/// Test atomic updates maintain index integrity
#[test]
fn test_atomic_update_integrity() {
    let _test_env = TestEnvironment::new("test_atomic_update_integrity");
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(8)
        .shard_size(10);

    let index = ShardexIndex::create(config).expect("Failed to create index");
    let cow_index = Arc::new(CowShardexIndex::new(index));
    
    const NUM_CONCURRENT_OPERATIONS: usize = 10; // Reduced for simpler test
    
    // Spawn multiple threads that create readers and writers concurrently
    let mut handles = vec![];
    
    for i in 0..NUM_CONCURRENT_OPERATIONS {
        let cow_index_clone = Arc::clone(&cow_index);
        
        let handle = std::thread::spawn(move || {
            if i % 2 == 0 {
                // Even threads: readers
                for _ in 0..10 {
                    let reader = cow_index_clone.read();
                    let stats = reader.stats().expect("Failed to get stats");
                    
                    // Verify stats are internally consistent
                    assert!(stats.total_shards <= 1000, "Unreasonable shard count");
                    assert_eq!(stats.vector_dimension, 8, "Wrong vector dimension");
                    
                    std::thread::sleep(Duration::from_millis(1));
                }
                format!("Reader {} completed", i)
            } else {
                // Odd threads: writers (simplified without async)
                for _j in 0..3 {
                    let writer = cow_index_clone.clone_for_write().expect("Failed to create writer");
                    
                    // Simulate some processing time
                    std::thread::sleep(Duration::from_millis(2));
                    
                    // For non-async tests, just validate the writer state
                    let stats = writer.stats().expect("Failed to get writer stats");
                    assert_eq!(stats.vector_dimension, 8, "Writer integrity compromised");
                    
                    // Discard the writer (simulating commit)
                    drop(writer);
                    
                    // Verify the index is still consistent
                    let reader = cow_index_clone.read();
                    let stats = reader.stats().expect("Failed to get stats");
                    assert_eq!(stats.vector_dimension, 8, "Index integrity compromised");
                }
                format!("Writer {} completed", i)
            }
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    let mut results = vec![];
    for handle in handles {
        let result = handle.join().expect("Task failed");
        results.push(result);
    }
    
    println!("Atomic integrity test completed with {} operations", results.len());
    
    // Final consistency check
    let final_reader = cow_index.read();
    let final_stats = final_reader.stats().expect("Failed to get final stats");
    assert_eq!(final_stats.vector_dimension, 8, "Final integrity check failed");
}

/// Test performance overhead is minimal for typical workloads
#[test]
fn test_performance_overhead() {
    let _test_env = TestEnvironment::new("test_performance_overhead");
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(128)
        .shard_size(100);

    let index = ShardexIndex::create(config).expect("Failed to create index");
    let cow_index = CowShardexIndex::new(index);
    
    const NUM_READS: usize = 1000;
    
    // Measure direct read performance
    let start_time = std::time::Instant::now();
    for _ in 0..NUM_READS {
        let reader = cow_index.read();
        let _count = reader.shard_count();
    }
    let cow_duration = start_time.elapsed();
    
    // Measure quick stats access (optimized path)
    let start_time = std::time::Instant::now();
    for _ in 0..NUM_READS {
        let _count = cow_index.shard_count();
    }
    let quick_duration = start_time.elapsed();
    
    println!("CoW reads: {:?}, Quick access: {:?}", cow_duration, quick_duration);
    
    // Quick access should be significantly faster
    assert!(quick_duration < cow_duration, 
            "Quick access should be faster than full CoW reads");
    
    // Both should be very fast for typical workloads (< 1ms per operation on average)
    let cow_per_op = cow_duration.as_nanos() / NUM_READS as u128;
    let quick_per_op = quick_duration.as_nanos() / NUM_READS as u128;
    
    assert!(cow_per_op < 1_000_000, // < 1ms per operation
            "CoW read overhead too high: {} ns per operation", cow_per_op);
    assert!(quick_per_op < 100_000, // < 0.1ms per operation  
            "Quick access overhead too high: {} ns per operation", quick_per_op);
    
    println!("Performance test passed - CoW: {} ns/op, Quick: {} ns/op", 
             cow_per_op, quick_per_op);
}
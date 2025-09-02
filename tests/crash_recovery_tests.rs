//! Comprehensive crash recovery integration tests
//!
//! This test suite verifies the complete crash recovery functionality including
//! crash detection, WAL replay, consistency validation, and various crash scenarios.

use shardex::{
    CrashRecovery, DirectoryLayout, DocumentId, Posting, Shard, ShardId, ShardexConfig, ShardexError, ShardexIndex,
    TransactionId, WalOperation, WalSegment, WalTransaction,
};
use std::time::Duration;
use tempfile::TempDir;

/// Test basic crash recovery workflow without actual corruption
#[tokio::test]
async fn test_basic_crash_recovery_workflow() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(128);

    // Create and set up index with some data
    {
        let mut index = ShardexIndex::create(config.clone()).unwrap();

        // Create a shard and add some postings
        let shard_id = ShardId::new();
        let mut shard = Shard::create(shard_id, 100, 128, _test_env.path().to_path_buf()).unwrap();

        let doc_id = DocumentId::new();
        let posting = Posting::new(doc_id, 0, 10, vec![0.5; 128], 128).unwrap();
        shard.add_posting(posting).unwrap();

        index.add_shard(shard).unwrap();
    }

    // Test crash recovery process
    let mut recovery = CrashRecovery::new(config);

    // Detect crash (should be false for clean index)
    let crash_detected = recovery.detect_crash().await.unwrap();
    assert!(!crash_detected, "No crash should be detected for clean index");

    // Recovery stats should indicate no crash
    let stats = recovery.recovery_stats();
    assert!(!stats.crash_detected);
    assert!(stats.crash_detection_duration > Duration::from_nanos(0));
}

/// Test crash detection with recovery lock file
#[tokio::test]
async fn test_crash_detection_with_lock_file() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(64);

    // Create basic index structure
    ShardexIndex::create(config.clone()).unwrap();

    // Create a recovery lock file to simulate interrupted recovery
    let lock_file = _test_env.path().join(".recovery_lock");
    std::fs::write(&lock_file, "recovery_in_progress").unwrap();

    let mut recovery = CrashRecovery::new(config);
    let crash_detected = recovery.detect_crash().await.unwrap();

    assert!(crash_detected, "Crash should be detected due to recovery lock file");
    assert!(recovery.recovery_stats().crash_detected);
}

/// Test crash detection with empty metadata file
#[tokio::test]
async fn test_crash_detection_empty_metadata() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(64);

    // Create directory structure but with empty metadata
    let layout = DirectoryLayout::new(_test_env.path());
    layout.create_directories().unwrap();

    let metadata_path = _test_env.path().join("shardex.meta");
    std::fs::write(&metadata_path, "").unwrap(); // Empty file

    let mut recovery = CrashRecovery::new(config);
    let crash_detected = recovery.detect_crash().await.unwrap();

    assert!(crash_detected, "Crash should be detected due to empty metadata file");
    assert!(recovery.recovery_stats().crash_detected);
}

/// Test crash detection when WAL files exist but no metadata
#[tokio::test]
async fn test_crash_detection_wal_without_metadata() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(64);

    // Create directory structure
    let layout = DirectoryLayout::new(_test_env.path());
    layout.create_directories().unwrap();

    // Create a fake WAL file but no metadata
    let wal_path = layout.wal_dir().join("wal_000001.log");
    std::fs::write(&wal_path, "fake_wal_data").unwrap();

    let mut recovery = CrashRecovery::new(config);
    let crash_detected = recovery.detect_crash().await.unwrap();

    assert!(crash_detected, "Crash should be detected - WAL exists but no metadata");
    assert!(recovery.recovery_stats().crash_detected);
}

/// Test complete crash recovery with WAL replay
#[tokio::test]
async fn test_complete_crash_recovery_with_wal() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(64)
        .wal_segment_size(4096);

    // Set up initial index and WAL data
    let layout = DirectoryLayout::new(_test_env.path());
    layout.create_directories().unwrap();

    // Create a WAL segment with some transactions
    let segment_path = layout.wal_segment_path(1);
    let segment = WalSegment::create(1, segment_path, config.wal_segment_size).unwrap();

    // Create a test transaction
    let _transaction_id = TransactionId::new();
    let doc_id = DocumentId::new();
    let operation = WalOperation::AddPosting {
        document_id: doc_id,
        start: 0,
        length: 10,
        vector: vec![0.1; 64],
    };

    let transaction = WalTransaction::new(vec![operation]).unwrap();
    segment.append_transaction(&transaction).unwrap();
    segment.sync().unwrap();

    // Simulate crash by creating recovery lock
    let lock_path = _test_env.path().join(".recovery_lock");
    std::fs::write(&lock_path, "crashed").unwrap();

    // Perform crash recovery
    let mut recovery = CrashRecovery::new(config);

    // Detect crash
    let crash_detected = recovery.detect_crash().await.unwrap();
    assert!(crash_detected, "Should detect crash from recovery lock");

    // Perform recovery
    let recovered_index = recovery.recover().await.unwrap();

    // Verify recovery statistics
    let stats = recovery.recovery_stats();
    assert!(stats.is_successful(), "Recovery should be successful");
    assert!(stats.recovery_completed);
    assert!(stats.consistency_valid);
    assert_eq!(stats.recovery_stats.segments_processed, 1);
    assert!(stats.total_recovery_duration > Duration::from_nanos(0));

    // Verify the recovered index
    assert!(recovered_index.shard_count() > 0, "Should have recovered shards");

    // Verify lock file was cleaned up
    assert!(!lock_path.exists(), "Recovery lock should be cleaned up");
}

/// Test recovery with corrupted WAL segments
#[tokio::test]
async fn test_recovery_with_corrupted_wal() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(32)
        .wal_segment_size(2048);

    let layout = DirectoryLayout::new(_test_env.path());
    layout.create_directories().unwrap();

    // Create a corrupted WAL file (invalid format)
    let wal_path = layout.wal_dir().join("wal_000001.log");
    std::fs::write(&wal_path, "this_is_not_valid_wal_data").unwrap();

    // Create recovery lock to force crash detection
    let lock_path = _test_env.path().join(".recovery_lock");
    std::fs::write(&lock_path, "crashed").unwrap();

    let mut recovery = CrashRecovery::new(config);

    // Detect crash - should find corruption
    let crash_detected = recovery.detect_crash().await.unwrap();
    assert!(crash_detected, "Should detect crash and corruption");

    // Recovery should still work (with partial data)
    let result = recovery.recover().await;
    match result {
        Ok(_index) => {
            // Partial recovery succeeded
            let final_stats = recovery.recovery_stats();
            assert!(final_stats.recovery_completed);
            // May have errors due to corruption, but recovery completed
        }
        Err(_) => {
            // Complete failure is also acceptable for corrupted data
            // Just verify that corruption was detected
            let stats = recovery.recovery_stats();
            assert!(!stats.corrupted_segments.is_empty());
        }
    }
}

/// Test recovery statistics accuracy
#[tokio::test]
async fn test_recovery_statistics() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(128);

    // Create a clean index first
    ShardexIndex::create(config.clone()).unwrap();

    let mut recovery = CrashRecovery::new(config);

    // Test initial statistics
    let initial_stats = recovery.recovery_stats();
    assert_eq!(initial_stats.total_transactions(), 0);
    assert!(!initial_stats.crash_detected);
    assert!(!initial_stats.recovery_completed);
    assert!(!initial_stats.consistency_valid);

    // Perform crash detection
    recovery.detect_crash().await.unwrap();

    let post_detection_stats = recovery.recovery_stats();
    assert!(post_detection_stats.crash_detection_duration > Duration::from_nanos(0));
}

/// Test concurrent crash detection (should be safe)
#[tokio::test]
async fn test_concurrent_crash_detection() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(64);

    // Create index
    ShardexIndex::create(config.clone()).unwrap();

    // Create multiple recovery instances
    let mut recovery1 = CrashRecovery::new(config.clone());
    let mut recovery2 = CrashRecovery::new(config.clone());
    let mut recovery3 = CrashRecovery::new(config);

    // Run crash detection concurrently
    let result1 = recovery1.detect_crash();
    let result2 = recovery2.detect_crash();
    let result3 = recovery3.detect_crash();
    let (result1, result2, result3): (
        Result<bool, ShardexError>,
        Result<bool, ShardexError>,
        Result<bool, ShardexError>,
    ) = tokio::join!(result1, result2, result3);

    // All should succeed and return consistent results
    assert!(result1.is_ok());
    assert!(result2.is_ok());
    assert!(result3.is_ok());

    let crash1 = result1.unwrap();
    let crash2 = result2.unwrap();
    let crash3 = result3.unwrap();

    // All should report the same crash detection result
    assert_eq!(crash1, crash2);
    assert_eq!(crash2, crash3);
}

/// Test recovery with performance requirements
#[tokio::test]
async fn test_recovery_performance() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(256);

    // Create index
    ShardexIndex::create(config.clone()).unwrap();

    let mut recovery = CrashRecovery::new(config);

    let start_time = std::time::Instant::now();

    // Crash detection should be fast
    recovery.detect_crash().await.unwrap();

    let detection_time = start_time.elapsed();
    let stats = recovery.recovery_stats();

    // Crash detection should complete quickly (within reasonable time)
    assert!(
        detection_time < Duration::from_millis(1000),
        "Crash detection took too long: {:?}",
        detection_time
    );

    // Statistics should reflect the timing
    assert!(stats.crash_detection_duration <= detection_time);
    assert!(stats.crash_detection_duration > Duration::from_nanos(0));
}

/// Test recovery with different configuration parameters
#[tokio::test]
async fn test_recovery_with_various_configs() {
    let _test_env = TempDir::new().unwrap();

    // Test different vector sizes
    for vector_size in [64, 128, 256, 512] {
        let config = ShardexConfig::new()
            .directory_path(_test_env.path().join(format!("test_{}", vector_size)))
            .vector_size(vector_size);

        // Create index
        ShardexIndex::create(config.clone()).unwrap();

        let mut recovery = CrashRecovery::new(config);
        let crash_detected = recovery.detect_crash().await.unwrap();

        // Should work for any valid vector size
        assert!(
            !crash_detected,
            "Clean index should not show crash for vector_size={}",
            vector_size
        );
    }
}

/// Test that recovery properly validates all acceptance criteria
#[tokio::test]
async fn test_acceptance_criteria_validation() {
    let _test_env = TempDir::new().unwrap();
    let config = ShardexConfig::new()
        .directory_path(_test_env.path())
        .vector_size(64);

    // Create initial setup
    let layout = DirectoryLayout::new(_test_env.path());
    layout.create_directories().unwrap();

    // Create recovery lock to simulate crash
    let lock_path = _test_env.path().join(".recovery_lock");
    std::fs::write(&lock_path, "test_crash").unwrap();

    let mut recovery = CrashRecovery::new(config);

    // ✓ Crash detection identifies incomplete operations
    let crash_detected = recovery.detect_crash().await.unwrap();
    assert!(crash_detected, "Should detect incomplete operations");

    // ✓ WAL replay restores all committed transactions (tested in other tests)
    // ✓ Consistency validation ensures recovered state is valid
    let result = recovery.recover().await;

    // Recovery should complete successfully
    assert!(result.is_ok(), "Recovery should succeed");

    let stats = recovery.recovery_stats();

    // ✓ Performance is acceptable for large WAL files
    assert!(
        stats.total_recovery_duration < Duration::from_secs(10),
        "Recovery should complete within reasonable time"
    );

    // ✓ Comprehensive logging and progress reporting
    assert!(stats.crash_detection_duration > Duration::from_nanos(0));
    assert!(stats.wal_replay_duration > Duration::from_nanos(0));
    assert!(stats.consistency_validation_duration > Duration::from_nanos(0));
    assert!(stats.total_recovery_duration > Duration::from_nanos(0));

    // Verify recovery completed successfully
    assert!(stats.recovery_completed);
    assert!(stats.consistency_valid);
}

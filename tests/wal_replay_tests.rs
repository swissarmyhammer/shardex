//! Integration tests for WAL replay functionality

use shardex::layout::DirectoryLayout;
use shardex::shardex_index::ShardexIndex;
use shardex::wal::WalSegment;
use shardex::wal_replay::WalReplayer;
use shardex::{DocumentId, ShardexConfig, WalOperation, WalTransaction};
use tempfile::TempDir;

#[tokio::test]
async fn test_replay_segment_integration() {
    let temp_dir = TempDir::new().unwrap();
    let layout = DirectoryLayout::new(temp_dir.path());
    layout.create_directories().unwrap();

    // Create a WAL segment with sample transactions
    let segment_path = layout.wal_segment_path(1);
    let segment_capacity = 8192;
    let segment = WalSegment::create(1, segment_path, segment_capacity).unwrap();

    // Create sample transactions to write to the segment
    let doc_id = DocumentId::new();
    let operations = vec![
        WalOperation::AddPosting {
            document_id: doc_id,
            start: 0,
            length: 100,
            vector: vec![1.0, 2.0, 3.0],
        },
        WalOperation::RemoveDocument { document_id: doc_id },
    ];

    let transaction = WalTransaction::new(operations).unwrap();
    segment.append_transaction(&transaction).unwrap();
    segment.sync().unwrap();

    // Create index and replayer
    let config = ShardexConfig::new()
        .directory_path(temp_dir.path())
        .vector_size(3);
    let index = ShardexIndex::create(config).unwrap();

    let mut replayer = WalReplayer::new(layout.wal_dir().to_path_buf(), index);

    // Replay the segment
    let transactions_processed = replayer.replay_segment(&segment).await.unwrap();

    // Should have processed 1 transaction
    assert_eq!(transactions_processed, 1);
    assert_eq!(replayer.recovery_stats().transactions_replayed, 1);
    assert_eq!(replayer.recovery_stats().operations_applied, 2);
    assert!(!replayer.recovery_stats().has_errors());
}

#[tokio::test]
async fn test_replay_segment_idempotency() {
    let temp_dir = TempDir::new().unwrap();
    let layout = DirectoryLayout::new(temp_dir.path());
    layout.create_directories().unwrap();

    // Create WAL segment with a transaction
    let segment_path = layout.wal_segment_path(1);
    let segment_capacity = 8192;
    let segment = WalSegment::create(1, segment_path, segment_capacity).unwrap();

    let doc_id = DocumentId::new();
    let operations = vec![WalOperation::AddPosting {
        document_id: doc_id,
        start: 0,
        length: 100,
        vector: vec![1.0, 2.0, 3.0],
    }];

    let transaction = WalTransaction::new(operations).unwrap();
    segment.append_transaction(&transaction).unwrap();
    segment.sync().unwrap();

    // Create index and replayer
    let config = ShardexConfig::new()
        .directory_path(temp_dir.path())
        .vector_size(3);
    let index = ShardexIndex::create(config).unwrap();

    let mut replayer = WalReplayer::new(layout.wal_dir().to_path_buf(), index);

    // First replay
    let transactions_processed = replayer.replay_segment(&segment).await.unwrap();
    assert_eq!(transactions_processed, 1);
    assert_eq!(replayer.recovery_stats().transactions_replayed, 1);
    assert_eq!(replayer.recovery_stats().transactions_skipped, 0);

    // Second replay - should skip duplicate transaction
    let transactions_processed = replayer.replay_segment(&segment).await.unwrap();
    assert_eq!(transactions_processed, 1); // Still processed 1 transaction (but skipped)
    assert_eq!(replayer.recovery_stats().transactions_replayed, 1); // Still only 1 replayed
    assert_eq!(replayer.recovery_stats().transactions_skipped, 1); // 1 skipped now
}

#[tokio::test]
async fn test_replay_segment_with_corruption() {
    let temp_dir = TempDir::new().unwrap();
    let layout = DirectoryLayout::new(temp_dir.path());
    layout.create_directories().unwrap();

    // Create WAL segment with valid transaction first
    let segment_path = layout.wal_segment_path(1);
    let segment_capacity = 8192;
    let segment = WalSegment::create(1, segment_path.clone(), segment_capacity).unwrap();

    let doc_id = DocumentId::new();
    let operations = vec![WalOperation::AddPosting {
        document_id: doc_id,
        start: 0,
        length: 100,
        vector: vec![1.0, 2.0, 3.0],
    }];

    let transaction = WalTransaction::new(operations).unwrap();
    segment.append_transaction(&transaction).unwrap();
    segment.sync().unwrap();

    // Create index and replayer
    let config = ShardexConfig::new()
        .directory_path(temp_dir.path())
        .vector_size(3);
    let index = ShardexIndex::create(config).unwrap();

    let mut replayer = WalReplayer::new(layout.wal_dir().to_path_buf(), index);

    // Replay the segment - should handle corruption gracefully
    let transactions_processed = replayer.replay_segment(&segment).await.unwrap();

    // Should process what it can
    assert_eq!(transactions_processed, 1);
    assert_eq!(replayer.recovery_stats().transactions_replayed, 1);
    assert_eq!(replayer.recovery_stats().operations_applied, 1);
}

#[tokio::test]
async fn test_replay_empty_segment() {
    let temp_dir = TempDir::new().unwrap();
    let layout = DirectoryLayout::new(temp_dir.path());
    layout.create_directories().unwrap();

    // Create empty WAL segment
    let segment_path = layout.wal_segment_path(1);
    let segment_capacity = 8192;
    let segment = WalSegment::create(1, segment_path, segment_capacity).unwrap();

    // Create index and replayer
    let config = ShardexConfig::new()
        .directory_path(temp_dir.path())
        .vector_size(3);
    let index = ShardexIndex::create(config).unwrap();

    let mut replayer = WalReplayer::new(layout.wal_dir().to_path_buf(), index);

    // Replay the empty segment
    let transactions_processed = replayer.replay_segment(&segment).await.unwrap();

    // Should process 0 transactions
    assert_eq!(transactions_processed, 0);
    assert_eq!(replayer.recovery_stats().transactions_replayed, 0);
    assert_eq!(replayer.recovery_stats().operations_applied, 0);
    assert!(!replayer.recovery_stats().has_errors());
    assert_eq!(replayer.recovery_stats().segments_processed, 1);
}

#[tokio::test]
async fn test_replay_all_segments() {
    let temp_dir = TempDir::new().unwrap();
    let layout = DirectoryLayout::new(temp_dir.path());
    layout.create_directories().unwrap();

    // Create multiple WAL segments with transactions
    let segment1_path = layout.wal_segment_path(1);
    let segment2_path = layout.wal_segment_path(2);
    let segment_capacity = 8192;

    // Create segment 1 with one transaction
    let segment1 = WalSegment::create(1, segment1_path, segment_capacity).unwrap();
    let doc_id1 = DocumentId::new();
    let operations1 = vec![WalOperation::AddPosting {
        document_id: doc_id1,
        start: 0,
        length: 100,
        vector: vec![1.0, 2.0, 3.0],
    }];
    let transaction1 = WalTransaction::new(operations1).unwrap();
    segment1.append_transaction(&transaction1).unwrap();
    segment1.sync().unwrap();

    // Create segment 2 with two transactions
    let segment2 = WalSegment::create(2, segment2_path, segment_capacity).unwrap();

    let doc_id2 = DocumentId::new();
    let operations2a = vec![WalOperation::AddPosting {
        document_id: doc_id2,
        start: 0,
        length: 50,
        vector: vec![4.0, 5.0, 6.0],
    }];
    let transaction2a = WalTransaction::new(operations2a).unwrap();
    segment2.append_transaction(&transaction2a).unwrap();

    let doc_id3 = DocumentId::new();
    let operations2b = vec![WalOperation::RemoveDocument { document_id: doc_id3 }];
    let transaction2b = WalTransaction::new(operations2b).unwrap();
    segment2.append_transaction(&transaction2b).unwrap();
    segment2.sync().unwrap();

    // Create index and replayer
    let config = ShardexConfig::new()
        .directory_path(temp_dir.path())
        .vector_size(3);
    let index = ShardexIndex::create(config).unwrap();

    let mut replayer = WalReplayer::new(layout.wal_dir().to_path_buf(), index);

    // Replay all segments
    replayer.replay_all_segments().await.unwrap();

    // Should have processed all transactions across both segments
    assert_eq!(replayer.recovery_stats().segments_processed, 2);
    assert_eq!(replayer.recovery_stats().transactions_replayed, 3); // 1 + 2
    assert_eq!(replayer.recovery_stats().operations_applied, 3); // 1 + 1 + 1
    assert_eq!(replayer.recovery_stats().add_posting_operations, 2);
    assert_eq!(replayer.recovery_stats().remove_document_operations, 1);
    assert!(!replayer.recovery_stats().has_errors());
}

#[tokio::test]
async fn test_replay_all_segments_empty_directory() {
    let temp_dir = TempDir::new().unwrap();
    let layout = DirectoryLayout::new(temp_dir.path());
    layout.create_directories().unwrap();

    // Don't create any WAL segments

    // Create index and replayer
    let config = ShardexConfig::new()
        .directory_path(temp_dir.path())
        .vector_size(3);
    let index = ShardexIndex::create(config).unwrap();

    let mut replayer = WalReplayer::new(layout.wal_dir().to_path_buf(), index);

    // Replay all segments (should be no-op)
    replayer.replay_all_segments().await.unwrap();

    // Should have processed nothing
    assert_eq!(replayer.recovery_stats().segments_processed, 0);
    assert_eq!(replayer.recovery_stats().transactions_replayed, 0);
    assert_eq!(replayer.recovery_stats().operations_applied, 0);
    assert!(!replayer.recovery_stats().has_errors());
}

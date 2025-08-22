//! Integration tests for PostingStorage and VectorStorage alignment
//!
//! These tests validate that PostingStorage and VectorStorage work correctly
//! together for parallel access patterns, ensuring indices correspond between
//! the two storage systems.

use shardex::{PostingStorage, VectorStorage, DocumentId, ShardexError};
use tempfile::TempDir;

/// Test that PostingStorage and VectorStorage can be used together with aligned indices
#[test]
fn test_parallel_storage_alignment() -> Result<(), ShardexError> {
    let temp_dir = TempDir::new().unwrap();
    let vector_path = temp_dir.path().join("vectors.dat");
    let posting_path = temp_dir.path().join("postings.dat");

    let capacity = 100;
    let vector_dimension = 128;

    // Create both storages with the same capacity
    let mut vector_storage = VectorStorage::create(&vector_path, vector_dimension, capacity)?;
    let mut posting_storage = PostingStorage::create(&posting_path, capacity)?;

    // Add parallel entries
    let doc_ids: Vec<DocumentId> = (0..10).map(|_| DocumentId::new()).collect();
    let mut vector_indices = Vec::new();
    let mut posting_indices = Vec::new();

    for (i, &doc_id) in doc_ids.iter().enumerate() {
        // Create a unique vector for each document
        let vector: Vec<f32> = (0..vector_dimension)
            .map(|j| (i * vector_dimension + j) as f32)
            .collect();

        // Add to both storages
        let vector_idx = vector_storage.add_vector(&vector)?;
        let posting_idx = posting_storage.add_posting(doc_id, (i * 100) as u32, 50)?;

        // Indices should be aligned
        assert_eq!(vector_idx, posting_idx);
        assert_eq!(vector_idx, i);

        vector_indices.push(vector_idx);
        posting_indices.push(posting_idx);
    }

    // Verify parallel access works correctly
    for i in 0..doc_ids.len() {
        let vector = vector_storage.get_vector(vector_indices[i])?;
        let (posting_doc_id, start, length) = posting_storage.get_posting(posting_indices[i])?;

        // Verify the vector has the expected values
        assert_eq!(vector[0], (i * vector_dimension) as f32);
        
        // Verify the posting has the expected values
        assert_eq!(posting_doc_id, doc_ids[i]);
        assert_eq!(start, (i * 100) as u32);
        assert_eq!(length, 50);
    }

    Ok(())
}

/// Test that removing items maintains alignment between storages
#[test]
fn test_parallel_storage_removal_alignment() -> Result<(), ShardexError> {
    let temp_dir = TempDir::new().unwrap();
    let vector_path = temp_dir.path().join("vectors.dat");
    let posting_path = temp_dir.path().join("postings.dat");

    let capacity = 50;
    let vector_dimension = 64;

    let mut vector_storage = VectorStorage::create(&vector_path, vector_dimension, capacity)?;
    let mut posting_storage = PostingStorage::create(&posting_path, capacity)?;

    // Add entries
    let doc_ids: Vec<DocumentId> = (0..20).map(|_| DocumentId::new()).collect();
    let mut indices = Vec::new();

    for (i, &doc_id) in doc_ids.iter().enumerate() {
        let vector = vec![i as f32; vector_dimension];
        let vector_idx = vector_storage.add_vector(&vector)?;
        let posting_idx = posting_storage.add_posting(doc_id, i as u32 * 10, 25)?;
        
        assert_eq!(vector_idx, posting_idx);
        indices.push(vector_idx);
    }

    // Remove some entries from both storages (using tombstone approach)
    let remove_indices = [2, 7, 11, 15];
    for &idx in &remove_indices {
        vector_storage.remove_vector(idx)?;
        posting_storage.remove_posting(idx)?;
    }

    // Verify that removed items are marked as deleted in both storages
    for &idx in &remove_indices {
        assert!(vector_storage.is_deleted(idx)?);
        assert!(posting_storage.is_deleted(idx)?);
    }

    // Verify that non-removed items are still accessible
    for (i, &idx) in indices.iter().enumerate() {
        if !remove_indices.contains(&idx) {
            // Vector should still be accessible
            let vector = vector_storage.get_vector(idx)?;
            assert_eq!(vector[0], i as f32);

            // Posting should still be accessible
            let (posting_doc_id, start, length) = posting_storage.get_posting(idx)?;
            assert_eq!(posting_doc_id, doc_ids[i]);
            assert_eq!(start, i as u32 * 10);
            assert_eq!(length, 25);

            // Both should report as not deleted
            assert!(!vector_storage.is_deleted(idx)?);
            assert!(!posting_storage.is_deleted(idx)?);
        }
    }

    // Verify counts are consistent
    assert_eq!(vector_storage.current_count(), posting_storage.current_count());
    assert_eq!(vector_storage.active_count(), posting_storage.active_count());
    
    // Should be 20 total - 4 removed = 16 active
    assert_eq!(vector_storage.active_count(), 16);
    assert_eq!(posting_storage.active_count(), 16);

    Ok(())
}

/// Test persistence and reopening of aligned storages
#[test]
fn test_parallel_storage_persistence() -> Result<(), ShardexError> {
    let temp_dir = TempDir::new().unwrap();
    let vector_path = temp_dir.path().join("vectors_persist.dat");
    let posting_path = temp_dir.path().join("postings_persist.dat");

    let capacity = 30;
    let vector_dimension = 256;
    let doc_ids: Vec<DocumentId> = (0..15).map(|_| DocumentId::new()).collect();

    // Create and populate storages
    {
        let mut vector_storage = VectorStorage::create(&vector_path, vector_dimension, capacity)?;
        let mut posting_storage = PostingStorage::create(&posting_path, capacity)?;

        for (i, &doc_id) in doc_ids.iter().enumerate() {
            let vector: Vec<f32> = (0..vector_dimension).map(|j| (i + j) as f32).collect();
            
            let vector_idx = vector_storage.add_vector(&vector)?;
            let posting_idx = posting_storage.add_posting(doc_id, (i * 50) as u32, 30)?;
            
            assert_eq!(vector_idx, posting_idx);
        }

        // Remove some items
        vector_storage.remove_vector(5)?;
        posting_storage.remove_posting(5)?;
        vector_storage.remove_vector(10)?;
        posting_storage.remove_posting(10)?;

        vector_storage.sync()?;
        posting_storage.sync()?;
    }

    // Reopen and verify alignment is maintained
    {
        let vector_storage = VectorStorage::open(&vector_path)?;
        let posting_storage = PostingStorage::open(&posting_path)?;

        // Verify basic counts match
        assert_eq!(vector_storage.capacity(), posting_storage.capacity());
        assert_eq!(vector_storage.current_count(), posting_storage.current_count());
        assert_eq!(vector_storage.active_count(), posting_storage.active_count());

        // Verify specific data integrity
        for i in 0..doc_ids.len() {
            let vector_deleted = vector_storage.is_deleted(i)?;
            let posting_deleted = posting_storage.is_deleted(i)?;
            
            // Deletion status should match
            assert_eq!(vector_deleted, posting_deleted);

            if !vector_deleted {
                // Verify data integrity for non-deleted items
                let vector = vector_storage.get_vector(i)?;
                let (posting_doc_id, start, length) = posting_storage.get_posting(i)?;

                assert_eq!(vector[0], i as f32); // First element should match index
                assert_eq!(posting_doc_id, doc_ids[i]);
                assert_eq!(start, (i * 50) as u32);
                assert_eq!(length, 30);
            }
        }
    }

    Ok(())
}

/// Test that both storages handle capacity limits consistently
#[test]
fn test_parallel_storage_capacity_limits() -> Result<(), ShardexError> {
    let temp_dir = TempDir::new().unwrap();
    let vector_path = temp_dir.path().join("vectors_capacity.dat");
    let posting_path = temp_dir.path().join("postings_capacity.dat");

    let capacity = 5; // Small capacity for testing
    let vector_dimension = 8;

    let mut vector_storage = VectorStorage::create(&vector_path, vector_dimension, capacity)?;
    let mut posting_storage = PostingStorage::create(&posting_path, capacity)?;

    // Fill to capacity
    for i in 0..capacity {
        let vector = vec![(i + 1) as f32; vector_dimension];
        let doc_id = DocumentId::new();
        
        let vector_idx = vector_storage.add_vector(&vector)?;
        let posting_idx = posting_storage.add_posting(doc_id, i as u32 * 10, 20)?;
        
        assert_eq!(vector_idx, posting_idx);
        assert_eq!(vector_idx, i);
    }

    // Both should be full
    assert!(vector_storage.is_full());
    assert!(posting_storage.is_full());
    assert_eq!(vector_storage.remaining_capacity(), 0);
    assert_eq!(posting_storage.remaining_capacity(), 0);

    // Trying to add more should fail for both
    let overflow_vector = vec![999.0; vector_dimension];
    let overflow_doc_id = DocumentId::new();
    
    let vector_result = vector_storage.add_vector(&overflow_vector);
    let posting_result = posting_storage.add_posting(overflow_doc_id, 999, 10);
    
    assert!(vector_result.is_err());
    assert!(posting_result.is_err());

    Ok(())
}

/// Test iteration over active items in both storages
#[test]
fn test_parallel_storage_active_iteration() -> Result<(), ShardexError> {
    let temp_dir = TempDir::new().unwrap();
    let vector_path = temp_dir.path().join("vectors_iter.dat");
    let posting_path = temp_dir.path().join("postings_iter.dat");

    let capacity = 20;
    let vector_dimension = 32;

    let mut vector_storage = VectorStorage::create(&vector_path, vector_dimension, capacity)?;
    let mut posting_storage = PostingStorage::create(&posting_path, capacity)?;

    // Add items
    let doc_ids: Vec<DocumentId> = (0..15).map(|_| DocumentId::new()).collect();
    for (i, &doc_id) in doc_ids.iter().enumerate() {
        let vector = vec![i as f32; vector_dimension];
        vector_storage.add_vector(&vector)?;
        posting_storage.add_posting(doc_id, i as u32 * 100, 50)?;
    }

    // Remove some items
    let remove_indices = [1, 5, 9, 13];
    for &idx in &remove_indices {
        vector_storage.remove_vector(idx)?;
        posting_storage.remove_posting(idx)?;
    }

    // Collect active postings
    let active_postings: Result<Vec<_>, _> = posting_storage.iter_active().collect();
    let active_postings = active_postings?;

    // Should have 15 - 4 = 11 active items
    assert_eq!(active_postings.len(), 11);

    // Verify that all active postings correspond to non-deleted vectors
    for (posting_idx, doc_id, start, length) in active_postings {
        // Vector at this index should not be deleted
        assert!(!vector_storage.is_deleted(posting_idx)?);
        
        // Vector should be accessible
        let vector = vector_storage.get_vector(posting_idx)?;
        assert_eq!(vector[0], posting_idx as f32);

        // Posting data should be correct
        assert_eq!(doc_id, doc_ids[posting_idx]);
        assert_eq!(start, posting_idx as u32 * 100);
        assert_eq!(length, 50);

        // This index should not be in the removed set
        assert!(!remove_indices.contains(&posting_idx));
    }

    Ok(())
}

/// Test basic performance characteristics
#[test]
fn test_parallel_storage_basic_performance() -> Result<(), ShardexError> {
    let temp_dir = TempDir::new().unwrap();
    let vector_path = temp_dir.path().join("vectors_perf.dat");
    let posting_path = temp_dir.path().join("postings_perf.dat");

    let capacity = 100; // Small capacity for fast test
    let vector_dimension = 32;

    let mut vector_storage = VectorStorage::create(&vector_path, vector_dimension, capacity)?;
    let mut posting_storage = PostingStorage::create(&posting_path, capacity)?;

    // Add items and verify they work together
    for i in 0..capacity {
        let vector: Vec<f32> = vec![i as f32; vector_dimension];
        let doc_id = DocumentId::new();
        
        let vector_idx = vector_storage.add_vector(&vector)?;
        let posting_idx = posting_storage.add_posting(doc_id, i as u32 * 10, 50)?;
        
        assert_eq!(vector_idx, posting_idx);
    }

    // Verify both storages have the same counts
    assert_eq!(vector_storage.current_count(), posting_storage.current_count());
    assert_eq!(vector_storage.active_count(), posting_storage.active_count());
    assert_eq!(vector_storage.current_count(), capacity);

    Ok(())
}
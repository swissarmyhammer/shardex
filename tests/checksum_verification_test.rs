//! Integration tests for checksum verification functionality
//!
//! Tests the DocumentTextStorage checksum verification system to ensure
//! data integrity checking works correctly.

use shardex::document_text_storage::DocumentTextStorage;
use shardex::identifiers::DocumentId;
use tempfile::TempDir;

#[test]
fn test_checksum_verification_on_empty_storage() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    // Empty storage should pass checksum verification
    assert!(storage.verify_checksums().is_ok());
}

#[test]
fn test_checksum_verification_with_stored_data() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    // Verify checksums on empty storage first
    println!("🔍 Verifying checksums on empty storage...");
    storage.verify_checksums().expect("Empty storage checksums should be valid");

    // Store some test data
    let doc_id = DocumentId::new();
    let text = "Test document for checksum verification.";
    println!("📝 Storing text data...");
    storage.store_text(doc_id, text).unwrap();
    
    println!("🔍 Entry count after store: {}", storage.entry_count());
    println!("🔍 Total text size after store: {}", storage.total_text_size());

    // Checksum verification should pass after storing data
    match storage.verify_checksums() {
        Ok(()) => {
            println!("✅ Checksum verification passed after storing data");
        }
        Err(e) => {
            panic!("❌ Checksum verification failed: {}", e);
        }
    }
}

#[test]
fn test_checksum_verification_after_multiple_operations() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    // Perform multiple operations
    let doc1 = DocumentId::new();
    let doc2 = DocumentId::new();
    
    storage.store_text(doc1, "First document").unwrap();
    storage.verify_checksums().expect("Checksums should be valid after first store");
    
    storage.store_text(doc2, "Second document").unwrap();
    storage.verify_checksums().expect("Checksums should be valid after second store");
    
    storage.store_text(doc1, "Updated first document").unwrap();
    storage.verify_checksums().expect("Checksums should be valid after update");
}

#[test]
fn test_checksum_verification_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let doc_id = DocumentId::new();
    let text = "Text that will persist across reloads.";

    // Create storage, store data, verify checksums
    {
        let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        storage.store_text(doc_id, text).unwrap();
        storage.verify_checksums().expect("Checksums should be valid before sync");
        storage.sync().unwrap();
    }

    // Reopen storage and verify checksums are still valid
    {
        let mut storage = DocumentTextStorage::open(&temp_dir).unwrap();
        storage.verify_checksums().expect("Checksums should be valid after reload");
        
        // Verify data integrity
        assert_eq!(storage.get_text(doc_id).unwrap(), text);
    }
}

#[test]
fn test_checksum_verification_with_unicode() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    // Store Unicode text and verify checksums
    let doc_id = DocumentId::new();
    let unicode_text = "Test with 世界 emojis 🌍 and special chars àáâãäå";
    storage.store_text(doc_id, unicode_text).unwrap();

    // Checksum verification should handle Unicode text correctly
    assert!(storage.verify_checksums().is_ok());
    assert_eq!(storage.get_text(doc_id).unwrap(), unicode_text);
}
//! Unit tests for DocumentTextEntry and headers
//!
//! Tests the core data structures used in document text storage:
//! - DocumentTextEntry structure and validation
//! - TextIndexHeader structure and operations
//! - TextDataHeader structure and operations
//! - Memory layout compatibility and safety
//! - Bytemuck Pod/Zeroable implementations

use bytemuck;
use shardex::document_text_entry::{
    DocumentTextEntry, TextDataHeader, TextIndexHeader, TEXT_DATA_MAGIC, TEXT_DATA_VERSION,
    TEXT_INDEX_MAGIC, TEXT_INDEX_VERSION,
};
use shardex::error::ShardexError;
use shardex::identifiers::DocumentId;

#[test]
fn test_document_text_entry_creation() {
    let doc_id = DocumentId::new();
    let entry = DocumentTextEntry::new(doc_id, 1024, 256);

    assert_eq!(entry.document_id, doc_id);
    assert_eq!(entry.text_offset, 1024);
    assert_eq!(entry.text_length, 256);
}

#[test]
fn test_document_text_entry_validation() {
    let doc_id = DocumentId::new();

    // Valid entry
    let valid_entry = DocumentTextEntry::new(doc_id, 1024, 256);
    assert!(valid_entry.validate().is_ok());

    // Invalid entry - zero length
    let zero_length_entry = DocumentTextEntry::new(doc_id, 1024, 0);
    let result = zero_length_entry.validate();
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidInput { field, reason, .. } => {
            assert_eq!(field, "text_length");
            assert!(reason.contains("cannot be zero"));
        }
        e => panic!("Expected InvalidInput error, got {:?}", e),
    }

    // Invalid entry - too large
    let too_large_entry = DocumentTextEntry::new(doc_id, 1024, 200 * 1024 * 1024); // 200MB
    let result = too_large_entry.validate();
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::InvalidInput { field, reason, .. } => {
            assert_eq!(field, "text_length");
            assert!(reason.contains("exceeds maximum"));
        }
        e => panic!("Expected InvalidInput error, got {:?}", e),
    }

    // Invalid entry - offset + length overflow
    let overflow_entry = DocumentTextEntry::new(doc_id, u64::MAX, 1);
    let result = overflow_entry.validate();
    assert!(result.is_err());
}

#[test]
fn test_document_text_entry_helper_methods() {
    let doc_id1 = DocumentId::new();
    let doc_id2 = DocumentId::new();
    let entry = DocumentTextEntry::new(doc_id1, 1000, 500);

    // Test is_for_document
    assert!(entry.is_for_document(doc_id1));
    assert!(!entry.is_for_document(doc_id2));

    // Test end_offset
    assert_eq!(entry.end_offset(), Some(1500));

    // Test overflow protection
    let overflow_entry = DocumentTextEntry::new(doc_id1, u64::MAX, 1);
    assert_eq!(overflow_entry.end_offset(), None);
}

#[test]
fn test_document_text_entry_overlap_detection() {
    let doc_id = DocumentId::new();

    let entry1 = DocumentTextEntry::new(doc_id, 100, 50); // 100-150
    let entry2 = DocumentTextEntry::new(doc_id, 125, 50); // 125-175 (overlaps)
    let entry3 = DocumentTextEntry::new(doc_id, 200, 50); // 200-250 (no overlap)
    let entry4 = DocumentTextEntry::new(doc_id, 75, 25); // 75-100 (adjacent, no overlap)

    assert!(entry1.overlaps_with(&entry2));
    assert!(entry2.overlaps_with(&entry1));
    assert!(!entry1.overlaps_with(&entry3));
    assert!(!entry3.overlaps_with(&entry1));
    assert!(!entry1.overlaps_with(&entry4)); // Adjacent but not overlapping
    assert!(!entry4.overlaps_with(&entry1));
}

#[test]
fn test_text_index_header_creation() {
    let header = TextIndexHeader::new();

    assert_eq!(header.entry_count, 0);
    assert_eq!(header.next_entry_offset, TextIndexHeader::SIZE as u64);
    assert_eq!(header._padding, [0; 12]);
    assert_eq!(header.file_header.magic, *TEXT_INDEX_MAGIC);
    assert_eq!(header.file_header.version, TEXT_INDEX_VERSION);
    assert!(header.is_empty());
}

#[test]
fn test_text_index_header_validation() {
    let mut header = TextIndexHeader::new();

    // Valid header
    assert!(header.validate().is_ok());
    assert!(header.validate_magic().is_ok());

    // Test invalid offset
    header.next_entry_offset = (TextIndexHeader::SIZE - 1) as u64; // Too small
    let result = header.validate();
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::Corruption(msg) => {
            assert!(msg.contains("Invalid next_entry_offset"));
        }
        e => panic!("Expected Corruption error, got {:?}", e),
    }

    // Reset to valid state and test padding
    header.next_entry_offset = TextIndexHeader::SIZE as u64;
    header._padding[0] = 1; // Invalid padding
    let result = header.validate();
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::Corruption(msg) => {
            assert!(msg.contains("padding is not zero"));
        }
        e => panic!("Expected Corruption error, got {:?}", e),
    }
}

#[test]
fn test_text_index_header_operations() {
    let mut header = TextIndexHeader::new();

    // Initially empty
    assert!(header.is_empty());
    assert_eq!(header.total_entries_size(), 0);

    // Add entries
    header.add_entry();
    assert_eq!(header.entry_count, 1);
    assert_eq!(
        header.next_entry_offset,
        TextIndexHeader::SIZE as u64 + DocumentTextEntry::SIZE as u64
    );
    assert!(!header.is_empty());
    assert_eq!(header.total_entries_size(), DocumentTextEntry::SIZE as u64);

    header.add_entry();
    assert_eq!(header.entry_count, 2);
    assert_eq!(
        header.next_entry_offset,
        TextIndexHeader::SIZE as u64 + 2 * DocumentTextEntry::SIZE as u64
    );
    assert_eq!(
        header.total_entries_size(),
        2 * DocumentTextEntry::SIZE as u64
    );
}

#[test]
fn test_text_index_header_offset_calculations() {
    let header = TextIndexHeader::new();

    assert_eq!(header.offset_for_entry(0), TextIndexHeader::SIZE as u64);
    assert_eq!(
        header.offset_for_entry(1),
        TextIndexHeader::SIZE as u64 + DocumentTextEntry::SIZE as u64
    );
    assert_eq!(
        header.offset_for_entry(10),
        TextIndexHeader::SIZE as u64 + 10 * DocumentTextEntry::SIZE as u64
    );

    assert_eq!(header.next_entry_offset(), TextIndexHeader::SIZE as u64);
}

#[test]
fn test_text_data_header_creation() {
    let header = TextDataHeader::new();

    assert_eq!(header.total_text_size, 0);
    assert_eq!(header.next_text_offset, TextDataHeader::SIZE as u64);
    assert_eq!(header._padding, [0; 8]);
    assert_eq!(header.file_header.magic, *TEXT_DATA_MAGIC);
    assert_eq!(header.file_header.version, TEXT_DATA_VERSION);
    assert!(header.is_empty());
}

#[test]
fn test_text_data_header_validation() {
    let mut header = TextDataHeader::new();

    // Valid header
    assert!(header.validate().is_ok());
    assert!(header.validate_magic().is_ok());

    // Test invalid offset
    header.next_text_offset = (TextDataHeader::SIZE - 1) as u64; // Too small
    let result = header.validate();
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::Corruption(msg) => {
            assert!(msg.contains("Invalid next_text_offset"));
        }
        e => panic!("Expected Corruption error, got {:?}", e),
    }

    // Reset to valid state and test padding
    header.next_text_offset = TextDataHeader::SIZE as u64;
    header._padding[0] = 1; // Invalid padding
    let result = header.validate();
    assert!(result.is_err());
    match result.unwrap_err() {
        ShardexError::Corruption(msg) => {
            assert!(msg.contains("padding is not zero"));
        }
        e => panic!("Expected Corruption error, got {:?}", e),
    }
}

#[test]
fn test_text_data_header_operations() {
    let mut header = TextDataHeader::new();

    // Initially empty
    assert!(header.is_empty());
    assert_eq!(header.utilization_ratio(), 0.0);

    // Add text
    header.add_text(512);
    assert_eq!(header.total_text_size, 512);
    assert_eq!(
        header.next_text_offset,
        TextDataHeader::SIZE as u64 + 512 + 8
    ); // +8 for length prefixes
    assert!(!header.is_empty());

    let utilization = header.utilization_ratio();
    assert!(utilization > 0.0 && utilization <= 1.0);

    // Add more text
    header.add_text(256);
    assert_eq!(header.total_text_size, 768);
    assert_eq!(
        header.next_text_offset,
        TextDataHeader::SIZE as u64 + 768 + 16
    ); // +16 for two sets of length prefixes

    let new_utilization = header.utilization_ratio();
    // Utilization might decrease due to overhead (length prefixes) relative to text size
    // But it should be reasonable (not zero and not over 100%)
    assert!(
        new_utilization > 0.0 && new_utilization <= 1.0,
        "New utilization {} should be between 0.0 and 1.0",
        new_utilization
    );
}

#[test]
fn test_text_data_header_utilization_calculation() {
    let mut header = TextDataHeader::new();

    // Empty header
    assert_eq!(header.utilization_ratio(), 0.0);

    // Add text to get realistic utilization
    header.total_text_size = 1000;
    header.next_text_offset = TextDataHeader::SIZE as u64 + 1500; // includes overhead

    let expected_ratio = 1000.0 / 1500.0; // total_text_size / (next_text_offset - header_size)
    let actual_ratio = header.utilization_ratio();
    assert!((actual_ratio - expected_ratio).abs() < 0.001);

    // Full utilization (theoretical maximum)
    header.total_text_size = 1500;
    header.next_text_offset = TextDataHeader::SIZE as u64 + 1500;
    assert!((header.utilization_ratio() - 1.0).abs() < 0.001);
}

#[test]
fn test_memory_layout_consistency() {
    // Ensure structures maintain expected memory layout
    assert_eq!(std::mem::size_of::<DocumentTextEntry>(), 32);
    assert_eq!(std::mem::align_of::<DocumentTextEntry>(), 16);

    // Verify SIZE constants match actual sizes
    assert_eq!(
        TextIndexHeader::SIZE,
        std::mem::size_of::<TextIndexHeader>()
    );
    assert_eq!(TextDataHeader::SIZE, std::mem::size_of::<TextDataHeader>());
    assert_eq!(
        DocumentTextEntry::SIZE,
        std::mem::size_of::<DocumentTextEntry>()
    );

    // Verify sizes are multiples of alignment (important for arrays)
    assert_eq!(
        TextIndexHeader::SIZE % std::mem::align_of::<TextIndexHeader>(),
        0
    );
    assert_eq!(
        TextDataHeader::SIZE % std::mem::align_of::<TextDataHeader>(),
        0
    );
    assert_eq!(
        DocumentTextEntry::SIZE % std::mem::align_of::<DocumentTextEntry>(),
        0
    );

    // Verify headers are reasonably sized
    assert!(TextIndexHeader::SIZE >= 80 + 4 + 8); // FileHeader + entry_count + next_entry_offset
    assert!(TextDataHeader::SIZE >= 80 + 8 + 8); // FileHeader + total_text_size + next_text_offset
}

#[test]
fn test_bytemuck_pod_compatibility() {
    let doc_id = DocumentId::new();
    let index_header = TextIndexHeader::new();
    let data_header = TextDataHeader::new();
    let entry = DocumentTextEntry::new(doc_id, 1024, 512);

    // Test Pod trait - should be able to cast to bytes
    let index_bytes: &[u8] = bytemuck::bytes_of(&index_header);
    let data_bytes: &[u8] = bytemuck::bytes_of(&data_header);
    let entry_bytes: &[u8] = bytemuck::bytes_of(&entry);

    assert_eq!(index_bytes.len(), TextIndexHeader::SIZE);
    assert_eq!(data_bytes.len(), TextDataHeader::SIZE);
    assert_eq!(entry_bytes.len(), DocumentTextEntry::SIZE);

    // Test round-trip conversion
    let index_restored: TextIndexHeader = bytemuck::pod_read_unaligned(index_bytes);
    let data_restored: TextDataHeader = bytemuck::pod_read_unaligned(data_bytes);
    let entry_restored: DocumentTextEntry = bytemuck::pod_read_unaligned(entry_bytes);

    // Compare the important fields (timestamps may differ)
    assert_eq!(index_header.entry_count, index_restored.entry_count);
    assert_eq!(
        index_header.next_entry_offset,
        index_restored.next_entry_offset
    );
    assert_eq!(index_header._padding, index_restored._padding);
    assert_eq!(
        index_header.file_header.magic,
        index_restored.file_header.magic
    );
    assert_eq!(
        index_header.file_header.version,
        index_restored.file_header.version
    );

    assert_eq!(data_header.total_text_size, data_restored.total_text_size);
    assert_eq!(data_header.next_text_offset, data_restored.next_text_offset);
    assert_eq!(data_header._padding, data_restored._padding);
    assert_eq!(
        data_header.file_header.magic,
        data_restored.file_header.magic
    );
    assert_eq!(
        data_header.file_header.version,
        data_restored.file_header.version
    );

    assert_eq!(entry, entry_restored);
}

#[test]
fn test_bytemuck_zeroable_compatibility() {
    let zero_index: TextIndexHeader = bytemuck::Zeroable::zeroed();
    let zero_data: TextDataHeader = bytemuck::Zeroable::zeroed();
    let zero_entry: DocumentTextEntry = bytemuck::Zeroable::zeroed();

    assert_eq!(zero_index.entry_count, 0);
    assert_eq!(zero_index.next_entry_offset, 0);
    assert_eq!(zero_index._padding, [0; 12]);

    assert_eq!(zero_data.total_text_size, 0);
    assert_eq!(zero_data.next_text_offset, 0);
    assert_eq!(zero_data._padding, [0; 8]);

    assert_eq!(zero_entry.document_id.raw(), 0);
    assert_eq!(zero_entry.text_offset, 0);
    assert_eq!(zero_entry.text_length, 0);
}

#[test]
fn test_memory_safety_with_arrays() {
    // Test that structures can be used safely in arrays
    let doc_id = DocumentId::new();
    let entries = [
        DocumentTextEntry::new(doc_id, 0, 100),
        DocumentTextEntry::new(doc_id, 100, 200),
        DocumentTextEntry::new(doc_id, 300, 150),
    ];

    // Convert array to bytes and back
    let entries_bytes: &[u8] = bytemuck::cast_slice(&entries);
    let restored_entries: &[DocumentTextEntry] = bytemuck::cast_slice(entries_bytes);

    assert_eq!(entries.len(), restored_entries.len());
    for (original, restored) in entries.iter().zip(restored_entries.iter()) {
        assert_eq!(original, restored);
    }
}

#[test]
fn test_headers_with_data_checksum() {
    let test_data = b"test data for checksum calculation";

    // Create headers with data for checksum
    let index_header = TextIndexHeader::new_with_data(test_data);
    let data_header = TextDataHeader::new_with_data(test_data);

    // Checksums should validate against the same data
    assert!(index_header.validate_checksum(test_data).is_ok());
    assert!(data_header.validate_checksum(test_data).is_ok());

    // Checksums should fail against different data
    let other_data = b"different test data";
    assert!(index_header.validate_checksum(other_data).is_err());
    assert!(data_header.validate_checksum(other_data).is_err());
}

#[test]
fn test_header_checksum_updates() {
    let initial_data = b"initial data";
    let updated_data = b"updated data with different content";

    let mut index_header = TextIndexHeader::new_with_data(initial_data);
    let mut data_header = TextDataHeader::new_with_data(initial_data);

    // Initially should validate against initial data
    assert!(index_header.validate_checksum(initial_data).is_ok());
    assert!(data_header.validate_checksum(initial_data).is_ok());

    // Update checksums
    index_header.update_checksum(updated_data);
    data_header.update_checksum(updated_data);

    // Should now validate against updated data
    assert!(index_header.validate_checksum(updated_data).is_ok());
    assert!(data_header.validate_checksum(updated_data).is_ok());

    // Should no longer validate against initial data
    assert!(index_header.validate_checksum(initial_data).is_err());
    assert!(data_header.validate_checksum(initial_data).is_err());
}

#[test]
fn test_constants_and_magic_bytes() {
    assert_eq!(TEXT_INDEX_MAGIC, b"TIDX");
    assert_eq!(TEXT_DATA_MAGIC, b"TDAT");
    assert_eq!(TEXT_INDEX_VERSION, 1);
    assert_eq!(TEXT_DATA_VERSION, 1);

    // Test magic byte validation
    let index_header = TextIndexHeader::new();
    let data_header = TextDataHeader::new();

    assert_eq!(index_header.file_header.magic, *TEXT_INDEX_MAGIC);
    assert_eq!(data_header.file_header.magic, *TEXT_DATA_MAGIC);

    assert!(index_header.validate_magic().is_ok());
    assert!(data_header.validate_magic().is_ok());
}

#[test]
fn test_default_implementations() {
    let default_index = TextIndexHeader::default();
    let default_data = TextDataHeader::default();
    let new_index = TextIndexHeader::new();
    let new_data = TextDataHeader::new();

    // Compare structure fields (timestamps may differ)
    assert_eq!(default_index.entry_count, new_index.entry_count);
    assert_eq!(default_index.next_entry_offset, new_index.next_entry_offset);
    assert_eq!(default_index._padding, new_index._padding);
    assert_eq!(default_index.file_header.magic, new_index.file_header.magic);
    assert_eq!(
        default_index.file_header.version,
        new_index.file_header.version
    );

    assert_eq!(default_data.total_text_size, new_data.total_text_size);
    assert_eq!(default_data.next_text_offset, new_data.next_text_offset);
    assert_eq!(default_data._padding, new_data._padding);
    assert_eq!(default_data.file_header.magic, new_data.file_header.magic);
    assert_eq!(
        default_data.file_header.version,
        new_data.file_header.version
    );
}

#[test]
fn test_boundary_conditions() {
    let doc_id = DocumentId::new();

    // Test minimum valid entry
    let min_entry = DocumentTextEntry::new(doc_id, 0, 1);
    assert!(min_entry.validate().is_ok());

    // Test maximum valid entry size (just under the limit)
    let max_valid_size = 100 * 1024 * 1024 - 1; // Just under 100MB
    let max_entry = DocumentTextEntry::new(doc_id, 0, max_valid_size);
    assert!(max_entry.validate().is_ok());

    // Test exactly at the limit
    let limit_size = 100 * 1024 * 1024;
    let limit_entry = DocumentTextEntry::new(doc_id, 0, limit_size);
    match limit_entry.validate() {
        Ok(_) => {} // Expected success
        Err(e) => panic!(
            "Limit entry validation failed: {:?}, size was {}",
            e, limit_size
        ),
    }

    // Test just over the limit
    let over_limit_entry = DocumentTextEntry::new(doc_id, 0, 100 * 1024 * 1024 + 1);
    assert!(over_limit_entry.validate().is_err());
}

#[test]
fn test_offset_boundary_conditions() {
    let doc_id = DocumentId::new();

    // Test large but valid offset (within 1TB file limit)
    let max_offset = 10_u64.pow(12) - 1000; // 1TB limit minus buffer for length
    let entry = DocumentTextEntry::new(doc_id, max_offset, 500);
    match entry.validate() {
        Ok(_) => assert!(entry.end_offset().is_some()),
        Err(e) => panic!("Expected validation to succeed but got error: {:?}", e),
    }

    // Test offset that would cause overflow
    let overflow_entry = DocumentTextEntry::new(doc_id, u64::MAX - 100, 200);
    assert!(overflow_entry.validate().is_err());
    assert!(overflow_entry.end_offset().is_none());
}

#[test]
fn test_header_version_validation() {
    let index_header = TextIndexHeader::new();
    let data_header = TextDataHeader::new();

    // Should validate current versions
    assert!(index_header
        .file_header
        .validate_version(TEXT_INDEX_VERSION, TEXT_INDEX_VERSION)
        .is_ok());
    assert!(data_header
        .file_header
        .validate_version(TEXT_DATA_VERSION, TEXT_DATA_VERSION)
        .is_ok());

    // Should reject versions outside valid range
    assert!(index_header
        .file_header
        .validate_version(TEXT_INDEX_VERSION + 1, TEXT_INDEX_VERSION + 2)
        .is_err());
    assert!(data_header
        .file_header
        .validate_version(TEXT_DATA_VERSION + 1, TEXT_DATA_VERSION + 2)
        .is_err());
}

#[test]
fn test_comprehensive_entry_validation_edge_cases() {
    let doc_id = DocumentId::new();

    // Test various invalid combinations
    let test_cases = vec![
        (0, 0, true),                  // Zero length - should fail
        (1000, 0, true),               // Zero length - should fail
        (u64::MAX, 1, true),           // Overflow - should fail
        (u64::MAX - 1, 2, true),       // Overflow - should fail
        (0, 200 * 1024 * 1024, true),  // Too large - should fail
        (0, 1, false),                 // Valid minimum - should pass
        (1000, 100, false),            // Valid normal - should pass
        (0, 100 * 1024 * 1024, false), // Valid maximum - should pass
    ];

    for (offset, length, should_fail) in test_cases {
        let entry = DocumentTextEntry::new(doc_id, offset, length);
        let result = entry.validate();

        if should_fail {
            assert!(
                result.is_err(),
                "Expected validation failure for offset={}, length={}",
                offset,
                length
            );
        } else {
            assert!(
                result.is_ok(),
                "Expected validation success for offset={}, length={}",
                offset,
                length
            );
        }
    }
}

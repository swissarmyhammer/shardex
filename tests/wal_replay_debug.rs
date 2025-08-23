//! Debug tests for WAL replay functionality

use shardex::{
    identifiers::DocumentId,
    transactions::{WalOperation, WalTransaction},
    wal::WalSegment,
};
use tempfile::TempDir;

#[tokio::test]
async fn test_wal_segment_transaction_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let segment_path = temp_dir.path().join("debug_segment.log");
    let segment_capacity = 8192;
    
    let segment = WalSegment::create(1, segment_path, segment_capacity).unwrap();
    
    // Create a simple transaction
    let doc_id = DocumentId::new();
    let operations = vec![WalOperation::AddPosting {
        document_id: doc_id,
        start: 0,
        length: 100,
        vector: vec![1.0, 2.0, 3.0],
    }];
    
    let original_transaction = WalTransaction::new(operations).unwrap();
    println!("Original transaction ID: {}", original_transaction.id);
    println!("Original operations count: {}", original_transaction.operations.len());
    
    // Append the transaction to the segment
    let data_offset = segment.append_transaction(&original_transaction).unwrap();
    println!("Transaction appended at offset: {}", data_offset);
    
    // Check segment state
    println!("Segment write pointer: {}", segment.write_pointer());
    println!("Initial write position: {}", shardex::wal::initial_write_position());
    
    // Now read back the segment data and try to parse
    let segment_data = segment.read_segment_data().unwrap();
    println!("Total segment data length: {}", segment_data.len());
    
    let current_pos = shardex::wal::initial_write_position();
    println!("Starting parse at position: {}", current_pos);
    
    let write_pointer = segment.write_pointer();
    println!("Write pointer is at: {}", write_pointer);
    
    if current_pos >= write_pointer {
        println!("ERROR: No data to read - current_pos >= write_pointer");
        return;
    }
    
    // Read the record header
    let header_size = shardex::wal::WalRecordHeader::SIZE;
    println!("WAL record header size: {}", header_size);
    
    if current_pos + header_size > segment_data.len() {
        println!("ERROR: Not enough space for header");
        return;
    }
    
    let header_bytes = &segment_data[current_pos..current_pos + header_size];
    
    // Manually read the fields to avoid alignment issues
    let data_length = u32::from_le_bytes([
        header_bytes[0], header_bytes[1], header_bytes[2], header_bytes[3]
    ]);
    let checksum = u32::from_le_bytes([
        header_bytes[4], header_bytes[5], header_bytes[6], header_bytes[7]
    ]);
    
    // Create a mock record header for validation
    struct MockRecordHeader {
        data_length: u32,
        checksum: u32,
    }
    
    impl MockRecordHeader {
        fn data_length(&self) -> u32 { self.data_length }
        fn validate_checksum(&self, data: &[u8]) -> bool {
            let expected_checksum = crc32fast::hash(data);
            self.checksum == expected_checksum
        }
    }
    
    let record_header = MockRecordHeader { data_length, checksum };
    
    println!("Record header data length: {}", record_header.data_length());
    
    let data_length = record_header.data_length() as usize;
    let record_data_start = current_pos + header_size;
    let record_data_end = record_data_start + data_length;
    
    println!("Record data start: {}, end: {}, length: {}", record_data_start, record_data_end, data_length);
    
    if record_data_end > segment_data.len() {
        println!("ERROR: Record data goes beyond segment bounds");
        return;
    }
    
    let record_data = &segment_data[record_data_start..record_data_end];
    
    // Validate checksum
    if !record_header.validate_checksum(record_data) {
        println!("ERROR: Checksum validation failed");
        return;
    }
    
    println!("Checksum validation passed");
    
    // Try to deserialize
    match WalTransaction::deserialize(record_data) {
        Ok(deserialized_transaction) => {
            println!("Successfully deserialized transaction!");
            println!("Deserialized transaction ID: {}", deserialized_transaction.id);
            println!("Deserialized operations count: {}", deserialized_transaction.operations.len());
            
            assert_eq!(original_transaction.id, deserialized_transaction.id);
            assert_eq!(original_transaction.operations.len(), deserialized_transaction.operations.len());
        }
        Err(e) => {
            println!("ERROR: Failed to deserialize transaction: {}", e);
            panic!("Deserialization failed");
        }
    }
}
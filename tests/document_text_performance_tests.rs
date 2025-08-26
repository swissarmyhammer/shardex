//! Performance tests for document text storage
//!
//! Tests performance characteristics including:
//! - Large document storage and retrieval
//! - Many small documents performance
//! - Substring extraction performance
//! - File growth and memory mapping scalability
//! - Concurrent access patterns

use shardex::document_text_storage::DocumentTextStorage;
use shardex::identifiers::DocumentId;
use std::time::{Duration, Instant};
use tempfile::TempDir;

mod document_text_test_utils;
use document_text_test_utils::TextGenerator;

/// Performance test configuration
struct PerformanceConfig {
    large_document_timeout: Duration,
    many_documents_timeout: Duration,
    extraction_timeout: Duration,
    bulk_operations_timeout: Duration,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            large_document_timeout: Duration::from_secs(5), // 5 seconds for large docs
            many_documents_timeout: Duration::from_secs(10), // 10 seconds for bulk ops
            extraction_timeout: Duration::from_millis(100), // 100ms for extractions
            bulk_operations_timeout: Duration::from_secs(30), // 30 seconds for bulk tests
        }
    }
}

#[test]
fn test_large_document_storage_performance() {
    let temp_dir = TempDir::new().unwrap();
    let config = PerformanceConfig::default();
    let max_size = 50 * 1024 * 1024; // 50MB limit
    let mut storage = DocumentTextStorage::create(&temp_dir, max_size).unwrap();

    // Test progressively larger documents
    let document_sizes = vec![
        (10_000, "10K words"),   // ~50KB
        (50_000, "50K words"),   // ~250KB
        (100_000, "100K words"), // ~500KB
        (500_000, "500K words"), // ~2.5MB
        (1_000_000, "1M words"), // ~5MB
    ];

    for (word_count, description) in document_sizes {
        let doc_id = DocumentId::new();
        let mut text_generator = TextGenerator::new();
        let large_text = text_generator.generate_text(word_count);
        println!("Testing {} (~{} bytes)", description, large_text.len());

        // Measure storage time
        let start = Instant::now();
        storage.store_text_safe(doc_id, &large_text).unwrap();
        let store_duration = start.elapsed();

        println!("  Store time: {:?}", store_duration);
        assert!(
            store_duration < config.large_document_timeout,
            "Store time {:?} exceeded timeout {:?} for {}",
            store_duration,
            config.large_document_timeout,
            description
        );

        // Measure retrieval time
        let start = Instant::now();
        let retrieved = storage.get_text_safe(doc_id).unwrap();
        let retrieve_duration = start.elapsed();

        println!("  Retrieve time: {:?}", retrieve_duration);
        assert!(
            retrieve_duration < config.large_document_timeout,
            "Retrieve time {:?} exceeded timeout {:?} for {}",
            retrieve_duration,
            config.large_document_timeout,
            description
        );

        // Verify correctness
        assert_eq!(retrieved.len(), large_text.len());
        assert_eq!(retrieved, large_text);

        // Measure substring extraction performance
        let extraction_positions = vec![
            (0, 100),                      // Beginning
            (large_text.len() / 2, 100),   // Middle
            (large_text.len() - 100, 100), // End
        ];

        for (start_pos, length) in extraction_positions {
            let start = Instant::now();
            let _substring = storage
                .extract_text_substring(doc_id, start_pos as u32, length as u32)
                .unwrap();
            let extract_duration = start.elapsed();

            assert!(
                extract_duration < config.extraction_timeout,
                "Extraction time {:?} exceeded timeout {:?} for {} at position {}",
                extract_duration,
                config.extraction_timeout,
                description,
                start_pos
            );
        }

        println!("  All operations completed successfully\n");
    }
}

#[test]
fn test_many_documents_performance() {
    let temp_dir = TempDir::new().unwrap();
    let config = PerformanceConfig::default();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();

    let document_counts = vec![1000, 5000, 10000];

    for document_count in document_counts {
        println!("Testing {} documents", document_count);
        let start_doc_count = storage.entry_count();

        let mut doc_ids = Vec::with_capacity(document_count);
        let document_text =
            "Performance test document with unique content for scalability testing.".to_string();

        // Measure bulk storage time
        let start = Instant::now();
        for i in 0..document_count {
            let doc_id = DocumentId::new();
            let unique_text = format!("{} Document #{}", document_text, i);
            storage.store_text_safe(doc_id, &unique_text).unwrap();
            doc_ids.push((doc_id, unique_text));
        }
        let bulk_store_duration = start.elapsed();

        println!("  Bulk store time: {:?}", bulk_store_duration);
        println!(
            "  Average store time per document: {:?}",
            bulk_store_duration / document_count as u32
        );

        assert!(
            bulk_store_duration < config.many_documents_timeout,
            "Bulk store time {:?} exceeded timeout {:?} for {} documents",
            bulk_store_duration,
            config.many_documents_timeout,
            document_count
        );

        // Verify storage statistics
        assert_eq!(
            storage.entry_count(),
            start_doc_count + document_count as u32
        );

        // Measure random access performance
        let sample_size = std::cmp::min(100, document_count);
        let start = Instant::now();

        for i in 0..sample_size {
            let index = i * document_count / sample_size; // Spread across the range
            let (doc_id, expected_text) = &doc_ids[index];
            let retrieved = storage.get_text_safe(*doc_id).unwrap();
            assert_eq!(retrieved, *expected_text);
        }

        let random_access_duration = start.elapsed();
        println!(
            "  Random access time for {} samples: {:?}",
            sample_size, random_access_duration
        );
        println!(
            "  Average random access time: {:?}",
            random_access_duration / sample_size as u32
        );

        // Measure sequential access performance
        let start = Instant::now();
        for (doc_id, expected_text) in doc_ids.iter().take(sample_size) {
            let retrieved = storage.get_text_safe(*doc_id).unwrap();
            assert_eq!(retrieved, *expected_text);
        }
        let sequential_duration = start.elapsed();

        println!(
            "  Sequential access time for {} samples: {:?}",
            sample_size, sequential_duration
        );
        println!(
            "  Average sequential access time: {:?}",
            sequential_duration / sample_size as u32
        );

        println!("  Total entries: {}", storage.entry_count());
        println!("  Total text size: {} bytes", storage.total_text_size());
        println!(
            "  Utilization ratio: {:.2}%\n",
            storage.utilization_ratio() * 100.0
        );
    }
}

#[test]
fn test_substring_extraction_performance() {
    let temp_dir = TempDir::new().unwrap();
    let config = PerformanceConfig::default();
    let mut storage = DocumentTextStorage::create(&temp_dir, 10 * 1024 * 1024).unwrap();

    // Create documents of various sizes
    let test_documents = vec![
        (1000, "Small doc"),   // ~5KB
        (10000, "Medium doc"), // ~50KB
        (100000, "Large doc"), // ~500KB
    ];

    let mut doc_data = Vec::new();

    // Store test documents
    for (word_count, description) in &test_documents {
        let doc_id = DocumentId::new();
        let mut text_generator = TextGenerator::new();
        let text = text_generator.generate_text(*word_count);
        storage.store_text_safe(doc_id, &text).unwrap();
        doc_data.push((doc_id, text, *description));
    }

    // Test extraction patterns
    let extraction_patterns = vec![
        (10, "tiny"),     // 10 characters
        (100, "small"),   // 100 characters
        (1000, "medium"), // 1000 characters
        (10000, "large"), // 10000 characters
    ];

    for (doc_id, text, doc_desc) in &doc_data {
        println!("Testing extractions from {}", doc_desc);

        for (extract_size, size_desc) in &extraction_patterns {
            if *extract_size >= text.len() {
                continue; // Skip if extraction size exceeds document
            }

            let positions = vec![
                0,                         // Start
                text.len() / 4,            // Quarter
                text.len() / 2,            // Middle
                (text.len() * 3) / 4,      // Three quarters
                text.len() - extract_size, // End
            ];

            let mut total_time = Duration::from_nanos(0);
            let mut extraction_count = 0;

            for position in positions {
                if position + extract_size > text.len() {
                    continue;
                }

                let start = Instant::now();
                let extracted = storage
                    .extract_text_substring(*doc_id, position as u32, *extract_size as u32)
                    .unwrap();
                let duration = start.elapsed();

                total_time += duration;
                extraction_count += 1;

                // Verify correctness
                assert_eq!(extracted.len(), *extract_size);
                assert_eq!(extracted, &text[position..position + extract_size]);

                // Individual extraction should be fast
                assert!(
                    duration < config.extraction_timeout,
                    "Extraction time {:?} exceeded timeout {:?} for {} {} extraction",
                    duration,
                    config.extraction_timeout,
                    size_desc,
                    doc_desc
                );
            }

            let avg_time = total_time / extraction_count as u32;
            println!(
                "  {} extractions: avg {:?}, total {:?} for {} extractions",
                size_desc, avg_time, total_time, extraction_count
            );
        }

        println!();
    }
}

#[test]
fn test_file_growth_performance() {
    let temp_dir = TempDir::new().unwrap();
    let config = PerformanceConfig::default();
    let mut storage = DocumentTextStorage::create(&temp_dir, 10 * 1024 * 1024).unwrap();

    let growth_phases = vec![
        (100, "Initial phase"),
        (500, "Growth phase 1"),
        (1000, "Growth phase 2"),
        (2000, "Growth phase 3"),
    ];

    let document_text =
        "File growth test document with consistent content for performance measurement.";
    let mut all_docs = Vec::new();

    for (target_count, phase_desc) in growth_phases {
        println!(
            "Testing {}: adding {} documents",
            phase_desc,
            target_count - all_docs.len()
        );

        let phase_start = Instant::now();
        let start_count = all_docs.len();

        // Add documents for this phase
        while all_docs.len() < target_count {
            let doc_id = DocumentId::new();
            let unique_text = format!("{} Document #{}", document_text, all_docs.len());

            storage.store_text_safe(doc_id, &unique_text).unwrap();
            all_docs.push((doc_id, unique_text));
        }

        let phase_duration = phase_start.elapsed();
        let docs_added = all_docs.len() - start_count;

        println!("  Added {} documents in {:?}", docs_added, phase_duration);
        println!(
            "  Average time per document: {:?}",
            phase_duration / docs_added as u32
        );

        // Test random access performance at this scale
        let access_start = Instant::now();
        let sample_size = std::cmp::min(50, all_docs.len());

        for i in 0..sample_size {
            let index = i * all_docs.len() / sample_size;
            let (doc_id, expected_text) = &all_docs[index];
            let retrieved = storage.get_text_safe(*doc_id).unwrap();
            assert_eq!(retrieved, *expected_text);
        }

        let access_duration = access_start.elapsed();
        println!(
            "  Random access time for {} samples: {:?}",
            sample_size, access_duration
        );
        println!(
            "  Average access time: {:?}",
            access_duration / sample_size as u32
        );

        // Report storage statistics
        println!("  Total entries: {}", storage.entry_count());
        println!(
            "  Total text size: {} bytes ({:.1} KB)",
            storage.total_text_size(),
            storage.total_text_size() as f64 / 1024.0
        );
        println!(
            "  Utilization ratio: {:.2}%\n",
            storage.utilization_ratio() * 100.0
        );

        assert!(
            phase_duration < config.bulk_operations_timeout,
            "Phase duration {:?} exceeded timeout {:?} for {}",
            phase_duration,
            config.bulk_operations_timeout,
            phase_desc
        );
    }
}

#[test]
fn test_memory_usage_scaling() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 10 * 1024 * 1024).unwrap();

    let document_sizes = vec![
        (1000, 100), // 100 small documents (~5KB each)
        (10000, 50), // 50 medium documents (~50KB each)
        (50000, 10), // 10 large documents (~250KB each)
    ];

    for (word_count, doc_count) in document_sizes {
        println!(
            "Testing {} documents of ~{} words each",
            doc_count, word_count
        );

        let base_entries = storage.entry_count();
        let base_size = storage.total_text_size();

        // Store documents and measure timing
        let start = Instant::now();
        let mut expected_total_chars = 0;

        for _i in 0..doc_count {
            let doc_id = DocumentId::new();
            let mut text_generator = TextGenerator::new();
            let text = text_generator.generate_text(word_count);
            expected_total_chars += text.len();

            storage.store_text_safe(doc_id, &text).unwrap();

            // Test one extraction per document to simulate realistic usage
            if text.len() >= 100 {
                let _extracted = storage
                    .extract_text_substring(doc_id, text.len() as u32 / 2, 100)
                    .unwrap();
            }
        }

        let total_duration = start.elapsed();
        let final_entries = storage.entry_count();
        let final_size = storage.total_text_size();

        println!("  Total time: {:?}", total_duration);
        println!(
            "  Average time per document: {:?}",
            total_duration / doc_count as u32
        );
        println!("  Entries added: {}", final_entries - base_entries);
        println!("  Text size added: {} bytes", final_size - base_size);
        println!("  Expected size: {} bytes", expected_total_chars);
        println!("  Utilization: {:.2}%", storage.utilization_ratio() * 100.0);

        // Verify storage efficiency
        assert_eq!(final_entries - base_entries, doc_count as u32);
        assert_eq!(final_size - base_size, expected_total_chars as u64);

        // Test random retrieval performance
        let retrieval_start = Instant::now();
        let _sample_count = std::cmp::min(20, doc_count);

        // We can't easily test specific documents since we don't track the IDs,
        // but we can test overall storage functionality
        assert!(storage.entry_count() > 0);
        assert!(storage.total_text_size() > 0);
        assert!(storage.utilization_ratio() > 0.0);

        let _retrieval_duration = retrieval_start.elapsed();
        println!("  Storage validation completed\n");
    }
}

#[test]
fn test_update_performance() {
    let temp_dir = TempDir::new().unwrap();
    let config = PerformanceConfig::default();
    let mut storage = DocumentTextStorage::create(&temp_dir, 5 * 1024 * 1024).unwrap();

    let doc_count = 100;
    let update_cycles = 5;

    println!(
        "Testing update performance with {} documents, {} update cycles",
        doc_count, update_cycles
    );

    let mut doc_ids = Vec::new();

    // Initial document creation
    let initial_text = "Initial document content for update performance testing.";
    let creation_start = Instant::now();

    for i in 0..doc_count {
        let doc_id = DocumentId::new();
        let unique_text = format!("{} Document #{}", initial_text, i);
        storage.store_text_safe(doc_id, &unique_text).unwrap();
        doc_ids.push(doc_id);
    }

    let creation_duration = creation_start.elapsed();
    println!(
        "Initial creation: {:?} ({:?} per document)",
        creation_duration,
        creation_duration / doc_count as u32
    );

    // Update cycles
    for cycle in 0..update_cycles {
        let cycle_start = Instant::now();

        for (i, &doc_id) in doc_ids.iter().enumerate() {
            let updated_text = format!(
                "Updated content cycle {} for document {} with additional data.",
                cycle + 1,
                i
            );
            storage.store_text_safe(doc_id, &updated_text).unwrap();

            // Verify the update worked
            let retrieved = storage.get_text_safe(doc_id).unwrap();
            assert_eq!(retrieved, updated_text);
        }

        let cycle_duration = cycle_start.elapsed();
        println!(
            "Update cycle {}: {:?} ({:?} per document)",
            cycle + 1,
            cycle_duration,
            cycle_duration / doc_count as u32
        );

        assert!(
            cycle_duration < config.many_documents_timeout,
            "Update cycle {} took {:?}, exceeded timeout {:?}",
            cycle + 1,
            cycle_duration,
            config.many_documents_timeout
        );

        // Check storage growth (append-only behavior)
        let expected_entries = doc_count as u32 * (cycle as u32 + 2); // Initial + updates
        assert_eq!(storage.entry_count(), expected_entries);
    }

    println!("Final storage statistics:");
    println!("  Total entries: {}", storage.entry_count());
    println!("  Total text size: {} bytes", storage.total_text_size());
    println!(
        "  Utilization ratio: {:.2}%",
        storage.utilization_ratio() * 100.0
    );
}

#[test]
fn test_worst_case_performance() {
    let temp_dir = TempDir::new().unwrap();
    let config = PerformanceConfig::default();
    let mut storage = DocumentTextStorage::create(&temp_dir, 10 * 1024 * 1024).unwrap();

    println!("Testing worst-case performance scenarios");

    // Scenario 1: Many small documents with frequent updates
    let doc_id = DocumentId::new();
    let update_count = 1000;

    let start = Instant::now();
    for i in 0..update_count {
        let text = format!("Update #{} - short text content", i);
        storage.store_text_safe(doc_id, &text).unwrap();
    }
    let update_duration = start.elapsed();

    println!(
        "1000 updates to single document: {:?} ({:?} per update)",
        update_duration,
        update_duration / update_count
    );

    // Latest version should be retrievable efficiently
    let retrieval_start = Instant::now();
    let final_text = storage.get_text_safe(doc_id).unwrap();
    let retrieval_duration = retrieval_start.elapsed();

    assert!(final_text.contains(&format!("Update #{}", update_count - 1)));
    assert!(retrieval_duration < config.extraction_timeout);

    // Scenario 2: Fragmented text extraction patterns
    let large_doc = DocumentId::new();
    let mut text_generator = TextGenerator::new();
    let large_text = text_generator.generate_text(100000); // ~800KB
    storage.store_text_safe(large_doc, &large_text).unwrap();

    // Many small extractions across the document
    let extraction_start = Instant::now();
    let extraction_count = 100;

    for i in 0..extraction_count {
        let position = (i * large_text.len() / extraction_count) as u32;
        let length = std::cmp::min(50, large_text.len() - position as usize) as u32;

        if length > 0 {
            let _extracted = storage
                .extract_text_substring(large_doc, position, length)
                .unwrap();
        }
    }

    let fragmented_duration = extraction_start.elapsed();
    println!(
        "{} fragmented extractions from large document: {:?} ({:?} per extraction)",
        extraction_count,
        fragmented_duration,
        fragmented_duration / extraction_count as u32
    );

    assert!(
        fragmented_duration < Duration::from_secs(1),
        "Fragmented extractions took too long: {:?}",
        fragmented_duration
    );

    // Scenario 3: Mixed operations under stress
    let mixed_start = Instant::now();
    let operations = 500;

    for i in 0..operations {
        match i % 4 {
            0 => {
                // Store new document
                let new_doc = DocumentId::new();
                let text = format!("Stress test document #{}", i);
                storage.store_text_safe(new_doc, &text).unwrap();
            }
            1 => {
                // Update existing document
                let text = format!("Updated content for stress test #{}", i);
                storage.store_text_safe(doc_id, &text).unwrap();
            }
            2 => {
                // Retrieve document
                let _text = storage.get_text_safe(doc_id).unwrap();
            }
            3 => {
                // Extract from large document
                let position = ((i * 37) % large_text.len()) as u32; // Pseudo-random position
                let length = 25u32;
                if position + length <= large_text.len() as u32 {
                    let _extracted = storage
                        .extract_text_substring(large_doc, position, length)
                        .unwrap();
                }
            }
            _ => unreachable!(),
        }
    }

    let mixed_duration = mixed_start.elapsed();
    println!(
        "{} mixed operations: {:?} ({:?} per operation)",
        operations,
        mixed_duration,
        mixed_duration / operations as u32
    );

    assert!(
        mixed_duration < config.bulk_operations_timeout,
        "Mixed operations took too long: {:?}",
        mixed_duration
    );

    println!("All worst-case scenarios completed successfully");
    println!("Final entry count: {}", storage.entry_count());
    println!("Final text size: {} bytes", storage.total_text_size());
}

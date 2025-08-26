//! Advanced document text storage example
//!
//! This example demonstrates advanced features:
//! - Batch document operations
//! - Document updates and versioning
//! - Comprehensive error handling patterns
//! - Performance optimization techniques
//! - Large document handling

use shardex::{DocumentId, Posting, Shardex, ShardexConfig, ShardexImpl, ShardexError};
use std::collections::HashMap;
use std::error::Error;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Document Text Storage - Advanced Example");
    println!("================================================");

    // Create a temporary directory for this example
    let temp_dir = std::env::temp_dir().join("shardex_text_advanced_example");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    let config = ShardexConfig::new()
        .directory_path(&temp_dir)
        .vector_size(256)
        .max_document_text_size(50 * 1024 * 1024) // 50MB per document
        .shard_size(50000)
        .batch_write_interval_ms(50);

    let mut index = ShardexImpl::create(config).await?;

    // Run different advanced scenarios
    batch_document_processing(&mut index).await?;
    document_updates_example(&mut index).await?;
    error_handling_examples(&index).await?;
    performance_example(&mut index).await?;
    large_document_example(&mut index).await?;

    // Clean up
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\\nAdvanced example completed successfully!");

    Ok(())
}

async fn batch_document_processing(index: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Batch Document Processing ===");
    
    // Prepare a collection of documents with metadata
    let documents = vec![
        ("Introduction to machine learning concepts and algorithms for beginners.", 
         "ML", vec!["machine", "learning", "algorithms", "concepts"]),
        ("Deep learning and neural networks: architectural patterns and training strategies.", 
         "DL", vec!["deep", "learning", "neural", "networks", "architecture"]),
        ("Natural language processing techniques for text analysis and understanding.", 
         "NLP", vec!["natural", "language", "processing", "text", "analysis"]),
        ("Computer vision applications in autonomous vehicles and robotics systems.", 
         "CV", vec!["computer", "vision", "autonomous", "vehicles", "robotics"]),
        ("Reinforcement learning algorithms for decision making in dynamic environments.", 
         "RL", vec!["reinforcement", "learning", "decision", "making", "dynamic"]),
    ];

    let start = Instant::now();
    let mut doc_metadata = HashMap::new();
    
    for (i, (text, category, keywords)) in documents.iter().enumerate() {
        let doc_id = DocumentId::from_raw((i + 100) as u128);
        
        // Create overlapping postings based on keywords
        let mut postings = Vec::new();
        let mut current_pos = 0u32;
        
        for keyword in keywords {
            // Find keyword positions in text
            let text_lower = text.to_lowercase();
            let keyword_lower = keyword.to_lowercase();
            
            if let Some(pos) = text_lower.find(&keyword_lower) {
                let posting = Posting {
                    document_id: doc_id,
                    start: pos as u32,
                    length: keyword.len() as u32,
                    vector: generate_keyword_vector(keyword, 256),
                };
                postings.push(posting);
            }
        }
        
        // Add a full-document posting
        let full_doc_posting = Posting {
            document_id: doc_id,
            start: 0,
            length: text.len() as u32,
            vector: generate_document_vector(text, 256),
        };
        postings.push(full_doc_posting);
        
        // Store document atomically
        index.replace_document_with_postings(doc_id, text.to_string(), postings).await?;
        
        // Track metadata
        doc_metadata.insert(doc_id, (category, keywords.clone()));
        
        println!("  Processed document {}: {} ({} postings)", 
                 i + 1, category, keywords.len() + 1);
    }
    
    let batch_duration = start.elapsed();
    println!("Batch processing of {} documents completed in {:?}", 
             documents.len(), batch_duration);
    
    // Verify all documents are accessible
    println!("\\nVerifying batch processing results:");
    for (doc_id, (category, _)) in &doc_metadata {
        match index.get_document_text(*doc_id).await {
            Ok(text) => {
                println!("  ✓ {} document: {} characters", 
                         category, text.len());
            }
            Err(e) => {
                println!("  ✗ Error accessing {} document: {}", category, e);
            }
        }
    }
    
    Ok(())
}

async fn document_updates_example(index: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Document Updates and Versioning ===");
    
    let doc_id = DocumentId::from_raw(200);
    
    // Version 1: Initial document
    let v1_text = "Original research paper on quantum computing applications.";
    let v1_postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 8, // "Original"
            vector: generate_keyword_vector("original", 256),
        },
        Posting {
            document_id: doc_id,
            start: 9,
            length: 8, // "research"
            vector: generate_keyword_vector("research", 256),
        },
        Posting {
            document_id: doc_id,
            start: 27,
            length: 16, // "quantum computing"
            vector: generate_keyword_vector("quantum computing", 256),
        }
    ];
    
    println!("Storing version 1...");
    index.replace_document_with_postings(doc_id, v1_text.to_string(), v1_postings).await?;
    
    let retrieved_v1 = index.get_document_text(doc_id).await?;
    println!("Version 1: '{}'", retrieved_v1);
    
    // Version 2: Updated document with more content
    let v2_text = "Updated comprehensive research paper on quantum computing applications in cryptography and optimization algorithms.";
    let v2_postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 7, // "Updated"
            vector: generate_keyword_vector("updated", 256),
        },
        Posting {
            document_id: doc_id,
            start: 8,
            length: 13, // "comprehensive"
            vector: generate_keyword_vector("comprehensive", 256),
        },
        Posting {
            document_id: doc_id,
            start: 22,
            length: 8, // "research"
            vector: generate_keyword_vector("research", 256),
        },
        Posting {
            document_id: doc_id,
            start: 40,
            length: 16, // "quantum computing"
            vector: generate_keyword_vector("quantum computing", 256),
        },
        Posting {
            document_id: doc_id,
            start: 71,
            length: 12, // "cryptography"
            vector: generate_keyword_vector("cryptography", 256),
        },
        Posting {
            document_id: doc_id,
            start: 88,
            length: 12, // "optimization"
            vector: generate_keyword_vector("optimization", 256),
        }
    ];
    
    println!("Updating to version 2...");
    index.replace_document_with_postings(doc_id, v2_text.to_string(), v2_postings).await?;
    
    let retrieved_v2 = index.get_document_text(doc_id).await?;
    println!("Version 2: '{}'", retrieved_v2);
    
    // Verify we get the latest version
    assert_eq!(retrieved_v2, v2_text);
    println!("✓ Document versioning working correctly");
    
    // Version 3: Major restructure
    let v3_text = "Quantum Computing in Practice: A comprehensive guide covering theoretical foundations, practical implementations, and real-world applications in secure communications, financial modeling, and scientific computing.";
    let v3_postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 17, // "Quantum Computing"
            vector: generate_keyword_vector("quantum computing", 256),
        },
        Posting {
            document_id: doc_id,
            start: 21,
            length: 8, // "Practice"
            vector: generate_keyword_vector("practice", 256),
        },
        Posting {
            document_id: doc_id,
            start: 50,
            length: 11, // "theoretical"
            vector: generate_keyword_vector("theoretical", 256),
        },
        Posting {
            document_id: doc_id,
            start: 74,
            length: 9, // "practical"
            vector: generate_keyword_vector("practical", 256),
        },
        Posting {
            document_id: doc_id,
            start: 84,
            length: 15, // "implementations"
            vector: generate_keyword_vector("implementations", 256),
        }
    ];
    
    println!("Updating to version 3 (major restructure)...");
    index.replace_document_with_postings(doc_id, v3_text.to_string(), v3_postings).await?;
    
    let retrieved_v3 = index.get_document_text(doc_id).await?;
    println!("Version 3: '{}'", 
             if retrieved_v3.len() > 80 { 
                 format!("{}...", &retrieved_v3[..80])
             } else {
                 retrieved_v3.clone()
             });
    
    println!("Document update sequence completed successfully");
    Ok(())
}

async fn error_handling_examples(index: &ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Comprehensive Error Handling ===");
    
    // Test 1: Document not found
    let nonexistent_doc = DocumentId::from_raw(9999);
    println!("Testing document not found...");
    match index.get_document_text(nonexistent_doc).await {
        Ok(_) => println!("  ✗ Unexpected success for nonexistent document"),
        Err(ShardexError::DocumentTextNotFound { document_id }) => {
            println!("  ✓ Correctly handled missing document: {}", document_id);
        }
        Err(e) => println!("  ? Unexpected error type: {}", e),
    }
    
    // Test 2: Invalid range extraction
    println!("Testing invalid range extraction...");
    let doc_id = DocumentId::from_raw(200); // Should exist from previous example
    
    // First, get the actual document length to test edge cases
    let actual_text = index.get_document_text(doc_id).await?;
    let doc_length = actual_text.len() as u32;
    println!("  Document length: {} characters", doc_length);
    
    let invalid_ranges = vec![
        (doc_length, 10, "start beyond document end"),
        (0, doc_length + 100, "length beyond document end"),
        (doc_length - 5, 20, "range extends beyond document"),
    ];
    
    for (start, length, description) in invalid_ranges {
        let invalid_posting = Posting {
            document_id: doc_id,
            start,
            length,
            vector: generate_keyword_vector("test", 256),
        };
        
        match index.extract_text(&invalid_posting).await {
            Ok(text) => println!("  ✗ Unexpected success for {}: '{}'", description, text),
            Err(ShardexError::InvalidRange { start, length, document_length }) => {
                println!("  ✓ Correctly handled {}: {}..{} for document length {}", 
                         description, start, start + length, document_length);
            }
            Err(e) => println!("  ? Unexpected error for {}: {}", description, e),
        }
    }
    
    // Test 3: Valid edge cases (should succeed)
    println!("Testing valid edge cases...");
    let edge_cases = vec![
        (0, 1, "single character at start"),
        (doc_length - 1, 1, "single character at end"),
        (0, doc_length, "entire document"),
        (doc_length / 2, 1, "single character in middle"),
    ];
    
    for (start, length, description) in edge_cases {
        let edge_posting = Posting {
            document_id: doc_id,
            start,
            length,
            vector: generate_keyword_vector("test", 256),
        };
        
        match index.extract_text(&edge_posting).await {
            Ok(text) => println!("  ✓ {}: '{}' (length: {})", 
                                 description, 
                                 if text.len() > 20 { format!("{}...", &text[..20]) } else { text },
                                 length),
            Err(e) => println!("  ✗ Unexpected error for {}: {}", description, e),
        }
    }
    
    Ok(())
}

async fn performance_example(index: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Performance Measurement ===");
    
    // Create a moderately large document for testing
    let large_text = format!("{} ", "Lorem ipsum dolor sit amet, consectetur adipiscing elit.").repeat(200);
    let doc_id = DocumentId::from_raw(300);
    
    println!("Testing with document size: {} bytes ({:.1} KB)", 
             large_text.len(), large_text.len() as f64 / 1024.0);
    
    // Create multiple postings across the document
    let mut postings = Vec::new();
    let words = ["Lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit"];
    
    for (i, word) in words.iter().enumerate() {
        // Find multiple occurrences of each word
        let mut start_pos = 0;
        let mut occurrence_count = 0;
        
        while let Some(pos) = large_text[start_pos..].find(word) {
            let actual_pos = start_pos + pos;
            
            postings.push(Posting {
                document_id: doc_id,
                start: actual_pos as u32,
                length: word.len() as u32,
                vector: generate_keyword_vector(word, 256),
            });
            
            start_pos = actual_pos + word.len();
            occurrence_count += 1;
            
            if occurrence_count >= 10 { // Limit to 10 occurrences per word
                break;
            }
        }
    }
    
    println!("Created {} postings for performance testing", postings.len());
    
    // Measure storage performance
    let start = Instant::now();
    index.replace_document_with_postings(doc_id, large_text.clone(), postings.clone()).await?;
    let store_time = start.elapsed();
    
    // Measure retrieval performance
    let start = Instant::now();
    let retrieved = index.get_document_text(doc_id).await?;
    let retrieve_time = start.elapsed();
    
    // Measure multiple extraction performance
    let start = Instant::now();
    let mut extraction_count = 0;
    for posting in postings.iter().take(20) { // Test 20 extractions
        let _extracted = index.extract_text(posting).await?;
        extraction_count += 1;
    }
    let extract_time = start.elapsed();
    
    // Search performance
    let search_vector = generate_keyword_vector("ipsum consectetur", 256);
    let start = Instant::now();
    let search_results = index.search(&search_vector, 10, None).await?;
    let search_time = start.elapsed();
    
    println!("\\nPerformance Results:");
    println!("  Store time: {:?} ({:.1} MB/s)", 
             store_time, 
             (large_text.len() as f64 / 1024.0 / 1024.0) / store_time.as_secs_f64());
    println!("  Retrieve time: {:?} ({:.1} MB/s)", 
             retrieve_time,
             (retrieved.len() as f64 / 1024.0 / 1024.0) / retrieve_time.as_secs_f64());
    println!("  Extract time: {:?} ({} extractions, {:.1} μs/extraction)", 
             extract_time, 
             extraction_count,
             extract_time.as_micros() as f64 / extraction_count as f64);
    println!("  Search time: {:?} ({} results found)", search_time, search_results.len());
    
    // Validate correctness
    assert_eq!(retrieved.len(), large_text.len());
    assert_eq!(retrieved, large_text);
    println!("  ✓ Data integrity verified");
    
    Ok(())
}

async fn large_document_example(index: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Large Document Handling ===");
    
    // Create a very large document (several MB)
    let base_content = "The field of artificial intelligence has evolved rapidly over the past decade, with breakthrough developments in machine learning, deep learning, natural language processing, computer vision, and robotics. These technologies are now being applied across industries including healthcare, finance, transportation, education, and entertainment. The impact of AI on society continues to grow as algorithms become more sophisticated and computational power increases. ";
    
    let large_document = base_content.repeat(10000); // ~3.7MB document
    let doc_id = DocumentId::from_raw(400);
    
    println!("Creating large document: {:.2} MB ({} characters)", 
             large_document.len() as f64 / 1024.0 / 1024.0, 
             large_document.len());
    
    // Create strategic postings across the large document
    let keywords = ["artificial", "intelligence", "machine", "learning", "deep", "natural", 
                   "language", "computer", "vision", "robotics", "healthcare", "finance"];
    
    let mut postings = Vec::new();
    
    for keyword in &keywords {
        // Find first few occurrences of each keyword
        let mut start_pos = 0;
        let mut count = 0;
        
        while let Some(pos) = large_document[start_pos..].find(keyword) {
            let actual_pos = start_pos + pos;
            
            postings.push(Posting {
                document_id: doc_id,
                start: actual_pos as u32,
                length: keyword.len() as u32,
                vector: generate_keyword_vector(keyword, 256),
            });
            
            start_pos = actual_pos + keyword.len();
            count += 1;
            
            if count >= 5 { // 5 occurrences per keyword
                break;
            }
        }
    }
    
    println!("Generated {} strategic postings", postings.len());
    
    // Store the large document
    let start = Instant::now();
    index.replace_document_with_postings(doc_id, large_document.clone(), postings.clone()).await?;
    let store_duration = start.elapsed();
    
    println!("Large document stored in {:?} ({:.1} MB/s)", 
             store_duration,
             (large_document.len() as f64 / 1024.0 / 1024.0) / store_duration.as_secs_f64());
    
    // Test random access patterns
    println!("Testing random access patterns...");
    let test_positions = [
        (0, 100),                                    // Beginning
        (large_document.len() / 4, 200),            // First quarter
        (large_document.len() / 2, 150),            // Middle
        (large_document.len() * 3 / 4, 180),       // Third quarter
        (large_document.len() - 100, 100),         // End
    ];
    
    for (i, (start, length)) in test_positions.iter().enumerate() {
        let test_posting = Posting {
            document_id: doc_id,
            start: *start as u32,
            length: *length as u32,
            vector: generate_keyword_vector("test", 256),
        };
        
        let extract_start = Instant::now();
        match index.extract_text(&test_posting).await {
            Ok(extracted) => {
                let extract_time = extract_start.elapsed();
                println!("  Position {}: {} characters in {:?} ({}..{})", 
                         i + 1, extracted.len(), extract_time, start, start + length);
            }
            Err(e) => println!("  Position {}: Error - {}", i + 1, e),
        }
    }
    
    // Test search performance on large document
    let search_terms = ["artificial intelligence", "machine learning", "computer vision"];
    
    println!("Testing search performance on large document...");
    for term in &search_terms {
        let query_vector = generate_keyword_vector(term, 256);
        let search_start = Instant::now();
        let results = index.search(&query_vector, 5, None).await?;
        let search_time = search_start.elapsed();
        
        println!("  '{}': {} results in {:?}", term, results.len(), search_time);
        
        // Extract text from first result if available
        if let Some(result) = results.first() {
            let result_posting = Posting {
                document_id: result.document_id,
                start: result.start,
                length: result.length,
                vector: result.vector.clone(),
            };
            
            if let Ok(result_text) = index.extract_text(&result_posting).await {
                println!("    Best match: '{}' (score: {:.4})", result_text, result.similarity_score);
            }
        }
    }
    
    Ok(())
}

/// Generate a keyword-based vector representation
fn generate_keyword_vector(keyword: &str, dimension: usize) -> Vec<f32> {
    let mut vector = vec![0.0; dimension];
    let keyword_lower = keyword.to_lowercase();
    
    // Multi-layered hash-based generation for better distribution
    for (i, word) in keyword_lower.split_whitespace().enumerate() {
        let primary_hash = simple_hash(word);
        let secondary_hash = simple_hash(&format!("{}:{}", word, i));
        
        // Primary features
        let index1 = (primary_hash % dimension as u32) as usize;
        vector[index1] += 1.0 / (i + 1) as f32;
        
        // Secondary features for better discrimination
        let index2 = (secondary_hash % dimension as u32) as usize;
        vector[index2] += 0.5 / (i + 1) as f32;
        
        // Character-level features
        for (j, ch) in word.chars().enumerate() {
            let char_hash = (ch as u32).wrapping_mul(31).wrapping_add(j as u32);
            let char_index = (char_hash % dimension as u32) as usize;
            vector[char_index] += 0.1 / ((j + 1) * (i + 1)) as f32;
        }
    }
    
    // Normalize
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut vector {
            *value /= magnitude;
        }
    }
    
    vector
}

/// Generate a document-level vector representation
fn generate_document_vector(text: &str, dimension: usize) -> Vec<f32> {
    let mut vector = vec![0.0; dimension];
    let lowercase_text = text.to_lowercase();
    let words: Vec<&str> = lowercase_text.split_whitespace().collect();
    
    // Document-level features with TF weighting
    let mut word_counts = HashMap::new();
    for word in &words {
        *word_counts.entry(word.to_string()).or_insert(0) += 1;
    }
    
    for (word, count) in word_counts {
        let tf_weight = (count as f32).log10() + 1.0; // Simple TF weighting
        let hash = simple_hash(&word);
        let index = (hash % dimension as u32) as usize;
        vector[index] += tf_weight / words.len() as f32;
    }
    
    // Add document structure features
    let sentences = text.split('.').count();
    let avg_sentence_length = words.len() as f32 / sentences as f32;
    
    // Encode structural information
    if dimension > 10 {
        vector[dimension - 1] = (sentences as f32).log10() / 10.0;
        vector[dimension - 2] = (avg_sentence_length).log10() / 10.0;
    }
    
    // Normalize
    let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if magnitude > 0.0 {
        for value in &mut vector {
            *value /= magnitude;
        }
    }
    
    vector
}

/// Simple hash function for demonstration purposes
fn simple_hash(s: &str) -> u32 {
    s.bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    })
}
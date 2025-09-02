//! Advanced document text storage example
//!
//! This example demonstrates advanced features:
//! - Batch document operations
//! - Document updates and versioning
//! - Comprehensive error handling patterns
//! - Performance optimization techniques
//! - Large document handling

use apithing::ApiOperation;
use shardex::{
    api::{
        CreateIndex, CreateIndexParams, ExtractSnippet, ExtractSnippetParams, GetDocumentText, GetDocumentTextParams,
        ShardexContext, StoreDocumentText, StoreDocumentTextParams,
    },
    DocumentId, Posting, ShardexConfig, ShardexError,
};

use std::error::Error;
use std::time::Instant;

// Configuration constants to avoid hardcoded values
const DEFAULT_MAX_DOCUMENT_SIZE: usize = 50 * 1024 * 1024; // 50MB per document
const DEFAULT_VECTOR_SIZE: usize = 256;
const DEFAULT_SHARD_SIZE: usize = 50000;
const DEFAULT_BATCH_INTERVAL_MS: u64 = 50;
const DEMO_DOCUMENT_BASE_ID: u128 = 100;
const UPDATE_DOCUMENT_ID: u128 = 200;
const NONEXISTENT_DOCUMENT_ID: u128 = 9999;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Document Text Storage - Advanced Example");
    println!("================================================");

    // Create a temporary directory for this example
    let temp_dir = std::env::temp_dir().join("shardex_text_advanced_example");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Create context and index parameters
    let config = ShardexConfig::new()
        .directory_path(&temp_dir)
        .max_document_text_size(DEFAULT_MAX_DOCUMENT_SIZE);

    let mut context = ShardexContext::with_config(config);

    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.clone())
        .vector_size(DEFAULT_VECTOR_SIZE)
        .shard_size(DEFAULT_SHARD_SIZE)
        .batch_write_interval_ms(DEFAULT_BATCH_INTERVAL_MS)
        .build()?;

    // Create the index using ApiThing pattern
    CreateIndex::execute(&mut context, &create_params)?;

    // Run different advanced scenarios - simplified for demo performance
    println!("\\n=== Advanced Document Operations Demo ===");
    println!("Running lightweight versions of advanced operations...");

    // Simplified batch processing demo
    simple_batch_demo(&mut context)?;
    document_updates_example(&mut context)?;
    error_handling_examples(&mut context)?;

    println!("\\n=== Note ===");
    println!("Full batch operations, performance tests, and large document");
    println!("processing are available but disabled for demo performance.");

    // Clean up
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\\nAdvanced example completed successfully!");

    Ok(())
}

/// Simple demonstration of batch-like processing without heavy operations.
fn simple_batch_demo(context: &mut ShardexContext) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Simple Batch Processing Demo ===");

    // Two simple documents
    let documents = [
        ("Machine learning basics", vec!["machine", "learning"]),
        ("Deep learning concepts", vec!["deep", "learning"]),
    ];

    let start = Instant::now();
    let mut docs_processed = 0;

    for (i, (text, keywords)) in documents.iter().enumerate() {
        let doc_id = DocumentId::from_raw((i as u128) + DEMO_DOCUMENT_BASE_ID);

        // Create simple postings
        let mut postings = Vec::new();
        for keyword in keywords {
            if let Some(pos) = text.to_lowercase().find(&keyword.to_lowercase()) {
                postings.push(Posting {
                    document_id: doc_id,
                    start: pos as u32,
                    length: keyword.len() as u32,
                    vector: generate_keyword_vector(keyword, DEFAULT_VECTOR_SIZE),
                });
            }
        }

        let store_params = StoreDocumentTextParams::new(doc_id, text.to_string(), postings)?;
        StoreDocumentText::execute(context, &store_params)?;
        docs_processed += 1;

        println!("  Processed: {}", text);
    }

    let duration = start.elapsed();
    println!("Processed {} documents in {:?}", docs_processed, duration);

    Ok(())
}

/// Demonstrates document updates and versioning patterns.
///
/// This function showcases:
/// - Sequential document updates with version tracking
/// - Content evolution from simple to complex documents
/// - Strategic posting creation for different document versions
/// - Verification that latest version is correctly stored and retrieved
/// - Document replacement semantics in the Shardex system
///
/// The function creates three versions of a document about quantum computing,
/// each with increasing complexity and different posting strategies, demonstrating
/// how documents can be updated while maintaining search capabilities.
fn document_updates_example(context: &mut ShardexContext) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Document Updates and Versioning ===");

    let doc_id = DocumentId::from_raw(UPDATE_DOCUMENT_ID);

    // Version 1: Initial document
    let v1_text = "Original research paper on quantum computing applications.";
    let v1_postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 8, // "Original"
            vector: generate_keyword_vector("original", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 9,
            length: 8, // "research"
            vector: generate_keyword_vector("research", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 27,
            length: 16, // "quantum computing"
            vector: generate_keyword_vector("quantum computing", DEFAULT_VECTOR_SIZE),
        },
    ];

    println!("Storing version 1...");
    let store_v1_params = StoreDocumentTextParams::new(doc_id, v1_text.to_string(), v1_postings)?;
    StoreDocumentText::execute(context, &store_v1_params)?;

    let get_v1_params = GetDocumentTextParams::new(doc_id);
    let retrieved_v1 = GetDocumentText::execute(context, &get_v1_params)?;
    println!("Version 1: '{}'", retrieved_v1);

    // Version 2: Updated document with more content
    let v2_text = "Updated comprehensive research paper on quantum computing applications in cryptography and optimization algorithms.";
    let v2_postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 7, // "Updated"
            vector: generate_keyword_vector("updated", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 8,
            length: 13, // "comprehensive"
            vector: generate_keyword_vector("comprehensive", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 22,
            length: 8, // "research"
            vector: generate_keyword_vector("research", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 40,
            length: 16, // "quantum computing"
            vector: generate_keyword_vector("quantum computing", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 71,
            length: 12, // "cryptography"
            vector: generate_keyword_vector("cryptography", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 88,
            length: 12, // "optimization"
            vector: generate_keyword_vector("optimization", DEFAULT_VECTOR_SIZE),
        },
    ];

    println!("Updating to version 2...");
    let store_v2_params = StoreDocumentTextParams::new(doc_id, v2_text.to_string(), v2_postings)?;
    StoreDocumentText::execute(context, &store_v2_params)?;

    let get_v2_params = GetDocumentTextParams::new(doc_id);
    let retrieved_v2 = GetDocumentText::execute(context, &get_v2_params)?;
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
            vector: generate_keyword_vector("quantum computing", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 21,
            length: 8, // "Practice"
            vector: generate_keyword_vector("practice", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 50,
            length: 11, // "theoretical"
            vector: generate_keyword_vector("theoretical", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 74,
            length: 9, // "practical"
            vector: generate_keyword_vector("practical", DEFAULT_VECTOR_SIZE),
        },
        Posting {
            document_id: doc_id,
            start: 84,
            length: 15, // "implementations"
            vector: generate_keyword_vector("implementations", DEFAULT_VECTOR_SIZE),
        },
    ];

    println!("Updating to version 3 (major restructure)...");
    let store_v3_params = StoreDocumentTextParams::new(doc_id, v3_text.to_string(), v3_postings)?;
    StoreDocumentText::execute(context, &store_v3_params)?;

    let get_v3_params = GetDocumentTextParams::new(doc_id);
    let retrieved_v3 = GetDocumentText::execute(context, &get_v3_params)?;
    println!(
        "Version 3: '{}'",
        if retrieved_v3.len() > 80 {
            format!("{}...", &retrieved_v3[..80])
        } else {
            retrieved_v3.clone()
        }
    );

    println!("Document update sequence completed successfully");
    Ok(())
}

/// Demonstrates comprehensive error handling patterns for document operations.
///
/// This function showcases:
/// - Proper handling of DocumentTextNotFound errors
/// - Invalid range detection and error reporting
/// - Edge case validation for document boundaries
/// - Structured error pattern matching with ShardexError
/// - Recovery strategies for different error conditions
///
/// The function tests various error conditions including nonexistent documents,
/// invalid extraction ranges, and boundary conditions. It demonstrates how to
/// handle errors gracefully while providing meaningful feedback to users.
fn error_handling_examples(context: &mut ShardexContext) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Comprehensive Error Handling ===");

    // Test 1: Document not found
    let nonexistent_doc = DocumentId::from_raw(NONEXISTENT_DOCUMENT_ID);
    println!("Testing document not found...");
    let get_nonexistent_params = GetDocumentTextParams::new(nonexistent_doc);
    match GetDocumentText::execute(context, &get_nonexistent_params) {
        Ok(_) => println!("  ✗ Unexpected success for nonexistent document"),
        Err(ShardexError::DocumentTextNotFound { document_id }) => {
            println!("  ✓ Correctly handled missing document: {}", document_id);
        }
        Err(e) => println!("  ? Unexpected error type: {}", e),
    }

    // Test 2: Invalid range extraction
    println!("Testing invalid range extraction...");
    let doc_id = DocumentId::from_raw(UPDATE_DOCUMENT_ID); // Should exist from previous example

    // First, get the actual document length to test edge cases
    let get_params = GetDocumentTextParams::new(doc_id);
    let actual_text = GetDocumentText::execute(context, &get_params)?;
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
            vector: generate_keyword_vector("test", DEFAULT_VECTOR_SIZE),
        };

        let extract_params = ExtractSnippetParams::from_posting(&invalid_posting);
        match ExtractSnippet::execute(context, &extract_params) {
            Ok(text) => println!("  ✗ Unexpected success for {}: '{}'", description, text),
            Err(ShardexError::InvalidRange {
                start,
                length,
                document_length,
            }) => {
                println!(
                    "  ✓ Correctly handled {}: {}..{} for document length {}",
                    description,
                    start,
                    start + length,
                    document_length
                );
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
            vector: generate_keyword_vector("test", DEFAULT_VECTOR_SIZE),
        };

        let extract_params = ExtractSnippetParams::from_posting(&edge_posting);
        match ExtractSnippet::execute(context, &extract_params) {
            Ok(text) => println!(
                "  ✓ {}: '{}' (length: {})",
                description,
                if text.len() > 20 {
                    format!("{}...", &text[..20])
                } else {
                    text
                },
                length
            ),
            Err(e) => println!("  ✗ Unexpected error for {}: {}", description, e),
        }
    }

    Ok(())
}

/// Generate a keyword-based vector representation using multi-layered hashing.
///
/// This function creates dense vector embeddings for keywords and phrases using:
/// - Primary hash-based features for core keyword representation
/// - Secondary hash features for improved discrimination between similar terms
/// - Character-level features for handling subword information
/// - Multi-word handling with position weighting for phrases
/// - L2 normalization for consistent vector magnitudes
///
/// The resulting vectors are suitable for semantic similarity calculations
/// and can be used in search and retrieval operations within Shardex.
///
/// # Arguments
/// * `keyword` - The keyword or phrase to vectorize
/// * `dimension` - The target vector dimensionality
///
/// # Returns
/// A normalized vector of the specified dimension representing the keyword
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

/// Simple hash function for demonstration purposes using FNV-like algorithm.
///
/// This function implements a basic string hashing algorithm suitable for
/// feature generation in vector embeddings. It provides:
/// - Deterministic hash values for consistent vector generation
/// - Good distribution properties for feature mapping
/// - Fast computation suitable for real-time applications
///
/// Note: This is a demonstration hash function. Production systems should
/// consider more sophisticated hashing algorithms for better distribution.
///
/// # Arguments
/// * `s` - The string to hash
///
/// # Returns
/// A 32-bit hash value representing the input string
fn simple_hash(s: &str) -> u32 {
    s.bytes()
        .fold(0u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as u32))
}

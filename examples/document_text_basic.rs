//! Basic document text storage example
//!
//! This example demonstrates the fundamental document text storage operations:
//! - Storing documents with text and postings
//! - Retrieving full document text
//! - Extracting text snippets from search results
//! - Error handling for text storage operations

use apithing::ApiOperation;
use shardex::{
    api::{
        CreateIndex, CreateIndexParams, ExtractSnippet, ExtractSnippetParams, Flush, FlushParams, GetDocumentText,
        GetDocumentTextParams, GetStats, GetStatsParams, Search, SearchParams, ShardexContext, StoreDocumentText,
        StoreDocumentTextParams,
    },
    DocumentId, Posting, ShardexConfig,
};
use std::error::Error;

/// Vector size used for text embeddings
const VECTOR_SIZE: usize = 128;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Document Text Storage - Basic Example");
    println!("==============================================");

    // Create a temporary directory for this example
    let temp_dir = std::env::temp_dir().join("shardex_text_basic_example");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Create context with text storage enabled
    let config = ShardexConfig::new()
        .directory_path(&temp_dir)
        .max_document_text_size(1024 * 1024); // 1MB per document
    let mut context = ShardexContext::with_config(config);

    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.clone())
        .vector_size(VECTOR_SIZE)
        .shard_size(10000)
        .batch_write_interval_ms(100)
        .build()?;

    println!("Creating index with text storage enabled");
    println!(
        "Max document text size: {} bytes",
        context.get_max_document_text_size().unwrap_or(0)
    );

    // Create the index using ApiThing pattern
    CreateIndex::execute(&mut context, &create_params)?;

    // Sample documents with text and corresponding embeddings
    let documents = &[
        (
            "The quick brown fox jumps over the lazy dog. This classic sentence contains every letter of the English alphabet.",
            vec![
                (0, 9, "The quick"),  // "The quick"
                (10, 9, "brown fox"), // "brown fox"
                (20, 5, "jumps"),     // "jumps"
                (31, 8, "the lazy"),  // "the lazy"
                (40, 3, "dog"),       // "dog"
            ],
        ),
        (
            "Artificial intelligence and machine learning are transforming how we process and analyze data in modern applications.",
            vec![
                (0, 20, "Artificial intelligence"), // "Artificial intelligence"
                (25, 16, "machine learning"),       // "machine learning"
                (46, 12, "transforming"),           // "transforming"
                (67, 7, "process"),                 // "process"
                (79, 7, "analyze"),                 // "analyze"
                (87, 4, "data"),                    // "data"
            ],
        ),
        (
            "Space exploration continues to push the boundaries of human knowledge and technological innovation.",
            vec![
                (0, 16, "Space exploration"), // "Space exploration"
                (30, 4, "push"),              // "push"
                (39, 10, "boundaries"),       // "boundaries"
                (53, 5, "human"),             // "human"
                (59, 9, "knowledge"),         // "knowledge"
                (73, 13, "technological"),    // "technological"
                (87, 10, "innovation"),       // "innovation"
            ],
        ),
    ];

    println!("\nStoring {} documents with text and postings...", documents.len());

    // Store each document with text and postings atomically
    for (i, (document_text, segments)) in documents.iter().enumerate() {
        let doc_id = DocumentId::from_raw((i + 1) as u128);

        // Create postings for this document
        let mut postings = Vec::new();
        for (start, length, _text_segment) in segments {
            let posting = Posting {
                document_id: doc_id,
                start: *start,
                length: *length,
                vector: generate_segment_vector(_text_segment),
            };
            postings.push(posting);
        }

        // Store document text and postings using new API
        let store_params = StoreDocumentTextParams::new(doc_id, document_text.to_string(), postings)?;
        StoreDocumentText::execute(&mut context, &store_params)?;

        println!(
            "  Document {}: {} characters, {} segments",
            i + 1,
            document_text.len(),
            segments.len()
        );
    }

    // Flush to ensure all data is written
    let flush_params = FlushParams::with_stats();
    let flush_stats = Flush::execute(&mut context, &flush_params)?;
    if let Some(stats) = flush_stats {
        println!("\nFlushed to disk - Operations: {}", stats.operations_applied);
    } else {
        println!("\nFlushed to disk");
    }

    // Demonstrate full document text retrieval
    println!("\nRetrieving full document text:");
    println!("==============================");

    for i in 1..=documents.len() {
        let doc_id = DocumentId::from_raw(i as u128);

        let get_params = GetDocumentTextParams::new(doc_id);
        match GetDocumentText::execute(&mut context, &get_params) {
            Ok(text) => {
                println!(
                    "Document {}: \"{}\"",
                    i,
                    if text.len() > 60 {
                        format!("{}...", &text[..60])
                    } else {
                        text
                    }
                );
            }
            Err(e) => println!("Error retrieving document {}: {}", i, e),
        }
    }

    // Demonstrate text extraction from postings
    println!("\nExtracting text from individual postings:");
    println!("=========================================");

    let doc_id = DocumentId::from_raw(1);
    let sample_postings = &[
        Posting {
            document_id: doc_id,
            start: 0,
            length: 9,
            vector: generate_segment_vector("The quick"),
        },
        Posting {
            document_id: doc_id,
            start: 10,
            length: 9,
            vector: generate_segment_vector("brown fox"),
        },
        Posting {
            document_id: doc_id,
            start: 20,
            length: 5,
            vector: generate_segment_vector("jumps"),
        },
    ];

    for (i, posting) in sample_postings.iter().enumerate() {
        let extract_params = ExtractSnippetParams::from_posting(posting);
        match ExtractSnippet::execute(&mut context, &extract_params) {
            Ok(extracted_text) => {
                println!(
                    "  Posting {}: '{}' ({}:{}+{})",
                    i + 1,
                    extracted_text,
                    posting.document_id.raw(),
                    posting.start,
                    posting.length
                );
            }
            Err(e) => println!("  Error extracting posting {}: {}", i + 1, e),
        }
    }

    // Demonstrate search integration with text extraction
    println!("\nSearching and extracting text from results:");
    println!("===========================================");

    let search_queries = vec![
        ("artificial intelligence", "artificial intelligence technology"),
        ("space and exploration", "space exploration universe"),
        ("quick brown animal", "quick brown fox animal"),
    ];

    for (query_desc, query_terms) in search_queries {
        println!("\nSearching for: {}", query_desc);
        let query_vector = generate_segment_vector(query_terms);

        // Search for top 3 most similar postings
        let search_params = SearchParams::builder()
            .query_vector(query_vector)
            .k(3)
            .slop_factor(None)
            .build()?;
        let results = Search::execute(&mut context, &search_params)?;

        if results.is_empty() {
            println!("  No results found");
            continue;
        }

        for (i, result) in results.iter().enumerate() {
            // Create posting from search result for text extraction
            let result_posting = Posting {
                document_id: result.document_id,
                start: result.start,
                length: result.length,
                vector: result.vector.clone(),
            };

            let extract_params = ExtractSnippetParams::from_posting(&result_posting);
            match ExtractSnippet::execute(&mut context, &extract_params) {
                Ok(result_text) => {
                    println!(
                        "  {}. '{}' (score: {:.4}, doc: {})",
                        i + 1,
                        result_text,
                        result.similarity_score,
                        result.document_id.raw()
                    );
                }
                Err(e) => {
                    println!(
                        "  {}. Error extracting text: {} (doc: {})",
                        i + 1,
                        e,
                        result.document_id.raw()
                    );
                }
            }
        }
    }

    // Demonstrate error handling
    println!("\nDemonstrating error handling:");
    println!("=============================");

    // Test with nonexistent document
    let nonexistent_doc = DocumentId::from_raw(999);
    let get_params = GetDocumentTextParams::new(nonexistent_doc);
    match GetDocumentText::execute(&mut context, &get_params) {
        Ok(_) => println!("  Unexpected success for nonexistent document"),
        Err(e) => println!("  ✓ Correctly handled missing document: {}", e),
    }

    // Test with invalid range
    let invalid_posting = Posting {
        document_id: DocumentId::from_raw(1),
        start: 1000, // Beyond document end
        length: 50,
        vector: generate_segment_vector("invalid"),
    };

    let extract_params = ExtractSnippetParams::from_posting(&invalid_posting);
    match ExtractSnippet::execute(&mut context, &extract_params) {
        Ok(_) => println!("  Unexpected success for invalid range"),
        Err(e) => println!("  ✓ Correctly handled invalid range: {}", e),
    }

    // Get final statistics
    println!("\nFinal Index Statistics:");
    println!("======================");
    let stats_params = GetStatsParams::new();
    let stats = GetStats::execute(&mut context, &stats_params)?;
    println!("- Total documents: {}", documents.len());
    println!("- Total postings: {}", stats.total_postings);
    println!("- Active postings: {}", stats.active_postings);
    println!("- Memory usage: {:.2} MB", stats.memory_usage as f64 / 1024.0 / 1024.0);

    // Clean up temporary directory
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\nExample completed successfully!");

    Ok(())
}

/// Generate a simple text-based vector representation for a text segment
/// In a real application, you would use a proper text embedding model
fn generate_segment_vector(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0; VECTOR_SIZE];
    let lowercase_text = text.to_lowercase();
    let words: Vec<&str> = lowercase_text.split_whitespace().collect();

    // Simple hash-based vector generation (for demonstration only)
    for (i, word) in words.iter().enumerate() {
        let hash = simple_hash(word);
        let index = (hash % VECTOR_SIZE as u32) as usize;
        vector[index] += 1.0 / (i + 1) as f32;

        // Add some character-based features for variety
        for (j, ch) in word.chars().enumerate() {
            let char_index = ((ch as u32 + j as u32) % VECTOR_SIZE as u32) as usize;
            vector[char_index] += 0.1 / (j + 1) as f32;
        }
    }

    // Normalize the vector
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
    s.bytes()
        .fold(0u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as u32))
}

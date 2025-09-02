//! Basic usage example for Shardex vector search engine using the ApiThing pattern
//!
//! This example demonstrates the new ApiThing-based API:
//! - Creating a new index with CreateIndex operation
//! - Adding postings with AddPostings operation
//! - Performing similarity search with Search operation
//! - Flushing operations with Flush operation
//! - Retrieving statistics with GetStats operation
//! - Centralized state management with ShardexContext

use apithing::ApiOperation;
use shardex::api::{
    AddPostings, AddPostingsParams, CreateIndex, CreateIndexParams, Flush, FlushParams, GetStats, GetStatsParams,
    Search, SearchParams, ShardexContext,
};
use shardex::{DocumentId, Posting};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Basic Usage Example");
    println!("===========================");

    // Create a temporary directory for this example
    let temp_dir = std::env::temp_dir().join("shardex_basic_example");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    println!("Creating new index at: {}", temp_dir.display());

    // Create a context and configure the index
    // The ShardexContext manages all state and is passed to each operation
    let mut context = ShardexContext::new();
    let create_params = CreateIndexParams::builder()
        .directory_path(temp_dir.clone())
        .vector_size(128) // 128-dimensional vectors
        .shard_size(10000) // Max 10,000 vectors per shard
        .batch_write_interval_ms(100) // Batch writes every 100ms
        .build()?;

    // Create the index using the CreateIndex operation
    // All operations follow the ApiThing pattern: Operation::execute(&mut context, &params)
    println!("About to call CreateIndex::execute...");
    CreateIndex::execute(&mut context, &create_params)?;
    println!("CreateIndex::execute completed successfully!");

    // Prepare some sample data
    let sample_documents = [
        (
            "Document about cats and dogs",
            generate_text_vector("cats dogs pets animals"),
        ),
        (
            "Article on machine learning",
            generate_text_vector("machine learning AI neural networks"),
        ),
        (
            "Cooking recipe for pasta",
            generate_text_vector("pasta cooking recipe italian food"),
        ),
        (
            "Travel guide to Japan",
            generate_text_vector("japan travel guide tokyo culture"),
        ),
        (
            "Programming tutorial",
            generate_text_vector("programming tutorial code software development"),
        ),
    ];

    // Create postings from sample documents
    let mut postings = Vec::new();
    for (i, (text, vector)) in sample_documents.iter().enumerate() {
        let document_id = DocumentId::from_raw((i + 1) as u128);

        let posting = Posting {
            document_id,
            start: 0,
            length: text.len() as u32,
            vector: vector.clone(),
        };

        postings.push(posting);
        println!("Added document {}: {}", i + 1, text);
    }

    // Add all postings to the index
    println!("\nIndexing {} documents...", postings.len());
    let add_params = AddPostingsParams::new(postings)?;
    AddPostings::execute(&mut context, &add_params)?;

    // Flush to ensure all data is written
    let flush_params = FlushParams::with_stats();
    let flush_stats = Flush::execute(&mut context, &flush_params)?;
    if let Some(stats) = flush_stats {
        println!("Flushed to disk - Operations: {}", stats.operations_applied);
    }

    // Get index statistics
    let stats_params = GetStatsParams::new();
    let stats = GetStats::execute(&mut context, &stats_params)?;
    println!("\nIndex Statistics:");
    println!("- Total shards: {}", stats.total_shards);
    println!("- Total postings: {}", stats.total_postings);
    println!("- Active postings: {}", stats.active_postings);
    println!("- Vector dimension: {}", stats.vector_dimension);
    println!("- Memory usage: {:.2} MB", stats.memory_usage as f64 / 1024.0 / 1024.0);

    // Perform some searches
    println!("\nPerforming similarity searches:");
    println!("==============================");

    let search_queries = vec![
        ("pets and animals", "cats dogs pets animals"),
        ("artificial intelligence", "artificial intelligence machine learning"),
        ("cooking and food", "cooking food recipes"),
        ("travel and tourism", "travel tourism destinations"),
        ("software engineering", "programming software engineering"),
    ];

    for (query_desc, query_terms) in search_queries {
        println!("\nSearching for: {}", query_desc);
        let query_vector = generate_text_vector(query_terms);

        // Search for top 3 most similar documents
        let search_params = SearchParams::builder()
            .query_vector(query_vector)
            .k(3)
            .slop_factor(None)
            .build()?;
        let results = Search::execute(&mut context, &search_params)?;

        for (i, result) in results.iter().enumerate() {
            println!(
                "  {}. Document {} (similarity: {:.3})",
                i + 1,
                result.document_id.raw(),
                result.similarity_score
            );
        }
    }

    // Demonstrate search with custom slop factor
    println!("\nSearching with custom slop factor (broader search):");
    let query_vector = generate_text_vector("food cooking");
    let search_params = SearchParams::builder()
        .query_vector(query_vector)
        .k(2)
        .slop_factor(Some(3))
        .build()?;
    let results = Search::execute(&mut context, &search_params)?;

    println!("Results with slop factor 3:");
    for result in results {
        println!(
            "  Document {} (similarity: {:.3})",
            result.document_id.raw(),
            result.similarity_score
        );
    }

    // Clean up temporary directory
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\nExample completed successfully!");

    Ok(())
}

/// Generate a simple text-based vector representation
/// In a real application, you would use a proper text embedding model
fn generate_text_vector(text: &str) -> Vec<f32> {
    let mut vector = vec![0.0; 128];
    let words: Vec<&str> = text.split_whitespace().collect();

    // Simple hash-based vector generation (for demonstration only)
    for (i, word) in words.iter().enumerate() {
        let hash = simple_hash(word);
        let index = (hash % 128) as usize;
        vector[index] += 1.0 / (i + 1) as f32;
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

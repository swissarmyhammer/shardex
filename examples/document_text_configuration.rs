//! Document text storage configuration example
//!
//! Demonstrates various configuration options and their effects on:
//! - Memory-constrained environments
//! - High-capacity document storage
//! - Performance-optimized setups
//! - Different use case scenarios

use apithing::ApiOperation;
use shardex::{
    api::{
        AddPostings, AddPostingsParams, CreateIndex, CreateIndexParams,
        ExtractSnippet, ExtractSnippetParams, Flush, FlushParams, GetDocumentText, GetDocumentTextParams, GetStats, GetStatsParams,
        Search, SearchParams, ShardexContext, StoreDocumentText, StoreDocumentTextParams,
    },
    DocumentId, Posting, ShardexConfig,
};
use std::error::Error;
use std::time::Instant;

fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Document Text Storage - Configuration Examples");
    println!("======================================================");

    // Create base temporary directory
    let base_temp_dir = std::env::temp_dir().join("shardex_text_config_examples");
    if base_temp_dir.exists() {
        std::fs::remove_dir_all(&base_temp_dir)?;
    }
    std::fs::create_dir_all(&base_temp_dir)?;

    // Run different configuration scenarios
    memory_constrained_config(&base_temp_dir)?;
    high_capacity_config(&base_temp_dir)?;
    performance_optimized_config(&base_temp_dir)?;
    use_case_specific_configs(&base_temp_dir)?;
    migration_scenarios(&base_temp_dir)?;

    // Clean up
    std::fs::remove_dir_all(&base_temp_dir)?;
    println!("\\nConfiguration examples completed successfully!");

    Ok(())
}

fn memory_constrained_config(base_dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Memory-Constrained Configuration ===");

    let index_dir = base_dir.join("memory_constrained");
    std::fs::create_dir_all(&index_dir)?;

    // Create context with memory-optimized configuration
    let config = ShardexConfig::new()
        .directory_path(&index_dir)
        .vector_size(64)  // Smaller vectors
        .shard_size(5_000) // Smaller shards
        .max_document_text_size(256 * 1024) // 256KB per document
        .shardex_segment_size(1_000)  // Smaller segments
        .wal_segment_size(64 * 1024)  // 64KB WAL segments
        .batch_write_interval_ms(200) // Less frequent batching
        .default_slop_factor(2)       // More focused search
        .bloom_filter_size(1024); // Smaller bloom filters

    let mut context = ShardexContext::with_config(config.clone());

    // Create index using CreateIndexParams from the config
    let create_params = CreateIndexParams::from_shardex_config(config.clone());
    
    println!("Configuration for memory-constrained environment:");
    println!("  Vector size: {} dimensions", config.vector_size);
    println!("  Shard size: {}", config.shard_size);
    println!("  Max document text size: {} KB", config.max_document_text_size / 1024);
    println!("  Shard segment size: {}", config.shardex_segment_size);
    println!("  WAL segment size: {} KB", config.wal_segment_size / 1024);
    println!("  Batch interval: {} ms", config.batch_write_interval_ms);
    println!("  Default slop factor: {}", config.slop_factor_config.default_factor);
    println!("  Bloom filter size: {}", config.bloom_filter_size);

    CreateIndex::execute(&mut context, &create_params)?;

    // Test with small documents
    let small_documents = &[
        "Short document about cats.",
        "Brief article on dogs.",
        "Quick note about birds.",
        "Small text about fish.",
        "Tiny document on pets.",
    ];

    let start = Instant::now();
    for (i, text) in small_documents.iter().enumerate() {
        let doc_id = DocumentId::from_raw((i + 1) as u128);
        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: text.len() as u32,
            vector: generate_simple_vector(&text.to_lowercase(), 64),
        };

        let store_params = StoreDocumentTextParams::new(doc_id, text.to_string(), vec![posting])?;
        StoreDocumentText::execute(&mut context, &store_params)?;
    }
    let processing_time = start.elapsed();

    let stats = GetStats::execute(&mut context, &GetStatsParams::new())?;
    println!("\\nResults:");
    println!("  Processing time: {:?}", processing_time);
    println!("  Memory usage: {:.2} KB", stats.memory_usage as f64 / 1024.0);
    println!("  Total postings: {}", stats.total_postings);
    println!("  Total shards: {}", stats.total_shards);

    // Test search performance
    let query_vector = generate_simple_vector("cats pets", 64);
    let search_start = Instant::now();
    let search_params = SearchParams::new(query_vector, 3)?;
    let results = Search::execute(&mut context, &search_params)?;
    let search_time = search_start.elapsed();

    println!("  Search time: {:?}", search_time);
    println!("  Search results: {}", results.len());

    // Test document size limit
    let large_text = "x".repeat(300 * 1024); // 300KB - should exceed limit
    let doc_id = DocumentId::from_raw(100);
    let posting = Posting {
        document_id: doc_id,
        start: 0,
        length: large_text.len() as u32,
        vector: generate_simple_vector("large", 64),
    };

    let store_params = StoreDocumentTextParams::new(doc_id, large_text, vec![posting]);
    match store_params.and_then(|params| StoreDocumentText::execute(&mut context, &params)) {
        Ok(()) => println!("  ✗ Large document was accepted unexpectedly"),
        Err(e) => println!("  ✓ Large document correctly rejected: {}", e),
    }

    Ok(())
}

fn high_capacity_config(base_dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    println!("\\n=== High-Capacity Configuration ===");

    let index_dir = base_dir.join("high_capacity");
    std::fs::create_dir_all(&index_dir)?;

    // Optimized for large-scale operations
    let config = ShardexConfig::new()
        .directory_path(&index_dir)
        .vector_size(768) // Large vectors (e.g., BERT-large)
        .shard_size(100_000) // Large shards
        .max_document_text_size(100 * 1024 * 1024) // 100MB per document
        .shardex_segment_size(50_000) // Large segments for efficiency
        .wal_segment_size(10 * 1024 * 1024) // 10MB WAL segments
        .batch_write_interval_ms(50)  // Frequent batching for throughput
        .default_slop_factor(5)       // Broad search for accuracy
        .bloom_filter_size(65_536); // Large bloom filters

    let mut context = ShardexContext::with_config(config.clone());

    // Create index using CreateIndexParams from the config
    let create_params = CreateIndexParams::from_shardex_config(config.clone());
    
    println!("Configuration for high-capacity environment:");
    println!("  Vector size: {} dimensions", config.vector_size);
    println!("  Shard size: {}", config.shard_size);
    println!(
        "  Max document text size: {} MB",
        config.max_document_text_size / (1024 * 1024)
    );
    println!("  Shard segment size: {}", config.shardex_segment_size);
    println!("  WAL segment size: {} MB", config.wal_segment_size / (1024 * 1024));
    println!("  Batch interval: {} ms", config.batch_write_interval_ms);
    println!("  Default slop factor: {}", config.slop_factor_config.default_factor);
    println!("  Bloom filter size: {}", config.bloom_filter_size);

    CreateIndex::execute(&mut context, &create_params)?;

    // Test with larger documents
    let base_content = "This is a comprehensive research document covering advanced topics in artificial intelligence, machine learning, deep neural networks, natural language processing, computer vision, robotics, autonomous systems, and their applications in healthcare, finance, transportation, education, and scientific research. ";

    let large_documents: Vec<String> = (1..=10)
        .map(|i| format!("Document {}: {}", i, base_content.repeat(i * 100)))
        .collect();

    println!("\\nProcessing {} large documents...", large_documents.len());
    let start = Instant::now();

    for (i, text) in large_documents.iter().enumerate() {
        let doc_id = DocumentId::from_raw((i + 1) as u128);

        // Create multiple postings per document
        let mut postings = Vec::new();
        let segments = [
            "artificial intelligence",
            "machine learning",
            "deep neural",
            "natural language",
            "computer vision",
            "robotics",
        ];

        for segment in segments.iter() {
            if let Some(pos) = text.find(segment) {
                postings.push(Posting {
                    document_id: doc_id,
                    start: pos as u32,
                    length: segment.len() as u32,
                    vector: generate_simple_vector(segment, 768),
                });
            }
        }

        // Add full document posting
        postings.push(Posting {
            document_id: doc_id,
            start: 0,
            length: text.len() as u32,
            vector: generate_simple_vector(&format!("document {}", i + 1), 768),
        });

        let store_params = StoreDocumentTextParams::new(doc_id, text.clone(), postings)?;
        StoreDocumentText::execute(&mut context, &store_params)?;

        if (i + 1) % 3 == 0 {
            println!(
                "  Processed {} documents ({:.1} MB total)",
                i + 1,
                large_documents
                    .iter()
                    .take(i + 1)
                    .map(|d| d.len())
                    .sum::<usize>() as f64
                    / (1024.0 * 1024.0)
            );
        }
    }

    let processing_time = start.elapsed();
    let stats = GetStats::execute(&mut context, &GetStatsParams::new())?;

    println!("\\nResults:");
    println!("  Processing time: {:?}", processing_time);
    println!(
        "  Memory usage: {:.2} MB",
        stats.memory_usage as f64 / (1024.0 * 1024.0)
    );
    println!("  Total postings: {}", stats.total_postings);
    println!("  Total shards: {}", stats.total_shards);
    println!(
        "  Average shard utilization: {:.1}%",
        stats.average_shard_utilization * 100.0
    );

    // Test complex search
    let complex_queries = vec![
        ("AI research", "artificial intelligence research"),
        ("ML applications", "machine learning applications"),
        ("computer vision", "computer vision systems"),
    ];

    println!("\\nTesting complex search performance:");
    for (desc, query_terms) in complex_queries {
        let query_vector = generate_simple_vector(query_terms, 768);
        let search_start = Instant::now();
        let search_params = SearchParams::with_slop_factor(query_vector, 10, 5)?;
        let results = Search::execute(&mut context, &search_params)?;
        let search_time = search_start.elapsed();

        println!("  '{}': {} results in {:?}", desc, results.len(), search_time);
    }

    Ok(())
}

fn performance_optimized_config(base_dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Performance-Optimized Configuration ===");

    let index_dir = base_dir.join("performance_optimized");
    std::fs::create_dir_all(&index_dir)?;

    // Balanced configuration for optimal performance
    let config = ShardexConfig::new()
        .directory_path(&index_dir)
        .vector_size(256) // Good balance of expressiveness and speed
        .shard_size(50_000) // Optimal shard size for search speed
        .max_document_text_size(10 * 1024 * 1024) // 10MB per document
        .shardex_segment_size(25_000) // Balanced segment size
        .wal_segment_size(2 * 1024 * 1024) // 2MB WAL segments
        .batch_write_interval_ms(75)  // Balanced batching
        .default_slop_factor(3)       // Good accuracy/speed tradeoff
        .bloom_filter_size(8_192); // Optimal bloom filter size

    let mut context = ShardexContext::with_config(config.clone());
    let create_params = CreateIndexParams::from_shardex_config(config.clone());
    
    println!("Configuration optimized for balanced performance:");
    println!("  Vector size: {} dimensions", config.vector_size);
    println!("  Shard size: {}", config.shard_size);
    println!(
        "  Max document text size: {} MB",
        config.max_document_text_size / (1024 * 1024)
    );

    CreateIndex::execute(&mut context, &create_params)?;

    // Performance test with realistic workload
    let test_documents = create_realistic_documents(50); // 50 realistic documents

    println!(
        "\\nPerformance testing with {} realistic documents...",
        test_documents.len()
    );

    // Bulk insert performance
    let insert_start = Instant::now();
    for (i, (text, keywords)) in test_documents.iter().enumerate() {
        let doc_id = DocumentId::from_raw((i + 1) as u128);

        let mut postings = Vec::new();
        for keyword in keywords {
            if let Some(pos) = text.find(keyword) {
                postings.push(Posting {
                    document_id: doc_id,
                    start: pos as u32,
                    length: keyword.len() as u32,
                    vector: generate_simple_vector(keyword, 256),
                });
            }
        }

        let store_params = StoreDocumentTextParams::new(doc_id, text.clone(), postings)?;
        StoreDocumentText::execute(&mut context, &store_params)?;
    }
    let insert_time = insert_start.elapsed();

    // Force flush and measure  
    let flush_start = Instant::now();
    let flush_params = FlushParams::with_stats();
    let flush_stats = Flush::execute(&mut context, &flush_params)?;
    let flush_time = flush_start.elapsed();

    let stats = GetStats::execute(&mut context, &GetStatsParams::new())?;

    println!("\\nInsert Performance:");
    println!(
        "  Bulk insert time: {:?} ({:.1} docs/sec)",
        insert_time,
        test_documents.len() as f64 / insert_time.as_secs_f64()
    );
    println!("  Flush time: {:?}", flush_time);
    if let Some(stats) = flush_stats {
        println!("  Operations flushed: {}", stats.operations_applied);
    } else {
        println!("  Flush completed (no stats available)");
    }

    // Search performance testing
    let search_queries = vec![
        "technology innovation",
        "data analysis",
        "software development",
        "artificial intelligence",
        "machine learning",
    ];

    println!("\\nSearch Performance:");
    let mut total_search_time = std::time::Duration::new(0, 0);
    let mut total_results = 0;

    for query in &search_queries {
        let query_vector = generate_simple_vector(query, 256);

        let search_start = Instant::now();
        let search_params = SearchParams::new(query_vector, 20)?;
        let results = Search::execute(&mut context, &search_params)?;
        let search_time = search_start.elapsed();

        total_search_time += search_time;
        total_results += results.len();

        println!("  '{}': {} results in {:?}", query, results.len(), search_time);
    }

    println!(
        "  Average search time: {:?}",
        total_search_time / search_queries.len() as u32
    );
    println!(
        "  Average results per query: {:.1}",
        total_results as f64 / search_queries.len() as f64
    );

    // Text extraction performance
    println!("\\nText Extraction Performance:");
    let extract_start = Instant::now();
    let mut extracted_count = 0;

    for i in 1..=std::cmp::min(test_documents.len(), 20) {
        let doc_id = DocumentId::from_raw(i as u128);

        // Test full document extraction
        let get_params = GetDocumentTextParams::new(doc_id);
        if GetDocumentText::execute(&mut context, &get_params).is_ok() {
            extracted_count += 1;
        }

        // Test partial extraction
        let extract_params = ExtractSnippetParams::new(doc_id, 0, 50)?;
        if ExtractSnippet::execute(&mut context, &extract_params).is_ok() {
            extracted_count += 1;
        }
    }

    let extract_time = extract_start.elapsed();
    println!(
        "  {} extractions in {:?} ({:.1} μs/extraction)",
        extracted_count,
        extract_time,
        extract_time.as_micros() as f64 / extracted_count as f64
    );

    println!("\\nFinal Statistics:");
    println!(
        "  Memory usage: {:.2} MB",
        stats.memory_usage as f64 / (1024.0 * 1024.0)
    );
    println!("  Total postings: {}", stats.total_postings);
    println!("  Active postings: {}", stats.active_postings);
    println!("  Total shards: {}", stats.total_shards);

    Ok(())
}

fn use_case_specific_configs(base_dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Use Case Specific Configurations ===");

    // Chat/Conversation History
    println!("\\n--- Chat/Conversation History Use Case ---");
    let chat_config = ShardexConfig::new()
        .directory_path(base_dir.join("chat_history"))
        .vector_size(384)  // Good for sentence embeddings
        .max_document_text_size(16 * 1024) // 16KB per message
        .shard_size(25_000)
        .batch_write_interval_ms(25)  // Fast response for real-time
        .default_slop_factor(2); // Focus on recent/relevant

    demonstrate_chat_use_case(chat_config)?;

    // Academic Papers/Research
    println!("\\n--- Academic Papers Use Case ---");
    let academic_config = ShardexConfig::new()
        .directory_path(base_dir.join("academic_papers"))
        .vector_size(768)  // Rich representations for complex content
        .max_document_text_size(50 * 1024 * 1024) // 50MB per paper
        .shard_size(75_000)
        .batch_write_interval_ms(100) // Can tolerate some latency
        .default_slop_factor(4); // Broad search for comprehensive results

    demonstrate_academic_use_case(academic_config)?;

    // Code Search
    println!("\\n--- Code Search Use Case ---");
    let code_config = ShardexConfig::new()
        .directory_path(base_dir.join("code_search"))
        .vector_size(512)  // Good for code structure
        .max_document_text_size(1024 * 1024) // 1MB per file
        .shard_size(30_000)
        .batch_write_interval_ms(50)  // Balance responsiveness and efficiency
        .default_slop_factor(2); // Precise matching for code

    demonstrate_code_search_use_case(code_config)?;

    Ok(())
}

fn demonstrate_chat_use_case(config: ShardexConfig) -> Result<(), Box<dyn Error>> {
    let mut context = ShardexContext::with_config(config.clone());
    let create_params = CreateIndexParams::from_shardex_config(config);
    CreateIndex::execute(&mut context, &create_params)?;

    let chat_messages = &[
        "Hi there! How can I help you today?",
        "I'm looking for information about machine learning algorithms.",
        "Great! Machine learning has many different approaches. Are you interested in supervised or unsupervised learning?",
        "I'd like to know about supervised learning, specifically classification algorithms.",
        "Perfect! Some popular classification algorithms include decision trees, random forests, SVM, and neural networks.",
        "Can you tell me more about decision trees?",
        "Decision trees are intuitive algorithms that make decisions by asking a series of questions about the data features.",
    ];

    println!("  Storing {} chat messages...", chat_messages.len());
    let start = Instant::now();

    for (i, message) in chat_messages.iter().enumerate() {
        let doc_id = DocumentId::from_raw((i + 1) as u128);
        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: message.len() as u32,
            vector: generate_simple_vector(message, 384),
        };

        let store_params = StoreDocumentTextParams::new(doc_id, message.to_string(), vec![posting])?;
        StoreDocumentText::execute(&mut context, &store_params)?;
    }

    let storage_time = start.elapsed();
    println!("  Storage time: {:?}", storage_time);

    // Search for relevant conversation context
    let query = "machine learning classification";
    let query_vector = generate_simple_vector(query, 384);
    let search_params = SearchParams::new(query_vector, 5)?;
    let search_results = Search::execute(&mut context, &search_params)?;

    println!(
        "  Search for '{}' found {} relevant messages",
        query,
        search_results.len()
    );
    for result in search_results.iter().take(3) {
        let get_params = GetDocumentTextParams::new(result.document_id);
        if let Ok(message) = GetDocumentText::execute(&mut context, &get_params) {
            println!(
                "    '{}' (score: {:.3})",
                if message.len() > 50 {
                    format!("{}...", &message[..50])
                } else {
                    message
                },
                result.similarity_score
            );
        }
    }

    Ok(())
}

fn demonstrate_academic_use_case(config: ShardexConfig) -> Result<(), Box<dyn Error>> {
    let mut context = ShardexContext::with_config(config.clone());
    let create_params = CreateIndexParams::from_shardex_config(config);
    CreateIndex::execute(&mut context, &create_params)?;

    let academic_abstracts = &[
        "This paper presents a novel deep learning approach for natural language understanding in conversational AI systems. We propose a transformer-based architecture that achieves state-of-the-art performance on multiple dialogue benchmarks.",
        "We investigate the application of reinforcement learning to autonomous vehicle navigation in complex urban environments. Our experimental results demonstrate significant improvements in safety and efficiency compared to traditional rule-based approaches.",
        "This study examines the effectiveness of federated learning for privacy-preserving machine learning in healthcare applications. We evaluate our approach on real-world medical datasets while maintaining patient privacy constraints.",
    ];

    println!("  Processing {} academic abstracts...", academic_abstracts.len());

    for (i, abstract_text) in academic_abstracts.iter().enumerate() {
        let doc_id = DocumentId::from_raw((i + 1) as u128);

        // Extract key terms and concepts
        let key_terms = extract_academic_terms(abstract_text);
        let mut postings = Vec::new();

        for term in &key_terms {
            if let Some(pos) = abstract_text.find(term) {
                postings.push(Posting {
                    document_id: doc_id,
                    start: pos as u32,
                    length: term.len() as u32,
                    vector: generate_simple_vector(term, 768),
                });
            }
        }

        // Full abstract posting
        postings.push(Posting {
            document_id: doc_id,
            start: 0,
            length: abstract_text.len() as u32,
            vector: generate_simple_vector(abstract_text, 768),
        });

        let store_params = StoreDocumentTextParams::new(doc_id, abstract_text.to_string(), postings)?;
        StoreDocumentText::execute(&mut context, &store_params)?;
    }

    // Search for research topics
    let research_queries = vec![
        "deep learning natural language",
        "reinforcement learning autonomous",
        "federated learning privacy healthcare",
    ];

    println!("  Testing academic search:");
    for query in research_queries {
        let query_vector = generate_simple_vector(query, 768);
        let search_params = SearchParams::new(query_vector, 2)?;
        let results = Search::execute(&mut context, &search_params)?;
        println!("    '{}': {} relevant papers found", query, results.len());
    }

    Ok(())
}

fn demonstrate_code_search_use_case(config: ShardexConfig) -> Result<(), Box<dyn Error>> {
    let mut context = ShardexContext::with_config(config.clone());
    let create_params = CreateIndexParams::from_shardex_config(config);
    CreateIndex::execute(&mut context, &create_params)?;

    let code_snippets = &[
        (
            "function_definitions.rs",
            "pub fn calculate_similarity(vec1: &[f32], vec2: &[f32]) -> f32 {\n    let dot_product: f32 = vec1.iter().zip(vec2.iter()).map(|(a, b)| a * b).sum();\n    let magnitude1: f32 = vec1.iter().map(|x| x * x).sum::<f32>().sqrt();\n    let magnitude2: f32 = vec2.iter().map(|x| x * x).sum::<f32>().sqrt();\n    dot_product / (magnitude1 * magnitude2)\n}",
        ),
        (
            "error_handling.rs",
            "match result {\n    Ok(value) => println!(\"Success: {}\", value),\n    Err(ShardexError::InvalidDimension { expected, actual }) => {\n        eprintln!(\"Dimension mismatch: expected {}, got {}\", expected, actual);\n    }\n    Err(e) => eprintln!(\"Other error: {}\", e),\n}",
        ),
        (
            "async_operations.rs",
            "async fn process_batch(documents: Vec<Document>) -> Result<(), ShardexError> {\n    for doc in documents {\n        let postings = generate_postings(&doc).await?;\n        index.replace_document_with_postings(doc.id, doc.text, postings).await?;\n    }\n    Ok(())\n}",
        ),
    ];

    println!("  Indexing {} code files...", code_snippets.len());

    for (i, (filename, code)) in code_snippets.iter().enumerate() {
        let doc_id = DocumentId::from_raw((i + 1) as u128);

        // Extract code elements (functions, types, keywords)
        let code_elements = extract_code_elements(code);
        let mut postings = Vec::new();

        for element in &code_elements {
            if let Some(pos) = code.find(element) {
                postings.push(Posting {
                    document_id: doc_id,
                    start: pos as u32,
                    length: element.len() as u32,
                    vector: generate_simple_vector(&format!("code:{}", element), 512),
                });
            }
        }

        // Full file posting
        postings.push(Posting {
            document_id: doc_id,
            start: 0,
            length: code.len() as u32,
            vector: generate_simple_vector(&format!("file:{}", filename), 512),
        });

        let store_params = StoreDocumentTextParams::new(doc_id, code.to_string(), postings)?;
        StoreDocumentText::execute(&mut context, &store_params)?;
        println!("    Indexed {} with {} code elements", filename, code_elements.len());
    }

    // Search for code patterns
    let code_queries = vec![
        "similarity calculation function",
        "error handling pattern",
        "async document processing",
    ];

    println!("  Testing code search:");
    for query in code_queries {
        let query_vector = generate_simple_vector(&format!("code:{}", query), 512);
        let search_params = SearchParams::new(query_vector, 2)?;
        let results = Search::execute(&mut context, &search_params)?;

        println!("    '{}': {} matches found", query, results.len());
        if let Some(result) = results.first() {
            let extract_params = ExtractSnippetParams::new(
                result.document_id,
                result.start,
                std::cmp::min(result.length, 100), // First 100 chars
            )?;
            if let Ok(code_snippet) = ExtractSnippet::execute(&mut context, &extract_params)
            {
                println!("      Match: '{}'", code_snippet.replace('\n', " "));
            }
        }
    }

    Ok(())
}

fn migration_scenarios(base_dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    println!("\\n=== Migration Scenarios ===");

    // Scenario 1: Upgrading existing index to include text storage
    println!("\\n--- Enabling Text Storage for Existing Index ---");

    let index_dir = base_dir.join("migration_scenario");
    std::fs::create_dir_all(&index_dir)?;

    // Create index without text storage first
    let initial_config = ShardexConfig::new()
        .directory_path(&index_dir)
        .vector_size(128)
        .max_document_text_size(0); // Text storage disabled

    let mut initial_context = ShardexContext::with_config(initial_config.clone());
    let initial_create_params = CreateIndexParams::from_shardex_config(initial_config);
    CreateIndex::execute(&mut initial_context, &initial_create_params)?;

    // Add some postings without text
    let doc_id = DocumentId::from_raw(1);
    let posting = Posting {
        document_id: doc_id,
        start: 0,
        length: 20,
        vector: generate_simple_vector("sample text", 128),
    };

    let add_params = AddPostingsParams::new(vec![posting])?;
    AddPostings::execute(&mut initial_context, &add_params)?;
    println!("  Created initial index without text storage");

    // Try to access text (should fail)
    let get_params = GetDocumentTextParams::new(doc_id);
    match GetDocumentText::execute(&mut initial_context, &get_params) {
        Ok(_) => println!("  ✗ Unexpected success accessing text"),
        Err(e) => println!("  ✓ Text access correctly failed: {}", e),
    }

    // Simulate index closure and reopening with text storage enabled
    drop(initial_context);

    // Reopen with text storage enabled
    let upgraded_config = ShardexConfig::new()
        .directory_path(&index_dir)
        .vector_size(128)
        .max_document_text_size(1024 * 1024); // Enable text storage

    // Note: In a real scenario, you'd use OpenIndex operation,
    // but for this demo we'll create a new index
    let mut upgraded_context = ShardexContext::with_config(upgraded_config.clone());
    let upgraded_create_params = CreateIndexParams::from_shardex_config(upgraded_config);
    CreateIndex::execute(&mut upgraded_context, &upgraded_create_params)?;

    // Now we can add documents with text
    let doc_with_text = "This is a document with text storage enabled.";
    let new_doc_id = DocumentId::from_raw(2);
    let new_posting = Posting {
        document_id: new_doc_id,
        start: 0,
        length: doc_with_text.len() as u32,
        vector: generate_simple_vector(doc_with_text, 128),
    };

    let store_params = StoreDocumentTextParams::new(new_doc_id, doc_with_text.to_string(), vec![new_posting])?;
    StoreDocumentText::execute(&mut upgraded_context, &store_params)?;

    // Verify text storage is working
    let get_params = GetDocumentTextParams::new(new_doc_id);
    match GetDocumentText::execute(&mut upgraded_context, &get_params) {
        Ok(text) => println!("  ✓ Text storage working: '{}'", text),
        Err(e) => println!("  ✗ Text storage failed: {}", e),
    }

    println!("  Migration to text-enabled index completed");

    Ok(())
}

fn create_realistic_documents(count: usize) -> Vec<(String, Vec<&'static str>)> {
    let templates = &[
        (
            "Our latest software update includes significant improvements to performance, security, and user experience. The development team has worked tirelessly to address user feedback and implement new features.",
            vec!["software", "performance", "security", "development", "features"],
        ),
        (
            "The quarterly financial report shows strong growth across all business segments. Revenue increased by 15% compared to the previous quarter, driven by expansion in emerging markets.",
            vec!["financial", "growth", "revenue", "business", "markets"],
        ),
        (
            "Research findings indicate that machine learning applications in healthcare are showing promising results. Early trials demonstrate improved diagnostic accuracy and patient outcomes.",
            vec!["research", "machine learning", "healthcare", "diagnostic", "patients"],
        ),
        (
            "The new marketing campaign focuses on digital channels and social media engagement. Initial metrics suggest improved brand awareness and customer acquisition rates.",
            vec!["marketing", "digital", "social media", "brand", "customer"],
        ),
        (
            "Environmental sustainability initiatives have been implemented across all company operations. These measures aim to reduce carbon footprint and promote renewable energy adoption.",
            vec![
                "environmental",
                "sustainability",
                "carbon",
                "renewable energy",
                "operations",
            ],
        ),
    ];

    (0..count)
        .map(|i| {
            let template_idx = i % templates.len();
            let (base_text, keywords) = &templates[template_idx];
            let document_text = format!("Document {}: {}", i + 1, base_text);
            (document_text, keywords.clone())
        })
        .collect()
}

fn extract_academic_terms(text: &str) -> Vec<&str> {
    let academic_keywords = vec![
        "deep learning",
        "neural networks",
        "machine learning",
        "artificial intelligence",
        "natural language",
        "reinforcement learning",
        "federated learning",
        "transformer",
        "autonomous",
        "privacy",
        "healthcare",
        "algorithm",
        "performance",
        "experimental",
    ];

    academic_keywords
        .into_iter()
        .filter(|keyword| text.to_lowercase().contains(&keyword.to_lowercase()))
        .collect()
}

fn extract_code_elements(code: &str) -> Vec<&str> {
    let mut elements = Vec::new();

    // Simple extraction of function names, types, and keywords
    let patterns = vec![
        "pub fn", "async fn", "fn", "struct", "enum", "impl", "match", "Ok", "Err", "Result", "Vec", "String", "async",
        "await",
    ];

    for pattern in patterns {
        if code.contains(pattern) {
            elements.push(pattern);
        }
    }

    elements
}

fn generate_simple_vector(text: &str, dimension: usize) -> Vec<f32> {
    let mut vector = vec![0.0; dimension];
    let lowercase_text = text.to_lowercase();
    let words: Vec<&str> = lowercase_text.split_whitespace().collect();

    for (i, word) in words.iter().enumerate() {
        let hash = simple_hash(word);
        let index = (hash % dimension as u32) as usize;
        vector[index] += 1.0 / (i + 1) as f32;
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

fn simple_hash(s: &str) -> u32 {
    s.bytes()
        .fold(0u32, |acc, byte| acc.wrapping_mul(31).wrapping_add(byte as u32))
}

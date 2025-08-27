# Step 16: Documentation and Examples - Complete Implementation

Refer to /Users/wballard/github/shardex/ideas/document.md

## Objective

Create comprehensive documentation and examples for document text storage functionality, ensuring users can effectively utilize all features.

## Tasks

### Update API Documentation

Update the main API reference with complete document text storage documentation:

#### Update `docs/api-reference.md`

Add comprehensive documentation for new API methods:

```markdown
## Document Text Storage

Shardex supports storing and retrieving document source text alongside vector embeddings, enabling rich search experiences with text extraction.

### Configuration

Enable document text storage by setting `max_document_text_size` in your configuration:

```rust
let config = ShardexConfig {
    directory_path: "./my_index".into(),
    vector_dimension: 128,
    max_document_text_size: 10 * 1024 * 1024, // 10MB per document
    ..Default::default()
};
```

### Core Methods

#### `get_document_text(document_id: DocumentId) -> Result<String, ShardexError>`

Retrieves the complete current text for a document.

**Parameters:**
- `document_id`: The document identifier

**Returns:**
- `Ok(String)`: The complete document text
- `Err(ShardexError)`: Error if document not found or text storage disabled

**Example:**
```rust
let document_text = shardex.get_document_text(document_id).await?;
println!("Document content: {}", document_text);
```

#### `extract_text(posting: &Posting) -> Result<String, ShardexError>`

Extracts text substring using posting coordinates (document_id, start, length).

**Parameters:**
- `posting`: Posting containing document coordinates

**Returns:**
- `Ok(String)`: Extracted text substring
- `Err(ShardexError)`: Error if coordinates invalid or document not found

**Example:**
```rust
// From search results
let search_results = shardex.search(&query_vector, 10, None).await?;
for result in search_results {
    let posting = Posting {
        document_id: result.document_id,
        start: result.start,
        length: result.length,
        vector: result.vector,
    };
    
    let text_snippet = shardex.extract_text(&posting).await?;
    println!("Found: '{}'", text_snippet);
}
```

#### `replace_document_with_postings(document_id: DocumentId, text: String, postings: Vec<Posting>) -> Result<(), ShardexError>`

Atomically replaces document text and all associated postings in a single transaction.

**Parameters:**
- `document_id`: Document identifier
- `text`: New document text content
- `postings`: New postings for the document

**Returns:**
- `Ok(())`: Operation completed successfully
- `Err(ShardexError)`: Error during atomic replacement

**Example:**
```rust
let document_text = "The quick brown fox jumps over the lazy dog.";
let postings = vec![
    Posting::new(doc_id, 0, 9, embedding1, 128)?,    // "The quick"
    Posting::new(doc_id, 10, 9, embedding2, 128)?,   // "brown fox"
    Posting::new(doc_id, 20, 5, embedding3, 128)?,   // "jumps"
];

shardex.replace_document_with_postings(doc_id, document_text.to_string(), postings).await?;
```

### Error Handling

Document text storage introduces specific error types:

- `DocumentTextNotFound`: Document text not available for the given document ID
- `InvalidRange`: Text extraction coordinates are invalid for the document
- `DocumentTooLarge`: Document exceeds configured size limit
- `TextCorruption`: Text storage file corruption detected

See [Error Handling](#error-handling) section for complete examples.
```

#### Update Architecture Documentation (`docs/architecture.md`)

Add document text storage architecture section:

```markdown
## Document Text Storage Architecture

### Overview

Document text storage is implemented at the index level (not shard level) using append-only memory-mapped files for efficient access and ACID transactions.

### File Layout

```
index_directory/
├── text_index.dat     # Document text index entries
├── text_data.dat      # Raw document text data
├── shard_0/           # Vector postings
├── shard_1/
└── wal.log           # Write-ahead log
```

### Memory Mapping Strategy

- **Index File**: Memory-mapped for O(n) backward search to find latest document versions
- **Data File**: Memory-mapped for O(1) text retrieval after index lookup
- **OS Page Cache**: Leverages operating system page caching for performance

### Transaction Coordination

Document text operations are fully integrated with Shardex's WAL system:

1. **Atomic Replacement**: Text and postings updated together in single transaction
2. **Crash Recovery**: WAL replay handles text operations during recovery
3. **Consistency**: ACID properties maintained across all operations

### Performance Characteristics

- **Storage**: O(n) space where n is total text size across all document versions
- **Lookup**: O(d) time where d is number of document versions (typically small)
- **Extraction**: O(1) time after lookup (memory-mapped access)
- **Concurrency**: Multiple readers, single writer per transaction
```

### Create Comprehensive Examples

#### Basic Usage Example (`examples/document_text_basic.rs`)

```rust
//! Basic document text storage example
//!
//! This example demonstrates the fundamental document text storage operations:
//! - Storing documents with text and postings
//! - Retrieving full document text
//! - Extracting text snippets from search results

use shardex::{Shardex, ShardexImpl, ShardexConfig, Posting, DocumentId, ShardexError};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Configure Shardex with text storage enabled
    let config = ShardexConfig {
        directory_path: "./example_index".into(),
        vector_dimension: 128,
        max_document_text_size: 1024 * 1024, // 1MB per document
        ..Default::default()
    };

    let mut shardex = ShardexImpl::create(config).await?;

    // Document content and embeddings
    let document_text = "The quick brown fox jumps over the lazy dog. This is a sample document for testing.";
    let doc_id = DocumentId::new();

    // Create postings with embeddings (simplified for example)
    let postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 9, // "The quick"
            vector: create_sample_embedding(128),
        },
        Posting {
            document_id: doc_id,
            start: 10,
            length: 9, // "brown fox"
            vector: create_sample_embedding(128),
        },
        Posting {
            document_id: doc_id,
            start: 20,
            length: 5, // "jumps"
            vector: create_sample_embedding(128),
        },
    ];

    // Store document with text and postings atomically
    println!("Storing document with text and postings...");
    shardex.replace_document_with_postings(doc_id, document_text.to_string(), postings.clone()).await?;

    // Retrieve full document text
    println!("\\nRetrieving full document text:");
    let retrieved_text = shardex.get_document_text(doc_id).await?;
    println!("Full text: '{}'", retrieved_text);

    // Extract text from individual postings
    println!("\\nExtracting text from postings:");
    for (i, posting) in postings.iter().enumerate() {
        let extracted_text = shardex.extract_text(posting).await?;
        println!("Posting {}: '{}'", i + 1, extracted_text);
    }

    // Demonstrate search integration
    println!("\\nSearching and extracting text from results:");
    let search_results = shardex.search(&postings[0].vector, 3, None).await?;
    
    for (i, result) in search_results.iter().enumerate() {
        // Create posting from search result
        let result_posting = Posting {
            document_id: result.document_id,
            start: result.start,
            length: result.length,
            vector: result.vector.clone(),
        };
        
        let result_text = shardex.extract_text(&result_posting).await?;
        println!("Result {}: '{}' (score: {:.4})", i + 1, result_text, result.similarity_score);
    }

    Ok(())
}

// Helper function to create sample embeddings
fn create_sample_embedding(dimension: usize) -> Vec<f32> {
    (0..dimension).map(|i| (i as f32) / (dimension as f32)).collect()
}
```

#### Advanced Usage Example (`examples/document_text_advanced.rs`)

```rust
//! Advanced document text storage example
//!
//! This example demonstrates advanced features:
//! - Batch document operations
//! - Error handling patterns
//! - Document updates and versioning
//! - Performance optimization techniques

use shardex::{Shardex, ShardexImpl, ShardexConfig, Posting, DocumentId, ShardexError};
use std::collections::HashMap;
use std::error::Error;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = ShardexConfig {
        directory_path: "./advanced_example_index".into(),
        vector_dimension: 256,
        max_document_text_size: 50 * 1024 * 1024, // 50MB per document
        ..Default::default()
    };

    let mut shardex = ShardexImpl::create(config).await?;

    // Batch document processing
    batch_document_processing(&mut shardex).await?;
    
    // Document updates and versioning
    document_updates_example(&mut shardex).await?;
    
    // Error handling demonstration
    error_handling_examples(&shardex).await?;
    
    // Performance measurement
    performance_example(&mut shardex).await?;

    Ok(())
}

async fn batch_document_processing(shardex: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("=== Batch Document Processing ===");
    
    let documents = vec![
        ("Document 1: Introduction to machine learning concepts.", create_sample_postings(1)),
        ("Document 2: Deep learning and neural networks overview.", create_sample_postings(2)),
        ("Document 3: Natural language processing techniques.", create_sample_postings(3)),
    ];

    let start = Instant::now();
    
    for (i, (text, postings)) in documents.iter().enumerate() {
        let doc_id = DocumentId::new();
        
        // Adjust posting document IDs
        let adjusted_postings: Vec<Posting> = postings.iter().map(|p| Posting {
            document_id: doc_id,
            start: p.start,
            length: p.length,
            vector: p.vector.clone(),
        }).collect();
        
        shardex.replace_document_with_postings(doc_id, text.to_string(), adjusted_postings).await?;
        println!("Stored document {}: {} characters", i + 1, text.len());
    }
    
    let duration = start.elapsed();
    println!("Batch processing completed in {:?}\\n", duration);
    
    Ok(())
}

async fn document_updates_example(shardex: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("=== Document Updates and Versioning ===");
    
    let doc_id = DocumentId::new();
    
    // Version 1
    let v1_text = "Original document content.";
    let v1_postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 8, // "Original"
            vector: create_sample_embedding(256),
        }
    ];
    
    shardex.replace_document_with_postings(doc_id, v1_text.to_string(), v1_postings).await?;
    println!("Stored version 1: '{}'", shardex.get_document_text(doc_id).await?);
    
    // Version 2 (complete replacement)
    let v2_text = "Updated document with new content and structure.";
    let v2_postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 7, // "Updated"
            vector: create_sample_embedding(256),
        },
        Posting {
            document_id: doc_id,
            start: 8,
            length: 8, // "document"
            vector: create_sample_embedding(256),
        }
    ];
    
    shardex.replace_document_with_postings(doc_id, v2_text.to_string(), v2_postings).await?;
    println!("Stored version 2: '{}'", shardex.get_document_text(doc_id).await?);
    
    println!("Document versioning completed\\n");
    Ok(())
}

async fn error_handling_examples(shardex: &ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("=== Error Handling Examples ===");
    
    // Test document not found
    let nonexistent_doc = DocumentId::new();
    match shardex.get_document_text(nonexistent_doc).await {
        Ok(_) => println!("Unexpected success for nonexistent document"),
        Err(ShardexError::DocumentTextNotFound { document_id }) => {
            println!("✓ Correctly handled missing document: {}", document_id);
        }
        Err(e) => println!("Unexpected error: {}", e),
    }
    
    // Test invalid range extraction
    let doc_id = DocumentId::new();
    let text = "Short text";
    let postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 5, // "Short"
            vector: create_sample_embedding(256),
        }
    ];
    
    shardex.replace_document_with_postings(doc_id, text.to_string(), postings).await?;
    
    // Try to extract beyond document bounds
    let invalid_posting = Posting {
        document_id: doc_id,
        start: 5,
        length: 20, // Beyond document end
        vector: create_sample_embedding(256),
    };
    
    match shardex.extract_text(&invalid_posting).await {
        Ok(_) => println!("Unexpected success for invalid range"),
        Err(ShardexError::InvalidRange { start, length, document_length }) => {
            println!("✓ Correctly handled invalid range: {}..{} for document length {}", 
                     start, start + length, document_length);
        }
        Err(e) => println!("Unexpected error: {}", e),
    }
    
    println!("Error handling examples completed\\n");
    Ok(())
}

async fn performance_example(shardex: &mut ShardexImpl) -> Result<(), Box<dyn Error>> {
    println!("=== Performance Measurement ===");
    
    let large_text = "Lorem ipsum ".repeat(10000); // ~110KB
    let doc_id = DocumentId::new();
    
    let postings = vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 100,
            vector: create_sample_embedding(256),
        }
    ];
    
    // Measure storage performance
    let start = Instant::now();
    shardex.replace_document_with_postings(doc_id, large_text.clone(), postings.clone()).await?;
    let store_time = start.elapsed();
    
    // Measure retrieval performance
    let start = Instant::now();
    let retrieved = shardex.get_document_text(doc_id).await?;
    let retrieve_time = start.elapsed();
    
    // Measure extraction performance
    let start = Instant::now();
    let extracted = shardex.extract_text(&postings[0]).await?;
    let extract_time = start.elapsed();
    
    println!("Performance results for {}KB document:", large_text.len() / 1024);
    println!("  Store time: {:?}", store_time);
    println!("  Retrieve time: {:?}", retrieve_time);
    println!("  Extract time: {:?}", extract_time);
    println!("  Extracted text length: {}", extracted.len());
    
    assert_eq!(retrieved.len(), large_text.len());
    assert_eq!(extracted.len(), 100);
    
    Ok(())
}

fn create_sample_postings(seed: u64) -> Vec<Posting> {
    let doc_id = DocumentId::new(); // Will be adjusted in calling code
    
    vec![
        Posting {
            document_id: doc_id,
            start: 0,
            length: 10,
            vector: create_sample_embedding_with_seed(256, seed),
        },
        Posting {
            document_id: doc_id,
            start: 11,
            length: 8,
            vector: create_sample_embedding_with_seed(256, seed + 1),
        }
    ]
}

fn create_sample_embedding(dimension: usize) -> Vec<f32> {
    (0..dimension).map(|i| (i as f32) / (dimension as f32)).collect()
}

fn create_sample_embedding_with_seed(dimension: usize, seed: u64) -> Vec<f32> {
    (0..dimension).map(|i| ((i as u64 + seed) as f32) / (dimension as f32)).collect()
}
```

#### Configuration Example (`examples/document_text_configuration.rs`)

```rust
//! Document text storage configuration example
//!
//! Demonstrates various configuration options and their effects

use shardex::{ShardexConfig, ShardexImpl, DistanceMetric};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Memory-constrained configuration
    memory_constrained_config().await?;
    
    // High-capacity configuration  
    high_capacity_config().await?;
    
    // Performance-optimized configuration
    performance_optimized_config().await?;

    Ok(())
}

async fn memory_constrained_config() -> Result<(), Box<dyn Error>> {
    println!("=== Memory-Constrained Configuration ===");
    
    let config = ShardexConfig {
        directory_path: "./memory_constrained_index".into(),
        vector_dimension: 64,  // Smaller vectors
        max_postings_per_shard: 10_000, // Smaller shards
        max_document_text_size: 512 * 1024, // 512KB per document
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    
    println!("Configuration: {:?}", config);
    let _shardex = ShardexImpl::create(config).await?;
    println!("Memory-constrained index created successfully\\n");
    
    Ok(())
}

async fn high_capacity_config() -> Result<(), Box<dyn Error>> {
    println!("=== High-Capacity Configuration ===");
    
    let config = ShardexConfig {
        directory_path: "./high_capacity_index".into(),
        vector_dimension: 1024, // Large vectors
        max_postings_per_shard: 1_000_000, // Large shards
        max_document_text_size: 100 * 1024 * 1024, // 100MB per document
        distance_metric: DistanceMetric::Euclidean,
        ..Default::default()
    };
    
    println!("Configuration: {:?}", config);
    let _shardex = ShardexImpl::create(config).await?;
    println!("High-capacity index created successfully\\n");
    
    Ok(())
}

async fn performance_optimized_config() -> Result<(), Box<dyn Error>> {
    println!("=== Performance-Optimized Configuration ===");
    
    let config = ShardexConfig {
        directory_path: "./performance_optimized_index".into(),
        vector_dimension: 256,
        max_postings_per_shard: 100_000,
        max_document_text_size: 10 * 1024 * 1024, // 10MB per document
        distance_metric: DistanceMetric::Cosine, // Generally fastest
        ..Default::default()
    };
    
    println!("Configuration: {:?}", config);
    let _shardex = ShardexImpl::create(config).await?;
    println!("Performance-optimized index created successfully\\n");
    
    Ok(())
}
```

### Update Getting Started Guide

Update `docs/getting-started.md` to include document text storage:

```markdown
## Document Text Storage

Shardex can store document source text alongside vector embeddings, enabling rich search experiences:

```rust
use shardex::{Shardex, ShardexImpl, ShardexConfig, Posting, DocumentId};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable text storage in configuration
    let config = ShardexConfig {
        directory_path: "./my_index".into(),
        vector_dimension: 128,
        max_document_text_size: 1024 * 1024, // 1MB per document
        ..Default::default()
    };

    let mut shardex = ShardexImpl::create(config).await?;

    // Store document with text and postings
    let document_text = "The quick brown fox jumps over the lazy dog.";
    let doc_id = DocumentId::new();
    
    let postings = vec![
        Posting::new(doc_id, 0, 9, embedding1, 128)?,   // "The quick"
        Posting::new(doc_id, 10, 9, embedding2, 128)?,  // "brown fox"
    ];

    // Atomic replacement of document text and postings
    shardex.replace_document_with_postings(doc_id, document_text.to_string(), postings).await?;

    // Retrieve full document text
    let full_text = shardex.get_document_text(doc_id).await?;
    println!("Document: {}", full_text);

    // Search and extract text from results
    let results = shardex.search(&query_vector, 10, None).await?;
    for result in results {
        let posting = Posting::from_search_result(result);
        let text_snippet = shardex.extract_text(&posting).await?;
        println!("Found: '{}'", text_snippet);
    }

    Ok(())
}
```
```

### Create Migration Guide

Create `docs/text-storage-migration.md` for existing users:

```markdown
# Migrating to Document Text Storage

This guide helps existing Shardex users migrate to document text storage functionality.

## Compatibility

Document text storage is **fully backward compatible**:

- Existing indexes work unchanged without text storage
- No breaking changes to existing APIs
- Text storage is opt-in via configuration

## Enabling Text Storage

### For New Indexes

Simply set `max_document_text_size` in your configuration:

```rust
let config = ShardexConfig {
    directory_path: "./my_index".into(),
    vector_dimension: 128,
    max_document_text_size: 10 * 1024 * 1024, // Enable with 10MB limit
    ..Default::default()
};
```

### For Existing Indexes

1. **Add Configuration**: Update your configuration to include `max_document_text_size`
2. **Gradual Migration**: Text storage is created when first used
3. **No Downtime**: Existing functionality continues to work

```rust
// Your existing code continues to work
shardex.add_postings(postings).await?;
let results = shardex.search(&query, 10, None).await?;

// New text storage features available when configured
if shardex.has_text_storage() {
    let text = shardex.get_document_text(doc_id).await?;
}
```

## Migration Strategies

### Strategy 1: Immediate Migration

Replace existing document operations with atomic replacement:

```rust
// Old approach
shardex.remove_documents(vec![doc_id]).await?;
shardex.add_postings(new_postings).await?;

// New approach (atomic)
shardex.replace_document_with_postings(doc_id, text, new_postings).await?;
```

### Strategy 2: Gradual Migration

Keep existing code, add text storage for new documents:

```rust
// Existing documents (no text storage)
shardex.add_postings(legacy_postings).await?;

// New documents (with text storage)
shardex.replace_document_with_postings(new_doc_id, text, postings).await?;
```

## Performance Considerations

- **Storage Overhead**: ~32 bytes per document + text size
- **Memory Usage**: Memory-mapped files, OS manages paging
- **Query Performance**: Text extraction is very fast (memory-mapped)
- **Write Performance**: Atomic operations may be slower than individual operations

## Best Practices

1. **Size Limits**: Set appropriate `max_document_text_size` based on your documents
2. **Batch Operations**: Use atomic replacement for better performance
3. **Error Handling**: Handle new error types (`DocumentTextNotFound`, `InvalidRange`, etc.)
4. **Monitoring**: Monitor text storage metrics if using existing monitoring

## Troubleshooting

### Common Issues

1. **"Text storage not enabled"**: Ensure `max_document_text_size > 0` in configuration
2. **Document too large**: Increase `max_document_text_size` or split documents
3. **Invalid range**: Verify posting coordinates match document text

### Getting Help

- Check error messages for specific guidance
- Review examples in `examples/document_text_*.rs`
- Consult API documentation for method details
```

## Implementation Requirements

1. **Comprehensive Coverage**: Documentation covers all features and use cases
2. **Clear Examples**: Examples are practical and well-commented
3. **Migration Support**: Existing users can migrate smoothly
4. **Performance Guidance**: Clear performance characteristics and optimization tips
5. **Troubleshooting**: Common issues and solutions documented

## Validation Criteria

- [ ] API documentation is complete and accurate
- [ ] Examples compile and run successfully
- [ ] Migration guide addresses common scenarios
- [ ] Performance characteristics clearly documented
- [ ] Troubleshooting covers common issues
- [ ] Documentation integrated with existing docs
- [ ] Code examples follow best practices

## Integration Points

- Updates existing documentation structure
- Uses examples from all previous steps
- Integrates with existing getting-started guide
- Compatible with existing documentation format

## Completion

This final step provides complete documentation and examples, enabling users to effectively utilize all document text storage functionality implemented in the previous 15 steps.
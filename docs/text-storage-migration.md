# Migrating to Document Text Storage

This guide helps existing Shardex users migrate to document text storage functionality introduced in recent versions.

## Compatibility Overview

Document text storage is designed with **full backward compatibility**:

- ✅ Existing indexes work unchanged without text storage
- ✅ No breaking changes to existing APIs
- ✅ Text storage is opt-in via configuration
- ✅ All existing code continues to function exactly as before

## Quick Start: Enabling Text Storage

### For New Projects

Simply add `max_document_text_size` to your configuration:

```rust
let config = ShardexConfig::new()
    .directory_path("./my_index")
    .vector_size(128)
    .max_document_text_size(10 * 1024 * 1024); // Enable with 10MB limit

let mut index = ShardexImpl::create(config).await?;
```

### For Existing Projects

Update your existing configuration - no code changes required:

```rust
// Your existing code works unchanged
let config = ShardexConfig::new()
    .directory_path("./existing_index")
    .vector_size(384)
    .shard_size(50000)
    // Add this line to enable text storage
    .max_document_text_size(5 * 1024 * 1024); // 5MB per document

// All your existing operations continue to work
index.add_postings(postings).await?;
let results = index.search(&query, 10, None).await?;
```

## Migration Strategies

### Strategy 1: Immediate Full Migration

**Best for**: New features, clean rewrites, or when you can update all code at once.

Replace existing document operations with atomic replacement:

```rust
// OLD approach (still works, but not atomic)
index.remove_documents(vec![doc_id]).await?;
index.add_postings(new_postings).await?;

// NEW approach (atomic operation with text storage)
index.replace_document_with_postings(doc_id, text, new_postings).await?;
```

**Benefits:**
- Atomic operations ensure data consistency
- Enables rich text extraction features
- Cleaner code with fewer operations

**Migration steps:**
1. Update your configuration to enable text storage
2. Replace `remove_documents` + `add_postings` with `replace_document_with_postings`
3. Add text extraction to your search results processing

### Strategy 2: Gradual Migration

**Best for**: Large codebases, production systems, or when you want to test incrementally.

Keep existing code unchanged, add text storage only for new documents:

```rust
// Existing documents continue to work (no text storage)
index.add_postings(legacy_postings).await?;
let legacy_results = index.search(&query, 10, None).await?;

// New documents can include text storage
index.replace_document_with_postings(new_doc_id, text, postings).await?;

// Handle mixed results
for result in search_results {
    match index.get_document_text(result.document_id).await {
        Ok(text) => println!("Document with text: {}", text),
        Err(ShardexError::DocumentTextNotFound { .. }) => {
            println!("Legacy document without text");
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

**Benefits:**
- Zero risk to existing functionality
- Incremental rollout and testing
- Easy rollback if needed

### Strategy 3: Feature-Flag Approach

**Best for**: Applications with feature toggles or A/B testing infrastructure.

Use runtime configuration to control text storage usage:

```rust
struct AppConfig {
    enable_text_storage: bool,
    max_text_size: usize,
}

async fn store_document(
    index: &mut ShardexImpl,
    doc_id: DocumentId,
    text: Option<String>,
    postings: Vec<Posting>,
    config: &AppConfig,
) -> Result<(), ShardexError> {
    if config.enable_text_storage && text.is_some() {
        // New path: atomic replacement with text
        index.replace_document_with_postings(doc_id, text.unwrap(), postings).await
    } else {
        // Legacy path: postings only
        index.add_postings(postings).await
    }
}
```

## Detailed Migration Guide

### Step 1: Update Dependencies

Ensure you're using a version of Shardex that supports document text storage:

```toml
[dependencies]
shardex = "0.2" # Or later version with text storage support
```

### Step 2: Update Configuration

Add text storage configuration to your setup:

```rust
let config = ShardexConfig::new()
    .directory_path("./my_index")
    .vector_size(384)
    // Add these configuration options:
    .max_document_text_size(10 * 1024 * 1024) // 10MB per document
    .shard_size(50000) // Existing config continues to work
    .batch_write_interval_ms(100); // Existing config continues to work
```

**Configuration recommendations by use case:**

```rust
// Chat/Messaging (small, frequent updates)
.max_document_text_size(16 * 1024) // 16KB per message
.batch_write_interval_ms(25) // Low latency

// Document Search (medium documents)
.max_document_text_size(1024 * 1024) // 1MB per document  
.batch_write_interval_ms(50) // Balanced

// Academic Papers (large documents)
.max_document_text_size(50 * 1024 * 1024) // 50MB per document
.batch_write_interval_ms(100) // Throughput optimized
```

### Step 3: Update Document Storage Logic

Choose your migration approach:

#### Option A: Full Migration
```rust
// Before
async fn update_document(
    index: &mut ShardexImpl, 
    doc_id: DocumentId, 
    new_postings: Vec<Posting>
) -> Result<(), ShardexError> {
    index.remove_documents(vec![doc_id]).await?;
    index.add_postings(new_postings).await?;
    index.flush().await
}

// After  
async fn update_document(
    index: &mut ShardexImpl,
    doc_id: DocumentId, 
    text: String,
    new_postings: Vec<Posting>
) -> Result<(), ShardexError> {
    index.replace_document_with_postings(doc_id, text, new_postings).await
    // Note: No separate flush() needed - operation is already atomic
}
```

#### Option B: Gradual Migration
```rust
async fn store_document(
    index: &mut ShardexImpl,
    doc_id: DocumentId,
    text: Option<String>,
    postings: Vec<Posting>
) -> Result<(), ShardexError> {
    match text {
        Some(document_text) => {
            // Use new atomic operation for documents with text
            index.replace_document_with_postings(doc_id, document_text, postings).await
        }
        None => {
            // Use legacy approach for documents without text
            index.add_postings(postings).await
        }
    }
}
```

### Step 4: Update Search Result Processing

Add text extraction to your search result handling:

```rust
// Before
async fn process_search_results(
    index: &ShardexImpl,
    results: Vec<SearchResult>
) -> Result<(), ShardexError> {
    for result in results {
        println!("Document {} (score: {:.3})", 
                 result.document_id.to_u128(), 
                 result.similarity_score);
    }
    Ok(())
}

// After
async fn process_search_results(
    index: &ShardexImpl,
    results: Vec<SearchResult>
) -> Result<(), ShardexError> {
    for result in results {
        // Create posting for text extraction
        let posting = Posting {
            document_id: result.document_id,
            start: result.start,
            length: result.length,
            vector: result.vector,
        };
        
        // Try to extract text snippet
        match index.extract_text(&posting).await {
            Ok(text_snippet) => {
                println!("Document {} (score: {:.3}): '{}'",
                         result.document_id.to_u128(),
                         result.similarity_score,
                         text_snippet);
            }
            Err(ShardexError::DocumentTextNotFound { .. }) => {
                // Handle legacy documents without text gracefully
                println!("Document {} (score: {:.3}) [no text available]",
                         result.document_id.to_u128(),
                         result.similarity_score);
            }
            Err(e) => {
                eprintln!("Text extraction error: {}", e);
            }
        }
    }
    Ok(())
}
```

### Step 5: Add Error Handling

Update your error handling to cover text-specific errors:

```rust
use shardex::ShardexError;

async fn safe_text_extraction(
    index: &ShardexImpl,
    posting: &Posting
) -> Option<String> {
    match index.extract_text(posting).await {
        Ok(text) => Some(text),
        Err(ShardexError::DocumentTextNotFound { document_id }) => {
            eprintln!("No text stored for document {}", document_id);
            None
        }
        Err(ShardexError::InvalidRange { start, length, document_length }) => {
            eprintln!("Invalid coordinates {}..{} for document length {}", 
                     start, start + length, document_length);
            None
        }
        Err(ShardexError::DocumentTooLarge { size, max_size }) => {
            eprintln!("Document {} bytes exceeds limit {} bytes", size, max_size);
            None  
        }
        Err(e) => {
            eprintln!("Unexpected error: {}", e);
            None
        }
    }
}
```

## Migration Checklist

### Pre-Migration
- [ ] Update Shardex to a version with text storage support
- [ ] Choose migration strategy (immediate, gradual, or feature-flag)
- [ ] Determine appropriate `max_document_text_size` for your use case
- [ ] Back up existing indexes (if valuable data)

### During Migration
- [ ] Update configuration to enable text storage
- [ ] Update document storage logic (based on chosen strategy)
- [ ] Update search result processing to handle text extraction
- [ ] Add proper error handling for text-specific errors
- [ ] Test with a small subset of data first

### Post-Migration
- [ ] Verify all existing functionality still works
- [ ] Test new text storage features
- [ ] Monitor performance impact (should be minimal for vector operations)
- [ ] Update monitoring/logging to track text storage usage
- [ ] Update documentation for your team

## Performance Considerations

### Memory Usage
- **Text storage overhead**: ~32 bytes per document + actual text size
- **Memory mapping**: OS manages paging, no significant memory impact
- **Vector operations**: No performance change to existing vector search

### Disk Usage
- **Text files**: Additional `text_index.dat` and `text_data.dat` files
- **Estimate**: roughly 1.1x the total size of your document texts
- **Growth**: Files grow as documents are added/updated (old versions retained until compaction)

### Search Performance
- **Vector search**: No change - text storage doesn't affect vector operations
- **Text extraction**: Very fast - O(1) memory-mapped access after index lookup
- **Mixed queries**: Handle text-enabled and text-disabled documents seamlessly

## Testing Your Migration

### Unit Tests
```rust
#[tokio::test]
async fn test_migration_compatibility() -> Result<(), Box<dyn std::error::Error>> {
    let config = ShardexConfig::new()
        .directory_path("./test_migration")
        .vector_size(128)
        .max_document_text_size(1024);
    
    let mut index = ShardexImpl::create(config).await?;
    
    // Test 1: Legacy posting operations still work
    let legacy_posting = Posting {
        document_id: DocumentId::from_u128(1),
        start: 0,
        length: 100,
        vector: vec![0.1; 128],
    };
    
    index.add_postings(vec![legacy_posting.clone()]).await?;
    let results = index.search(&vec![0.1; 128], 1, None).await?;
    assert!(!results.is_empty());
    
    // Test 2: New text storage operations work
    let doc_text = "Test document with text";
    let text_posting = Posting {
        document_id: DocumentId::from_u128(2),
        start: 0,
        length: doc_text.len() as u32,
        vector: vec![0.2; 128],
    };
    
    index.replace_document_with_postings(
        text_posting.document_id,
        doc_text.to_string(),
        vec![text_posting]
    ).await?;
    
    let retrieved_text = index.get_document_text(DocumentId::from_u128(2)).await?;
    assert_eq!(retrieved_text, doc_text);
    
    // Test 3: Mixed results handling
    let all_results = index.search(&vec![0.15; 128], 5, None).await?;
    
    for result in all_results {
        match index.get_document_text(result.document_id).await {
            Ok(text) => println!("Document with text: {}", text),
            Err(ShardexError::DocumentTextNotFound { .. }) => {
                println!("Legacy document: {}", result.document_id.to_u128());
            }
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }
    
    Ok(())
}
```

### Integration Tests
```rust
#[tokio::test]
async fn test_large_scale_migration() -> Result<(), Box<dyn std::error::Error>> {
    let config = ShardexConfig::new()
        .directory_path("./test_large_migration")
        .vector_size(256)
        .max_document_text_size(10 * 1024); // 10KB per doc
    
    let mut index = ShardexImpl::create(config).await?;
    
    // Add 1000 legacy documents
    let mut legacy_postings = Vec::new();
    for i in 0..1000 {
        legacy_postings.push(Posting {
            document_id: DocumentId::from_u128(i),
            start: 0,
            length: 100,
            vector: generate_test_vector(i as f32, 256),
        });
    }
    
    index.add_postings(legacy_postings).await?;
    
    // Add 1000 text-enabled documents
    for i in 1000..2000 {
        let doc_text = format!("Document {} with text content for testing", i);
        let posting = Posting {
            document_id: DocumentId::from_u128(i),
            start: 0,
            length: doc_text.len() as u32,
            vector: generate_test_vector(i as f32, 256),
        };
        
        index.replace_document_with_postings(
            posting.document_id,
            doc_text,
            vec![posting]
        ).await?;
    }
    
    // Test search across both types
    let query = generate_test_vector(1500.0, 256);
    let results = index.search(&query, 50, None).await?;
    
    let mut text_enabled_count = 0;
    let mut legacy_count = 0;
    
    for result in results {
        match index.get_document_text(result.document_id).await {
            Ok(_) => text_enabled_count += 1,
            Err(ShardexError::DocumentTextNotFound { .. }) => legacy_count += 1,
            Err(e) => panic!("Unexpected error: {}", e),
        }
    }
    
    println!("Mixed results: {} with text, {} legacy", text_enabled_count, legacy_count);
    assert!(text_enabled_count > 0);
    assert!(legacy_count > 0);
    
    Ok(())
}

fn generate_test_vector(seed: f32, dimension: usize) -> Vec<f32> {
    (0..dimension).map(|i| (seed + i as f32) / dimension as f32).collect()
}
```

## Rollback Procedures

If you need to rollback your migration:

### Option 1: Configuration Rollback
Simply set `max_document_text_size = 0` to disable text storage:

```rust
let config = ShardexConfig::new()
    .directory_path("./my_index")
    .vector_size(384)
    .max_document_text_size(0); // Disable text storage
```

All text storage operations will return `DocumentTextNotFound` errors, but vector operations continue normally.

### Option 2: Code Rollback
Revert to previous version of your code:

```rust
// Remove text-related operations
// index.replace_document_with_postings(...) 
// becomes:
index.add_postings(postings).await?;

// Remove text extraction from search results
// Keep only the similarity scores and document IDs
```

### Option 3: Full Index Rebuild
If you want to completely remove text storage:

1. Export all vector data from existing index
2. Create new index with `max_document_text_size = 0`
3. Re-import only the vector data
4. Text storage files can be deleted

## Common Issues and Solutions

### Issue: "Document text not found" errors after migration
**Cause**: Trying to access text for documents added before text storage was enabled.

**Solution**: Handle the error gracefully:
```rust
match index.get_document_text(doc_id).await {
    Ok(text) => println!("Text: {}", text),
    Err(ShardexError::DocumentTextNotFound { .. }) => {
        println!("Legacy document - no text available");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### Issue: Performance degradation after enabling text storage
**Cause**: Usually not caused by text storage itself, but by changes in usage patterns.

**Solutions:**
1. Monitor actual memory usage - text storage uses memory-mapped files
2. Check if you're accidentally reading large documents frequently
3. Verify your `max_document_text_size` is appropriate
4. Use text extraction selectively, not for every search result

### Issue: "Document too large" errors
**Cause**: Documents exceed the `max_document_text_size` limit.

**Solutions:**
```rust
// Increase limit if appropriate
.max_document_text_size(50 * 1024 * 1024) // 50MB

// Or split large documents into smaller chunks
fn split_large_document(text: &str, max_size: usize) -> Vec<String> {
    text.chars()
        .collect::<Vec<char>>()
        .chunks(max_size)
        .map(|chunk| chunk.iter().collect())
        .collect()
}
```

### Issue: WAL files growing large after migration
**Cause**: Document replacement operations can generate more WAL entries.

**Solution**: Ensure regular flushing:
```rust
// Flush periodically for long-running operations
if batch_count % 100 == 0 {
    index.flush().await?;
}
```

## Best Practices

1. **Start Small**: Test with a small subset of your data first
2. **Monitor Performance**: Watch memory usage and search latency during migration
3. **Graceful Degradation**: Always handle `DocumentTextNotFound` errors gracefully
4. **Atomic Operations**: Use `replace_document_with_postings` for consistency
5. **Size Limits**: Set appropriate document size limits for your use case
6. **Error Handling**: Implement comprehensive error handling for all text operations
7. **Testing**: Test both text-enabled and legacy document scenarios
8. **Documentation**: Update your team's documentation about the new capabilities

## Getting Help

If you encounter issues during migration:

1. **Check Error Messages**: Text storage errors are descriptive and include suggestions
2. **Review Examples**: Look at `examples/document_text_*.rs` for working code
3. **Test Incrementally**: Migrate small portions at a time to isolate issues
4. **Community Support**: File issues on GitHub with minimal reproduction cases
5. **Rollback Plan**: Always have a rollback strategy ready

## Summary

Document text storage migration is designed to be:
- **Safe**: Full backward compatibility with existing code
- **Gradual**: Multiple migration strategies to fit your needs  
- **Robust**: Comprehensive error handling and recovery procedures
- **Flexible**: Enable/disable at configuration level

Choose the migration strategy that best fits your project's constraints and requirements. The gradual migration approach is recommended for production systems, while immediate migration works well for new features or development environments.
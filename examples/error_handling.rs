//! Error handling example for Shardex
//!
//! This example demonstrates:
//! - Comprehensive error handling patterns
//! - Recovery from various failure scenarios
//! - Input validation and error prevention
//! - Robust application patterns

use shardex::{Shardex, ShardexConfig, ShardexImpl, ShardexError, Posting, DocumentId};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Shardex Error Handling Example");
    println!("===============================");

    let temp_dir = std::env::temp_dir().join("shardex_error_example");
    if temp_dir.exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    std::fs::create_dir_all(&temp_dir)?;

    // Example 1: Configuration validation errors
    println!("\n1. Configuration Validation");
    println!("===========================");

    demonstrate_config_errors(&temp_dir).await;

    // Example 2: Create a valid index for further tests
    let config = ShardexConfig::new()
        .directory_path(&temp_dir)
        .vector_size(128);

    let mut index = match ShardexImpl::create(config.clone()).await {
        Ok(index) => {
            println!("✓ Successfully created index");
            index
        }
        Err(e) => {
            eprintln!("✗ Failed to create index: {}", e);
            return Err(e.into());
        }
    };

    // Example 3: Input validation errors
    println!("\n2. Input Validation");
    println!("===================");

    demonstrate_input_validation(&mut index).await;

    // Example 4: File system and I/O errors
    println!("\n3. File System and I/O Errors");
    println!("==============================");

    demonstrate_io_errors(&temp_dir).await;

    // Example 5: Search parameter errors
    println!("\n4. Search Parameter Validation");
    println!("===============================");

    demonstrate_search_errors(&index).await;

    // Example 6: Robust error handling patterns
    println!("\n5. Robust Error Handling Patterns");
    println!("==================================");

    demonstrate_robust_patterns(&temp_dir).await?;

    // Example 7: Error recovery strategies
    println!("\n6. Error Recovery Strategies");
    println!("============================");

    demonstrate_recovery_strategies(&temp_dir).await?;

    // Clean up
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\nError handling examples completed!");

    Ok(())
}

async fn demonstrate_config_errors(temp_dir: &std::path::Path) {
    // Invalid vector size
    let invalid_config = ShardexConfig::new()
        .directory_path(temp_dir.join("invalid1"))
        .vector_size(0);

    match ShardexImpl::create(invalid_config).await {
        Ok(_) => println!("✗ Unexpected success with invalid vector size"),
        Err(ShardexError::Config(msg)) => {
            println!("✓ Caught configuration error: {}", msg);
        }
        Err(e) => println!("✗ Unexpected error type: {}", e),
    }

    // Invalid shard size
    let invalid_config = ShardexConfig::new()
        .directory_path(temp_dir.join("invalid2"))
        .shard_size(0);

    match ShardexImpl::create(invalid_config).await {
        Ok(_) => println!("✗ Unexpected success with invalid shard size"),
        Err(ShardexError::Config(msg)) => {
            println!("✓ Caught configuration error: {}", msg);
        }
        Err(e) => println!("✗ Unexpected error type: {}", e),
    }

    // Invalid directory path (empty)
    let invalid_config = ShardexConfig::new()
        .directory_path("");

    match ShardexImpl::create(invalid_config).await {
        Ok(_) => println!("✗ Unexpected success with empty directory path"),
        Err(e) => {
            println!("✓ Caught error for empty path: {}", e);
        }
    }
}

async fn demonstrate_input_validation(index: &mut ShardexImpl) {
    // Wrong vector dimension
    let posting_wrong_dim = Posting {
        document_id: DocumentId::from_raw(1),
        start: 0,
        length: 100,
        vector: vec![0.1, 0.2], // Wrong size: expected 128, got 2
    };

    match index.add_postings(vec![posting_wrong_dim]).await {
        Ok(_) => println!("✗ Unexpected success with wrong vector dimension"),
        Err(ShardexError::InvalidDimension { expected, actual }) => {
            println!("✓ Caught dimension error: expected {}, got {}", expected, actual);
        }
        Err(e) => println!("✗ Unexpected error type: {}", e),
    }

    // Empty vector
    let posting_empty_vector = Posting {
        document_id: DocumentId::from_raw(2),
        start: 0,
        length: 100,
        vector: vec![], // Empty vector
    };

    match index.add_postings(vec![posting_empty_vector]).await {
        Ok(_) => println!("✗ Unexpected success with empty vector"),
        Err(e) => {
            println!("✓ Caught error for empty vector: {}", e);
        }
    }

    // Empty postings list
    match index.add_postings(vec![]).await {
        Ok(_) => println!("✓ Empty postings list handled gracefully"),
        Err(e) => println!("✗ Unexpected error for empty postings: {}", e),
    }

    // Invalid document IDs for removal
    match index.remove_documents(vec![]).await {
        Ok(_) => println!("✓ Empty document list handled gracefully"),
        Err(e) => println!("✗ Unexpected error for empty documents: {}", e),
    }
}

async fn demonstrate_io_errors(temp_dir: &std::path::Path) {
    // Try to open non-existent index
    let non_existent = temp_dir.join("does_not_exist");
    match ShardexImpl::open(&non_existent).await {
        Ok(_) => println!("✗ Unexpected success opening non-existent index"),
        Err(ShardexError::Io(io_err)) => {
            println!("✓ Caught I/O error for non-existent index: {}", io_err);
        }
        Err(e) => println!("✗ Unexpected error type: {}", e),
    }

    // Create a file where directory should be
    let file_path = temp_dir.join("file_not_dir");
    std::fs::write(&file_path, "not a directory").unwrap();

    let config = ShardexConfig::new().directory_path(&file_path);
    match ShardexImpl::create(config).await {
        Ok(_) => println!("✗ Unexpected success with file instead of directory"),
        Err(e) => {
            println!("✓ Caught error when directory path is a file: {}", e);
        }
    }
}

async fn demonstrate_search_errors(index: &ShardexImpl) {
    // Wrong query vector dimension
    let wrong_query = vec![0.1, 0.2]; // Wrong size: expected 128, got 2
    match index.search(&wrong_query, 5, None).await {
        Ok(_) => println!("✗ Unexpected success with wrong query dimension"),
        Err(ShardexError::InvalidDimension { expected, actual }) => {
            println!("✓ Caught search dimension error: expected {}, got {}", expected, actual);
        }
        Err(e) => println!("✗ Unexpected error type: {}", e),
    }

    // Empty query vector
    let empty_query = vec![];
    match index.search(&empty_query, 5, None).await {
        Ok(_) => println!("✗ Unexpected success with empty query"),
        Err(e) => {
            println!("✓ Caught error for empty query: {}", e);
        }
    }

    // Invalid k value
    let valid_query = vec![0.0; 128];
    match index.search(&valid_query, 0, None).await {
        Ok(_) => println!("✗ Unexpected success with k=0"),
        Err(e) => {
            println!("✓ Caught error for k=0: {}", e);
        }
    }
}

async fn demonstrate_robust_patterns(temp_dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    println!("Demonstrating robust error handling patterns...");

    // Pattern 1: Retry with backoff
    println!("\n  Pattern 1: Retry with backoff");
    let result = retry_with_backoff(
        || async {
            // Simulate operation that might fail
            let config = ShardexConfig::new()
                .directory_path(temp_dir.join("robust_index"));
            ShardexImpl::create(config).await
        },
        3, // max retries
    ).await;

    match result {
        Ok(_) => println!("    ✓ Operation succeeded"),
        Err(e) => println!("    ✗ Operation failed after retries: {}", e),
    }

    // Pattern 2: Graceful degradation
    println!("\n  Pattern 2: Graceful degradation");
    let mut index = create_or_recover_index(temp_dir.join("graceful_index")).await?;

    // Try to add some data
    let postings = vec![
        Posting {
            document_id: DocumentId::from_raw(1),
            start: 0,
            length: 100,
            vector: vec![0.1; 128],
        }
    ];

    match index.add_postings(postings).await {
        Ok(_) => {
            println!("    ✓ Successfully added postings");
            match index.flush().await {
                Ok(_) => println!("    ✓ Successfully flushed data"),
                Err(e) => {
                    println!("    ⚠ Flush failed, but data is still in WAL: {}", e);
                    // Continue operation - data will be recovered on restart
                }
            }
        }
        Err(e) => println!("    ✗ Failed to add postings: {}", e),
    }

    Ok(())
}

async fn demonstrate_recovery_strategies(temp_dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    println!("Demonstrating error recovery strategies...");

    // Strategy 1: Automatic retry for transient errors
    println!("\n  Strategy 1: Automatic retry for transient errors");
    
    let index_path = temp_dir.join("recovery_index");
    let mut attempts = 0;
    
    let index = loop {
        attempts += 1;
        let config = ShardexConfig::new().directory_path(&index_path);
        
        match ShardexImpl::create(config).await {
            Ok(index) => {
                println!("    ✓ Index created successfully on attempt {}", attempts);
                break index;
            }
            Err(ShardexError::Io(_)) if attempts < 3 => {
                println!("    ⚠ I/O error on attempt {}, retrying...", attempts);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                continue;
            }
            Err(e) => {
                println!("    ✗ Failed to create index after {} attempts: {}", attempts, e);
                return Err(e.into());
            }
        }
    };

    // Strategy 2: Validate before operations
    println!("\n  Strategy 2: Input validation before operations");
    
    let validate_and_add = |postings: Vec<Posting>| async {
        // Pre-validate postings
        for (i, posting) in postings.iter().enumerate() {
            if posting.vector.is_empty() {
                return Err(format!("Posting {} has empty vector", i));
            }
            if posting.vector.len() != 128 {
                return Err(format!("Posting {} has wrong vector dimension: {}", i, posting.vector.len()));
            }
        }
        
        Ok(postings)
    };

    let test_postings = vec![
        Posting {
            document_id: DocumentId::from_raw(1),
            start: 0,
            length: 100,
            vector: vec![0.1; 128],
        }
    ];

    match validate_and_add(test_postings).await {
        Ok(validated_postings) => {
            println!("    ✓ Postings validated successfully");
            // Would proceed with index.add_postings(validated_postings)
        }
        Err(e) => println!("    ✗ Validation failed: {}", e),
    }

    Ok(())
}

async fn retry_with_backoff<F, Fut, T, E>(
    mut operation: F,
    max_retries: usize,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    for attempt in 0..max_retries {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt == max_retries - 1 {
                    return Err(e);
                }
                // Exponential backoff
                let delay = std::time::Duration::from_millis(100 * (2_u64.pow(attempt as u32)));
                tokio::time::sleep(delay).await;
            }
        }
    }
    
    unreachable!()
}

async fn create_or_recover_index(path: std::path::PathBuf) -> Result<ShardexImpl, ShardexError> {
    // Try to open existing index first
    match ShardexImpl::open(&path).await {
        Ok(index) => {
            println!("    ✓ Recovered existing index");
            Ok(index)
        }
        Err(_) => {
            // Create new index if opening failed
            let config = ShardexConfig::new().directory_path(path);
            let index = ShardexImpl::create(config).await?;
            println!("    ✓ Created new index");
            Ok(index)
        }
    }
}
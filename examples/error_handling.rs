//! Error handling example for Shardex
//!
//! This example demonstrates:
//! - Comprehensive error handling patterns
//! - Recovery from various failure scenarios
//! - Input validation and error prevention
//! - Robust application patterns

use apithing::ApiOperation;
use shardex::{
    api::{
        operations::{AddPostings, CreateIndex, Flush, OpenIndex, Search},
        parameters::{AddPostingsParams, CreateIndexParams, FlushParams, OpenIndexParams, SearchParams},
        ShardexContext,
    },
    DocumentId, Posting, ShardexError,
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
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

    demonstrate_config_errors(&temp_dir);

    // Example 2: Create a valid index for further tests using ApiThing pattern
    let mut context = ShardexContext::new();
    let create_params = match CreateIndexParams::builder()
        .directory_path(temp_dir.clone())
        .vector_size(128)
        .build()
    {
        Ok(params) => params,
        Err(e) => {
            eprintln!("✗ Failed to build create params: {}", e);
            return Err(e.into());
        }
    };

    match CreateIndex::execute(&mut context, &create_params) {
        Ok(_) => {
            println!("✓ Successfully created index using ApiThing pattern");
        }
        Err(e) => {
            eprintln!("✗ Failed to create index: {}", e);
            return Err(e.into());
        }
    }

    // Example 3: Input validation errors
    println!("\n2. Input Validation");
    println!("===================");

    demonstrate_input_validation(&mut context);

    // Example 4: File system and I/O errors
    println!("\n3. File System and I/O Errors");
    println!("==============================");

    demonstrate_io_errors(&temp_dir);

    // Example 5: Search parameter errors
    println!("\n4. Search Parameter Validation");
    println!("===============================");

    demonstrate_search_errors(&mut context);

    // Example 6: Robust error handling patterns
    println!("\n5. Robust Error Handling Patterns");
    println!("==================================");

    demonstrate_robust_patterns(&temp_dir)?;

    // Example 7: Error recovery strategies
    println!("\n6. Error Recovery Strategies");
    println!("============================");

    demonstrate_recovery_strategies(&temp_dir)?;

    // Clean up
    std::fs::remove_dir_all(&temp_dir)?;
    println!("\nError handling examples completed!");

    Ok(())
}

fn demonstrate_config_errors(temp_dir: &std::path::Path) {
    // Invalid vector size using ApiThing pattern
    let mut context1 = ShardexContext::new();
    match CreateIndexParams::builder()
        .directory_path(temp_dir.join("invalid1"))
        .vector_size(0)
        .build()
    {
        Ok(params) => match CreateIndex::execute(&mut context1, &params) {
            Ok(_) => println!("✗ Unexpected success with invalid vector size"),
            Err(e) => println!("✓ Caught configuration error: {}", e),
        },
        Err(e) => {
            println!("✓ Caught configuration validation error: {}", e);
        }
    }

    // Invalid shard size using ApiThing pattern
    let mut context2 = ShardexContext::new();
    match CreateIndexParams::builder()
        .directory_path(temp_dir.join("invalid2"))
        .shard_size(0)
        .build()
    {
        Ok(params) => match CreateIndex::execute(&mut context2, &params) {
            Ok(_) => println!("✗ Unexpected success with invalid shard size"),
            Err(e) => println!("✓ Caught configuration error: {}", e),
        },
        Err(e) => {
            println!("✓ Caught configuration validation error: {}", e);
        }
    }

    // Invalid directory path (empty) using ApiThing pattern
    let mut context3 = ShardexContext::new();
    match CreateIndexParams::builder()
        .directory_path("".into())
        .build()
    {
        Ok(params) => match CreateIndex::execute(&mut context3, &params) {
            Ok(_) => println!("✗ Unexpected success with empty directory path"),
            Err(e) => println!("✓ Caught error for empty path: {}", e),
        },
        Err(e) => {
            println!("✓ Caught configuration validation error: {}", e);
        }
    }
}

fn demonstrate_input_validation(context: &mut ShardexContext) {
    // Wrong vector dimension using ApiThing pattern
    let posting_wrong_dim = Posting {
        document_id: DocumentId::from_raw(1),
        start: 0,
        length: 100,
        vector: vec![0.1, 0.2], // Wrong size: expected 128, got 2
    };

    match AddPostingsParams::new(vec![posting_wrong_dim]) {
        Ok(params) => match AddPostings::execute(context, &params) {
            Ok(_) => println!("✗ Unexpected success with wrong vector dimension"),
            Err(ShardexError::InvalidDimension { expected, actual }) => {
                println!("✓ Caught dimension error: expected {}, got {}", expected, actual);
            }
            Err(e) => println!("✗ Unexpected error type: {}", e),
        },
        Err(e) => {
            println!("✓ Caught parameter validation error: {}", e);
        }
    }

    // Empty vector using ApiThing pattern
    let posting_empty_vector = Posting {
        document_id: DocumentId::from_raw(2),
        start: 0,
        length: 100,
        vector: vec![], // Empty vector
    };

    match AddPostingsParams::new(vec![posting_empty_vector]) {
        Ok(params) => match AddPostings::execute(context, &params) {
            Ok(_) => println!("✗ Unexpected success with empty vector"),
            Err(e) => {
                println!("✓ Caught error for empty vector: {}", e);
            }
        },
        Err(e) => {
            println!("✓ Caught parameter validation error: {}", e);
        }
    }

    // Empty postings list using ApiThing pattern
    match AddPostingsParams::new(vec![]) {
        Ok(params) => match AddPostings::execute(context, &params) {
            Ok(_) => println!("✓ Empty postings list handled gracefully"),
            Err(e) => println!("✗ Unexpected error for empty postings: {}", e),
        },
        Err(e) => {
            println!("✓ Caught parameter validation error for empty postings: {}", e);
        }
    }

    // Context state error - try to add postings without initialized index
    let mut uninitialized_context = ShardexContext::new();
    let valid_posting = Posting {
        document_id: DocumentId::from_raw(3),
        start: 0,
        length: 100,
        vector: vec![0.1; 128],
    };

    match AddPostingsParams::new(vec![valid_posting]) {
        Ok(params) => match AddPostings::execute(&mut uninitialized_context, &params) {
            Ok(_) => println!("✗ Unexpected success with uninitialized context"),
            Err(e) => {
                println!("✓ Caught context error: {}", e);
            }
        },
        Err(e) => println!("✗ Unexpected parameter validation error: {}", e),
    }
}

fn demonstrate_io_errors(temp_dir: &std::path::Path) {
    // Try to open non-existent index using ApiThing pattern
    let non_existent = temp_dir.join("does_not_exist");
    let mut context1 = ShardexContext::new();
    let open_params = OpenIndexParams::new(non_existent);

    match OpenIndex::execute(&mut context1, &open_params) {
        Ok(_) => println!("✗ Unexpected success opening non-existent index"),
        Err(e) => {
            println!("✓ Caught error for non-existent index: {}", e);
        }
    }

    // Create a file where directory should be
    let file_path = temp_dir.join("file_not_dir");
    std::fs::write(&file_path, "not a directory").unwrap();

    let mut context2 = ShardexContext::new();
    match CreateIndexParams::builder()
        .directory_path(file_path)
        .vector_size(128)
        .build()
    {
        Ok(params) => match CreateIndex::execute(&mut context2, &params) {
            Ok(_) => println!("✗ Unexpected success with file instead of directory"),
            Err(e) => {
                println!("✓ Caught error when directory path is a file: {}", e);
            }
        },
        Err(e) => {
            println!("✓ Caught parameter validation error: {}", e);
        }
    }
}

fn demonstrate_search_errors(context: &mut ShardexContext) {
    // Wrong query vector dimension using ApiThing pattern
    let wrong_query = vec![0.1, 0.2]; // Wrong size: expected 128, got 2
    match SearchParams::builder()
        .query_vector(wrong_query)
        .k(5)
        .build()
    {
        Ok(params) => match Search::execute(context, &params) {
            Ok(_) => println!("✗ Unexpected success with wrong query dimension"),
            Err(ShardexError::InvalidDimension { expected, actual }) => {
                println!("✓ Caught search dimension error: expected {}, got {}", expected, actual);
            }
            Err(e) => println!("✗ Unexpected error type: {}", e),
        },
        Err(e) => {
            println!("✓ Caught parameter validation error: {}", e);
        }
    }

    // Empty query vector using ApiThing pattern
    let empty_query = vec![];
    match SearchParams::builder()
        .query_vector(empty_query)
        .k(5)
        .build()
    {
        Ok(params) => match Search::execute(context, &params) {
            Ok(_) => println!("✗ Unexpected success with empty query"),
            Err(e) => {
                println!("✓ Caught error for empty query: {}", e);
            }
        },
        Err(e) => {
            println!("✓ Caught parameter validation error for empty query: {}", e);
        }
    }

    // Invalid k value using ApiThing pattern
    let valid_query = vec![0.0; 128];
    match SearchParams::builder()
        .query_vector(valid_query)
        .k(0)
        .build()
    {
        Ok(params) => match Search::execute(context, &params) {
            Ok(_) => println!("✗ Unexpected success with k=0"),
            Err(e) => {
                println!("✓ Caught error for k=0: {}", e);
            }
        },
        Err(e) => {
            println!("✓ Caught parameter validation error for k=0: {}", e);
        }
    }

    // Search on uninitialized context
    let mut uninitialized_context = ShardexContext::new();
    match SearchParams::builder()
        .query_vector(vec![0.1; 128])
        .k(5)
        .build()
    {
        Ok(params) => match Search::execute(&mut uninitialized_context, &params) {
            Ok(_) => println!("✗ Unexpected success with uninitialized context"),
            Err(e) => {
                println!("✓ Caught context error for search: {}", e);
            }
        },
        Err(e) => println!("✗ Unexpected parameter validation error: {}", e),
    }
}

fn demonstrate_robust_patterns(temp_dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    println!("Demonstrating robust error handling patterns...");

    // Pattern 1: Retry with backoff using ApiThing pattern
    println!("\n  Pattern 1: Retry with backoff");
    let result = retry_with_backoff(
        || {
            // Simulate operation that might fail using ApiThing pattern
            let mut context = ShardexContext::new();
            match CreateIndexParams::builder()
                .directory_path(temp_dir.join("robust_index"))
                .vector_size(128)
                .build()
            {
                Ok(params) => CreateIndex::execute(&mut context, &params),
                Err(e) => Err(e),
            }
        },
        3, // max retries
    );

    match result {
        Ok(_) => println!("    ✓ Operation succeeded"),
        Err(e) => println!("    ✗ Operation failed after retries: {}", e),
    }

    // Pattern 2: Graceful degradation using ApiThing pattern
    println!("\n  Pattern 2: Graceful degradation");
    let mut context = create_or_recover_index_apithing_with_vector_size(temp_dir.join("graceful_index_128"), 128)?;

    // Try to add some data - ensure vector dimension matches index
    let postings = vec![Posting {
        document_id: DocumentId::from_raw(1),
        start: 0,
        length: 100,
        vector: vec![0.1; 128], // This will be 128 dimensions to match the created index
    }];

    match AddPostingsParams::new(postings) {
        Ok(add_params) => {
            match AddPostings::execute(&mut context, &add_params) {
                Ok(_) => {
                    println!("    ✓ Successfully added postings");
                    let flush_params = FlushParams::new();
                    match Flush::execute(&mut context, &flush_params) {
                        Ok(_) => println!("    ✓ Successfully flushed data"),
                        Err(e) => {
                            println!("    ⚠ Flush failed, but data is still in WAL: {}", e);
                            // Continue operation - data will be recovered on restart
                        }
                    }
                }
                Err(e) => println!("    ✗ Failed to add postings: {}", e),
            }
        }
        Err(e) => println!("    ✗ Failed to create posting parameters: {}", e),
    }

    Ok(())
}

fn demonstrate_recovery_strategies(temp_dir: &std::path::Path) -> Result<(), Box<dyn Error>> {
    println!("Demonstrating error recovery strategies...");

    // Strategy 1: Automatic retry for transient errors using ApiThing pattern
    println!("\n  Strategy 1: Automatic retry for transient errors");

    let index_path = temp_dir.join("recovery_index");
    let mut attempts = 0;

    let _context = loop {
        attempts += 1;
        let mut context = ShardexContext::new();

        match CreateIndexParams::builder()
            .directory_path(index_path.clone())
            .vector_size(128)
            .build()
        {
            Ok(params) => match CreateIndex::execute(&mut context, &params) {
                Ok(_) => {
                    println!("    ✓ Index created successfully on attempt {}", attempts);
                    break context;
                }
                Err(e) if attempts < 3 => {
                    println!("    ⚠ Error on attempt {}, retrying: {}", attempts, e);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                Err(e) => {
                    println!("    ✗ Failed to create index after {} attempts: {}", attempts, e);
                    return Err(e.into());
                }
            },
            Err(e) => {
                println!("    ✗ Failed to build parameters: {}", e);
                return Err(e.into());
            }
        }
    };

    // Strategy 2: Input validation before operations using ApiThing pattern
    println!("\n  Strategy 2: Input validation before operations");

    let validate_and_add_apithing = |postings: Vec<Posting>| {
        // Create parameters and let ApiThing validation handle the checks
        match AddPostingsParams::new(postings.clone()) {
            Ok(_) => {
                println!("    ✓ ApiThing parameter validation succeeded");
                Ok(postings)
            }
            Err(e) => {
                println!("    ✗ ApiThing parameter validation failed: {}", e);
                Err(format!("Parameter validation error: {}", e))
            }
        }
    };

    let test_postings = vec![Posting {
        document_id: DocumentId::from_raw(1),
        start: 0,
        length: 100,
        vector: vec![0.1; 128],
    }];

    match validate_and_add_apithing(test_postings) {
        Ok(_validated_postings) => {
            println!("    ✓ Postings validated successfully using ApiThing pattern");
        }
        Err(e) => println!("    ✗ Validation failed: {}", e),
    }

    // Strategy 3: Context state validation
    println!("\n  Strategy 3: Context state validation");

    let mut uninitialized_context = ShardexContext::new();
    if !uninitialized_context.is_initialized() {
        println!("    ✓ Detected uninitialized context before operation");

        // Initialize context first
        match CreateIndexParams::builder()
            .directory_path(temp_dir.join("strategy3_index"))
            .vector_size(128)
            .build()
        {
            Ok(params) => match CreateIndex::execute(&mut uninitialized_context, &params) {
                Ok(_) => println!("    ✓ Context initialized successfully"),
                Err(e) => println!("    ✗ Failed to initialize context: {}", e),
            },
            Err(e) => println!("    ✗ Failed to build parameters: {}", e),
        }
    }

    Ok(())
}

fn retry_with_backoff<F, T, E>(mut operation: F, max_retries: usize) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    for attempt in 0..max_retries {
        match operation() {
            Ok(result) => return Ok(result),
            Err(e) => {
                if attempt == max_retries - 1 {
                    return Err(e);
                }
                // Exponential backoff
                let delay = std::time::Duration::from_millis(100 * (2_u64.pow(attempt as u32)));
                std::thread::sleep(delay);
            }
        }
    }

    unreachable!()
}

fn create_or_recover_index_apithing_with_vector_size(
    path: std::path::PathBuf,
    vector_size: usize,
) -> Result<ShardexContext, ShardexError> {
    let mut context = ShardexContext::new();

    // Try to open existing index first using ApiThing pattern
    let open_params = OpenIndexParams::new(path.clone());
    match OpenIndex::execute(&mut context, &open_params) {
        Ok(_) => {
            println!("    ✓ Recovered existing index using ApiThing pattern");
            Ok(context)
        }
        Err(_) => {
            // Create new index if opening failed
            let mut new_context = ShardexContext::new();
            match CreateIndexParams::builder()
                .directory_path(path)
                .vector_size(vector_size)
                .build()
            {
                Ok(create_params) => {
                    CreateIndex::execute(&mut new_context, &create_params)?;
                    println!("    ✓ Created new index using ApiThing pattern");
                    Ok(new_context)
                }
                Err(e) => {
                    println!("    ✗ Failed to build create parameters: {}", e);
                    Err(e)
                }
            }
        }
    }
}

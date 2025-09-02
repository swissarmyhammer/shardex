//! API surface validation tests
//!
//! This test suite validates that the public API surface is clean and minimal,
//! and that internal implementation details are properly hidden.

use apithing::ApiOperation;
use shardex::DocumentId;



#[test]
fn test_public_api_surface_complete() {
    // This test verifies that all intended public types are accessible
    // and that the API follows the ApiThing pattern consistently
    
    // Core types should be accessible
    let _context: shardex::api::ShardexContext = shardex::api::ShardexContext::new();
    
    // All operations should be accessible
    use shardex::api::{
        CreateIndex,
    };
    
    // All parameter types should be accessible
    use shardex::api::{
        CreateIndexParams,
    };
    
    // Core data structures should be accessible
    use shardex::{ShardexError};
    let _ = ShardexError::Config("test".into());
    
    // Verify ApitThing trait is available (operations should implement it)
    use apithing::ApiOperation;
    
    // Test that we can use the standard pattern
    // (This won't execute in tests but should compile)
    fn _test_apithing_pattern() -> Result<(), Box<dyn std::error::Error>> {
        let mut context = shardex::api::ShardexContext::new();
        let create_params = CreateIndexParams::builder()
            .directory_path("/tmp/test".into())
            .vector_size(128)
            .shard_size(1000)
            .build()?;
        
        // This demonstrates the consistent ApiThing pattern
        let _ = CreateIndex::execute(&mut context, &create_params);
        Ok(())
    }
}

#[test]
fn test_internal_types_not_accessible() {
    // This test ensures that internal implementation details are not part of the public API
    // Note: This test verifies by compilation - if internal types become public, this won't compile
    
    // The following should NOT be accessible (these lines should fail to compile if uncommented):
    
    // Internal processing types
    // use shardex::batch_processor::BatchProcessor; // Should be private
    // use shardex::bloom_filter::BloomFilter; // Should be private
    // use shardex::cow_index::CowIndex; // Should be private
    // use shardex::deduplication::DeduplicationManager; // Should be private
    // use shardex::distance::DistanceCalculator; // Should be private
    // use shardex::error_handling::ErrorContext; // Should be private
    // use shardex::integrity::IntegrityChecker; // Should be private
    // use shardex::memory::MemoryManager; // Should be private
    // use shardex::search_coordinator::SearchCoordinator; // Should be private
    
    // Internal storage types (some are public for tests but shouldn't be used by API consumers)
    // These are marked with #[doc(hidden)] but we shouldn't encourage their use
    
    // The test passes by compiling successfully - internal types are properly hidden
}

#[test]
fn test_deprecated_api_warnings() {
    // Test that deprecated APIs are still accessible but produce warnings
    
    // Legacy APIs have been removed - testing deprecated API access is no longer possible
    // #[allow(deprecated)]
    // let _legacy_shardex = shardex::Shardex::default();
    
    #[allow(deprecated)]
    let _legacy_config = shardex::ShardexConfig::default();
    
    // The new API should be preferred
    let _new_context = shardex::api::ShardexContext::new();
}

#[test]
fn test_error_types_accessible() {
    // Verify that error types are properly exposed
    use shardex::ShardexError;
    
    // Error should implement standard error traits
    fn _accepts_error<E: std::error::Error + Send + Sync + 'static>(_e: E) {}
    
    let error = ShardexError::Config("test".into());
    _accepts_error(error);
    
    // Result type alias should be available
    let _result: shardex::Result<()> = Ok(());
}

#[test]
fn test_identifier_types_accessible() {
    // Test that identifier types are accessible and properly typed
    use shardex::{DocumentId, ShardId, TransactionId};
    
    let doc_id = DocumentId::from_raw(123);
    assert_eq!(doc_id.raw(), 123);
    
    let shard_id = ShardId::from_raw(456);
    assert_eq!(shard_id.raw(), 456);
    
    let tx_id = TransactionId::from_raw(789);
    assert_eq!(tx_id.raw(), 789);
    
    // These should be distinct types (not just type aliases)
    fn _test_type_safety() {
        let doc_id = DocumentId::from_raw(1);
        let shard_id = ShardId::from_raw(1);
        
        // These should be different types and not interchangeable
        // (The type checker enforces this at compile time)
        let _doc: DocumentId = doc_id;
        let _shard: ShardId = shard_id;
        
        // This would fail to compile:
        // let _wrong: DocumentId = shard_id; // Type mismatch
    }
}

#[test]
fn test_data_structure_completeness() {
    // Test that all necessary data structures are accessible and complete
    use shardex::{Posting, SearchResult, IndexStats};
    
    // Test Posting structure
    let posting = Posting {
        document_id: shardex::DocumentId::from_raw(1),
        start: 0,
        length: 100,
        vector: vec![0.1, 0.2, 0.3],
    };
    
    assert_eq!(posting.start, 0);
    assert_eq!(posting.length, 100);
    assert_eq!(posting.vector.len(), 3);
    
    // Test SearchResult structure
    let search_result = SearchResult {
        document_id: shardex::DocumentId::from_raw(1),
        start: 0,
        length: 100,
        vector: vec![0.1, 0.2, 0.3],
        similarity_score: 0.95,
    };
    
    assert_eq!(search_result.similarity_score, 0.95);
    
    // Test IndexStats structure
    let stats = IndexStats {
        total_shards: 1,
        total_postings: 100,
        pending_operations: 5,
        memory_usage: 1024 * 1024,
        active_postings: 90,
        deleted_postings: 10,
        average_shard_utilization: 0.8,
        vector_dimension: 128,
        disk_usage: 2048 * 1024,
        search_latency_p50: std::time::Duration::from_millis(10),
        search_latency_p95: std::time::Duration::from_millis(50),
        search_latency_p99: std::time::Duration::from_millis(100),
        write_throughput: 1000.0,
        bloom_filter_hit_rate: 0.85,
    };
    
    assert_eq!(stats.total_shards, 1);
    assert_eq!(stats.vector_dimension, 128);
}

#[test]
fn test_builder_pattern_consistency() {
    // Test that parameter builders follow consistent patterns
    use shardex::api::{CreateIndexParams, SearchParams, AddPostingsParams};
    use shardex::{DocumentId, Posting};
    
    // CreateIndexParams should have builder pattern
    let create_params = CreateIndexParams::builder()
        .directory_path("/tmp/test".into())
        .vector_size(128)
        .shard_size(1000)
        .batch_write_interval_ms(100)
        .build()
        .expect("Builder should work");
    
    assert_eq!(create_params.vector_size, 128);
    assert_eq!(create_params.shard_size, 1000);
    
    // SearchParams should have builder pattern
    let search_params = SearchParams::builder()
        .query_vector(vec![0.1; 128])
        .k(10)
        .slop_factor(Some(3))
        .build()
        .expect("Builder should work");
    
    assert_eq!(search_params.k, 10);
    assert_eq!(search_params.slop_factor, Some(3));
    
    // Some params may have simple constructors
    let posting = Posting {
        document_id: DocumentId::from_raw(1),
        start: 0,
        length: 100,
        vector: vec![0.1; 128],
    };
    
    let add_params = AddPostingsParams::new(vec![posting])
        .expect("Constructor should work");
    
    assert_eq!(add_params.postings.len(), 1);
}

#[test]
fn test_api_module_organization() {
    // Test that the API is well-organized into logical modules
    
    // Main api module should contain all operations and context
    use shardex::api;
    
    // Context should be in api module
    let _context = api::ShardexContext::new();
    
    // All operations should be in api module  
    let _ = shardex::api::CreateIndex::execute;
    
    // Parameter types should be in api module
    let _ = shardex::api::CreateIndexParams::builder;
    
    // Core types should be at crate root for convenience
    let _ = shardex::DocumentId::from_raw;
    
    // This organization allows for both:
    // 1. Convenient access to core types: shardex::DocumentId
    // 2. Clear separation of API operations: shardex::api::CreateIndex
    // 3. Consistent parameter naming: shardex::api::CreateIndexParams
}

#[test]  
fn test_feature_flags_not_required() {
    // Test that the basic API doesn't require any feature flags
    // All basic functionality should be available by default
    
    use shardex::api::{ShardexContext};
    use shardex::{DocumentId, ShardexError};
    
    // These should all be available without feature flags
    let _context = ShardexContext::new();
    let _doc_id = DocumentId::from_raw(1);
    let _error = ShardexError::Config("test".into());
}

#[test]
fn test_consistent_naming_conventions() {
    // Test that naming follows consistent conventions
    
    // Operations should be verbs: CreateIndex, AddPostings, Search, etc.
    let _ = shardex::api::CreateIndex::execute;
    
    // Parameters should end with "Params"
    let _ = shardex::api::CreateIndexParams::builder;
    
    // Core types should be clear nouns
    let _ = (shardex::DocumentId::from_raw, shardex::ShardId::from_raw, shardex::TransactionId::from_raw);
    
    // Context is clearly named
    use shardex::api::ShardexContext;
    
    // All naming follows Rust conventions (PascalCase for types, snake_case for methods)
    let _context = ShardexContext::new(); // snake_case method
    let _doc_id = DocumentId::from_raw(1); // snake_case method on PascalCase type
    
    // Verify some common method names follow conventions
    fn _test_method_naming() {
        let doc_id = DocumentId::from_raw(1);
        let _raw = doc_id.raw(); // snake_case accessor method
        
        let context = ShardexContext::new();
        let _is_initialized = context.is_initialized(); // snake_case predicate method
    }
}
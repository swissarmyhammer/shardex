//! # Shardex - High-Performance Vector Search Engine
//!
//! Shardex provides a memory-mapped vector search engine with the ApiThing pattern
//! for consistent, composable, and type-safe operations.
//!
//! ## Architecture
//!
//! The library is built around three core concepts:
//!
//! - **[`ShardexContext`](api::ShardexContext)**: Shared state and resource management
//! - **Operations**: Types implementing [`ApiOperation`](apithing::ApiOperation) trait
//! - **Parameters**: Type-safe input objects for each operation
//!
//! ## Core Operations
//!
//! ### Index Management
//! - [`CreateIndex`](api::CreateIndex) - Create new index
//!
//! ### Document Operations
//! - [`AddPostings`](api::AddPostings) - Add vector postings
//! - [`StoreDocumentText`](api::StoreDocumentText) - Store document text
//! - [`BatchStoreDocumentText`](api::BatchStoreDocumentText) - Batch text storage
//!
//! ### Search Operations
//! - [`Search`](api::Search) - Vector similarity search
//! - [`GetDocumentText`](api::GetDocumentText) - Retrieve document text
//! - [`ExtractSnippet`](api::ExtractSnippet) - Extract text snippets
//!
//! ### Maintenance Operations
//! - [`Flush`](api::Flush) - Flush pending operations
//! - [`GetStats`](api::GetStats) - Index statistics
//! - [`GetPerformanceStats`](api::GetPerformanceStats) - Performance metrics
//!
//! ## Usage Patterns
//!
//! All operations follow the same pattern:
//!
//! ```rust
//! use apithing::ApiOperation;
//!
//! let result = OperationType::execute(&mut context, &parameters)?;
//! ```
//!
//! ## Quick Start
//!
//! ```rust
//! use shardex::api::{
//!     ShardexContext, CreateIndex, AddPostings, Search,
//!     CreateIndexParams, AddPostingsParams, SearchParams
//! };
//! use shardex::{DocumentId, Posting};
//! use apithing::ApiOperation;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create context and index
//! let mut context = ShardexContext::new();
//! let create_params = CreateIndexParams::builder()
//!     .directory_path("./my_index".into())
//!     .vector_size(384)
//!     .shard_size(10000)
//!     .batch_write_interval_ms(100)
//!     .build()?;
//!     
//! CreateIndex::execute(&mut context, &create_params)?;
//!
//! // Add postings
//! let postings = vec![Posting {
//!     document_id: DocumentId::from_raw(1),
//!     start: 0,
//!     length: 100,
//!     vector: vec![0.1; 384],
//! }];
//! AddPostings::execute(&mut context, &AddPostingsParams::new(postings)?)?;
//!
//! // Search
//! let results = Search::execute(&mut context, &SearchParams::builder()
//!     .query_vector(vec![0.1; 384])
//!     .k(10)
//!     .build()?
//! )?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Examples
//!
//! The `examples/` directory contains comprehensive examples:
//!
//! - `basic_usage.rs` - Basic operations
//! - `configuration.rs` - Configuration options
//! - `batch_operations.rs` - Batch processing
//! - `document_text_basic.rs` - Text storage
//! - `monitoring.rs` - Performance monitoring
//!
//! Run examples with:
//! ```bash
//! cargo run --example basic_usage
//! ```
//!
//! ## Features
//!
//! - **Consistent API**: All operations use the ApiThing pattern
//! - **Type Safety**: Parameter objects prevent errors
//! - **Shared Context**: Efficient resource management
//! - **Memory-mapped storage** for zero-copy operations and fast startup
//! - **ACID transactions** via write-ahead logging (WAL)
//! - **Incremental updates** without full index rebuilds
//! - **Document text storage** with snippet extraction
//! - **Performance monitoring** and detailed statistics
//! - **Dynamic shard management** with automatic splitting
//! - **Concurrent reads** during write operations
//! - **Bloom filter optimization** for efficient document deletion
//! - **Crash recovery** from unexpected shutdowns
//!
//! # Development Guidelines
//!
//! ## Struct Definition Standards
//!
//! ### Default Implementation Rules
//!
//! 1. **PREFER** `#[derive(Default)]` for structs with all zero/empty defaults:
//!    ```rust
//!    #[derive(Debug, Clone, Default)]
//!    pub struct SimpleMetrics {
//!        pub count: u64,
//!        pub total: u64,
//!    }
//!    ```
//!
//! 2. **USE** manual `impl Default` only when:
//!    - Non-zero defaults are needed
//!    - Complex initialization is required  
//!    - Fields contain non-Default types
//!    ```rust
//!    use std::time::Instant;
//!    
//!    #[derive(Debug, Clone)]
//!    pub struct ComplexMetrics {
//!        pub start_time: Instant,
//!        pub threshold: f64,
//!    }
//!    
//!    impl Default for ComplexMetrics {
//!        fn default() -> Self {
//!            Self {
//!                start_time: Instant::now(), // Can't derive this
//!                threshold: 0.95,           // Non-zero default
//!            }
//!        }
//!    }
//!    ```
//!
//! 3. **AVOID** redundant patterns like:
//!    ```rust
//!    // DON'T DO THIS - just derive Default instead
//!    #[derive(Debug, Clone)]
//!    pub struct SomeStruct {
//!        pub count: u64,
//!    }
//!    
//!    impl SomeStruct {
//!        pub fn new() -> Self {
//!            Self { count: 0 }
//!        }
//!    }
//!    
//!    impl Default for SomeStruct {
//!        fn default() -> Self {
//!            Self::new() // If new() just sets zero/empty values
//!        }
//!    }
//!    ```
//!
//! ### Struct Size Guidelines
//!
//! 1. **MAXIMUM** 15 fields per struct (prefer 10 or fewer)
//! 2. **BREAK DOWN** large structs into logical sub-structures:
//!    ```rust
//!    // Instead of one large struct with 30+ fields:
//!    #[derive(Debug, Clone, Default)]
//!    pub struct DocumentMetrics {
//!        pub documents_stored: u64,
//!        pub total_size: u64,
//!        pub average_latency: f64,
//!    }
//!    ```
//! 3. **GROUP** related fields into cohesive types
//! 4. **USE** composition over large flat structures
//!
//! ### Derive Attribute Ordering
//!
//! Always use consistent ordering for derive attributes:
//! ```rust
//! #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
//! ```
//!
//! Order: Debug, Clone, Copy (if applicable), Default, PartialEq, Eq, Hash, Serialize, Deserialize
//!
//! ### Builder Pattern Usage
//!
//! Use builder patterns for:
//! - Configuration structs with many optional parameters
//! - Complex initialization sequences
//! - Structs with validation requirements
//!
//! ```rust
//! #[derive(Debug, Clone, Default)]
//! pub struct MyConfig {
//!     pub timeout: u64,
//! }
//!
//! impl MyConfig {
//!     pub fn new() -> Self { Self::default() }
//!     
//!     pub fn with_timeout(mut self, timeout: u64) -> Self {
//!         self.timeout = timeout;
//!         self
//!     }
//! }
//! ```

// Public API modules
pub mod api;
pub mod error;
pub mod identifiers;
pub mod structures;

// Legacy API modules (with deprecation warnings)
pub mod config;
pub mod shardex;

// Internal implementation modules (some made public for tests)
pub mod async_document_text_storage; // public for tests
pub(crate) mod batch_processor;
pub(crate) mod bloom_filter;
pub mod concurrent; // public for tests
pub mod concurrent_document_text_storage; // public for tests
pub(crate) mod config_persistence;
pub(crate) mod constants;
pub mod cow_index; // public for tests
pub mod crash_recovery; // public for tests
pub(crate) mod deduplication;
pub(crate) mod distance;
pub mod document_text_entry; // public for tests
pub(crate) mod document_text_performance;
pub mod document_text_storage; // public for tests
pub(crate) mod document_transaction_coordinator;
pub(crate) mod error_context_integration_test;
pub(crate) mod error_handling;
pub(crate) mod integrity;
pub mod layout; // public for tests
pub(crate) mod memory;
pub mod monitoring;
pub mod posting_storage; // public for tests
pub(crate) mod prelude;
pub(crate) mod search_coordinator;
pub mod shard; // public for tests
pub mod shardex_index; // public for tests
pub mod text_memory_pool; // public for tests
pub mod transactions; // public for tests
pub mod vector_storage; // public for tests
pub mod wal; // public for tests
pub mod wal_replay; // public for tests

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
pub mod statistics_integration_test;

#[cfg(test)]
pub mod error_handling_integration_test;

// Core public API - Essential types that users need
pub use error::ShardexError;
pub use identifiers::{DocumentId, ShardId, TransactionId};
pub use structures::{IndexStats, Posting, SearchResult};

// Primary API - ApiThing pattern (recommended approach)
pub use api::{
    // Operations
    AddPostings,
    // Parameter types
    AddPostingsParams,
    BatchAddPostings,
    BatchAddPostingsParams,
    BatchDocumentTextStats,
    // Output types
    BatchStats,
    BatchStoreDocumentText,
    BatchStoreDocumentTextParams,
    CreateIndex,
    CreateIndexParams,
    DetailedIndexStats,
    DetailedPerformanceMetrics,
    DocumentTextEntry,
    ExtractSnippet,
    ExtractSnippetParams,
    Flush,
    FlushParams,
    GetDocumentText,
    GetDocumentTextParams,
    GetPerformanceStats,
    GetPerformanceStatsParams,
    GetStats,
    GetStatsParams,
    IncrementalAdd,
    IncrementalAddParams,
    IncrementalStats,
    PerformanceStats,
    RemovalStats,
    RemoveDocuments,
    RemoveDocumentsParams,
    Search,
    SearchParams,
    SearchResultWithText,
    // Context
    ShardexContext,
    StoreDocumentText,
    StoreDocumentTextParams,
};

// Legacy API (deprecated - use api module instead)
#[deprecated(since = "0.2.0", note = "Use api::ShardexContext and operations instead")]
pub use shardex::{Shardex, ShardexImpl};

#[deprecated(since = "0.2.0", note = "Use api::CreateIndexParams instead")]
pub use config::ShardexConfig;

// Internal re-exports for testing (not part of public API - subject to change)
#[doc(hidden)]
pub use async_document_text_storage::{AsyncDocumentTextStorage, AsyncStorageConfig};
#[doc(hidden)]
pub use concurrent_document_text_storage::{ConcurrentDocumentTextStorage, ConcurrentStorageConfig};
#[doc(hidden)]
pub use cow_index::CowShardexIndex;
#[doc(hidden)]
pub use crash_recovery::CrashRecovery;
#[doc(hidden)]
pub use document_text_storage::DocumentTextStorage;
#[doc(hidden)]
pub use layout::DirectoryLayout;
#[doc(hidden)]
pub use monitoring::PerformanceMonitor as MonitoringPerformanceMonitor;
#[doc(hidden)]
pub use posting_storage::PostingStorage;
#[doc(hidden)]
pub use shard::Shard;
#[doc(hidden)]
pub use shardex_index::ShardexIndex;
#[doc(hidden)]
pub use text_memory_pool::{MemoryPoolConfig, TextMemoryPool};
#[doc(hidden)]
pub use transactions::{WalOperation, WalTransaction};
#[doc(hidden)]
pub use vector_storage::VectorStorage;
#[doc(hidden)]
pub use wal::WalSegment;

/// Type alias for Results using ShardexError
pub type Result<T> = std::result::Result<T, ShardexError>;

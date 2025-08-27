//! Shardex - A high-performance memory-mapped vector search engine
//!
//! Shardex is a memory mapped vector search engine implemented in Rust that supports
//! incremental updating of index entries and document postings. Indexes are arranged
//! in shards, each consisting of embedding vectors and postings data structures.
//!
//! # Quick Start
//!
//! ```rust
//! use shardex::{ShardexConfig, Posting, DocumentId};
//!
//! // Create configuration
//! let config = ShardexConfig::new()
//!     .directory_path("./my_index")
//!     .vector_size(384)
//!     .shard_size(10000);
//!
//! // Create a posting
//! let posting = Posting {
//!     document_id: DocumentId::from_raw(1),
//!     start: 0,
//!     length: 100,
//!     vector: vec![0.1; 384], // 384-dimensional vector
//! };
//!
//! assert_eq!(posting.document_id.raw(), 1);
//! assert_eq!(posting.vector.len(), 384);
//! assert_eq!(config.vector_size, 384);
//! ```
//!
//! # Features
//!
//! - **Memory-mapped storage** for zero-copy operations and fast startup
//! - **ACID transactions** via write-ahead logging (WAL)
//! - **Incremental updates** without full index rebuilds
//! - **Dynamic shard management** with automatic splitting
//! - **Concurrent reads** during write operations
//! - **Configurable vector dimensions** and index parameters
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

pub mod async_document_text_storage;
pub mod batch_processor;
pub mod bloom_filter;
pub mod concurrent;
pub mod concurrent_document_text_storage;
pub mod config;
pub mod config_persistence;
pub mod constants;
pub mod cow_index;
pub mod crash_recovery;
pub mod deduplication;
pub mod distance;
pub mod document_text_entry;
pub mod document_text_performance;
pub mod document_text_storage;
pub mod document_transaction_coordinator;
pub mod error;
pub mod error_context_integration_test;
pub mod error_handling;
pub mod identifiers;
pub mod integrity;
pub mod layout;
pub mod memory;
pub mod monitoring;
pub mod posting_storage;
pub mod prelude;
pub mod search_coordinator;
pub mod shard;
pub mod shardex;
pub mod shardex_index;
pub mod structures;
pub mod text_memory_pool;
pub mod transactions;
pub mod vector_storage;
pub mod wal;
pub mod wal_replay;

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
pub mod statistics_integration_test;

#[cfg(test)]
pub mod error_handling_integration_test;

pub use async_document_text_storage::{AsyncDocumentTextStorage, AsyncStorageConfig, AsyncStorageMetrics};
pub use batch_processor::BatchProcessor;
pub use bloom_filter::{BloomFilter, BloomFilterBuilder, BloomFilterHeader, BloomFilterStats};
pub use concurrent::{ConcurrencyConfig, ConcurrencyMetrics, ConcurrentShardex, WriteOperationType};
pub use concurrent_document_text_storage::{
    ConcurrentDocumentTextStorage, ConcurrentStorageConfig, ConcurrentStorageMetrics,
};
pub use config::ShardexConfig;
pub use config_persistence::{ConfigurationManager, PersistedConfig};
pub use cow_index::{CowShardexIndex, IndexWriter};
pub use crash_recovery::{CrashRecovery, CrashRecoveryStats};
pub use deduplication::{DeduplicationPolicy, DeduplicationStats, ResultDeduplicator};
pub use distance::DistanceMetric;
pub use document_text_entry::{
    DocumentTextEntry, TextDataHeader, TextIndexHeader, TEXT_DATA_MAGIC, TEXT_DATA_VERSION, TEXT_INDEX_MAGIC,
    TEXT_INDEX_VERSION,
};
pub use document_text_performance::{
    AccessPattern, CacheHealth, CacheHealthReport, OptimizedMappingStats, OptimizedMemoryMapping,
};
pub use document_text_storage::DocumentTextStorage;
pub use document_transaction_coordinator::{DocumentTransactionCoordinator, TransactionStatistics};
pub use error::ShardexError;
pub use error_handling::{
    BackupInfo, BackupManager, BackupRetentionPolicy, RecoveryConfig, RecoveryResult, RecoveryStrategy, RestoreResult,
    TextStorageHealth, TextStorageHealthMonitor, TextStorageRecoveryManager,
};
pub use identifiers::{DocumentId, ShardId, TransactionId};
pub use integrity::{CorruptionReport, IntegrityConfig, IntegrityManager, ValidationResult};
pub use layout::{CleanupManager, DirectoryLayout, FileDiscovery, IndexMetadata};
pub use memory::{FileHeader, MemoryMappedFile, StandardHeader};
pub use monitoring::{
    BloomFilterMetrics, DetailedIndexStats, DocumentTextMetrics, DocumentTextOperation, HistoricalData,
    HistoricalDataPoint, PercentileCalculator, PerformanceMonitor as MonitoringPerformanceMonitor, ResourceMetrics,
    TrendAnalysis, WriteMetrics,
};
pub use posting_storage::{PostingStorage, PostingStorageHeader};
pub use search_coordinator::{PerformanceMonitor, SearchCoordinator, SearchCoordinatorConfig, SearchMetrics};
pub use shard::{Shard, ShardMetadata};
pub use shardex::{Shardex, ShardexImpl};
pub use shardex_index::{IndexConfig, IndexStatistics, ShardexIndex, ShardexMetadata};
pub use structures::{FlushStats, IndexStats, Posting, PostingHeader, SearchResult, SearchResultHeader};
pub use text_memory_pool::{MemoryPoolConfig, MemoryPoolStats, PooledBytes, PooledString, TextMemoryPool};
pub use transactions::{
    BatchConfig, BatchStats, WalBatchHandle, WalBatchManager, WalOperation, WalTransaction, WalTransactionHeader,
};
pub use vector_storage::VectorStorage;
pub use wal::{WalManager, WalSegment};
pub use wal_replay::{RecoveryStats, WalReplayer};

/// Type alias for Results using ShardexError
pub type Result<T> = std::result::Result<T, ShardexError>;

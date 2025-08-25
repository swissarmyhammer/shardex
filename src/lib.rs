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

pub mod batch_processor;
pub mod bloom_filter;
pub mod concurrent;
pub mod config;
pub mod config_persistence;
pub mod cow_index;
pub mod crash_recovery;
pub mod deduplication;
pub mod distance;
pub mod document_text_entry;
pub mod document_text_storage;
pub mod document_transaction_coordinator;
pub mod error;
pub mod identifiers;
pub mod integrity;
pub mod layout;
pub mod memory;
pub mod monitoring;
pub mod posting_storage;
pub mod search_coordinator;
pub mod shard;
pub mod shardex;
pub mod shardex_index;
pub mod structures;
pub mod transactions;
pub mod vector_storage;
pub mod wal;
pub mod wal_replay;

#[cfg(test)]
pub mod test_utils;

#[cfg(test)]
pub mod statistics_integration_test;

pub use batch_processor::BatchProcessor;
pub use bloom_filter::{BloomFilter, BloomFilterBuilder, BloomFilterHeader, BloomFilterStats};
pub use concurrent::{
    ConcurrencyConfig, ConcurrencyMetrics, ConcurrentShardex, WriteOperationType,
};
pub use config::ShardexConfig;
pub use config_persistence::{ConfigurationManager, PersistedConfig};
pub use cow_index::{CowShardexIndex, IndexWriter};
pub use crash_recovery::{CrashRecovery, CrashRecoveryStats};
pub use deduplication::{DeduplicationPolicy, DeduplicationStats, ResultDeduplicator};
pub use distance::DistanceMetric;
pub use document_text_entry::{
    DocumentTextEntry, TextDataHeader, TextIndexHeader, TEXT_DATA_MAGIC, TEXT_DATA_VERSION,
    TEXT_INDEX_MAGIC, TEXT_INDEX_VERSION,
};
pub use document_text_storage::DocumentTextStorage;
pub use document_transaction_coordinator::{DocumentTransactionCoordinator, TransactionStatistics};
pub use error::ShardexError;
pub use identifiers::{DocumentId, ShardId, TransactionId};
pub use integrity::{CorruptionReport, IntegrityConfig, IntegrityManager, ValidationResult};
pub use layout::{CleanupManager, DirectoryLayout, FileDiscovery, IndexMetadata};
pub use memory::{FileHeader, MemoryMappedFile, StandardHeader};
pub use monitoring::{
    BloomFilterMetrics, DetailedIndexStats, HistoricalData, HistoricalDataPoint,
    PercentileCalculator, PerformanceMonitor as MonitoringPerformanceMonitor, ResourceMetrics,
    TrendAnalysis, WriteMetrics,
};
pub use posting_storage::{PostingStorage, PostingStorageHeader};
pub use search_coordinator::{
    PerformanceMonitor, SearchCoordinator, SearchCoordinatorConfig, SearchMetrics,
};
pub use shard::{Shard, ShardMetadata};
pub use shardex::{Shardex, ShardexImpl};
pub use shardex_index::{IndexConfig, IndexStatistics, ShardexIndex, ShardexMetadata};
pub use structures::{
    FlushStats, IndexStats, Posting, PostingHeader, SearchResult, SearchResultHeader,
};
pub use transactions::{
    BatchConfig, BatchStats, WalBatchHandle, WalBatchManager, WalOperation, WalTransaction,
    WalTransactionHeader,
};
pub use vector_storage::VectorStorage;
pub use wal::{WalManager, WalSegment};
pub use wal_replay::{RecoveryStats, WalReplayer};

/// Type alias for Results using ShardexError
pub type Result<T> = std::result::Result<T, ShardexError>;

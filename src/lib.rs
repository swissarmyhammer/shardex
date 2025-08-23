//! Shardex - A high-performance memory-mapped vector search engine
//!
//! Shardex is a memory mapped vector search engine implemented in Rust that supports
//! incremental updating of index entries and document postings. Indexes are arranged
//! in shards, each consisting of embedding vectors and postings data structures.

pub mod batch_processor;
pub mod config;
pub mod cow_index;
pub mod error;
pub mod identifiers;
pub mod integrity;
pub mod layout;
pub mod memory;
pub mod posting_storage;
pub mod shard;
pub mod shardex_index;
pub mod structures;
pub mod transactions;
pub mod vector_storage;
pub mod wal;
pub mod wal_replay;

#[cfg(test)]
pub mod test_utils;

pub use batch_processor::BatchProcessor;
pub use config::ShardexConfig;
pub use cow_index::{CowShardexIndex, IndexWriter};
pub use error::ShardexError;
pub use identifiers::{DocumentId, ShardId, TransactionId};
pub use integrity::{CorruptionReport, IntegrityConfig, IntegrityManager, ValidationResult};
pub use layout::{CleanupManager, DirectoryLayout, FileDiscovery, IndexMetadata};
pub use memory::{FileHeader, MemoryMappedFile, StandardHeader};
pub use posting_storage::{PostingStorage, PostingStorageHeader};
pub use shard::{Shard, ShardMetadata};
pub use shardex_index::{IndexStatistics, ShardexIndex, ShardexMetadata};
pub use structures::{IndexStats, Posting, PostingHeader, SearchResult, SearchResultHeader};
pub use transactions::{
    BatchConfig, BatchStats, WalBatchHandle, WalBatchManager, WalOperation, WalTransaction,
    WalTransactionHeader,
};
pub use vector_storage::VectorStorage;
pub use wal::{WalManager, WalSegment};
pub use wal_replay::{RecoveryStats, WalReplayer};

/// Type alias for Results using ShardexError
pub type Result<T> = std::result::Result<T, ShardexError>;

//! Shardex - A high-performance memory-mapped vector search engine
//!
//! Shardex is a memory mapped vector search engine implemented in Rust that supports
//! incremental updating of index entries and document postings. Indexes are arranged
//! in shards, each consisting of embedding vectors and postings data structures.

pub mod config;
pub mod error;
pub mod identifiers;
pub mod integrity;
pub mod layout;
pub mod memory;
pub mod posting_storage;
pub mod shard;
pub mod shardex_index;
pub mod structures;
pub mod vector_storage;

pub use config::ShardexConfig;
pub use error::ShardexError;
pub use identifiers::{DocumentId, ShardId};
pub use integrity::{CorruptionReport, IntegrityConfig, IntegrityManager, ValidationResult};
pub use layout::{CleanupManager, DirectoryLayout, FileDiscovery, IndexMetadata};
pub use memory::{FileHeader, MemoryMappedFile, StandardHeader};
pub use posting_storage::{PostingStorage, PostingStorageHeader};
pub use shard::{Shard, ShardMetadata};
pub use shardex_index::{IndexStatistics, ShardexIndex, ShardexMetadata};
pub use structures::{IndexStats, Posting, PostingHeader, SearchResult, SearchResultHeader};
pub use vector_storage::VectorStorage;

/// Type alias for Results using ShardexError
pub type Result<T> = std::result::Result<T, ShardexError>;

//! Shardex - A high-performance memory-mapped vector search engine
//!
//! Shardex is a memory mapped vector search engine implemented in Rust that supports
//! incremental updating of index entries and document postings. Indexes are arranged
//! in shards, each consisting of embedding vectors and postings data structures.

pub mod config;
pub mod error;
pub mod identifiers;
pub mod memory;
pub mod structures;

pub use config::ShardexConfig;
pub use error::ShardexError;
pub use identifiers::{DocumentId, ShardId};
pub use memory::{FileHeader, MemoryMappedFile};
pub use structures::{IndexStats, Posting, PostingHeader, SearchResult, SearchResultHeader};

/// Type alias for Results using ShardexError
pub type Result<T> = std::result::Result<T, ShardexError>;

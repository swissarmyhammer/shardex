//! Shardex - A high-performance memory-mapped vector search engine
//!
//! Shardex is a memory mapped vector search engine implemented in Rust that supports
//! incremental updating of index entries and document postings. Indexes are arranged
//! in shards, each consisting of embedding vectors and postings data structures.

pub mod config;
pub mod error;

pub use config::ShardexConfig;
pub use error::ShardexError;

/// Type alias for Results using ShardexError
pub type Result<T> = std::result::Result<T, ShardexError>;

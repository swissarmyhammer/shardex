//! API module for Shardex using the ApiThing pattern
//!
//! This module provides the foundation for converting Shardex to use the ApiThing
//! pattern with a single context and parameter objects for each operation.

pub mod context;
pub mod operations;
pub mod parameters;

// Re-export core components for convenience
pub use apithing::ApiOperation;
pub use context::ShardexContext;
pub use operations::{
    AddPostings, BatchAddPostings, BatchDocumentTextStats, BatchStats, BatchStoreDocumentText, CreateIndex,
    DetailedPerformanceMetrics, ExtractSnippet, Flush, GetDocumentText, GetPerformanceStats, GetStats, IncrementalAdd,
    IncrementalStats, PerformanceStats, RemovalStats, RemoveDocuments, Search, SearchResultWithText, StoreDocumentText,
};

// Re-export monitoring types for backward compatibility
pub use crate::monitoring::DetailedIndexStats;
pub use parameters::{
    AddPostingsParams, BatchAddPostingsParams, BatchStoreDocumentTextParams, CreateIndexParams, DocumentTextEntry,
    ExtractSnippetParams, FlushParams, GetDocumentTextParams, GetPerformanceStatsParams, GetStatsParams,
    IncrementalAddParams, RemoveDocumentsParams, SearchParams, StoreDocumentTextParams,
};

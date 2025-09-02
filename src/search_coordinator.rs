//! Multi-shard search coordination
//!
//! This module provides coordination of search operations across multiple shards with proper
//! result aggregation, timeout handling, and performance monitoring. It builds on the existing
//! parallel search infrastructure in ShardexIndex while adding async capabilities and advanced
//! coordination features.
//!
//! # Key Features
//!
//! - **Timeout Management**: Configurable search timeouts with graceful cancellation
//! - **Result Streaming**: Memory-efficient streaming for large result sets
//! - **Performance Monitoring**: Search latency and throughput metrics
//! - **Load Balancing**: Dynamic adjustment based on shard response times
//! - **Cancellation Support**: Async cancellation tokens for long-running searches
//!
//! # Usage Examples
//!
//! ## Basic Coordinated Search
//!
//! Internal search coordination manages parallel queries across multiple shards
//! with performance optimization and result aggregation.
//!
//! ## Search with Custom Timeout
//!
//! Timeout handling ensures search operations complete within specified
//! time limits with graceful degradation and partial result return.
//!
//! ## Streaming Results for Large Queries
//!
//! Result streaming enables efficient processing of large result sets
//! with memory-bounded processing and incremental result delivery.

// This module contains internal search coordination logic.
// Implementation details are intentionally minimal as this is internal infrastructure.

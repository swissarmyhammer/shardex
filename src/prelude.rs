//! Prelude module providing commonly used types and imports
//!
//! This module consolidates frequently imported types to reduce import duplication
//! across the codebase and improve maintainability.

// Core error and result types
pub use crate::error::ShardexError;
pub use crate::Result;

// Identifier types used throughout the codebase
pub use crate::identifiers::{DocumentId, ShardId, TransactionId};

// Common standard library imports
pub use std::sync::Arc;
pub use std::time::{Duration, SystemTime};

// Common external dependencies
pub use serde::{Deserialize, Serialize};

// Core configuration types
pub use crate::config::ShardexConfig;

// Memory management types
pub use crate::memory::{FileHeader, MemoryMappedFile};

// Common data structures
pub use crate::structures::{Posting, SearchResult};

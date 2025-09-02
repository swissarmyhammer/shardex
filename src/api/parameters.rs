//! Parameter structures for Shardex API operations
//!
//! This module defines parameter structures for the core Shardex operations
//! following the ApiThing pattern. Each operation has a dedicated parameter
//! struct with validation methods and builder patterns.
//!
//! # Core Operations
//!
//! - [`CreateIndexParams`]: Parameters for creating new indices
//! - [`OpenIndexParams`]: Parameters for opening existing indices
//! - [`ValidateConfigParams`]: Parameters for configuration validation
//! - [`AddPostingsParams`]: Parameters for adding document postings
//! - [`SearchParams`]: Parameters for similarity search operations
//! - [`FlushParams`]: Parameters for flushing operations
//! - [`GetStatsParams`]: Parameters for retrieving index statistics
//!
//! # Usage Examples
//!
//! ## Creating an Index
//!
//! ```rust
//! use shardex::api::parameters::CreateIndexParams;
//! use std::path::PathBuf;
//!
//! let params = CreateIndexParams::builder()
//!     .directory_path(PathBuf::from("./my_index"))
//!     .vector_size(768)
//!     .shard_size(50000)
//!     .build()?;
//!
//! // Validate parameters before use
//! params.validate()?;
//! # Ok::<(), shardex::error::ShardexError>(())
//! ```
//!
//! ## Adding Postings
//!
//! ```rust
//! use shardex::api::parameters::AddPostingsParams;
//! use shardex::{Posting, DocumentId};
//!
//! let postings = vec![
//!     Posting {
//!         document_id: DocumentId::from_raw(1),
//!         start: 0,
//!         length: 100,
//!         vector: vec![0.1; 384],
//!     }
//! ];
//!
//! let params = AddPostingsParams::new(postings)?;
//! params.validate()?;
//! # Ok::<(), shardex::error::ShardexError>(())
//! ```
//!
//! ## Searching
//!
//! ```rust
//! use shardex::api::parameters::SearchParams;
//!
//! let params = SearchParams::builder()
//!     .query_vector(vec![0.1; 384])
//!     .k(10)
//!     .slop_factor(Some(5))
//!     .build()?;
//!
//! params.validate()?;
//! # Ok::<(), shardex::error::ShardexError>(())
//! ```

use crate::config::ShardexConfig;
use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use crate::structures::Posting;
use std::path::PathBuf;

/// Parameters for creating a new Shardex index
///
/// This struct contains all the configuration parameters needed to create
/// a new index, derived from the `ShardexConfig` structure but focused
/// specifically on the creation operation.
#[derive(Debug, Clone)]
pub struct CreateIndexParams {
    /// Directory path where the index will be stored
    pub directory_path: PathBuf,
    /// Size of embedding vectors in dimensions
    pub vector_size: usize,
    /// Maximum number of entries per shard
    pub shard_size: usize,
    /// Interval between batch writes in milliseconds
    pub batch_write_interval_ms: u64,
    /// Size of each WAL segment in bytes
    pub wal_segment_size: usize,
    /// Size of bloom filters in bits
    pub bloom_filter_size: usize,
    /// Default slop factor for search operations
    pub default_slop_factor: u32,
}

impl CreateIndexParams {
    /// Create a new parameter builder
    pub fn builder() -> CreateIndexParamsBuilder {
        CreateIndexParamsBuilder::default()
    }

    /// Create high-performance configuration parameters
    ///
    /// Creates a configuration optimized for high throughput and performance
    /// with larger vectors and shard sizes suitable for production workloads.
    ///
    /// # Arguments
    ///
    /// * `directory` - Directory path where the index will be stored
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::parameters::CreateIndexParams;
    /// use std::path::PathBuf;
    ///
    /// let params = CreateIndexParams::high_performance(PathBuf::from("./high_perf_index"));
    /// assert_eq!(params.vector_size, 256);
    /// assert_eq!(params.shard_size, 15000);
    /// # Ok::<(), shardex::error::ShardexError>(())
    /// ```
    pub fn high_performance(directory: PathBuf) -> Self {
        Self {
            directory_path: directory,
            vector_size: 256,
            shard_size: 15000,
            batch_write_interval_ms: 75,
            wal_segment_size: 2 * 1024 * 1024, // 2MB
            bloom_filter_size: 2048,
            default_slop_factor: 4,
        }
    }

    /// Create memory-optimized configuration parameters
    ///
    /// Creates a configuration optimized for memory usage with smaller
    /// vectors and shard sizes suitable for resource-constrained environments.
    ///
    /// # Arguments
    ///
    /// * `directory` - Directory path where the index will be stored
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::parameters::CreateIndexParams;
    /// use std::path::PathBuf;
    ///
    /// let params = CreateIndexParams::memory_optimized(PathBuf::from("./memory_opt_index"));
    /// assert_eq!(params.vector_size, 128);
    /// assert_eq!(params.shard_size, 5000);
    /// # Ok::<(), shardex::error::ShardexError>(())
    /// ```
    pub fn memory_optimized(directory: PathBuf) -> Self {
        Self {
            directory_path: directory,
            vector_size: 128,
            shard_size: 5000,
            batch_write_interval_ms: 200,
            wal_segment_size: 256 * 1024, // 256KB
            bloom_filter_size: 512,
            default_slop_factor: 2,
        }
    }

    /// Create parameters from a ShardexConfig
    ///
    /// Converts a `ShardexConfig` into `CreateIndexParams`, allowing
    /// configurations to be used with the ApiThing pattern.
    ///
    /// # Arguments
    ///
    /// * `config` - The ShardexConfig to convert
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::parameters::CreateIndexParams;
    /// use shardex::ShardexConfig;
    ///
    /// let config = ShardexConfig::new()
    ///     .directory_path("./config_index")
    ///     .vector_size(512)
    ///     .shard_size(25000);
    ///
    /// let params = CreateIndexParams::from_shardex_config(config);
    /// assert_eq!(params.vector_size, 512);
    /// assert_eq!(params.shard_size, 25000);
    /// # Ok::<(), shardex::error::ShardexError>(())
    /// ```
    pub fn from_shardex_config(config: ShardexConfig) -> Self {
        Self::from(config)
    }

    /// Validate the create index parameters
    ///
    /// Performs comprehensive validation of all parameters to ensure they
    /// are within valid ranges and compatible with each other.
    ///
    /// # Errors
    ///
    /// Returns `ShardexError::Config` if any parameter is invalid.
    pub fn validate(&self) -> Result<(), ShardexError> {
        // Validate vector size
        if self.vector_size == 0 {
            return Err(ShardexError::config_error(
                "vector_size",
                "must be greater than 0",
                "Set vector_size to match your embedding model dimensions (e.g., 384, 768, 1536)",
            ));
        }

        if self.vector_size > 100_000 {
            return Err(ShardexError::config_error(
                "vector_size",
                "exceeds maximum supported size",
                "Use vector_size <= 100,000 dimensions for optimal performance",
            ));
        }

        // Validate shard size
        if self.shard_size == 0 {
            return Err(ShardexError::config_error(
                "shard_size",
                "must be greater than 0",
                "Set shard_size to control memory usage (recommended: 10,000-100,000)",
            ));
        }

        if self.shard_size > 10_000_000 {
            return Err(ShardexError::config_error(
                "shard_size",
                "exceeds recommended maximum",
                "Use shard_size <= 10,000,000 to avoid excessive memory usage",
            ));
        }

        // Validate batch write interval
        if self.batch_write_interval_ms == 0 {
            return Err(ShardexError::config_error(
                "batch_write_interval_ms",
                "must be greater than 0",
                "Set batch_write_interval_ms to balance throughput and latency (recommended: 50-500ms)",
            ));
        }

        // Validate WAL segment size
        if self.wal_segment_size < 1024 {
            return Err(ShardexError::config_error(
                "wal_segment_size",
                "must be at least 1KB",
                "Set wal_segment_size to at least 1024 bytes for proper WAL operation",
            ));
        }

        // Validate bloom filter size
        if self.bloom_filter_size == 0 {
            return Err(ShardexError::config_error(
                "bloom_filter_size",
                "must be greater than 0",
                "Set bloom_filter_size to optimize memory vs false positive trade-off (recommended: 1024-65536)",
            ));
        }

        // Validate default slop factor
        if self.default_slop_factor == 0 {
            return Err(ShardexError::config_error(
                "default_slop_factor",
                "must be greater than 0",
                "Set default_slop_factor to control search breadth (recommended: 3-10)",
            ));
        }

        if self.default_slop_factor > 1000 {
            return Err(ShardexError::config_error(
                "default_slop_factor",
                "exceeds reasonable maximum",
                "Use default_slop_factor <= 1000 to maintain search performance",
            ));
        }

        // Validate directory path
        if self.directory_path.as_os_str().is_empty() {
            return Err(ShardexError::config_error(
                "directory_path",
                "cannot be empty",
                "Provide a valid directory path where the index will be stored",
            ));
        }

        Ok(())
    }
}

impl From<ShardexConfig> for CreateIndexParams {
    fn from(config: ShardexConfig) -> Self {
        Self {
            directory_path: config.directory_path,
            vector_size: config.vector_size,
            shard_size: config.shard_size,
            batch_write_interval_ms: config.batch_write_interval_ms,
            wal_segment_size: config.wal_segment_size,
            bloom_filter_size: config.bloom_filter_size,
            default_slop_factor: config.slop_factor_config.default_factor as u32,
        }
    }
}

/// Builder for CreateIndexParams
#[derive(Debug, Clone, Default)]
pub struct CreateIndexParamsBuilder {
    directory_path: Option<PathBuf>,
    vector_size: Option<usize>,
    shard_size: Option<usize>,
    batch_write_interval_ms: Option<u64>,
    wal_segment_size: Option<usize>,
    bloom_filter_size: Option<usize>,
    default_slop_factor: Option<u32>,
}

impl CreateIndexParamsBuilder {
    /// Set the directory path
    pub fn directory_path(mut self, path: PathBuf) -> Self {
        self.directory_path = Some(path);
        self
    }

    /// Set the vector size
    pub fn vector_size(mut self, size: usize) -> Self {
        self.vector_size = Some(size);
        self
    }

    /// Set the shard size
    pub fn shard_size(mut self, size: usize) -> Self {
        self.shard_size = Some(size);
        self
    }

    /// Set the batch write interval
    pub fn batch_write_interval_ms(mut self, ms: u64) -> Self {
        self.batch_write_interval_ms = Some(ms);
        self
    }

    /// Set the WAL segment size
    pub fn wal_segment_size(mut self, size: usize) -> Self {
        self.wal_segment_size = Some(size);
        self
    }

    /// Set the bloom filter size
    pub fn bloom_filter_size(mut self, size: usize) -> Self {
        self.bloom_filter_size = Some(size);
        self
    }

    /// Set the default slop factor
    pub fn default_slop_factor(mut self, factor: u32) -> Self {
        self.default_slop_factor = Some(factor);
        self
    }

    /// Build the CreateIndexParams with validation
    pub fn build(self) -> Result<CreateIndexParams, ShardexError> {
        let params = CreateIndexParams {
            directory_path: self
                .directory_path
                .unwrap_or_else(|| PathBuf::from("./shardex_index")),
            vector_size: self.vector_size.unwrap_or(384),
            shard_size: self.shard_size.unwrap_or(10000),
            batch_write_interval_ms: self.batch_write_interval_ms.unwrap_or(100),
            wal_segment_size: self.wal_segment_size.unwrap_or(1024 * 1024), // 1MB
            bloom_filter_size: self.bloom_filter_size.unwrap_or(1024),
            default_slop_factor: self.default_slop_factor.unwrap_or(3),
        };
        params.validate()?;
        Ok(params)
    }
}

/// Parameters for adding postings to an index
///
/// Contains a collection of document postings to be added to the index
/// in a single batch operation.
#[derive(Debug, Clone)]
pub struct AddPostingsParams {
    /// Vector of postings to be added to the index
    pub postings: Vec<Posting>,
}

impl AddPostingsParams {
    /// Create new add postings parameters
    ///
    /// # Arguments
    ///
    /// * `postings` - Vector of postings to add
    ///
    /// # Errors
    ///
    /// Returns `ShardexError::Config` if the postings vector is empty.
    pub fn new(postings: Vec<Posting>) -> Result<Self, ShardexError> {
        if postings.is_empty() {
            return Err(ShardexError::config_error(
                "postings",
                "cannot be empty",
                "Provide at least one posting to add to the index",
            ));
        }
        Ok(Self { postings })
    }

    /// Validate the add postings parameters
    ///
    /// Checks that all postings have consistent vector dimensions and
    /// valid document IDs.
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.postings.is_empty() {
            return Err(ShardexError::config_error(
                "postings",
                "cannot be empty",
                "Provide at least one posting to add to the index",
            ));
        }

        // Check for consistent vector dimensions
        if let Some(first) = self.postings.first() {
            let expected_dim = first.vector.len();
            for (i, posting) in self.postings.iter().enumerate() {
                if posting.vector.len() != expected_dim {
                    return Err(ShardexError::config_error(
                        format!("postings[{}].vector", i),
                        format!(
                            "dimension mismatch: expected {}, got {}",
                            expected_dim,
                            posting.vector.len()
                        ),
                        "Ensure all postings have vectors with the same dimensions",
                    ));
                }

                // Validate vector contains only finite values
                for (j, &value) in posting.vector.iter().enumerate() {
                    if !value.is_finite() {
                        return Err(ShardexError::config_error(
                            format!("postings[{}].vector[{}]", i, j),
                            "contains non-finite value",
                            "Ensure all vector components are finite numbers (not NaN or infinity)",
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the number of postings
    pub fn len(&self) -> usize {
        self.postings.len()
    }

    /// Check if the postings collection is empty
    pub fn is_empty(&self) -> bool {
        self.postings.is_empty()
    }
}

/// Parameters for search operations
///
/// Defines the query vector, number of results, and search configuration
/// for similarity search operations.
#[derive(Debug, Clone)]
pub struct SearchParams {
    /// Query vector for similarity search
    pub query_vector: Vec<f32>,
    /// Number of top results to return
    pub k: usize,
    /// Optional slop factor override for this search
    pub slop_factor: Option<u32>,
}

impl SearchParams {
    /// Create a new search parameters builder
    pub fn builder() -> SearchParamsBuilder {
        SearchParamsBuilder::default()
    }

    /// Create search parameters with required fields
    pub fn new(query_vector: Vec<f32>, k: usize) -> Result<Self, ShardexError> {
        let params = Self {
            query_vector,
            k,
            slop_factor: None,
        };
        params.validate()?;
        Ok(params)
    }

    /// Create search parameters with slop factor
    pub fn with_slop_factor(query_vector: Vec<f32>, k: usize, slop_factor: u32) -> Result<Self, ShardexError> {
        let params = Self {
            query_vector,
            k,
            slop_factor: Some(slop_factor),
        };
        params.validate()?;
        Ok(params)
    }

    /// Validate the search parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        // Validate query vector
        if self.query_vector.is_empty() {
            return Err(ShardexError::config_error(
                "query_vector",
                "cannot be empty",
                "Provide a query vector with at least one dimension",
            ));
        }

        // Validate vector values are finite
        for (i, &value) in self.query_vector.iter().enumerate() {
            if !value.is_finite() {
                return Err(ShardexError::config_error(
                    format!("query_vector[{}]", i),
                    "contains non-finite value",
                    "Ensure all query vector components are finite numbers (not NaN or infinity)",
                ));
            }
        }

        // Validate k parameter
        if self.k == 0 {
            return Err(ShardexError::config_error(
                "k",
                "must be greater than 0",
                "Set k to the number of top results you want to retrieve",
            ));
        }

        if self.k > 10_000 {
            return Err(ShardexError::config_error(
                "k",
                "exceeds reasonable maximum",
                "Use k <= 10,000 to maintain reasonable response times and memory usage",
            ));
        }

        // Validate slop factor if provided
        if let Some(slop) = self.slop_factor {
            if slop == 0 {
                return Err(ShardexError::config_error(
                    "slop_factor",
                    "must be greater than 0 when provided",
                    "Set slop_factor to a positive value or use None for default",
                ));
            }

            if slop > 1000 {
                return Err(ShardexError::config_error(
                    "slop_factor",
                    "exceeds reasonable maximum",
                    "Use slop_factor <= 1000 to maintain search performance",
                ));
            }
        }

        Ok(())
    }

    /// Get the vector dimension
    pub fn vector_dimension(&self) -> usize {
        self.query_vector.len()
    }
}

/// Builder for SearchParams
#[derive(Debug, Clone, Default)]
pub struct SearchParamsBuilder {
    query_vector: Option<Vec<f32>>,
    k: Option<usize>,
    slop_factor: Option<u32>,
}

impl SearchParamsBuilder {
    /// Set the query vector
    pub fn query_vector(mut self, vector: Vec<f32>) -> Self {
        self.query_vector = Some(vector);
        self
    }

    /// Set the number of results to return
    pub fn k(mut self, k: usize) -> Self {
        self.k = Some(k);
        self
    }

    /// Set the slop factor
    pub fn slop_factor(mut self, factor: Option<u32>) -> Self {
        self.slop_factor = factor;
        self
    }

    /// Build the SearchParams with validation
    pub fn build(self) -> Result<SearchParams, ShardexError> {
        let query_vector = self.query_vector.ok_or_else(|| {
            ShardexError::config_error(
                "query_vector",
                "is required",
                "Provide a query vector using query_vector()",
            )
        })?;

        let k = self
            .k
            .ok_or_else(|| ShardexError::config_error("k", "is required", "Provide the number of results using k()"))?;

        let params = SearchParams {
            query_vector,
            k,
            slop_factor: self.slop_factor,
        };
        params.validate()?;
        Ok(params)
    }
}

/// Parameters for flush operations
///
/// Controls flush behavior and whether to return statistics.
#[derive(Debug, Clone, Default)]
pub struct FlushParams {
    /// Whether to include statistics in the flush response
    pub with_stats: bool,
}

impl FlushParams {
    /// Create default flush parameters
    pub fn new() -> Self {
        Self::default()
    }

    /// Create flush parameters with statistics
    pub fn with_stats() -> Self {
        Self { with_stats: true }
    }

    /// Set whether to include statistics
    pub fn set_with_stats(mut self, with_stats: bool) -> Self {
        self.with_stats = with_stats;
        self
    }

    /// Validate the flush parameters (always succeeds)
    pub fn validate(&self) -> Result<(), ShardexError> {
        // Flush parameters are always valid
        Ok(())
    }
}

/// Parameters for retrieving index statistics
///
/// Currently empty but provided for consistency and future extensibility.
#[derive(Debug, Clone, Default)]
pub struct GetStatsParams;

impl GetStatsParams {
    /// Create new stats parameters
    pub fn new() -> Self {
        Self
    }

    /// Validate the stats parameters (always succeeds)
    pub fn validate(&self) -> Result<(), ShardexError> {
        // Stats parameters are always valid
        Ok(())
    }
}

/// Parameters for opening an existing Shardex index
///
/// Contains the directory path where an existing index is stored.
/// The index configuration will be read from the index metadata.
#[derive(Debug, Clone)]
pub struct OpenIndexParams {
    /// Directory path where the existing index is stored
    pub directory_path: PathBuf,
}

impl OpenIndexParams {
    /// Create new open index parameters
    ///
    /// # Arguments
    ///
    /// * `directory_path` - Path to the directory containing the existing index
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::parameters::OpenIndexParams;
    /// use std::path::PathBuf;
    ///
    /// let params = OpenIndexParams::new(PathBuf::from("./existing_index"));
    /// assert_eq!(params.directory_path, PathBuf::from("./existing_index"));
    /// ```
    pub fn new(directory_path: PathBuf) -> Self {
        Self { directory_path }
    }

    /// Validate the open index parameters
    ///
    /// Checks that the directory path is not empty. Additional validation
    /// (like directory existence) is performed during the open operation.
    ///
    /// # Errors
    ///
    /// Returns `ShardexError::Config` if the directory path is empty.
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.directory_path.as_os_str().is_empty() {
            return Err(ShardexError::config_error(
                "directory_path",
                "cannot be empty",
                "Provide a valid directory path where the existing index is stored",
            ));
        }
        Ok(())
    }
}

/// Parameters for validating a ShardexConfig
///
/// Contains a ShardexConfig to be validated without creating or opening an index.
/// This is useful for testing configuration validity before performing operations.
#[derive(Debug, Clone)]
pub struct ValidateConfigParams {
    /// The configuration to validate
    pub config: ShardexConfig,
}

impl ValidateConfigParams {
    /// Create new validate config parameters
    ///
    /// # Arguments
    ///
    /// * `config` - The ShardexConfig to validate
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::parameters::ValidateConfigParams;
    /// use shardex::ShardexConfig;
    ///
    /// let config = ShardexConfig::new().vector_size(384).shard_size(10000);
    /// let params = ValidateConfigParams::new(config);
    /// assert_eq!(params.config.vector_size, 384);
    /// ```
    pub fn new(config: ShardexConfig) -> Self {
        Self { config }
    }

    /// Validate the parameters
    ///
    /// This validates the contained ShardexConfig and always succeeds for
    /// ValidateConfigParams itself (the config validation is the operation).
    ///
    /// # Errors
    ///
    /// Returns `ShardexError::Config` if the contained configuration is invalid.
    pub fn validate(&self) -> Result<(), ShardexError> {
        // The validation of the config itself is performed by the ValidateConfig operation
        // This method validates that we have a config to validate (always true)
        Ok(())
    }

    /// Get a reference to the configuration
    pub fn get_config(&self) -> &ShardexConfig {
        &self.config
    }
}

/// Parameters for batch adding postings to an index
///
/// Contains a collection of document postings to be added to the index
/// in a single batch operation with performance tracking options.
#[derive(Debug, Clone)]
pub struct BatchAddPostingsParams {
    /// Vector of postings to be added to the index
    pub postings: Vec<Posting>,
    /// Whether to flush immediately after adding postings
    pub flush_immediately: bool,
    /// Whether to track detailed performance metrics for this operation
    pub track_performance: bool,
}

impl BatchAddPostingsParams {
    /// Create new batch add postings parameters
    ///
    /// # Arguments
    ///
    /// * `postings` - Vector of postings to add
    /// * `flush_immediately` - Whether to flush immediately after adding
    /// * `track_performance` - Whether to collect performance metrics
    pub fn new(postings: Vec<Posting>, flush_immediately: bool, track_performance: bool) -> Result<Self, ShardexError> {
        if postings.is_empty() {
            return Err(ShardexError::config_error(
                "postings",
                "cannot be empty",
                "Provide at least one posting to add to the index",
            ));
        }
        Ok(Self {
            postings,
            flush_immediately,
            track_performance,
        })
    }

    /// Create batch parameters with default settings (no immediate flush, no performance tracking)
    pub fn simple(postings: Vec<Posting>) -> Result<Self, ShardexError> {
        Self::new(postings, false, false)
    }

    /// Create batch parameters with immediate flush
    pub fn with_immediate_flush(postings: Vec<Posting>) -> Result<Self, ShardexError> {
        Self::new(postings, true, false)
    }

    /// Create batch parameters with performance tracking
    pub fn with_performance_tracking(postings: Vec<Posting>) -> Result<Self, ShardexError> {
        Self::new(postings, false, true)
    }

    /// Create batch parameters with both immediate flush and performance tracking
    pub fn with_flush_and_tracking(postings: Vec<Posting>) -> Result<Self, ShardexError> {
        Self::new(postings, true, true)
    }

    /// Validate the batch add postings parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.postings.is_empty() {
            return Err(ShardexError::config_error(
                "postings",
                "cannot be empty",
                "Provide at least one posting to add to the index",
            ));
        }

        // Check for consistent vector dimensions
        if let Some(first) = self.postings.first() {
            let expected_dim = first.vector.len();
            for (i, posting) in self.postings.iter().enumerate() {
                if posting.vector.len() != expected_dim {
                    return Err(ShardexError::config_error(
                        format!("postings[{}].vector", i),
                        format!(
                            "dimension mismatch: expected {}, got {}",
                            expected_dim,
                            posting.vector.len()
                        ),
                        "Ensure all postings have vectors with the same dimensions",
                    ));
                }

                // Validate vector contains only finite values
                for (j, &value) in posting.vector.iter().enumerate() {
                    if !value.is_finite() {
                        return Err(ShardexError::config_error(
                            format!("postings[{}].vector[{}]", i, j),
                            "contains non-finite value",
                            "Ensure all vector components are finite numbers (not NaN or infinity)",
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the number of postings in this batch
    pub fn len(&self) -> usize {
        self.postings.len()
    }

    /// Check if the postings collection is empty
    pub fn is_empty(&self) -> bool {
        self.postings.is_empty()
    }
}

/// Parameters for retrieving performance statistics
///
/// Controls the level of detail in performance statistics retrieval.
#[derive(Debug, Clone, Default)]
pub struct GetPerformanceStatsParams {
    /// Whether to include detailed performance metrics (memory breakdown, operation counts)
    pub include_detailed: bool,
}

impl GetPerformanceStatsParams {
    /// Create new performance stats parameters with basic metrics only
    pub fn new() -> Self {
        Self::default()
    }

    /// Create parameters requesting detailed performance metrics
    pub fn detailed() -> Self {
        Self { include_detailed: true }
    }

    /// Set whether to include detailed metrics
    pub fn set_include_detailed(mut self, include_detailed: bool) -> Self {
        self.include_detailed = include_detailed;
        self
    }

    /// Validate the performance stats parameters (always succeeds)
    pub fn validate(&self) -> Result<(), ShardexError> {
        Ok(())
    }
}

/// Parameters for incremental adding operations
///
/// Supports incremental posting additions with optional batch tracking.
#[derive(Debug, Clone)]
pub struct IncrementalAddParams {
    /// Vector of postings to be added incrementally
    pub postings: Vec<Posting>,
    /// Optional batch identifier for tracking related incremental operations
    pub batch_id: Option<String>,
}

impl IncrementalAddParams {
    /// Create new incremental add parameters
    ///
    /// # Arguments
    ///
    /// * `postings` - Vector of postings to add
    /// * `batch_id` - Optional batch identifier for tracking
    pub fn new(postings: Vec<Posting>, batch_id: Option<String>) -> Result<Self, ShardexError> {
        if postings.is_empty() {
            return Err(ShardexError::config_error(
                "postings",
                "cannot be empty",
                "Provide at least one posting to add to the index",
            ));
        }
        Ok(Self { postings, batch_id })
    }

    /// Create incremental parameters without batch tracking
    pub fn simple(postings: Vec<Posting>) -> Result<Self, ShardexError> {
        Self::new(postings, None)
    }

    /// Create incremental parameters with batch tracking
    pub fn with_batch_id(postings: Vec<Posting>, batch_id: String) -> Result<Self, ShardexError> {
        Self::new(postings, Some(batch_id))
    }

    /// Validate the incremental add parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.postings.is_empty() {
            return Err(ShardexError::config_error(
                "postings",
                "cannot be empty",
                "Provide at least one posting to add to the index",
            ));
        }

        // Check for consistent vector dimensions
        if let Some(first) = self.postings.first() {
            let expected_dim = first.vector.len();
            for (i, posting) in self.postings.iter().enumerate() {
                if posting.vector.len() != expected_dim {
                    return Err(ShardexError::config_error(
                        format!("postings[{}].vector", i),
                        format!(
                            "dimension mismatch: expected {}, got {}",
                            expected_dim,
                            posting.vector.len()
                        ),
                        "Ensure all postings have vectors with the same dimensions",
                    ));
                }

                // Validate vector contains only finite values
                for (j, &value) in posting.vector.iter().enumerate() {
                    if !value.is_finite() {
                        return Err(ShardexError::config_error(
                            format!("postings[{}].vector[{}]", i, j),
                            "contains non-finite value",
                            "Ensure all vector components are finite numbers (not NaN or infinity)",
                        ));
                    }
                }
            }
        }

        // Validate batch_id if provided
        if let Some(ref batch_id) = self.batch_id {
            if batch_id.trim().is_empty() {
                return Err(ShardexError::config_error(
                    "batch_id",
                    "cannot be empty when provided",
                    "Provide a non-empty batch identifier or use None",
                ));
            }
        }

        Ok(())
    }

    /// Get the number of postings in this incremental batch
    pub fn len(&self) -> usize {
        self.postings.len()
    }

    /// Check if the postings collection is empty
    pub fn is_empty(&self) -> bool {
        self.postings.is_empty()
    }
}

/// Parameters for removing documents from the index
///
/// Contains document IDs to be removed in a batch operation.
#[derive(Debug, Clone)]
pub struct RemoveDocumentsParams {
    /// Vector of document IDs to remove from the index
    pub document_ids: Vec<u128>,
}

impl RemoveDocumentsParams {
    /// Create new remove documents parameters
    ///
    /// # Arguments
    ///
    /// * `document_ids` - Vector of document IDs to remove
    pub fn new(document_ids: Vec<u128>) -> Result<Self, ShardexError> {
        if document_ids.is_empty() {
            return Err(ShardexError::config_error(
                "document_ids",
                "cannot be empty",
                "Provide at least one document ID to remove",
            ));
        }
        Ok(Self { document_ids })
    }

    /// Validate the remove documents parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.document_ids.is_empty() {
            return Err(ShardexError::config_error(
                "document_ids",
                "cannot be empty",
                "Provide at least one document ID to remove",
            ));
        }
        Ok(())
    }

    /// Get the number of documents to remove
    pub fn len(&self) -> usize {
        self.document_ids.len()
    }

    /// Check if the document IDs collection is empty
    pub fn is_empty(&self) -> bool {
        self.document_ids.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifiers::DocumentId;

    #[test]
    fn test_create_index_params_builder() {
        let params = CreateIndexParams::builder()
            .directory_path(PathBuf::from("./test"))
            .vector_size(768)
            .shard_size(50000)
            .build()
            .unwrap();

        assert_eq!(params.directory_path, PathBuf::from("./test"));
        assert_eq!(params.vector_size, 768);
        assert_eq!(params.shard_size, 50000);
    }

    #[test]
    fn test_create_index_params_validation_vector_size_zero() {
        let params = CreateIndexParams::builder().vector_size(0).build();

        assert!(params.is_err());
    }

    #[test]
    fn test_create_index_params_from_config() {
        let config = ShardexConfig::new().vector_size(512).shard_size(25000);

        let params: CreateIndexParams = config.into();
        assert_eq!(params.vector_size, 512);
        assert_eq!(params.shard_size, 25000);
    }

    #[test]
    fn test_add_postings_params_empty() {
        let result = AddPostingsParams::new(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_postings_params_valid() {
        let posting = Posting {
            document_id: DocumentId::from_raw(1),
            start: 0,
            length: 100,
            vector: vec![0.1, 0.2, 0.3],
        };

        let params = AddPostingsParams::new(vec![posting]).unwrap();
        assert_eq!(params.len(), 1);
        assert!(!params.is_empty());
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_add_postings_params_dimension_mismatch() {
        let posting1 = Posting {
            document_id: DocumentId::from_raw(1),
            start: 0,
            length: 100,
            vector: vec![0.1, 0.2, 0.3],
        };

        let posting2 = Posting {
            document_id: DocumentId::from_raw(2),
            start: 0,
            length: 100,
            vector: vec![0.1, 0.2], // Different dimension
        };

        let params = AddPostingsParams::new(vec![posting1, posting2]).unwrap();
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_search_params_builder() {
        let params = SearchParams::builder()
            .query_vector(vec![0.1, 0.2, 0.3])
            .k(10)
            .slop_factor(Some(5))
            .build()
            .unwrap();

        assert_eq!(params.vector_dimension(), 3);
        assert_eq!(params.k, 10);
        assert_eq!(params.slop_factor, Some(5));
    }

    #[test]
    fn test_search_params_validation_empty_vector() {
        let result = SearchParams::new(vec![], 10);
        assert!(result.is_err());
    }

    #[test]
    fn test_search_params_validation_k_zero() {
        let result = SearchParams::new(vec![0.1, 0.2, 0.3], 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_search_params_with_slop_factor() {
        let params = SearchParams::with_slop_factor(vec![0.1, 0.2, 0.3], 10, 5).unwrap();
        assert_eq!(params.slop_factor, Some(5));
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_flush_params() {
        let params1 = FlushParams::new();
        assert!(!params1.with_stats);
        assert!(params1.validate().is_ok());

        let params2 = FlushParams::with_stats();
        assert!(params2.with_stats);
        assert!(params2.validate().is_ok());
    }

    #[test]
    fn test_get_stats_params() {
        let params = GetStatsParams::new();
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_create_index_params_high_performance() {
        let params = CreateIndexParams::high_performance(PathBuf::from("./high_perf"));

        assert_eq!(params.directory_path, PathBuf::from("./high_perf"));
        assert_eq!(params.vector_size, 256);
        assert_eq!(params.shard_size, 15000);
        assert_eq!(params.batch_write_interval_ms, 75);
        assert_eq!(params.wal_segment_size, 2 * 1024 * 1024);
        assert_eq!(params.bloom_filter_size, 2048);
        assert_eq!(params.default_slop_factor, 4);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_create_index_params_memory_optimized() {
        let params = CreateIndexParams::memory_optimized(PathBuf::from("./memory_opt"));

        assert_eq!(params.directory_path, PathBuf::from("./memory_opt"));
        assert_eq!(params.vector_size, 128);
        assert_eq!(params.shard_size, 5000);
        assert_eq!(params.batch_write_interval_ms, 200);
        assert_eq!(params.wal_segment_size, 256 * 1024);
        assert_eq!(params.bloom_filter_size, 512);
        assert_eq!(params.default_slop_factor, 2);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_create_index_params_from_shardex_config() {
        let config = ShardexConfig::new()
            .directory_path("./config_test")
            .vector_size(512)
            .shard_size(25000);

        let params = CreateIndexParams::from_shardex_config(config);
        assert_eq!(params.directory_path, PathBuf::from("./config_test"));
        assert_eq!(params.vector_size, 512);
        assert_eq!(params.shard_size, 25000);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_open_index_params() {
        let params = OpenIndexParams::new(PathBuf::from("./existing_index"));

        assert_eq!(params.directory_path, PathBuf::from("./existing_index"));
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_open_index_params_empty_path() {
        let params = OpenIndexParams::new(PathBuf::from(""));
        assert!(params.validate().is_err());
    }

    #[test]
    fn test_validate_config_params() {
        let config = ShardexConfig::new().vector_size(384).shard_size(10000);
        let params = ValidateConfigParams::new(config);

        assert_eq!(params.get_config().vector_size, 384);
        assert_eq!(params.get_config().shard_size, 10000);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_batch_add_postings_params() {
        let posting = Posting {
            document_id: DocumentId::from_raw(1),
            start: 0,
            length: 100,
            vector: vec![0.1, 0.2, 0.3],
        };

        let params = BatchAddPostingsParams::simple(vec![posting]).unwrap();
        assert_eq!(params.len(), 1);
        assert!(!params.is_empty());
        assert!(!params.flush_immediately);
        assert!(!params.track_performance);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_batch_add_postings_params_with_options() {
        let posting = Posting {
            document_id: DocumentId::from_raw(1),
            start: 0,
            length: 100,
            vector: vec![0.1, 0.2, 0.3],
        };

        let params = BatchAddPostingsParams::with_flush_and_tracking(vec![posting]).unwrap();
        assert!(params.flush_immediately);
        assert!(params.track_performance);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_batch_add_postings_params_empty() {
        let result = BatchAddPostingsParams::simple(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_performance_stats_params() {
        let params1 = GetPerformanceStatsParams::new();
        assert!(!params1.include_detailed);
        assert!(params1.validate().is_ok());

        let params2 = GetPerformanceStatsParams::detailed();
        assert!(params2.include_detailed);
        assert!(params2.validate().is_ok());
    }

    #[test]
    fn test_incremental_add_params() {
        let posting = Posting {
            document_id: DocumentId::from_raw(1),
            start: 0,
            length: 100,
            vector: vec![0.1, 0.2, 0.3],
        };

        let params1 = IncrementalAddParams::simple(vec![posting.clone()]).unwrap();
        assert_eq!(params1.len(), 1);
        assert!(!params1.is_empty());
        assert!(params1.batch_id.is_none());
        assert!(params1.validate().is_ok());

        let params2 = IncrementalAddParams::with_batch_id(vec![posting], "batch_123".to_string()).unwrap();
        assert_eq!(params2.batch_id.as_ref().unwrap(), "batch_123");
        assert!(params2.validate().is_ok());
    }

    #[test]
    fn test_incremental_add_params_empty_batch_id() {
        let posting = Posting {
            document_id: DocumentId::from_raw(1),
            start: 0,
            length: 100,
            vector: vec![0.1, 0.2, 0.3],
        };

        let params = IncrementalAddParams::new(vec![posting], Some("   ".to_string()));
        assert!(params.is_ok()); // Creation succeeds
        assert!(params.unwrap().validate().is_err()); // But validation fails
    }

    #[test]
    fn test_remove_documents_params() {
        let params = RemoveDocumentsParams::new(vec![1, 2, 3]).unwrap();
        assert_eq!(params.len(), 3);
        assert!(!params.is_empty());
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_remove_documents_params_empty() {
        let result = RemoveDocumentsParams::new(vec![]);
        assert!(result.is_err());
    }
}

/// Parameters for storing document text with postings
///
/// Contains the document ID, full text content, and associated postings
/// to be stored atomically in the index.
#[derive(Debug, Clone)]
pub struct StoreDocumentTextParams {
    /// Document ID for the text and postings
    pub document_id: DocumentId,
    /// Full text content of the document
    pub text: String,
    /// Vector of postings associated with this document
    pub postings: Vec<Posting>,
}

impl StoreDocumentTextParams {
    /// Create new store document text parameters
    ///
    /// # Arguments
    ///
    /// * `document_id` - Unique identifier for the document
    /// * `text` - Full text content of the document
    /// * `postings` - Vector of postings associated with the document
    pub fn new(document_id: DocumentId, text: String, postings: Vec<Posting>) -> Result<Self, ShardexError> {
        let params = Self {
            document_id,
            text,
            postings,
        };
        params.validate()?;
        Ok(params)
    }

    /// Validate the store document text parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        // Check that text is not empty
        if self.text.is_empty() {
            return Err(ShardexError::config_error(
                "text",
                "cannot be empty",
                "Provide non-empty text content for the document",
            ));
        }

        // Check that all postings belong to this document
        for (i, posting) in self.postings.iter().enumerate() {
            if posting.document_id != self.document_id {
                return Err(ShardexError::config_error(
                    format!("postings[{}].document_id", i),
                    format!(
                        "mismatch: expected {}, got {}",
                        self.document_id.raw(),
                        posting.document_id.raw()
                    ),
                    "Ensure all postings belong to the same document",
                ));
            }

            // Validate posting ranges are within document bounds
            let end_pos = posting.start + posting.length;
            if end_pos > self.text.len() as u32 {
                return Err(ShardexError::config_error(
                    format!("postings[{}]", i),
                    format!(
                        "range {}+{} exceeds document length {}",
                        posting.start,
                        posting.length,
                        self.text.len()
                    ),
                    "Ensure all posting ranges are within document bounds",
                ));
            }

            // Validate vector contains only finite values
            for (j, &value) in posting.vector.iter().enumerate() {
                if !value.is_finite() {
                    return Err(ShardexError::config_error(
                        format!("postings[{}].vector[{}]", i, j),
                        "contains non-finite value",
                        "Ensure all vector components are finite numbers (not NaN or infinity)",
                    ));
                }
            }
        }

        // Check for consistent vector dimensions among postings
        if let Some(first) = self.postings.first() {
            let expected_dim = first.vector.len();
            for (i, posting) in self.postings.iter().enumerate() {
                if posting.vector.len() != expected_dim {
                    return Err(ShardexError::config_error(
                        format!("postings[{}].vector", i),
                        format!(
                            "dimension mismatch: expected {}, got {}",
                            expected_dim,
                            posting.vector.len()
                        ),
                        "Ensure all postings have vectors with the same dimensions",
                    ));
                }
            }
        }

        Ok(())
    }

    /// Get the number of postings
    pub fn len(&self) -> usize {
        self.postings.len()
    }

    /// Check if the postings collection is empty
    pub fn is_empty(&self) -> bool {
        self.postings.is_empty()
    }

    /// Get the size of the document text in bytes
    pub fn text_size(&self) -> usize {
        self.text.len()
    }
}

/// Parameters for retrieving document text
///
/// Contains the document ID for text retrieval operations.
#[derive(Debug, Clone)]
pub struct GetDocumentTextParams {
    /// Document ID to retrieve text for
    pub document_id: DocumentId,
}

impl GetDocumentTextParams {
    /// Create new get document text parameters
    ///
    /// # Arguments
    ///
    /// * `document_id` - Unique identifier for the document
    pub fn new(document_id: DocumentId) -> Self {
        Self { document_id }
    }

    /// Validate the get document text parameters (always succeeds)
    pub fn validate(&self) -> Result<(), ShardexError> {
        Ok(())
    }
}

/// Parameters for extracting text snippets from postings
///
/// Contains the document ID and range information for snippet extraction.
#[derive(Debug, Clone)]
pub struct ExtractSnippetParams {
    /// Document ID to extract snippet from
    pub document_id: DocumentId,
    /// Starting position in the document text
    pub start: u32,
    /// Length of the snippet to extract
    pub length: u32,
}

impl ExtractSnippetParams {
    /// Create new extract snippet parameters
    ///
    /// # Arguments
    ///
    /// * `document_id` - Unique identifier for the document
    /// * `start` - Starting position in the document text
    /// * `length` - Length of the snippet to extract
    pub fn new(document_id: DocumentId, start: u32, length: u32) -> Result<Self, ShardexError> {
        let params = Self {
            document_id,
            start,
            length,
        };
        params.validate()?;
        Ok(params)
    }

    /// Create extract snippet parameters from a posting
    ///
    /// Convenience method that extracts the document ID, start, and length
    /// from an existing posting.
    ///
    /// # Arguments
    ///
    /// * `posting` - The posting to extract snippet parameters from
    pub fn from_posting(posting: &Posting) -> Self {
        Self {
            document_id: posting.document_id,
            start: posting.start,
            length: posting.length,
        }
    }

    /// Validate the extract snippet parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.length == 0 {
            return Err(ShardexError::config_error(
                "length",
                "must be greater than 0",
                "Provide a positive length for the snippet to extract",
            ));
        }

        // Check for potential overflow
        if let Some(end_pos) = self.start.checked_add(self.length) {
            if end_pos == 0 {
                return Err(ShardexError::config_error(
                    "start + length",
                    "results in zero end position",
                    "Ensure the snippet range is valid",
                ));
            }
        } else {
            return Err(ShardexError::config_error(
                "start + length",
                "results in overflow",
                "Ensure the start position and length don't exceed u32 limits",
            ));
        }

        Ok(())
    }

    /// Get the end position of the snippet
    pub fn end_position(&self) -> u32 {
        self.start + self.length
    }
}

/// Entry for batch document text operations
///
/// Contains document text and associated postings for batch storage.
#[derive(Debug, Clone)]
pub struct DocumentTextEntry {
    /// Document ID for this entry
    pub document_id: DocumentId,
    /// Full text content of the document
    pub text: String,
    /// Vector of postings associated with this document
    pub postings: Vec<Posting>,
}

impl DocumentTextEntry {
    /// Create a new document text entry
    ///
    /// # Arguments
    ///
    /// * `document_id` - Unique identifier for the document
    /// * `text` - Full text content of the document
    /// * `postings` - Vector of postings associated with the document
    pub fn new(document_id: DocumentId, text: String, postings: Vec<Posting>) -> Self {
        Self {
            document_id,
            text,
            postings,
        }
    }

    /// Get the size of the document text in bytes
    pub fn text_size(&self) -> usize {
        self.text.len()
    }

    /// Get the number of postings
    pub fn posting_count(&self) -> usize {
        self.postings.len()
    }
}

/// Parameters for batch storing document text operations
///
/// Contains multiple document text entries to be stored in a single batch.
#[derive(Debug, Clone)]
pub struct BatchStoreDocumentTextParams {
    /// Vector of document entries to store
    pub documents: Vec<DocumentTextEntry>,
    /// Whether to flush immediately after batch storage
    pub flush_immediately: bool,
    /// Whether to track performance metrics for this batch
    pub track_performance: bool,
}

impl BatchStoreDocumentTextParams {
    /// Create new batch store document text parameters
    ///
    /// # Arguments
    ///
    /// * `documents` - Vector of document entries to store
    /// * `flush_immediately` - Whether to flush immediately after storage
    /// * `track_performance` - Whether to collect performance metrics
    pub fn new(
        documents: Vec<DocumentTextEntry>,
        flush_immediately: bool,
        track_performance: bool,
    ) -> Result<Self, ShardexError> {
        if documents.is_empty() {
            return Err(ShardexError::config_error(
                "documents",
                "cannot be empty",
                "Provide at least one document entry for batch storage",
            ));
        }

        let params = Self {
            documents,
            flush_immediately,
            track_performance,
        };
        params.validate()?;
        Ok(params)
    }

    /// Create batch parameters with default settings (no immediate flush, no performance tracking)
    pub fn simple(documents: Vec<DocumentTextEntry>) -> Result<Self, ShardexError> {
        Self::new(documents, false, false)
    }

    /// Create batch parameters with immediate flush
    pub fn with_immediate_flush(documents: Vec<DocumentTextEntry>) -> Result<Self, ShardexError> {
        Self::new(documents, true, false)
    }

    /// Create batch parameters with performance tracking
    pub fn with_performance_tracking(documents: Vec<DocumentTextEntry>) -> Result<Self, ShardexError> {
        Self::new(documents, false, true)
    }

    /// Create batch parameters with both immediate flush and performance tracking
    pub fn with_flush_and_tracking(documents: Vec<DocumentTextEntry>) -> Result<Self, ShardexError> {
        Self::new(documents, true, true)
    }

    /// Validate the batch store document text parameters
    pub fn validate(&self) -> Result<(), ShardexError> {
        if self.documents.is_empty() {
            return Err(ShardexError::config_error(
                "documents",
                "cannot be empty",
                "Provide at least one document entry for batch storage",
            ));
        }

        // Validate each document entry
        for (i, entry) in self.documents.iter().enumerate() {
            // Check that text is not empty
            if entry.text.is_empty() {
                return Err(ShardexError::config_error(
                    format!("documents[{}].text", i),
                    "cannot be empty",
                    "Provide non-empty text content for all document entries",
                ));
            }

            // Check that all postings belong to this document
            for (j, posting) in entry.postings.iter().enumerate() {
                if posting.document_id != entry.document_id {
                    return Err(ShardexError::config_error(
                        format!("documents[{}].postings[{}].document_id", i, j),
                        format!(
                            "mismatch: expected {}, got {}",
                            entry.document_id.raw(),
                            posting.document_id.raw()
                        ),
                        "Ensure all postings belong to their respective documents",
                    ));
                }

                // Validate posting ranges are within document bounds
                let end_pos = posting.start + posting.length;
                if end_pos > entry.text.len() as u32 {
                    return Err(ShardexError::config_error(
                        format!("documents[{}].postings[{}]", i, j),
                        format!(
                            "range {}+{} exceeds document length {}",
                            posting.start,
                            posting.length,
                            entry.text.len()
                        ),
                        "Ensure all posting ranges are within document bounds",
                    ));
                }

                // Validate vector contains only finite values
                for (k, &value) in posting.vector.iter().enumerate() {
                    if !value.is_finite() {
                        return Err(ShardexError::config_error(
                            format!("documents[{}].postings[{}].vector[{}]", i, j, k),
                            "contains non-finite value",
                            "Ensure all vector components are finite numbers (not NaN or infinity)",
                        ));
                    }
                }
            }

            // Check for consistent vector dimensions within each document
            if let Some(first) = entry.postings.first() {
                let expected_dim = first.vector.len();
                for (j, posting) in entry.postings.iter().enumerate() {
                    if posting.vector.len() != expected_dim {
                        return Err(ShardexError::config_error(
                            format!("documents[{}].postings[{}].vector", i, j),
                            format!(
                                "dimension mismatch: expected {}, got {}",
                                expected_dim,
                                posting.vector.len()
                            ),
                            "Ensure all postings within a document have vectors with the same dimensions",
                        ));
                    }
                }
            }
        }

        // Check for unique document IDs across the batch
        let mut seen_ids = std::collections::HashSet::new();
        for (i, entry) in self.documents.iter().enumerate() {
            if !seen_ids.insert(entry.document_id) {
                return Err(ShardexError::config_error(
                    format!("documents[{}].document_id", i),
                    format!("duplicate document ID: {}", entry.document_id.raw()),
                    "Ensure all documents in the batch have unique document IDs",
                ));
            }
        }

        Ok(())
    }

    /// Get the number of documents in this batch
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Check if the documents collection is empty
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Get the total number of postings across all documents
    pub fn total_postings(&self) -> usize {
        self.documents.iter().map(|doc| doc.postings.len()).sum()
    }

    /// Get the total size of text across all documents
    pub fn total_text_size(&self) -> usize {
        self.documents.iter().map(|doc| doc.text.len()).sum()
    }

    /// Get the average document size in bytes
    pub fn average_document_size(&self) -> usize {
        if self.documents.is_empty() {
            0
        } else {
            self.total_text_size() / self.documents.len()
        }
    }
}

#[cfg(test)]
mod document_text_tests {
    use super::*;
    use crate::identifiers::DocumentId;

    #[test]
    fn test_store_document_text_params() {
        let doc_id = DocumentId::from_raw(1);
        let text = "Hello world".to_string();
        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: 5,
            vector: vec![0.1, 0.2, 0.3],
        };

        let params = StoreDocumentTextParams::new(doc_id, text, vec![posting]).unwrap();
        assert_eq!(params.document_id, doc_id);
        assert_eq!(params.text, "Hello world");
        assert_eq!(params.len(), 1);
        assert_eq!(params.text_size(), 11);
        assert!(!params.is_empty());
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_store_document_text_params_empty_text() {
        let doc_id = DocumentId::from_raw(1);
        let result = StoreDocumentTextParams::new(doc_id, String::new(), vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_store_document_text_params_document_id_mismatch() {
        let doc_id1 = DocumentId::from_raw(1);
        let doc_id2 = DocumentId::from_raw(2);
        let text = "Hello world".to_string();
        let posting = Posting {
            document_id: doc_id2, // Different from params doc_id
            start: 0,
            length: 5,
            vector: vec![0.1, 0.2, 0.3],
        };

        let result = StoreDocumentTextParams::new(doc_id1, text, vec![posting]);
        assert!(result.is_err());
    }

    #[test]
    fn test_store_document_text_params_range_exceeds_document() {
        let doc_id = DocumentId::from_raw(1);
        let text = "Hi".to_string(); // Only 2 characters
        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: 10, // Exceeds document length
            vector: vec![0.1, 0.2, 0.3],
        };

        let result = StoreDocumentTextParams::new(doc_id, text, vec![posting]);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_document_text_params() {
        let doc_id = DocumentId::from_raw(1);
        let params = GetDocumentTextParams::new(doc_id);
        assert_eq!(params.document_id, doc_id);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_extract_snippet_params() {
        let doc_id = DocumentId::from_raw(1);
        let params = ExtractSnippetParams::new(doc_id, 10, 5).unwrap();
        assert_eq!(params.document_id, doc_id);
        assert_eq!(params.start, 10);
        assert_eq!(params.length, 5);
        assert_eq!(params.end_position(), 15);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_extract_snippet_params_from_posting() {
        let doc_id = DocumentId::from_raw(1);
        let posting = Posting {
            document_id: doc_id,
            start: 5,
            length: 10,
            vector: vec![0.1, 0.2, 0.3],
        };

        let params = ExtractSnippetParams::from_posting(&posting);
        assert_eq!(params.document_id, doc_id);
        assert_eq!(params.start, 5);
        assert_eq!(params.length, 10);
    }

    #[test]
    fn test_extract_snippet_params_zero_length() {
        let doc_id = DocumentId::from_raw(1);
        let result = ExtractSnippetParams::new(doc_id, 10, 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_document_text_entry() {
        let doc_id = DocumentId::from_raw(1);
        let text = "Hello world".to_string();
        let posting = Posting {
            document_id: doc_id,
            start: 0,
            length: 5,
            vector: vec![0.1, 0.2, 0.3],
        };

        let entry = DocumentTextEntry::new(doc_id, text.clone(), vec![posting]);
        assert_eq!(entry.document_id, doc_id);
        assert_eq!(entry.text, text);
        assert_eq!(entry.text_size(), 11);
        assert_eq!(entry.posting_count(), 1);
    }

    #[test]
    fn test_batch_store_document_text_params() {
        let doc_id = DocumentId::from_raw(1);
        let entry = DocumentTextEntry::new(
            doc_id,
            "Hello world".to_string(),
            vec![Posting {
                document_id: doc_id,
                start: 0,
                length: 5,
                vector: vec![0.1, 0.2, 0.3],
            }],
        );

        let params = BatchStoreDocumentTextParams::simple(vec![entry]).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params.total_postings(), 1);
        assert_eq!(params.total_text_size(), 11);
        assert_eq!(params.average_document_size(), 11);
        assert!(!params.flush_immediately);
        assert!(!params.track_performance);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_batch_store_document_text_params_with_options() {
        let doc_id = DocumentId::from_raw(1);
        let entry = DocumentTextEntry::new(
            doc_id,
            "Hello world".to_string(),
            vec![Posting {
                document_id: doc_id,
                start: 0,
                length: 5,
                vector: vec![0.1, 0.2, 0.3],
            }],
        );

        let params = BatchStoreDocumentTextParams::with_flush_and_tracking(vec![entry]).unwrap();
        assert!(params.flush_immediately);
        assert!(params.track_performance);
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_batch_store_document_text_params_empty() {
        let result = BatchStoreDocumentTextParams::simple(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn test_batch_store_document_text_params_duplicate_ids() {
        let doc_id = DocumentId::from_raw(1);
        let entry1 = DocumentTextEntry::new(doc_id, "First".to_string(), vec![]);
        let entry2 = DocumentTextEntry::new(doc_id, "Second".to_string(), vec![]); // Duplicate ID

        let result = BatchStoreDocumentTextParams::simple(vec![entry1, entry2]);
        assert!(result.is_err());
    }
}

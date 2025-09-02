//! ShardexContext - Central context object for ApiThing pattern
//!
//! This module provides the `ShardexContext` struct that serves as the central
//! state holder for all Shardex operations following the ApiThing pattern.
//! The context manages the lifecycle of the index, configuration, and monitoring
//! components without requiring immediate index initialization.
//!
//! # Usage Examples
//!
//! ## Creating a Context
//!
//! ```rust
//! use shardex::api::ShardexContext;
//! use shardex::ShardexConfig;
//!
//! // Create with default configuration
//! let context = ShardexContext::new();
//! assert!(!context.is_initialized());
//!
//! // Create with custom configuration
//! let config = ShardexConfig::new()
//!     .directory_path("./my_index")
//!     .vector_size(768);
//!
//! let context = ShardexContext::with_config(config);
//! assert_eq!(context.get_config().vector_size, 768);
//! ```
//!
//! ## State Management
//!
//! ```rust
//! use shardex::api::ShardexContext;
//! use shardex::ShardexConfig;
//!
//! let mut context = ShardexContext::new()
//!     .set_directory_path("./test_index");
//!
//! // Context can be created without initializing the index
//! assert!(!context.is_initialized());
//!
//! // Configuration can be updated
//! let new_config = ShardexConfig::new().vector_size(512);
//! context.update_config(new_config).expect("Valid config");
//! assert_eq!(context.get_config().vector_size, 512);
//! ```

use crate::config::ShardexConfig;
use crate::error::ShardexError;
use crate::monitoring::{DetailedIndexStats, PerformanceMonitor as MonitoringPerformanceMonitor};
use crate::shardex::ShardexImpl;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Performance metrics tracking for batch operations
#[derive(Debug, Clone, Default)]
pub struct PerformanceTracker {
    /// Whether performance tracking is currently active
    pub is_active: bool,
    /// Start time for the current tracking session
    pub start_time: Option<Instant>,
    /// Total operation count across all types
    pub total_operations: u64,
    /// Operation timing breakdown by operation type
    pub operation_timings: HashMap<String, Vec<Duration>>,
    /// Cumulative time spent on different operation categories
    pub cumulative_timings: HashMap<String, Duration>,
}

impl PerformanceTracker {
    /// Create a new performance tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Start performance tracking
    pub fn start(&mut self) {
        self.is_active = true;
        self.start_time = Some(Instant::now());
    }

    /// Stop performance tracking
    pub fn stop(&mut self) {
        self.is_active = false;
        self.start_time = None;
    }

    /// Record an operation with its duration
    pub fn record_operation(&mut self, operation_type: &str, duration: Duration) {
        if !self.is_active {
            return;
        }

        self.total_operations += 1;

        // Track individual operation timings
        self.operation_timings
            .entry(operation_type.to_string())
            .or_default()
            .push(duration);

        // Track cumulative timings
        *self
            .cumulative_timings
            .entry(operation_type.to_string())
            .or_insert(Duration::ZERO) += duration;
    }

    /// Get average latency for a specific operation type
    pub fn average_latency(&self, operation_type: &str) -> Option<Duration> {
        self.operation_timings.get(operation_type).map(|timings| {
            if timings.is_empty() {
                Duration::ZERO
            } else {
                let total: Duration = timings.iter().sum();
                total / timings.len() as u32
            }
        })
    }

    /// Get overall average latency across all operations
    pub fn overall_average_latency(&self) -> Duration {
        if self.total_operations == 0 {
            return Duration::ZERO;
        }

        let total_time: Duration = self.cumulative_timings.values().sum();
        total_time / self.total_operations as u32
    }

    /// Calculate throughput in operations per second
    pub fn throughput(&self) -> f64 {
        if let Some(start_time) = self.start_time {
            let elapsed = start_time.elapsed().as_secs_f64();
            if elapsed > 0.0 {
                self.total_operations as f64 / elapsed
            } else {
                0.0
            }
        } else {
            0.0
        }
    }

    /// Reset all tracking data
    pub fn reset(&mut self) {
        self.total_operations = 0;
        self.operation_timings.clear();
        self.cumulative_timings.clear();
        if self.is_active {
            self.start_time = Some(Instant::now());
        }
    }
}

/// Central context object for Shardex operations following the ApiThing pattern.
///
/// `ShardexContext` holds shared state for all operations and manages the lifecycle
/// of Shardex components. The context can be created without requiring immediate
/// index initialization, allowing for flexible configuration and setup.
///
/// # Key Features
///
/// - **Lazy Initialization**: Index creation/opening can be deferred
/// - **Configuration Management**: Centralized config with validation
/// - **Performance Monitoring**: Integrated stats and monitoring
/// - **State Tracking**: Clear state management and lifecycle control
/// - **Batch Processing Support**: Performance tracking for batch operations
///
/// # Thread Safety
///
/// The context is designed to be used in single-threaded scenarios within
/// an ApiThing operation. For concurrent access, wrap in appropriate
/// synchronization primitives.
pub struct ShardexContext {
    /// Core index functionality (None until initialized)
    index: Option<ShardexImpl>,

    /// Configuration (can be updated)
    config: ShardexConfig,

    /// State tracking (None until index is opened/created)
    stats: Option<DetailedIndexStats>,

    /// Performance monitoring (None until enabled)
    monitor: Option<MonitoringPerformanceMonitor>,

    /// Directory path for the index (can be different from config)
    directory_path: Option<PathBuf>,

    /// Performance tracking for batch operations
    performance_tracker: PerformanceTracker,

    /// Text storage configuration and statistics
    text_storage_enabled: bool,

    /// Maximum document text size if text storage is enabled
    max_document_text_size: Option<usize>,
}

impl Default for ShardexContext {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ShardexContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShardexContext")
            .field("index", &self.index.is_some())
            .field("config", &self.config)
            .field("stats", &self.stats)
            .field("monitor", &self.monitor.is_some())
            .field("directory_path", &self.directory_path)
            .field("performance_tracker", &self.performance_tracker)
            .field("text_storage_enabled", &self.text_storage_enabled)
            .field("max_document_text_size", &self.max_document_text_size)
            .finish()
    }
}

impl ShardexContext {
    /// Create a new empty context with default configuration.
    ///
    /// The returned context will not have an initialized index and will use
    /// default `ShardexConfig` values. Call `is_initialized()` to check if
    /// the index has been created or opened.
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    ///
    /// let context = ShardexContext::new();
    /// assert!(!context.is_initialized());
    /// assert_eq!(context.get_config().vector_size, 384); // Default
    /// ```
    pub fn new() -> Self {
        Self {
            index: None,
            config: ShardexConfig::default(),
            stats: None,
            monitor: None,
            directory_path: None,
            performance_tracker: PerformanceTracker::new(),
            text_storage_enabled: false,
            max_document_text_size: None,
        }
    }

    /// Create a context with the specified configuration.
    ///
    /// The configuration will be validated when `build()` is called or when
    /// the index is initialized. Invalid configurations will cause operations
    /// to fail at that point.
    ///
    /// # Arguments
    ///
    /// * `config` - The ShardexConfig to use for this context
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    /// use shardex::ShardexConfig;
    ///
    /// let config = ShardexConfig::new()
    ///     .vector_size(768)
    ///     .shard_size(50000);
    ///
    /// let context = ShardexContext::with_config(config);
    /// assert_eq!(context.get_config().vector_size, 768);
    /// assert_eq!(context.get_config().shard_size, 50000);
    /// ```
    pub fn with_config(config: ShardexConfig) -> Self {
        let text_storage_enabled = config.max_document_text_size > 0;
        let max_document_text_size = if text_storage_enabled {
            Some(config.max_document_text_size)
        } else {
            None
        };

        Self {
            index: None,
            config,
            stats: None,
            monitor: None,
            directory_path: None,
            performance_tracker: PerformanceTracker::new(),
            text_storage_enabled,
            max_document_text_size,
        }
    }

    /// Check if the index is initialized (created or opened).
    ///
    /// Returns `true` if the underlying `ShardexImpl` has been created or opened,
    /// `false` otherwise. Operations that require an initialized index will
    /// fail if this returns `false`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    ///
    /// let context = ShardexContext::new();
    /// assert!(!context.is_initialized());
    ///
    /// // After create() or open() operations (not shown)
    /// // assert!(context.is_initialized());
    /// ```
    pub fn is_initialized(&self) -> bool {
        self.index.is_some()
    }

    /// Get a reference to the current configuration.
    ///
    /// Returns the `ShardexConfig` currently held by this context. The
    /// configuration may be updated using `update_config()`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    /// use shardex::ShardexConfig;
    ///
    /// let config = ShardexConfig::new().vector_size(512);
    /// let context = ShardexContext::with_config(config);
    ///
    /// assert_eq!(context.get_config().vector_size, 512);
    /// ```
    pub fn get_config(&self) -> &ShardexConfig {
        &self.config
    }

    /// Get the current index statistics if available.
    ///
    /// Returns `Some(DetailedIndexStats)` if the index is initialized and
    /// statistics are available, `None` otherwise. Statistics are typically
    /// available after the index has been opened or created.
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    ///
    /// let context = ShardexContext::new();
    /// assert!(context.get_stats().is_none());
    ///
    /// // After index initialization (not shown)
    /// // let stats = context.get_stats().expect("Stats available");
    /// // println!("Total postings: {}", stats.total_postings);
    /// ```
    pub fn get_stats(&self) -> Option<&DetailedIndexStats> {
        self.stats.as_ref()
    }

    /// Get the current directory path if set.
    ///
    /// Returns the directory path that will be used for index operations,
    /// or `None` if no path has been explicitly set. When `None`, the
    /// path from the configuration will be used.
    pub fn get_directory_path(&self) -> Option<&PathBuf> {
        self.directory_path.as_ref()
    }

    /// Get the effective directory path for operations.
    ///
    /// Returns the directory path that will actually be used for index
    /// operations. This is either the explicitly set directory path or
    /// the path from the configuration.
    pub fn effective_directory_path(&self) -> &PathBuf {
        self.directory_path
            .as_ref()
            .unwrap_or(&self.config.directory_path)
    }

    /// Set the directory path for index operations.
    ///
    /// This sets an explicit directory path that will be used instead of
    /// the path in the configuration. Useful when the same configuration
    /// needs to be used with different directories.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory where the index will be stored
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    ///
    /// let context = ShardexContext::new()
    ///     .set_directory_path("./custom_index");
    ///
    /// assert_eq!(
    ///     context.get_directory_path().unwrap().to_str().unwrap(),
    ///     "./custom_index"
    /// );
    /// ```
    pub fn set_directory_path<P: Into<PathBuf>>(mut self, path: P) -> Self {
        self.directory_path = Some(path.into());
        self
    }

    /// Update the context configuration.
    ///
    /// Replaces the current configuration with a new one. The configuration
    /// will be validated before being applied. If the index is already
    /// initialized, some configuration changes may not take effect until
    /// the next create/open operation.
    ///
    /// # Arguments
    ///
    /// * `config` - The new configuration to apply
    ///
    /// # Errors
    ///
    /// Returns `ShardexError::Config` if the configuration is invalid.
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    /// use shardex::ShardexConfig;
    ///
    /// let mut context = ShardexContext::new();
    /// assert_eq!(context.get_config().vector_size, 384);
    ///
    /// let new_config = ShardexConfig::new().vector_size(768);
    /// context.update_config(new_config).expect("Valid config");
    /// assert_eq!(context.get_config().vector_size, 768);
    /// ```
    pub fn update_config(&mut self, config: ShardexConfig) -> Result<(), ShardexError> {
        // Validate the configuration before applying
        config.validate()?;

        // Update text storage configuration
        self.text_storage_enabled = config.max_document_text_size > 0;
        self.max_document_text_size = if self.text_storage_enabled {
            Some(config.max_document_text_size)
        } else {
            None
        };

        self.config = config;
        Ok(())
    }

    /// Clear the directory path override.
    ///
    /// After calling this method, `effective_directory_path()` will return
    /// the path from the configuration instead of an explicitly set path.
    pub fn clear_directory_path(&mut self) {
        self.directory_path = None;
    }

    /// Check if performance monitoring is enabled.
    ///
    /// Returns `true` if a performance monitor is attached to this context.
    pub fn has_monitor(&self) -> bool {
        self.monitor.is_some()
    }

    /// Get a reference to the performance monitor if available.
    pub fn get_monitor(&self) -> Option<&MonitoringPerformanceMonitor> {
        self.monitor.as_ref()
    }

    /// Start performance tracking for batch operations
    ///
    /// Enables performance monitoring and resets any existing tracking data.
    /// This should be called before starting a batch operation sequence.
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    ///
    /// let mut context = ShardexContext::new();
    /// context.start_performance_tracking();
    /// assert!(context.is_performance_tracking_active());
    /// ```
    pub fn start_performance_tracking(&mut self) {
        self.performance_tracker.start();
    }

    /// Stop performance tracking for batch operations
    ///
    /// Disables performance monitoring. Tracked data remains available
    /// for retrieval until the next start or reset.
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    ///
    /// let mut context = ShardexContext::new();
    /// context.start_performance_tracking();
    /// context.stop_performance_tracking();
    /// assert!(!context.is_performance_tracking_active());
    /// ```
    pub fn stop_performance_tracking(&mut self) {
        self.performance_tracker.stop();
    }

    /// Record an operation with its duration for performance tracking
    ///
    /// If performance tracking is active, records the operation type and
    /// duration for later analysis. This is typically called automatically
    /// by batch operations.
    ///
    /// # Arguments
    ///
    /// * `operation` - Name of the operation type (e.g., "batch_add", "search", "flush")
    /// * `duration` - Time taken to complete the operation
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    /// use std::time::Duration;
    ///
    /// let mut context = ShardexContext::new();
    /// context.start_performance_tracking();
    /// context.record_operation("test_operation", Duration::from_millis(100));
    /// ```
    pub fn record_operation(&mut self, operation: &str, duration: Duration) {
        self.performance_tracker
            .record_operation(operation, duration);
    }

    /// Get current performance statistics from the tracking system
    ///
    /// Returns performance metrics collected since tracking was started.
    /// This includes operation counts, average latencies, and throughput data.
    ///
    /// # Returns
    ///
    /// Returns performance data including:
    /// - Total operations performed
    /// - Average latency across all operations
    /// - Current throughput (operations per second)
    /// - Operation-specific breakdowns
    ///
    /// # Example
    ///
    /// ```rust
    /// use shardex::api::ShardexContext;
    /// use std::time::Duration;
    ///
    /// let mut context = ShardexContext::new();
    /// context.start_performance_tracking();
    /// context.record_operation("test", Duration::from_millis(50));
    ///
    /// let total_ops = context.get_total_operations();
    /// let avg_latency = context.get_average_latency();
    /// let throughput = context.get_throughput();
    ///
    /// assert_eq!(total_ops, 1);
    /// assert!(avg_latency.as_millis() > 0);
    /// assert!(throughput >= 0.0);
    /// ```
    pub fn get_total_operations(&self) -> u64 {
        self.performance_tracker.total_operations
    }

    /// Get the average latency across all tracked operations
    pub fn get_average_latency(&self) -> Duration {
        self.performance_tracker.overall_average_latency()
    }

    /// Get the current throughput in operations per second
    pub fn get_throughput(&self) -> f64 {
        self.performance_tracker.throughput()
    }

    /// Get the average latency for a specific operation type
    pub fn get_operation_average_latency(&self, operation_type: &str) -> Option<Duration> {
        self.performance_tracker.average_latency(operation_type)
    }

    /// Check if performance tracking is currently active
    pub fn is_performance_tracking_active(&self) -> bool {
        self.performance_tracker.is_active
    }

    /// Reset performance tracking data
    ///
    /// Clears all accumulated performance data while maintaining the
    /// current active/inactive state of the tracker.
    pub fn reset_performance_tracking(&mut self) {
        self.performance_tracker.reset();
    }

    /// Get a reference to the performance tracker for detailed analysis
    pub fn get_performance_tracker(&self) -> &PerformanceTracker {
        &self.performance_tracker
    }

    /// Check if text storage is enabled for this context
    ///
    /// Returns `true` if the index configuration supports document text storage,
    /// `false` otherwise. Text storage is enabled when max_document_text_size > 0.
    pub fn is_text_storage_enabled(&self) -> bool {
        self.text_storage_enabled
    }

    /// Get the maximum document text size if text storage is enabled
    ///
    /// Returns `Some(size)` if text storage is enabled, `None` otherwise.
    pub fn get_max_document_text_size(&self) -> Option<usize> {
        self.max_document_text_size
    }

    /// Validate that text storage is available for the given text size
    ///
    /// Checks that text storage is enabled and the text size is within limits.
    ///
    /// # Arguments
    ///
    /// * `text_size` - Size of the text to validate in bytes
    ///
    /// # Errors
    ///
    /// Returns `ShardexError::Config` if text storage is not enabled or
    /// the text size exceeds the configured maximum.
    pub fn validate_text_storage(&self, text_size: usize) -> Result<(), ShardexError> {
        if !self.text_storage_enabled {
            return Err(ShardexError::config_error(
                "text_storage",
                "text storage is not enabled",
                "Enable text storage by setting max_document_text_size > 0 in the configuration",
            ));
        }

        if let Some(max_size) = self.max_document_text_size {
            if text_size > max_size {
                return Err(ShardexError::config_error(
                    "text_size",
                    format!("text size {} bytes exceeds maximum {}", text_size, max_size),
                    format!(
                        "Reduce document size or increase max_document_text_size to at least {} bytes",
                        text_size
                    ),
                ));
            }
        }

        Ok(())
    }

    /// Validate that text storage is available for multiple documents
    ///
    /// Checks that text storage is enabled and all document text sizes are within limits.
    ///
    /// # Arguments
    ///
    /// * `text_sizes` - Iterator of text sizes to validate in bytes
    ///
    /// # Errors
    ///
    /// Returns `ShardexError::Config` if text storage is not enabled or
    /// any text size exceeds the configured maximum.
    pub fn validate_batch_text_storage<I>(&self, text_sizes: I) -> Result<(), ShardexError>
    where
        I: IntoIterator<Item = usize>,
    {
        if !self.text_storage_enabled {
            return Err(ShardexError::config_error(
                "text_storage",
                "text storage is not enabled",
                "Enable text storage by setting max_document_text_size > 0 in the configuration",
            ));
        }

        if let Some(max_size) = self.max_document_text_size {
            for (i, text_size) in text_sizes.into_iter().enumerate() {
                if text_size > max_size {
                    return Err(ShardexError::config_error(
                        format!("text_sizes[{}]", i),
                        format!("text size {} bytes exceeds maximum {}", text_size, max_size),
                        format!(
                            "Reduce document sizes or increase max_document_text_size to handle {} bytes",
                            text_size
                        ),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Internal method to set the index implementation.
    ///
    /// This is used by operations that create or open indices to store
    /// the initialized index in the context.
    pub(crate) fn set_index(&mut self, index: ShardexImpl) {
        self.index = Some(index);
    }

    /// Internal method to get a reference to the index implementation.
    ///
    /// Returns `Some(&ShardexImpl)` if the index is initialized, `None` otherwise.
    /// This is used by operations that need to access the underlying index.
    pub(crate) fn get_index(&self) -> Option<&ShardexImpl> {
        self.index.as_ref()
    }

    /// Internal method to get a mutable reference to the index implementation.
    ///
    /// Returns `Some(&mut ShardexImpl)` if the index is initialized, `None` otherwise.
    /// This is used by operations that need to modify the underlying index.
    pub(crate) fn get_index_mut(&mut self) -> Option<&mut ShardexImpl> {
        self.index.as_mut()
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_context() {
        let context = ShardexContext::new();
        assert!(!context.is_initialized());
        assert!(context.get_stats().is_none());
        assert!(context.get_directory_path().is_none());
        assert!(!context.has_monitor());
    }

    #[test]
    fn test_default_context() {
        let context = ShardexContext::default();
        assert_eq!(context.get_config().vector_size, 384); // Default
        assert!(!context.is_initialized());
    }

    #[test]
    fn test_with_config() {
        let config = ShardexConfig::new().vector_size(768).shard_size(50000);
        let context = ShardexContext::with_config(config);

        assert_eq!(context.get_config().vector_size, 768);
        assert_eq!(context.get_config().shard_size, 50000);
        assert!(!context.is_initialized());
    }

    #[test]
    fn test_set_directory_path() {
        let context = ShardexContext::new().set_directory_path("./test_index");

        assert_eq!(context.get_directory_path().unwrap().to_str().unwrap(), "./test_index");
        assert_eq!(context.effective_directory_path().to_str().unwrap(), "./test_index");
    }

    #[test]
    fn test_effective_directory_path_fallback() {
        let config = ShardexConfig::new().directory_path("./config_path");
        let context = ShardexContext::with_config(config);

        // Should use config path when no explicit path is set
        assert_eq!(context.effective_directory_path().to_str().unwrap(), "./config_path");
    }

    #[test]
    fn test_update_config_valid() {
        let mut context = ShardexContext::new();
        assert_eq!(context.get_config().vector_size, 384);

        let new_config = ShardexConfig::new().vector_size(768);
        let result = context.update_config(new_config);

        assert!(result.is_ok());
        assert_eq!(context.get_config().vector_size, 768);
    }

    #[test]
    fn test_update_config_invalid() {
        let mut context = ShardexContext::new();

        let invalid_config = ShardexConfig::new().vector_size(0); // Invalid
        let result = context.update_config(invalid_config);

        assert!(result.is_err());
        // Original config should be unchanged
        assert_eq!(context.get_config().vector_size, 384);
    }

    #[test]
    fn test_clear_directory_path() {
        let mut context = ShardexContext::new().set_directory_path("./test");

        assert!(context.get_directory_path().is_some());

        context.clear_directory_path();
        assert!(context.get_directory_path().is_none());
    }

    #[test]
    fn test_context_creation_equality() {
        let config = ShardexConfig::new().vector_size(512);
        let context1 = ShardexContext::with_config(config.clone()).set_directory_path("./test");

        let context2 = ShardexContext::with_config(config).set_directory_path("./test");

        assert_eq!(context1.get_config().vector_size, context2.get_config().vector_size);
        assert_eq!(context1.get_directory_path(), context2.get_directory_path());
        assert_eq!(context1.is_initialized(), context2.is_initialized());
    }

    #[test]
    fn test_context_debug() {
        let context = ShardexContext::new();
        let debug_str = format!("{:?}", context);
        assert!(debug_str.contains("ShardexContext"));
        assert!(debug_str.contains("performance_tracker"));
    }

    #[test]
    fn test_performance_tracking() {
        let mut context = ShardexContext::new();

        assert!(!context.is_performance_tracking_active());
        assert_eq!(context.get_total_operations(), 0);

        // Start tracking
        context.start_performance_tracking();
        assert!(context.is_performance_tracking_active());

        // Record some operations
        context.record_operation("test_op", Duration::from_millis(100));
        context.record_operation("test_op", Duration::from_millis(200));
        context.record_operation("other_op", Duration::from_millis(50));

        assert_eq!(context.get_total_operations(), 3);
        assert!(context.get_average_latency().as_millis() > 0);
        assert!(context.get_throughput() >= 0.0);

        // Test operation-specific latency
        let test_op_latency = context.get_operation_average_latency("test_op");
        assert!(test_op_latency.is_some());
        assert_eq!(test_op_latency.unwrap(), Duration::from_millis(150)); // Average of 100 and 200

        let other_op_latency = context.get_operation_average_latency("other_op");
        assert!(other_op_latency.is_some());
        assert_eq!(other_op_latency.unwrap(), Duration::from_millis(50));

        // Stop tracking
        context.stop_performance_tracking();
        assert!(!context.is_performance_tracking_active());

        // Data should still be available
        assert_eq!(context.get_total_operations(), 3);
    }

    #[test]
    fn test_performance_tracking_reset() {
        let mut context = ShardexContext::new();

        context.start_performance_tracking();
        context.record_operation("test", Duration::from_millis(100));
        assert_eq!(context.get_total_operations(), 1);

        context.reset_performance_tracking();
        assert_eq!(context.get_total_operations(), 0);
        assert!(context.is_performance_tracking_active()); // Should still be active
    }

    #[test]
    fn test_performance_tracker_standalone() {
        let mut tracker = PerformanceTracker::new();

        assert!(!tracker.is_active);
        assert_eq!(tracker.total_operations, 0);

        tracker.start();
        assert!(tracker.is_active);

        tracker.record_operation("test", Duration::from_millis(100));
        tracker.record_operation("test", Duration::from_millis(200));

        assert_eq!(tracker.total_operations, 2);
        assert_eq!(tracker.average_latency("test"), Some(Duration::from_millis(150)));
        assert_eq!(tracker.overall_average_latency(), Duration::from_millis(150));

        tracker.stop();
        assert!(!tracker.is_active);
    }

    #[test]
    fn test_performance_tracker_throughput() {
        let mut tracker = PerformanceTracker::new();
        tracker.start();

        // Record operations and check that throughput can be calculated
        tracker.record_operation("test", Duration::from_millis(10));

        // Throughput should be calculable (operations per second)
        let throughput = tracker.throughput();
        assert!(throughput >= 0.0);
    }

    #[test]
    fn test_text_storage_configuration() {
        // Test with text storage disabled (default)
        let context = ShardexContext::new();
        assert!(!context.is_text_storage_enabled());
        assert!(context.get_max_document_text_size().is_none());

        // Test with text storage enabled
        let config = ShardexConfig::new().max_document_text_size(1024 * 1024); // 1MB
        let context = ShardexContext::with_config(config);
        assert!(context.is_text_storage_enabled());
        assert_eq!(context.get_max_document_text_size(), Some(1024 * 1024));
    }

    #[test]
    fn test_text_storage_validation() {
        // Test with text storage disabled
        let context = ShardexContext::new();
        let result = context.validate_text_storage(1000);
        assert!(result.is_err());

        // Test with text storage enabled
        let config = ShardexConfig::new().max_document_text_size(2048); // 2KB
        let context = ShardexContext::with_config(config);

        // Valid size should pass
        let result = context.validate_text_storage(1000);
        assert!(result.is_ok());

        // Size exceeding limit should fail
        let result = context.validate_text_storage(3000);
        assert!(result.is_err());

        // Size at limit should pass
        let result = context.validate_text_storage(2048);
        assert!(result.is_ok());
    }

    #[test]
    fn test_batch_text_storage_validation() {
        let config = ShardexConfig::new().max_document_text_size(1000);
        let context = ShardexContext::with_config(config);

        // All valid sizes
        let sizes = vec![500, 800, 1000];
        let result = context.validate_batch_text_storage(sizes);
        assert!(result.is_ok());

        // One size exceeds limit
        let sizes = vec![500, 1200, 800];
        let result = context.validate_batch_text_storage(sizes);
        assert!(result.is_err());

        // Empty batch should pass
        let sizes: Vec<usize> = vec![];
        let result = context.validate_batch_text_storage(sizes);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_update_with_text_storage() {
        let mut context = ShardexContext::new();
        assert!(!context.is_text_storage_enabled());

        // Enable text storage through config update
        let new_config = ShardexConfig::new().max_document_text_size(5 * 1024 * 1024);
        context.update_config(new_config).unwrap();

        assert!(context.is_text_storage_enabled());
        assert_eq!(context.get_max_document_text_size(), Some(5 * 1024 * 1024));

        // Disable text storage
        let disable_config = ShardexConfig::new().max_document_text_size(0);
        context.update_config(disable_config).unwrap();

        assert!(!context.is_text_storage_enabled());
        assert!(context.get_max_document_text_size().is_none());
    }

    #[test]
    fn test_context_debug_includes_text_storage() {
        let config = ShardexConfig::new().max_document_text_size(1024);
        let context = ShardexContext::with_config(config);
        let debug_str = format!("{:?}", context);

        assert!(debug_str.contains("text_storage_enabled: true"));
        assert!(debug_str.contains("max_document_text_size: Some(1024)"));
    }
}

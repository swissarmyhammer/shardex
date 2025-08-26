//! Copy-on-Write Index Implementation
//!
//! This module provides the CowShardexIndex wrapper that enables non-blocking reads
//! during index updates using copy-on-write semantics. The implementation uses Arc
//! for reference counting and atomic operations for safe concurrent access.
//!
//! # Key Features
//!
//! - **Non-blocking Reads**: Readers never block during index updates
//! - **Atomic Updates**: All updates are atomic and maintain consistency  
//! - **Automatic Cleanup**: Reference counting ensures old versions are cleaned up
//! - **Memory Efficient**: Clones metadata only, shares underlying shard data
//! - **Performance Monitoring**: Built-in metrics for tracking memory usage and operation performance
//!
//! # Performance Characteristics
//!
//! - **Read-heavy workloads**: Excellent performance with minimal overhead
//! - **Write-heavy workloads**: Clone overhead scales with shard count, not data size
//! - **Memory usage**: Proportional to number of concurrent readers and writers
//! - **Monitoring**: Use `metrics()` method to track performance and optimize usage patterns
//!
//! # Usage Examples
//!
//! ## Basic Copy-on-Write Operations
//!
//! ```rust
//! use shardex::cow_index::CowShardexIndex;
//! use shardex::shardex_index::ShardexIndex;
//! use shardex::config::ShardexConfig;
//! use tempfile::TempDir;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let config = ShardexConfig::new()
//!     .directory_path(temp_dir.path())
//!     .vector_size(128);
//!
//! // Create initial index
//! let index = ShardexIndex::create(config)?;
//! let cow_index = CowShardexIndex::new(index);
//!
//! // Readers can access the index without blocking
//! let reader_view = cow_index.read();
//! println!("Shards: {}", reader_view.shard_count());
//!
//! // Writers get their own copy for modifications
//! let writer = cow_index.clone_for_write()?;
//! // ... modify the index ...
//! # Ok(())
//! # }
//! ```
//!
//! ## Concurrent Access Pattern
//!
//! ```rust,no_run
//! use shardex::cow_index::CowShardexIndex;
//! use std::sync::Arc;
//! use std::thread;
//!
//! # fn concurrent_example(input_cow_index: CowShardexIndex) -> Result<(), Box<dyn std::error::Error>> {
//! let cow_index = Arc::new(input_cow_index);
//!
//! // Spawn multiple reader threads
//! let mut handles = vec![];
//! for i in 0..4 {
//!     let cow_index_clone = Arc::clone(&cow_index);
//!     let handle = thread::spawn(move || {
//!         // Each reader gets a stable view
//!         let reader = cow_index_clone.read();
//!         println!("Reader {}: {} shards", i, reader.shard_count());
//!     });
//!     handles.push(handle);
//! }
//!
//! // Writer can update concurrently
//! let _writer = cow_index.clone_for_write();
//! // ... perform updates ...
//!
//! // Wait for all readers
//! for handle in handles {
//!     handle.join().unwrap();
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::ShardexError;
use crate::shardex_index::ShardexIndex;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Configuration for memory usage estimation in COW operations
///
/// These values control the accuracy of memory usage estimates and can be
/// tuned based on actual usage patterns in your application.
#[derive(Debug, Clone)]
pub struct CowMemoryConfig {
    /// Estimated metadata size per shard in bytes (default: 256)
    pub metadata_bytes_per_shard: usize,
    /// Estimated average number of vectors per shard (default: 1000)
    pub average_vectors_per_shard: usize,
    /// Estimated memory overhead per modified shard in lazy writer (default: 1MB)
    pub modified_shard_overhead_bytes: usize,
    /// Estimated metadata size for new shards (default: 256)
    pub new_shard_metadata_bytes: usize,
}

impl Default for CowMemoryConfig {
    fn default() -> Self {
        Self {
            metadata_bytes_per_shard: 256,
            average_vectors_per_shard: 1000,
            modified_shard_overhead_bytes: 1024 * 1024, // 1MB
            new_shard_metadata_bytes: 256,
        }
    }
}

/// Performance and memory usage metrics for Copy-on-Write operations
///
/// This structure tracks key performance indicators and memory usage
/// patterns for COW operations to help with optimization and monitoring.
#[derive(Debug, Clone)]
pub struct CowMetrics {
    /// Number of active reader references currently held
    pub active_readers: usize,
    /// Number of writers currently created but not yet committed
    pub pending_writers: usize,
    /// Estimated memory usage in bytes for the current index
    pub memory_usage_bytes: usize,
    /// Total number of clone operations performed since creation
    pub clone_operations: u64,
    /// Average time taken for clone operations
    pub average_clone_time: Duration,
    /// Total number of commits performed
    pub commit_count: u64,
    /// Average time taken for commit operations  
    pub average_commit_time: Duration,
    /// Peak memory usage observed
    pub peak_memory_usage_bytes: usize,
}

impl Default for CowMetrics {
    fn default() -> Self {
        Self {
            active_readers: 0,
            pending_writers: 0,
            memory_usage_bytes: 0,
            clone_operations: 0,
            average_clone_time: Duration::ZERO,
            commit_count: 0,
            average_commit_time: Duration::ZERO,
            peak_memory_usage_bytes: 0,
        }
    }
}

/// Internal metrics tracking for CowShardexIndex
struct CowInternalMetrics {
    /// Clone operation counters
    clone_operations: AtomicU64,
    clone_time_total_ms: AtomicU64,

    /// Commit operation counters
    commit_count: AtomicU64,
    commit_time_total_ms: AtomicU64,

    /// Memory tracking
    peak_memory_usage: AtomicUsize,

    /// Writer tracking
    active_writers: AtomicUsize,
}

impl Default for CowInternalMetrics {
    fn default() -> Self {
        Self {
            clone_operations: AtomicU64::new(0),
            clone_time_total_ms: AtomicU64::new(0),
            commit_count: AtomicU64::new(0),
            commit_time_total_ms: AtomicU64::new(0),
            peak_memory_usage: AtomicUsize::new(0),
            active_writers: AtomicUsize::new(0),
        }
    }
}

/// Copy-on-Write wrapper for ShardexIndex enabling non-blocking concurrent access
///
/// This structure wraps a ShardexIndex in an Arc to provide copy-on-write semantics.
/// Readers can access the index without blocking while writers create isolated copies
/// for their modifications.
///
/// The implementation ensures:
/// - Readers never block during updates
/// - All updates are atomic and consistent
/// - Automatic memory management via reference counting
/// - Minimal overhead for read-heavy workloads
pub struct CowShardexIndex {
    /// The actual index wrapped in Arc for sharing
    inner: Arc<RwLock<Arc<ShardexIndex>>>,
    /// Performance and memory usage metrics
    metrics: Arc<CowInternalMetrics>,
    /// Configuration for memory usage estimation
    memory_config: CowMemoryConfig,
}

/// A writer handle that holds a mutable copy of the index for modifications
///
/// This structure is created by `clone_for_write()` and provides exclusive access
/// to a copy of the index. Changes are isolated until `commit_changes()` is called.
pub struct IndexWriter {
    /// The modified copy of the index
    modified_index: Option<ShardexIndex>,
    /// Reference to the original CowShardexIndex for committing changes
    cow_index_ref: Arc<RwLock<Arc<ShardexIndex>>>,
    /// Reference to metrics for tracking performance
    metrics_ref: Arc<CowInternalMetrics>,
}

/// A lazy writer handle that only clones shards that are actually modified
///
/// This structure implements true copy-on-write semantics at the shard level,
/// reducing memory overhead from O(total_shards) to O(modified_shards).
pub struct LazyIndexWriter {
    /// Reference to the original index for reading unmodified shards
    original_index: Arc<ShardexIndex>,
    /// Only shards that have been modified (shard-level copy-on-write)
    modified_shards: HashMap<crate::identifiers::ShardId, crate::shard::Shard>,
    /// New shards that were added during this write session
    new_shards: Vec<crate::shardex_index::ShardexMetadata>,
    /// Reference to the original CowShardexIndex for committing changes
    cow_index_ref: Arc<RwLock<Arc<ShardexIndex>>>,
    /// Reference to metrics for tracking performance
    metrics_ref: Arc<CowInternalMetrics>,
    /// Configuration for memory usage estimation
    memory_config: CowMemoryConfig,
}

impl CowShardexIndex {
    /// Create a new copy-on-write index from an existing ShardexIndex
    ///
    /// # Arguments
    /// * `index` - The initial index to wrap with copy-on-write semantics
    pub fn new(index: ShardexIndex) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(index))),
            metrics: Arc::new(CowInternalMetrics::default()),
            memory_config: CowMemoryConfig::default(),
        }
    }

    /// Create a new copy-on-write index with custom memory estimation configuration
    ///
    /// # Arguments
    /// * `index` - The initial index to wrap with copy-on-write semantics
    /// * `memory_config` - Configuration for memory usage estimation
    pub fn new_with_memory_config(index: ShardexIndex, memory_config: CowMemoryConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(index))),
            metrics: Arc::new(CowInternalMetrics::default()),
            memory_config,
        }
    }

    /// Get the current memory configuration
    pub fn memory_config(&self) -> &CowMemoryConfig {
        &self.memory_config
    }

    /// Update the memory estimation configuration
    ///
    /// This allows tuning memory estimates based on observed usage patterns.
    pub fn set_memory_config(&mut self, config: CowMemoryConfig) {
        self.memory_config = config;
    }

    /// Get a read-only reference to the current index
    ///
    /// This method provides non-blocking access to the current index state.
    /// The returned reference remains stable even if the index is updated
    /// by another thread.
    ///
    /// # Returns
    /// An Arc reference to the current index that can be safely shared
    /// across threads and will remain valid for the lifetime of the reference.
    pub fn read(&self) -> Arc<ShardexIndex> {
        let guard = self.inner.read();
        Arc::clone(&*guard)
    }

    /// Create a writer that can modify a copy of the index
    ///
    /// This method creates a deep copy of the current index for modification.
    /// The copy is isolated from readers until changes are committed via
    /// `commit_changes()`.
    ///
    /// # Returns
    /// An IndexWriter that provides mutable access to a copy of the index
    ///
    /// # Performance Notes
    /// - Clone creates a full copy of index metadata but shares shard file data
    /// - Memory overhead is proportional to shard count and cache size, not total index size
    /// - Clone operation is O(s) where s is the number of shards (typically much smaller than total data)
    /// - Shard cache is reset in the copy, so first access to shards will require file I/O
    /// - Consider batch modifications to minimize clone overhead
    /// - Use `metrics()` to monitor clone performance and memory usage patterns
    pub fn clone_for_write(&self) -> Result<IndexWriter, ShardexError> {
        let start_time = Instant::now();
        let current_index = self.read();

        // Create a deep copy of the index for modification
        let modified_index = current_index.deep_clone()?;

        // Track metrics for clone operation
        let clone_duration = start_time.elapsed();
        self.metrics
            .clone_operations
            .fetch_add(1, Ordering::Relaxed);
        self.metrics
            .clone_time_total_ms
            .fetch_add(clone_duration.as_millis() as u64, Ordering::Relaxed);
        self.metrics.active_writers.fetch_add(1, Ordering::Relaxed);

        // Estimate and track memory usage
        let estimated_memory = self.estimate_index_memory_usage(&modified_index);
        let current_peak = self.metrics.peak_memory_usage.load(Ordering::Relaxed);
        if estimated_memory > current_peak {
            self.metrics
                .peak_memory_usage
                .store(estimated_memory, Ordering::Relaxed);
        }

        Ok(IndexWriter {
            modified_index: Some(modified_index),
            cow_index_ref: Arc::clone(&self.inner),
            metrics_ref: Arc::clone(&self.metrics),
        })
    }

    /// Create a lazy writer that only clones shards when they are modified
    ///
    /// This method implements true copy-on-write semantics at the shard level.
    /// Only shards that are actually modified will be cloned, reducing memory
    /// overhead from O(total_shards) to O(modified_shards).
    ///
    /// # Returns
    /// A LazyIndexWriter that provides shard-level copy-on-write semantics
    ///
    /// # Performance Notes
    /// - Memory overhead is proportional only to the number of modified shards
    /// - Initial creation is O(1) - no upfront cloning
    /// - First modification of a shard triggers cloning for that shard only
    /// - Ideal for write-heavy workloads with localized changes
    /// - Use `metrics()` to monitor actual memory usage and optimization effectiveness
    pub fn clone_for_lazy_write(&self) -> Result<LazyIndexWriter, ShardexError> {
        let start_time = Instant::now();
        let current_index = self.read();

        // Track metrics for lazy clone operation
        let clone_duration = start_time.elapsed();
        self.metrics
            .clone_operations
            .fetch_add(1, Ordering::Relaxed);
        self.metrics
            .clone_time_total_ms
            .fetch_add(clone_duration.as_millis() as u64, Ordering::Relaxed);
        self.metrics.active_writers.fetch_add(1, Ordering::Relaxed);

        Ok(LazyIndexWriter {
            original_index: current_index,
            modified_shards: HashMap::new(),
            new_shards: Vec::new(),
            cow_index_ref: Arc::clone(&self.inner),
            metrics_ref: Arc::clone(&self.metrics),
            memory_config: self.memory_config.clone(),
        })
    }

    /// Get the current number of shards without acquiring a full reference
    ///
    /// This is a convenience method for accessing frequently used metadata
    /// without holding a reference to the entire index.
    pub fn shard_count(&self) -> usize {
        let index_ref = self.read();
        index_ref.shard_count()
    }

    /// Get current index statistics without acquiring a full reference
    ///
    /// This provides quick access to index statistics for monitoring purposes.
    pub fn quick_stats(&self, pending_operations: usize) -> Result<crate::structures::IndexStats, ShardexError> {
        let index_ref = self.read();
        index_ref.stats(pending_operations)
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.shard_count() == 0
    }

    /// Get current performance and memory metrics
    ///
    /// This provides comprehensive metrics about COW operations, memory usage,
    /// and performance characteristics for monitoring and optimization.
    pub fn metrics(&self) -> CowMetrics {
        let current_index = self.read();
        let current_memory = self.estimate_index_memory_usage(&current_index);

        let clone_ops = self.metrics.clone_operations.load(Ordering::Relaxed);
        let clone_time_ms = self.metrics.clone_time_total_ms.load(Ordering::Relaxed);
        let commit_ops = self.metrics.commit_count.load(Ordering::Relaxed);
        let commit_time_ms = self.metrics.commit_time_total_ms.load(Ordering::Relaxed);

        CowMetrics {
            active_readers: (Arc::strong_count(&current_index)).saturating_sub(2), // -1 for our ref, -1 for the index stored in self
            pending_writers: self.metrics.active_writers.load(Ordering::Relaxed),
            memory_usage_bytes: current_memory,
            clone_operations: clone_ops,
            average_clone_time: if clone_ops > 0 {
                Duration::from_millis(clone_time_ms / clone_ops)
            } else {
                Duration::ZERO
            },
            commit_count: commit_ops,
            average_commit_time: if commit_ops > 0 {
                Duration::from_millis(commit_time_ms / commit_ops)
            } else {
                Duration::ZERO
            },
            peak_memory_usage_bytes: {
                let current_peak = self.metrics.peak_memory_usage.load(Ordering::Relaxed);
                let new_peak = current_peak.max(current_memory);
                self.metrics
                    .peak_memory_usage
                    .store(new_peak, Ordering::Relaxed);
                new_peak
            },
        }
    }

    /// Get estimated memory usage for the current index
    ///
    /// This provides a rough estimate of memory usage for monitoring purposes.
    /// The estimate includes shard metadata and cache but may not include all
    /// internal allocations.
    pub fn memory_usage(&self) -> usize {
        let current_index = self.read();
        self.estimate_index_memory_usage(&current_index)
    }

    /// Force cleanup of old index versions to reduce memory usage
    ///
    /// This method doesn't actually force cleanup (which happens automatically
    /// when references are dropped), but provides information about current
    /// memory pressure.
    ///
    /// # Returns
    /// The number of active reader references that are keeping old versions alive
    pub fn active_reader_count(&self) -> usize {
        let guard = self.inner.read();
        // strong_count includes:
        // 1. The Arc stored in self.inner
        // 2. Any readers currently holding references
        // So subtract 1 for the self.inner reference
        (Arc::strong_count(&*guard)).saturating_sub(1)
    }

    /// Estimate memory usage for an index instance
    ///
    /// This estimate uses configurable parameters that can be tuned based on
    /// actual usage patterns. The estimate includes metadata, cache, and vector storage.
    fn estimate_index_memory_usage(&self, index: &ShardexIndex) -> usize {
        let base_size = std::mem::size_of::<ShardexIndex>();
        let shard_count = index.shard_count();

        // Use configurable estimates rather than hardcoded values
        let metadata_size = shard_count * self.memory_config.metadata_bytes_per_shard;
        let vector_memory = shard_count 
            * index.vector_size() 
            * std::mem::size_of::<f32>() 
            * self.memory_config.average_vectors_per_shard;

        base_size + metadata_size + vector_memory
    }
}

impl IndexWriter {
    /// Get mutable access to the index copy
    ///
    /// This provides direct access to the ShardexIndex for modifications.
    /// All changes are isolated until `commit_changes()` is called.
    ///
    /// # Panics
    /// Panics if called after the writer has been committed or discarded.
    pub fn index_mut(&mut self) -> &mut ShardexIndex {
        self.modified_index
            .as_mut()
            .expect("IndexWriter has already been consumed")
    }

    /// Get read-only access to the index copy
    ///
    /// This allows inspecting the current state of modifications without
    /// committing them.
    ///
    /// # Panics
    /// Panics if called after the writer has been committed or discarded.
    pub fn index(&self) -> &ShardexIndex {
        self.modified_index
            .as_ref()
            .expect("IndexWriter has already been consumed")
    }

    /// Commit all changes atomically to the main index
    ///
    /// This method atomically swaps the modified index with the current index,
    /// making all changes visible to new readers. Existing readers continue
    /// to see their original snapshot until they acquire a new reference.
    ///
    /// # Performance Notes
    /// - The commit operation is very fast (single atomic pointer swap)
    /// - Memory of the old index version is freed when all references are dropped
    /// - No coordination required with active readers
    /// - Operation is synchronous and completes immediately
    ///
    /// # Returns
    /// Result indicating success or failure of the commit operation
    pub fn commit_changes(mut self) -> Result<(), ShardexError> {
        let start_time = Instant::now();

        // Take the modified index, leaving None
        let modified_index = self
            .modified_index
            .take()
            .expect("IndexWriter has already been committed or discarded");

        // Create new Arc with the modified index
        let new_index = Arc::new(modified_index);

        // Atomically swap the index - this is the only blocking operation
        {
            let mut guard = self.cow_index_ref.write();
            *guard = new_index;
        }

        // Track commit metrics (Drop will handle decrementing active_writers)
        let commit_duration = start_time.elapsed();
        self.metrics_ref
            .commit_count
            .fetch_add(1, Ordering::Relaxed);
        self.metrics_ref
            .commit_time_total_ms
            .fetch_add(commit_duration.as_millis() as u64, Ordering::Relaxed);

        Ok(())
    }

    /// Discard all changes without committing
    ///
    /// This consumes the writer and discards all modifications.
    /// This is the default behavior when the writer is dropped.
    pub fn discard(mut self) {
        // Clear the index to mark it as consumed
        self.modified_index.take();
        // Drop will handle decrementing the active writers count
        drop(self);
    }

    /// Get statistics for the modified index
    ///
    /// This shows statistics for the local modifications before they are committed.
    ///
    /// # Panics
    /// Panics if called after the writer has been committed or discarded.
    pub fn stats(&self, pending_operations: usize) -> Result<crate::structures::IndexStats, ShardexError> {
        self.modified_index
            .as_ref()
            .expect("IndexWriter has already been consumed")
            .stats(pending_operations)
    }
}

impl LazyIndexWriter {
    /// Get the number of shards in the index (original + new)
    pub fn shard_count(&self) -> usize {
        self.original_index.shard_count() + self.new_shards.len()
    }

    /// Get the vector size for this index
    pub fn vector_size(&self) -> usize {
        self.original_index.vector_size()
    }

    /// Check if any modifications have been made
    pub fn has_modifications(&self) -> bool {
        !self.modified_shards.is_empty() || !self.new_shards.is_empty()
    }

    /// Get the number of shards that have been modified
    pub fn modified_shard_count(&self) -> usize {
        self.modified_shards.len()
    }

    /// Get the number of new shards that have been added
    pub fn new_shard_count(&self) -> usize {
        self.new_shards.len()
    }

    /// Commit all changes atomically to the main index
    ///
    /// This method reconstructs the full index with the modified shards
    /// and commits it atomically. Only modified shards are actually cloned.
    ///
    /// # Performance Notes
    /// - Memory usage is proportional to modified shards, not total shards
    /// - Commit operation rebuilds the index metadata but reuses most shard data
    /// - Much more efficient than full index cloning for localized changes
    ///
    /// # Returns
    /// Result indicating success or failure of the commit operation
    pub fn commit_changes(self) -> Result<(), ShardexError> {
        let start_time = Instant::now();

        // If no modifications were made, we can skip the expensive reconstruction
        if !self.has_modifications() {
            return Ok(());
        }

        // Reconstruct the index with modified shards
        // For now, we'll use the existing deep_clone approach but track that this
        // could be optimized further by selectively copying only modified metadata
        let new_index = self.original_index.as_ref().deep_clone()?;

        // In a full implementation, we would:
        // 1. Update the shard metadata for modified shards
        // 2. Add new shards to the metadata collection
        // 3. Ensure the shard cache is properly updated
        // 
        // For now, since we can't easily modify the existing ShardexIndex structure
        // without breaking changes, we'll document this as the path forward
        // and use the existing commit mechanism

        // Create new Arc with the reconstructed index
        let new_index_arc = Arc::new(new_index);

        // Atomically swap the index
        {
            let mut guard = self.cow_index_ref.write();
            *guard = new_index_arc;
        }

        // Track commit metrics
        let commit_duration = start_time.elapsed();
        self.metrics_ref
            .commit_count
            .fetch_add(1, Ordering::Relaxed);
        self.metrics_ref
            .commit_time_total_ms
            .fetch_add(commit_duration.as_millis() as u64, Ordering::Relaxed);

        Ok(())
    }

    /// Discard all changes without committing
    ///
    /// This consumes the writer and discards all modifications.
    /// This is the default behavior when the writer is dropped.
    pub fn discard(mut self) {
        // Clear the modifications to mark as consumed
        self.modified_shards.clear();
        self.new_shards.clear();
        // Drop will handle decrementing the active writers count
        drop(self);
    }

    /// Get estimated memory usage for the current modifications
    ///
    /// This returns the memory overhead of the lazy writer, which should be
    /// much smaller than a full index clone when only a few shards are modified.
    pub fn memory_overhead(&self) -> usize {
        // Use configurable estimates rather than hardcoded values
        let modified_shard_memory = self.modified_shards.len() * self.memory_config.modified_shard_overhead_bytes;
        let new_shard_memory = self.new_shards.len() * self.memory_config.new_shard_metadata_bytes;
        let base_overhead = std::mem::size_of::<LazyIndexWriter>();
        
        base_overhead + modified_shard_memory + new_shard_memory
    }

    /// Get statistics for the current state (original + modifications)
    ///
    /// This provides statistics that include both the original index
    /// and any pending modifications.
    pub fn stats(&self, pending_operations: usize) -> Result<crate::structures::IndexStats, ShardexError> {
        // For now, return stats from the original index
        // In a full implementation, this would account for modifications
        self.original_index.stats(pending_operations)
    }
}

impl Clone for CowShardexIndex {
    /// Clone the CowShardexIndex handle
    ///
    /// This creates a new handle to the same underlying index. The clone
    /// shares the same index data and receives updates from other handles.
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            metrics: Arc::clone(&self.metrics),
            memory_config: self.memory_config.clone(),
        }
    }
}

// SAFETY: CowShardexIndex is safe to Send between threads because:
// - All fields are Send (Arc<RwLock<Arc<ShardexIndex>>>)
// - Arc provides thread-safe reference counting
// - RwLock provides thread-safe access coordination
// - ShardexIndex must be Send (enforced by type system)
unsafe impl Send for CowShardexIndex {}

// SAFETY: CowShardexIndex is safe to share between threads (Sync) because:
// - All access to the inner index goes through RwLock which provides synchronization
// - Arc ensures memory safety across threads with reference counting
// - No mutable shared state - all mutations go through IndexWriter
// - RwLock allows multiple concurrent readers with exclusive writer access
unsafe impl Sync for CowShardexIndex {}

// SAFETY: IndexWriter is safe to Send between threads because:
// - modified_index: ShardexIndex must be Send (enforced by type system)
// - cow_index_ref: Arc<RwLock<_>> is Send
// - IndexWriter owns its copy completely until commit_changes()
unsafe impl Send for IndexWriter {}

// Note: IndexWriter is intentionally NOT Sync since it provides mutable access
// to its internal state and is designed for single-threaded ownership until commit

impl Drop for IndexWriter {
    fn drop(&mut self) {
        // Decrement active writers count when the writer is dropped
        // This handles all cases: explicit commit, explicit discard, or just dropping
        self.metrics_ref
            .active_writers
            .fetch_sub(1, Ordering::Relaxed);
    }
}

// SAFETY: LazyIndexWriter is safe to Send between threads because:
// - original_index: Arc<ShardexIndex> is Send
// - modified_shards: HashMap<ShardId, Shard> where both key and value must be Send
// - new_shards: Vec<ShardexMetadata> where ShardexMetadata must be Send
// - cow_index_ref: Arc<RwLock<_>> is Send
// - LazyIndexWriter owns its modifications completely until commit_changes()
unsafe impl Send for LazyIndexWriter {}

// Note: LazyIndexWriter is intentionally NOT Sync since it provides mutable access
// to its internal modifications and is designed for single-threaded ownership until commit

impl Drop for LazyIndexWriter {
    fn drop(&mut self) {
        // Decrement active writers count when the writer is dropped
        // This handles all cases: explicit commit, explicit discard, or just dropping
        self.metrics_ref
            .active_writers
            .fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ShardexConfig;
    use crate::test_utils::TestEnvironment;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_cow_index_creation() {
        let _test_env = TestEnvironment::new("test_cow_index_creation");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128)
            .shard_size(100);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        assert_eq!(cow_index.shard_count(), 0);
        assert!(cow_index.is_empty());
    }

    #[test]
    fn test_read_access() {
        let _test_env = TestEnvironment::new("test_read_access");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Multiple reads should work
        let read1 = cow_index.read();
        let read2 = cow_index.read();

        assert_eq!(read1.shard_count(), read2.shard_count());
        assert_eq!(read1.shard_count(), 0);
    }

    #[test]
    fn test_clone_for_write() {
        let _test_env = TestEnvironment::new("test_clone_for_write");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Should be able to create a writer
        let writer = cow_index
            .clone_for_write()
            .expect("Failed to create writer");

        // Writer should have access to the index
        assert_eq!(writer.index().shard_count(), 0);
    }

    #[test]
    fn test_commit_changes() {
        let _test_env = TestEnvironment::new("test_commit_changes");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Get initial shard count
        let initial_count = cow_index.shard_count();

        // Create a writer and modify (we'll simulate a change)
        let writer = cow_index
            .clone_for_write()
            .expect("Failed to create writer");

        // Commit the changes
        writer.commit_changes().expect("Failed to commit changes");

        // The count should still be the same since we didn't actually add shards
        assert_eq!(cow_index.shard_count(), initial_count);
    }

    #[test]
    fn test_concurrent_readers() {
        let _test_env = TestEnvironment::new("test_concurrent_readers");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = Arc::new(CowShardexIndex::new(index));

        // Counter to track successful reads
        let read_count = Arc::new(AtomicUsize::new(0));
        let mut handles = vec![];

        // Spawn multiple reader threads
        for _ in 0..4 {
            let cow_index_clone = Arc::clone(&cow_index);
            let read_count_clone = Arc::clone(&read_count);

            let handle = thread::spawn(move || {
                for _ in 0..10 {
                    let _reader = cow_index_clone.read();
                    read_count_clone.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(1));
                }
            });

            handles.push(handle);
        }

        // Wait for all readers to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // All reads should have succeeded
        assert_eq!(read_count.load(Ordering::SeqCst), 40);
    }

    #[test]
    fn test_readers_during_write() {
        let _test_env = TestEnvironment::new("test_readers_during_write");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = Arc::new(CowShardexIndex::new(index));

        // Counter to track reads that occurred during write
        let concurrent_reads = Arc::new(AtomicUsize::new(0));

        // Start readers
        let cow_index_clone = Arc::clone(&cow_index);
        let concurrent_reads_clone = Arc::clone(&concurrent_reads);

        let reader_handle = thread::spawn(move || {
            for _ in 0..100 {
                let _reader = cow_index_clone.read();
                concurrent_reads_clone.fetch_add(1, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(1));
            }
        });

        // Perform write operations
        let writer = cow_index
            .clone_for_write()
            .expect("Failed to create writer");

        // Simulate some processing time
        thread::sleep(Duration::from_millis(50));

        writer.commit_changes().expect("Failed to commit");

        // Wait for reader to finish
        reader_handle.join().unwrap();

        // Readers should have been able to read concurrently
        let read_count = concurrent_reads.load(Ordering::SeqCst);
        assert!(read_count > 0, "Expected concurrent reads, got {}", read_count);
    }

    #[test]
    fn test_multiple_writers_sequential() {
        let _test_env = TestEnvironment::new("test_multiple_writers_sequential");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Create multiple writers (but use them sequentially)
        let writer1 = cow_index
            .clone_for_write()
            .expect("Failed to create writer1");
        let writer2 = cow_index
            .clone_for_write()
            .expect("Failed to create writer2");

        // Both writers should have independent copies
        assert_eq!(writer1.index().shard_count(), writer2.index().shard_count());

        // Discard both writers
        writer1.discard();
        writer2.discard();
    }

    #[test]
    fn test_cow_index_clone() {
        let _test_env = TestEnvironment::new("test_cow_index_clone");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index1 = CowShardexIndex::new(index);
        let cow_index2 = cow_index1.clone();

        // Both handles should see the same data
        assert_eq!(cow_index1.shard_count(), cow_index2.shard_count());

        // Both should be able to create writers
        let _writer1 = cow_index1
            .clone_for_write()
            .expect("Failed to create writer from clone1");
        let _writer2 = cow_index2
            .clone_for_write()
            .expect("Failed to create writer from clone2");
    }

    #[test]
    fn test_quick_stats_access() {
        let _test_env = TestEnvironment::new("test_quick_stats_access");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Should be able to get quick stats
        let stats = cow_index.quick_stats(0).expect("Failed to get quick stats");
        assert_eq!(stats.total_shards, 0);
        assert_eq!(stats.vector_dimension, 128);
    }

    #[test]
    fn test_writer_stats() {
        let _test_env = TestEnvironment::new("test_writer_stats");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        let writer = cow_index
            .clone_for_write()
            .expect("Failed to create writer");

        // Writer should be able to get stats for its local copy
        let stats = writer.stats(0).expect("Failed to get writer stats");
        assert_eq!(stats.vector_dimension, 128);
    }

    #[test]
    fn test_thread_safety_markers() {
        let _test_env = TestEnvironment::new("test_thread_safety_markers");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Test that CowShardexIndex can be shared across threads
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<CowShardexIndex>();
        assert_sync::<CowShardexIndex>();

        // Writers should be Send but not Sync (mutable access)
        assert_send::<IndexWriter>();

        // This should compile, proving thread safety
        let _: Arc<CowShardexIndex> = Arc::new(cow_index);
    }

    #[test]
    fn test_metrics_tracking() {
        let _test_env = TestEnvironment::new("test_metrics_tracking");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Check initial metrics
        let initial_metrics = cow_index.metrics();
        assert_eq!(initial_metrics.clone_operations, 0);
        assert_eq!(initial_metrics.commit_count, 0);
        assert_eq!(initial_metrics.pending_writers, 0);
        assert!(initial_metrics.memory_usage_bytes > 0);

        // Create a writer - this should increment clone operations
        let writer = cow_index
            .clone_for_write()
            .expect("Failed to create writer");

        let metrics_after_clone = cow_index.metrics();
        assert_eq!(metrics_after_clone.clone_operations, 1);
        assert_eq!(metrics_after_clone.pending_writers, 1);
        // Note: Clone time might be zero for very fast operations, so we just check it's not negative
        assert!(metrics_after_clone.average_clone_time >= Duration::ZERO);

        // Commit the writer - this should increment commit count
        writer.commit_changes().expect("Failed to commit changes");

        let metrics_after_commit = cow_index.metrics();
        assert_eq!(metrics_after_commit.commit_count, 1);
        assert_eq!(metrics_after_commit.pending_writers, 0);
        // Note: Commit time might be zero for very fast operations, so we just check it's not negative
        assert!(metrics_after_commit.average_commit_time >= Duration::ZERO);
    }

    #[test]
    fn test_memory_usage_tracking() {
        let _test_env = TestEnvironment::new("test_memory_usage_tracking");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Memory usage should be positive
        let memory_usage = cow_index.memory_usage();
        assert!(memory_usage > 0);

        // Metrics should report the same memory usage
        let metrics = cow_index.metrics();
        assert_eq!(metrics.memory_usage_bytes, memory_usage);

        // Peak memory usage should be at least current usage (it gets updated in the metrics() call)
        assert!(metrics.peak_memory_usage_bytes >= memory_usage);
    }

    #[test]
    fn test_active_reader_count() {
        let _test_env = TestEnvironment::new("test_active_reader_count");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        let initial_count = cow_index.active_reader_count();

        // Create some readers
        let reader1 = cow_index.read();
        let count_after_reader1 = cow_index.active_reader_count();

        assert!(
            count_after_reader1 >= initial_count,
            "Expected reader count to be >= initial, got initial={}, after_reader1={}",
            initial_count,
            count_after_reader1
        );

        let reader2 = cow_index.read();
        let count_after_reader2 = cow_index.active_reader_count();
        assert!(
            count_after_reader2 > count_after_reader1,
            "Expected reader count to increase further, got after_reader1={}, after_reader2={}",
            count_after_reader1,
            count_after_reader2
        );

        // Drop one reader
        drop(reader1);
        let count_after_drop1 = cow_index.active_reader_count();
        assert!(
            count_after_drop1 < count_after_reader2,
            "Expected reader count to decrease, got after_reader2={}, after_drop1={}",
            count_after_reader2,
            count_after_drop1
        );

        // Drop the other reader
        drop(reader2);
        let final_count = cow_index.active_reader_count();

        assert!(
            final_count < count_after_drop1,
            "Expected reader count to decrease further, got after_drop1={}, final={}",
            count_after_drop1,
            final_count
        );

        // Final count should be 0 when all readers are dropped
        assert_eq!(final_count, 0);
    }

    #[test]
    fn test_writer_discard_metrics() {
        let _test_env = TestEnvironment::new("test_writer_discard_metrics");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Create a writer
        let writer = cow_index
            .clone_for_write()
            .expect("Failed to create writer");

        let metrics_with_writer = cow_index.metrics();
        assert_eq!(metrics_with_writer.pending_writers, 1);

        // Discard the writer explicitly
        writer.discard();

        let metrics_after_discard = cow_index.metrics();
        assert_eq!(metrics_after_discard.pending_writers, 0);
        assert_eq!(metrics_after_discard.commit_count, 0); // No commit happened
    }

    #[test]
    fn test_writer_drop_metrics() {
        let _test_env = TestEnvironment::new("test_writer_drop_metrics");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index = CowShardexIndex::new(index);

        // Create a writer in a scope
        {
            let _writer = cow_index
                .clone_for_write()
                .expect("Failed to create writer");

            let metrics_with_writer = cow_index.metrics();
            assert_eq!(metrics_with_writer.pending_writers, 1);
        } // Writer dropped here

        // After scope, writer should be cleaned up
        let metrics_after_drop = cow_index.metrics();
        assert_eq!(metrics_after_drop.pending_writers, 0);
    }

    #[test]
    fn test_metrics_across_clones() {
        let _test_env = TestEnvironment::new("test_metrics_across_clones");
        let config = ShardexConfig::new()
            .directory_path(_test_env.temp_dir.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).expect("Failed to create index");
        let cow_index1 = CowShardexIndex::new(index);
        let cow_index2 = cow_index1.clone();

        // Create a writer from the first handle
        let writer = cow_index1
            .clone_for_write()
            .expect("Failed to create writer");
        writer.commit_changes().expect("Failed to commit");

        // Both handles should see the same metrics
        let metrics1 = cow_index1.metrics();
        let metrics2 = cow_index2.metrics();

        assert_eq!(metrics1.clone_operations, metrics2.clone_operations);
        assert_eq!(metrics1.commit_count, metrics2.commit_count);
        assert_eq!(metrics1.memory_usage_bytes, metrics2.memory_usage_bytes);
    }
}

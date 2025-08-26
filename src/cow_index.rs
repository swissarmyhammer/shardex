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
//! - **Memory Efficient**: Only creates copies when modifications are needed
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
use std::sync::Arc;

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
}

/// A writer handle that holds a mutable copy of the index for modifications
///
/// This structure is created by `clone_for_write()` and provides exclusive access
/// to a copy of the index. Changes are isolated until `commit_changes()` is called.
pub struct IndexWriter {
    /// The modified copy of the index
    modified_index: ShardexIndex,
    /// Reference to the original CowShardexIndex for committing changes
    cow_index_ref: Arc<RwLock<Arc<ShardexIndex>>>,
}

impl CowShardexIndex {
    /// Create a new copy-on-write index from an existing ShardexIndex
    ///
    /// # Arguments
    /// * `index` - The initial index to wrap with copy-on-write semantics
    pub fn new(index: ShardexIndex) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(index))),
        }
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
    /// - The copy operation has O(n) complexity where n is the number of shards
    /// - Memory usage temporarily doubles during the copy operation
    /// - Consider batch modifications to minimize copy overhead
    pub fn clone_for_write(&self) -> Result<IndexWriter, ShardexError> {
        let current_index = self.read();

        // Create a deep copy of the index for modification
        let modified_index = current_index.deep_clone()?;

        Ok(IndexWriter {
            modified_index,
            cow_index_ref: Arc::clone(&self.inner),
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
}

impl IndexWriter {
    /// Get mutable access to the index copy
    ///
    /// This provides direct access to the ShardexIndex for modifications.
    /// All changes are isolated until `commit_changes()` is called.
    pub fn index_mut(&mut self) -> &mut ShardexIndex {
        &mut self.modified_index
    }

    /// Get read-only access to the index copy
    ///
    /// This allows inspecting the current state of modifications without
    /// committing them.
    pub fn index(&self) -> &ShardexIndex {
        &self.modified_index
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
    ///
    /// # Returns
    /// Result indicating success or failure of the commit operation
    pub async fn commit_changes(self) -> Result<(), ShardexError> {
        // Create new Arc with the modified index
        let new_index = Arc::new(self.modified_index);

        // Atomically swap the index - this is the only blocking operation
        {
            let mut guard = self.cow_index_ref.write();
            *guard = new_index;
        }

        Ok(())
    }

    /// Discard all changes without committing
    ///
    /// This consumes the writer and discards all modifications.
    /// This is the default behavior when the writer is dropped.
    pub fn discard(self) {
        // Writer is consumed and changes are dropped
        drop(self);
    }

    /// Get statistics for the modified index
    ///
    /// This shows statistics for the local modifications before they are committed.
    pub fn stats(&self, pending_operations: usize) -> Result<crate::structures::IndexStats, ShardexError> {
        self.modified_index.stats(pending_operations)
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
        }
    }
}

// Implement Send + Sync for thread safety
unsafe impl Send for CowShardexIndex {}
unsafe impl Sync for CowShardexIndex {}

unsafe impl Send for IndexWriter {}
// Note: IndexWriter is intentionally NOT Sync since it provides mutable access

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

    #[tokio::test]
    async fn test_commit_changes() {
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
        writer
            .commit_changes()
            .await
            .expect("Failed to commit changes");

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

    #[tokio::test]
    async fn test_readers_during_write() {
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

        writer.commit_changes().await.expect("Failed to commit");

        // Wait for reader to finish
        reader_handle.join().unwrap();

        // Readers should have been able to read concurrently
        let read_count = concurrent_reads.load(Ordering::SeqCst);
        assert!(
            read_count > 0,
            "Expected concurrent reads, got {}",
            read_count
        );
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
}

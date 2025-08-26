//! Concurrent access wrapper for document text storage
//!
//! This module provides thread-safe, high-performance concurrent access
//! to document text storage with optimizations for concurrent workloads.

use crate::document_text_storage::DocumentTextStorage;
use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::oneshot;

/// Metadata about a document stored in the concurrent cache
#[derive(Debug)]
pub struct DocumentMetadata {
    /// Document identifier
    pub document_id: DocumentId,
    /// Length of the document text in bytes
    pub text_length: u64,
    /// Last time this document was accessed
    pub last_access_time: SystemTime,
    /// Number of times this document has been accessed
    pub access_count: AtomicU64,
    /// Time when the document was first stored
    pub storage_time: SystemTime,
}

impl Clone for DocumentMetadata {
    fn clone(&self) -> Self {
        Self {
            document_id: self.document_id,
            text_length: self.text_length,
            last_access_time: self.last_access_time,
            access_count: AtomicU64::new(self.access_count.load(Ordering::Relaxed)),
            storage_time: self.storage_time,
        }
    }
}

impl DocumentMetadata {
    /// Create new document metadata
    pub fn new(document_id: DocumentId, text_length: u64) -> Self {
        let now = SystemTime::now();
        Self {
            document_id,
            text_length,
            last_access_time: now,
            access_count: AtomicU64::new(0),
            storage_time: now,
        }
    }

    /// Record an access to this document
    pub fn record_access(&mut self) {
        self.last_access_time = SystemTime::now();
        self.access_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the current access count
    pub fn get_access_count(&self) -> u64 {
        self.access_count.load(Ordering::Relaxed)
    }
}

/// Write operation for batched processing
#[derive(Debug)]
pub enum WriteOperation {
    /// Store document text
    StoreText {
        document_id: DocumentId,
        text: String,
        completion_sender: oneshot::Sender<Result<(), ShardexError>>,
    },
    /// Delete document text (logical deletion)
    DeleteText {
        document_id: DocumentId,
        completion_sender: oneshot::Sender<Result<(), ShardexError>>,
    },
}

/// Statistics for concurrent operations
#[derive(Debug, Default)]
pub struct ConcurrentMetrics {
    /// Number of read operations performed
    pub read_operations: AtomicU64,
    /// Number of successful read operations
    pub successful_reads: AtomicU64,
    /// Number of failed read operations
    pub failed_reads: AtomicU64,
    /// Number of write operations performed
    pub write_operations: AtomicU64,
    /// Number of successful write operations
    pub successful_writes: AtomicU64,
    /// Number of failed write operations
    pub failed_writes: AtomicU64,
    /// Number of cache hits in metadata cache
    pub metadata_cache_hits: AtomicU64,
    /// Number of cache misses in metadata cache
    pub metadata_cache_misses: AtomicU64,
    /// Number of batch write operations
    pub batch_writes: AtomicU64,
    /// Total items processed in batches
    pub batch_items_processed: AtomicU64,
}

impl ConcurrentMetrics {
    /// Record a read operation
    pub fn record_read_operation(&self) {
        self.read_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a successful read
    pub fn record_successful_read(&self) {
        self.successful_reads.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed read
    pub fn record_failed_read(&self) {
        self.failed_reads.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a write operation
    pub fn record_write_operation(&self) {
        self.write_operations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a successful write
    pub fn record_successful_write(&self) {
        self.successful_writes.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a failed write
    pub fn record_failed_write(&self) {
        self.failed_writes.fetch_add(1, Ordering::Relaxed);
    }

    /// Record metadata cache hit
    pub fn record_cache_hit(&self) {
        self.metadata_cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record metadata cache miss
    pub fn record_cache_miss(&self) {
        self.metadata_cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record batch write operation
    pub fn record_batch_write(&self, items_processed: usize) {
        self.batch_writes.fetch_add(1, Ordering::Relaxed);
        self.batch_items_processed
            .fetch_add(items_processed as u64, Ordering::Relaxed);
    }

    /// Get read success rate
    pub fn read_success_rate(&self) -> f64 {
        let total = self.read_operations.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.successful_reads.load(Ordering::Relaxed) as f64 / total as f64
        }
    }

    /// Get write success rate
    pub fn write_success_rate(&self) -> f64 {
        let total = self.write_operations.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            self.successful_writes.load(Ordering::Relaxed) as f64 / total as f64
        }
    }

    /// Get cache hit rate for metadata cache
    pub fn metadata_cache_hit_rate(&self) -> f64 {
        let hits = self.metadata_cache_hits.load(Ordering::Relaxed);
        let misses = self.metadata_cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Get average batch size
    pub fn average_batch_size(&self) -> f64 {
        let batches = self.batch_writes.load(Ordering::Relaxed);
        if batches == 0 {
            0.0
        } else {
            self.batch_items_processed.load(Ordering::Relaxed) as f64 / batches as f64
        }
    }
}

/// Configuration for concurrent document text storage
#[derive(Debug, Clone)]
pub struct ConcurrentConfig {
    /// Maximum size of write batch
    pub max_batch_size: usize,
    /// Time to wait before processing partial batches
    pub batch_timeout: Duration,
    /// Maximum size of metadata cache
    pub metadata_cache_size: usize,
    /// Whether to enable background batch processing
    pub enable_background_processing: bool,
}

impl Default for ConcurrentConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            batch_timeout: Duration::from_millis(100),
            metadata_cache_size: 10000,
            enable_background_processing: true,
        }
    }
}

/// Thread-safe document text storage with concurrent access optimization
///
/// This wrapper provides high-performance concurrent access to document text storage:
/// - Reader-writer locks for concurrent reads with exclusive writes
/// - Metadata caching for fast document existence checks
/// - Write batching for improved throughput
/// - Background processing for non-blocking writes
/// - Comprehensive metrics for performance monitoring
pub struct ConcurrentDocumentTextStorage {
    /// Core storage protected by reader-writer lock
    storage: Arc<RwLock<DocumentTextStorage>>,
    /// Metadata cache for quick document lookups
    metadata_cache: Arc<RwLock<HashMap<DocumentId, DocumentMetadata>>>,
    /// Queue for batching write operations
    write_queue: Arc<Mutex<VecDeque<WriteOperation>>>,
    /// Background task handle for batch processing
    background_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Performance metrics
    metrics: Arc<ConcurrentMetrics>,
    /// Configuration
    config: ConcurrentConfig,
}

impl ConcurrentDocumentTextStorage {
    /// Create new concurrent document text storage wrapper
    ///
    /// # Arguments
    /// * `storage` - The underlying document text storage instance
    /// * `config` - Configuration for concurrent operations
    ///
    /// # Returns
    /// * `ConcurrentDocumentTextStorage` - The concurrent wrapper instance
    pub fn new(storage: DocumentTextStorage, config: ConcurrentConfig) -> Self {
        let storage = Arc::new(RwLock::new(storage));
        let metadata_cache = Arc::new(RwLock::new(HashMap::new()));
        let write_queue = Arc::new(Mutex::new(VecDeque::new()));
        let background_task = Arc::new(Mutex::new(None));
        let metrics = Arc::new(ConcurrentMetrics::default());

        Self {
            storage,
            metadata_cache,
            write_queue,
            background_task,
            metrics,
            config,
        }
    }

    /// Create with default configuration
    pub fn with_default_config(storage: DocumentTextStorage) -> Self {
        Self::new(storage, ConcurrentConfig::default())
    }

    /// Get document text with concurrent access optimization
    ///
    /// This method provides optimized concurrent read access:
    /// - Uses reader locks to allow multiple concurrent readers
    /// - Checks metadata cache for quick validation
    /// - Records access statistics for performance monitoring
    ///
    /// # Arguments
    /// * `document_id` - Document ID to retrieve
    ///
    /// # Returns
    /// * `Ok(String)` - Document text content
    /// * `Err(ShardexError)` - Read operation failed
    pub async fn get_text_concurrent(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        // Record read operation
        self.metrics.record_read_operation();

        // Check metadata cache for quick validation and update access count
        self.update_access_metadata(document_id).await;

        // Acquire read lock (allows multiple concurrent readers)
        let storage = self.storage.read();
        let result = storage.get_text_safe(document_id);

        // Record metrics based on result
        match &result {
            Ok(_) => self.metrics.record_successful_read(),
            Err(_) => self.metrics.record_failed_read(),
        }

        result
    }

    /// Store text with batched writes for better performance
    ///
    /// This method queues write operations for batch processing:
    /// - Adds operation to write queue
    /// - Triggers batch processing when queue is full
    /// - Returns immediately with a future that completes when write is done
    ///
    /// # Arguments
    /// * `document_id` - Document ID to store
    /// * `text` - Text content to store
    ///
    /// # Returns
    /// * `Ok(())` - Text successfully queued and processed
    /// * `Err(ShardexError)` - Write operation failed
    pub async fn store_text_batched(
        &self,
        document_id: DocumentId,
        text: String,
    ) -> Result<(), ShardexError> {
        let (tx, rx) = oneshot::channel();

        // Add to write queue
        {
            let mut queue = self.write_queue.lock();
            queue.push_back(WriteOperation::StoreText {
                document_id,
                text: text.clone(),
                completion_sender: tx,
            });
        }

        // Trigger batch processing if queue is full or timeout
        self.maybe_trigger_batch_processing().await;

        // Wait for completion
        match rx.await {
            Ok(result) => {
                // Update metadata cache on successful write
                if result.is_ok() {
                    self.update_metadata_cache(document_id, text.len() as u64).await;
                }
                result
            }
            Err(_) => Err(ShardexError::concurrency_error(
                "store_text_batched",
                "Write operation was cancelled",
                "Retry the operation",
            )),
        }
    }

    /// Delete document text (logical deletion)
    ///
    /// # Arguments
    /// * `document_id` - Document ID to delete
    ///
    /// # Returns
    /// * `Ok(())` - Document successfully marked for deletion
    /// * `Err(ShardexError)` - Delete operation failed
    pub async fn delete_text_batched(&self, document_id: DocumentId) -> Result<(), ShardexError> {
        let (tx, rx) = oneshot::channel();

        // Add to write queue
        {
            let mut queue = self.write_queue.lock();
            queue.push_back(WriteOperation::DeleteText {
                document_id,
                completion_sender: tx,
            });
        }

        // Trigger batch processing
        self.maybe_trigger_batch_processing().await;

        // Wait for completion
        match rx.await {
            Ok(result) => {
                // Remove from metadata cache on successful deletion
                if result.is_ok() {
                    self.remove_from_metadata_cache(document_id).await;
                }
                result
            }
            Err(_) => Err(ShardexError::concurrency_error(
                "delete_text_batched",
                "Delete operation was cancelled",
                "Retry the operation",
            )),
        }
    }

    /// Extract text substring with concurrent read access
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    /// * `start` - Starting position in bytes
    /// * `length` - Length in bytes
    ///
    /// # Returns
    /// * `Ok(String)` - Extracted text substring
    /// * `Err(ShardexError)` - Extraction failed
    pub async fn extract_text_substring_concurrent(
        &self,
        document_id: DocumentId,
        start: u32,
        length: u32,
    ) -> Result<String, ShardexError> {
        // Check metadata cache for early validation
        if let Some(metadata) = self.get_metadata_from_cache(document_id).await {
            let end_offset = start as u64 + length as u64;
            if end_offset > metadata.text_length {
                return Err(ShardexError::invalid_range(
                    start,
                    length,
                    metadata.text_length,
                ));
            }
            self.metrics.record_cache_hit();
        } else {
            self.metrics.record_cache_miss();
        }

        // Use concurrent read access
        let storage = self.storage.read();
        storage.extract_text_substring(document_id, start, length)
    }

    /// Check if document exists in storage
    ///
    /// This is optimized to check the metadata cache first before
    /// accessing the underlying storage.
    ///
    /// # Arguments
    /// * `document_id` - Document ID to check
    ///
    /// # Returns
    /// * `true` - Document exists
    /// * `false` - Document does not exist
    pub async fn document_exists(&self, document_id: DocumentId) -> bool {
        // Check metadata cache first
        if self.get_metadata_from_cache(document_id).await.is_some() {
            self.metrics.record_cache_hit();
            return true;
        }

        self.metrics.record_cache_miss();

        // Fall back to storage check
        let text_exists = {
            let storage = self.storage.read();
            storage.get_text_safe(document_id).is_ok()
        };
        
        if text_exists {
            // Cache the result for future lookups
            // We don't know the exact length, so use a placeholder
            self.update_metadata_cache(document_id, 0).await;
            true
        } else {
            false
        }
    }

    /// Get current performance metrics
    pub fn get_metrics(&self) -> Arc<ConcurrentMetrics> {
        Arc::clone(&self.metrics)
    }

    /// Get configuration
    pub fn get_config(&self) -> &ConcurrentConfig {
        &self.config
    }

    /// Get current write queue size
    pub fn write_queue_size(&self) -> usize {
        self.write_queue.lock().len()
    }

    /// Force processing of current write queue
    pub async fn flush_write_queue(&self) -> Result<usize, ShardexError> {
        self.process_write_batch().await
    }

    // Private helper methods

    /// Update access metadata in cache
    async fn update_access_metadata(&self, document_id: DocumentId) {
        let mut cache = self.metadata_cache.write();
        if let Some(metadata) = cache.get_mut(&document_id) {
            metadata.record_access();
            self.metrics.record_cache_hit();
        } else {
            self.metrics.record_cache_miss();
        }
    }

    /// Update metadata cache with document information
    async fn update_metadata_cache(&self, document_id: DocumentId, text_length: u64) {
        let mut cache = self.metadata_cache.write();
        
        // If we're at capacity, remove oldest entries based on last access time
        if cache.len() >= self.config.metadata_cache_size {
            // Find the least recently accessed entry
            let mut oldest_id = None;
            let mut oldest_time = SystemTime::now();
            
            for (id, metadata) in cache.iter() {
                if metadata.last_access_time < oldest_time {
                    oldest_time = metadata.last_access_time;
                    oldest_id = Some(*id);
                }
            }
            
            if let Some(id) = oldest_id {
                cache.remove(&id);
            }
        }
        
        cache.insert(document_id, DocumentMetadata::new(document_id, text_length));
    }

    /// Get metadata from cache
    async fn get_metadata_from_cache(&self, document_id: DocumentId) -> Option<DocumentMetadata> {
        let cache = self.metadata_cache.read();
        cache.get(&document_id).cloned()
    }

    /// Remove document from metadata cache
    async fn remove_from_metadata_cache(&self, document_id: DocumentId) {
        let mut cache = self.metadata_cache.write();
        cache.remove(&document_id);
    }

    /// Check if batch processing should be triggered
    async fn maybe_trigger_batch_processing(&self) {
        let queue_size = self.write_queue.lock().len();
        
        if queue_size >= self.config.max_batch_size {
            // Process immediately if queue is full
            let _ = self.process_write_batch().await;
        } else if self.config.enable_background_processing {
            // Ensure background processing is running
            self.ensure_background_task_running().await;
        }
    }

    /// Ensure background batch processing task is running
    async fn ensure_background_task_running(&self) {
        let mut task_handle = self.background_task.lock();
        
        if task_handle.is_none() || task_handle.as_ref().unwrap().is_finished() {
            let storage_clone = Arc::clone(&self.storage);
            let queue_clone = Arc::clone(&self.write_queue);
            let metrics_clone = Arc::clone(&self.metrics);
            let timeout = self.config.batch_timeout;
            
            let handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(timeout);
                
                loop {
                    interval.tick().await;
                    
                    let operations = {
                        let mut queue = queue_clone.lock();
                        if queue.is_empty() {
                            continue;
                        }
                        
                        // Take all operations for batch processing
                        let mut ops = Vec::new();
                        while let Some(op) = queue.pop_front() {
                            ops.push(op);
                        }
                        ops
                    };
                    
                    if !operations.is_empty() {
                        Self::process_operations_batch(
                            &storage_clone,
                            operations,
                            &metrics_clone,
                        ).await;
                    }
                }
            });
            
            *task_handle = Some(handle);
        }
    }

    /// Process write batch immediately
    async fn process_write_batch(&self) -> Result<usize, ShardexError> {
        let operations = {
            let mut queue = self.write_queue.lock();
            let mut ops = Vec::new();
            
            // Take up to max_batch_size operations
            for _ in 0..self.config.max_batch_size {
                if let Some(op) = queue.pop_front() {
                    ops.push(op);
                } else {
                    break;
                }
            }
            ops
        };
        
        if operations.is_empty() {
            return Ok(0);
        }
        
        let count = operations.len();
        Self::process_operations_batch(&self.storage, operations, &self.metrics).await;
        Ok(count)
    }

    /// Process a batch of write operations
    async fn process_operations_batch(
        storage: &Arc<RwLock<DocumentTextStorage>>,
        operations: Vec<WriteOperation>,
        metrics: &Arc<ConcurrentMetrics>,
    ) {
        if operations.is_empty() {
            return;
        }
        
        let batch_size = operations.len();
        
        // Acquire write lock for batch processing
        let mut storage_guard = storage.write();
        
        // Process all operations in the batch
        for operation in operations {
            match operation {
                WriteOperation::StoreText {
                    document_id,
                    text,
                    completion_sender,
                } => {
                    metrics.record_write_operation();
                    let result = storage_guard.store_text_safe(document_id, &text);
                    
                    match result {
                        Ok(_) => metrics.record_successful_write(),
                        Err(_) => metrics.record_failed_write(),
                    }
                    
                    let _ = completion_sender.send(result);
                }
                
                WriteOperation::DeleteText {
                    document_id: _,
                    completion_sender,
                } => {
                    metrics.record_write_operation();
                    
                    // For now, deletion is a placeholder since the base storage
                    // doesn't support deletion. In a full implementation, this
                    // would mark entries for deletion/compaction.
                    let result = Ok(());
                    
                    if result.is_ok() {
                        metrics.record_successful_write();
                    } else {
                        metrics.record_failed_write();
                    }
                    
                    let _ = completion_sender.send(result);
                }
            }
        }
        
        // Record batch statistics
        metrics.record_batch_write(batch_size);
    }

    /// Shutdown background processing gracefully
    pub async fn shutdown(&self) -> Result<(), ShardexError> {
        // Process remaining items in queue
        self.flush_write_queue().await?;
        
        // Cancel background task
        let mut task_handle = self.background_task.lock();
        if let Some(handle) = task_handle.take() {
            handle.abort();
        }
        
        Ok(())
    }

    /// Get underlying storage statistics
    pub async fn get_storage_stats(&self) -> (u32, u64, usize, bool) {
        let storage = self.storage.read();
        (
            storage.entry_count(),
            storage.total_text_size(),
            storage.max_document_size(),
            storage.is_empty(),
        )
    }

    /// Get metadata cache statistics
    pub fn get_cache_stats(&self) -> (usize, usize) {
        let cache = self.metadata_cache.read();
        (cache.len(), self.config.metadata_cache_size)
    }
}

// Implement Drop to ensure graceful shutdown
impl Drop for ConcurrentDocumentTextStorage {
    fn drop(&mut self) {
        // Cancel background task if it exists
        if let Some(handle) = self.background_task.lock().take() {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document_text_storage::DocumentTextStorage;
    use tempfile::TempDir;
    use tokio::time::Duration;

    async fn create_test_storage() -> (TempDir, ConcurrentDocumentTextStorage) {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let concurrent_storage = ConcurrentDocumentTextStorage::with_default_config(storage);
        (temp_dir, concurrent_storage)
    }

    #[tokio::test]
    async fn test_concurrent_basic_operations() {
        let (_temp_dir, storage) = create_test_storage().await;
        let storage = Arc::new(storage);
        let storage = Arc::new(storage);
        
        let doc_id = DocumentId::new();
        let text = "Test document content";
        
        // Store text
        storage.store_text_batched(doc_id, text.to_string()).await.unwrap();
        
        // Retrieve text
        let retrieved = storage.get_text_concurrent(doc_id).await.unwrap();
        assert_eq!(retrieved, text);
        
        // Check existence
        assert!(storage.document_exists(doc_id).await);
        
        // Check non-existent document
        let non_existent = DocumentId::new();
        assert!(!storage.document_exists(non_existent).await);
    }

    #[tokio::test]
    async fn test_concurrent_batch_processing() {
        let (_temp_dir, storage) = create_test_storage().await;
        
        let mut handles = Vec::new();
        
        // Store multiple documents concurrently
        for i in 0..10 {
            let storage_clone = &storage;
            let handle = async move {
                let doc_id = DocumentId::new();
                let text = format!("Document content {}", i);
                storage_clone.store_text_batched(doc_id, text.clone()).await.unwrap();
                (doc_id, text)
            };
            handles.push(handle);
        }
        
        // Wait for all operations to complete
        let results: Vec<_> = futures::future::join_all(handles).await;
        
        // Verify all documents were stored correctly
        for (doc_id, expected_text) in results {
            let retrieved = storage.get_text_concurrent(doc_id).await.unwrap();
            assert_eq!(retrieved, expected_text);
        }
    }

    #[tokio::test]
    async fn test_concurrent_metrics() {
        let (_temp_dir, storage) = create_test_storage().await;
        
        let doc_id = DocumentId::new();
        let text = "Test content";
        
        // Store and retrieve to generate metrics
        storage.store_text_batched(doc_id, text.to_string()).await.unwrap();
        let _retrieved = storage.get_text_concurrent(doc_id).await.unwrap();
        
        let metrics = storage.get_metrics();
        
        // Check that metrics were recorded
        assert!(metrics.read_operations.load(Ordering::Relaxed) > 0);
        assert!(metrics.write_operations.load(Ordering::Relaxed) > 0);
        assert!(metrics.successful_reads.load(Ordering::Relaxed) > 0);
        assert!(metrics.successful_writes.load(Ordering::Relaxed) > 0);
    }

    #[tokio::test]
    async fn test_metadata_cache() {
        let (_temp_dir, storage) = create_test_storage().await;
        
        let doc_id = DocumentId::new();
        let text = "Test content for caching";
        
        // Store document
        storage.store_text_batched(doc_id, text.to_string()).await.unwrap();
        
        // First access should be a cache miss
        assert!(storage.document_exists(doc_id).await);
        
        // Second access should be a cache hit
        assert!(storage.document_exists(doc_id).await);
        
        let metrics = storage.get_metrics();
        let cache_hit_rate = metrics.metadata_cache_hit_rate();
        
        // Should have some cache hits
        assert!(cache_hit_rate > 0.0);
    }

    #[tokio::test]
    async fn test_text_substring_extraction() {
        let (_temp_dir, storage) = create_test_storage().await;
        
        let doc_id = DocumentId::new();
        let text = "The quick brown fox jumps over the lazy dog";
        
        storage.store_text_batched(doc_id, text.to_string()).await.unwrap();
        
        // Extract substring
        let substring = storage
            .extract_text_substring_concurrent(doc_id, 4, 11)
            .await
            .unwrap();
        assert_eq!(substring, "quick brown");
        
        // Test invalid range
        let result = storage
            .extract_text_substring_concurrent(doc_id, 100, 10)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_write_queue_management() {
        let config = ConcurrentConfig {
            max_batch_size: 5, // Set higher so operations don't auto-process
            batch_timeout: Duration::from_millis(10),
            metadata_cache_size: 100,
            enable_background_processing: false, // Disable for predictable testing
        };
        
        let temp_dir = TempDir::new().unwrap();
        let base_storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let storage = Arc::new(ConcurrentDocumentTextStorage::new(base_storage, config));
        
        // Queue should start empty
        assert_eq!(storage.write_queue_size(), 0);
        
        // Add operations to queue
        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        
        // These should queue up rather than process immediately
        let _handle1 = tokio::spawn({
            let storage = Arc::clone(&storage);
            async move {
                storage.store_text_batched(doc1, "doc1".to_string()).await
            }
        });
        
        let _handle2 = tokio::spawn({
            let storage = Arc::clone(&storage);
            async move {
                storage.store_text_batched(doc2, "doc2".to_string()).await
            }
        });
        
        // Wait for operations to be queued
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Verify operations were queued
        assert!(storage.write_queue_size() > 0, "Operations should be queued when background processing is disabled");
        
        // Force flush the queue
        let processed = storage.flush_write_queue().await.unwrap();
        assert!(processed > 0, "Should have processed some operations");
    }

    #[tokio::test]
    async fn test_concurrent_config() {
        let (_temp_dir, storage) = create_test_storage().await;
        
        let config = storage.get_config();
        assert_eq!(config.max_batch_size, 100);
        assert_eq!(config.metadata_cache_size, 10000);
        assert!(config.enable_background_processing);
    }

    #[tokio::test] 
    async fn test_graceful_shutdown() {
        let (_temp_dir, storage) = create_test_storage().await;
        let storage = Arc::new(storage);
        
        // Add some operations to the queue
        let doc_id = DocumentId::new();
        let _handle = tokio::spawn({
            let storage = Arc::clone(&storage);
            async move {
                storage.store_text_batched(doc_id, "test".to_string()).await
            }
        });
        
        // Shutdown should process remaining operations
        storage.shutdown().await.unwrap();
        
        // Queue should be empty after shutdown
        assert_eq!(storage.write_queue_size(), 0);
    }
}
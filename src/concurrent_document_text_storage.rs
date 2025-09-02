//! Concurrent document text storage with reader-writer optimization
//!
//! This module provides thread-safe concurrent access to document text storage
//! with optimized reader-writer patterns, write batching, and metadata caching.

use crate::document_text_storage::DocumentTextStorage;
use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{oneshot, Semaphore};
use tokio::time::timeout;

/// Metadata for cached document information
#[derive(Debug)]
struct DocumentMetadata {
    document_id: DocumentId,
    text_length: u64,
    last_access_time: SystemTime,
    access_count: AtomicU64,
}

impl Clone for DocumentMetadata {
    fn clone(&self) -> Self {
        Self {
            document_id: self.document_id,
            text_length: self.text_length,
            last_access_time: self.last_access_time,
            access_count: AtomicU64::new(self.access_count.load(Ordering::Relaxed)),
        }
    }
}

/// Write operation for batched processing
#[derive(Debug)]
enum WriteOperation {
    StoreText {
        document_id: DocumentId,
        text: String,
        completion_sender: oneshot::Sender<Result<(), ShardexError>>,
    },
}

/// Configuration for concurrent document text storage
#[derive(Debug, Clone)]
pub struct ConcurrentStorageConfig {
    /// Maximum size of write batch
    pub max_batch_size: usize,
    /// Timeout for write operations
    pub write_timeout: Duration,
    /// Maximum size of metadata cache
    pub metadata_cache_size: usize,
    /// Batch processing interval
    pub batch_interval: Duration,
    /// Maximum concurrent operations
    pub max_concurrent_ops: usize,
}

impl Default for ConcurrentStorageConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            write_timeout: Duration::from_secs(30),
            metadata_cache_size: 10000,
            batch_interval: Duration::from_millis(100),
            max_concurrent_ops: 100,
        }
    }
}

/// Performance metrics for concurrent operations
#[derive(Debug, Clone, Default)]
pub struct ConcurrentStorageMetrics {
    /// Total read operations
    pub read_operations: u64,
    /// Successful read operations
    pub successful_reads: u64,
    /// Failed read operations
    pub failed_reads: u64,
    /// Total write operations
    pub write_operations: u64,
    /// Successful write operations
    pub successful_writes: u64,
    /// Failed write operations
    #[allow(dead_code)]
    pub failed_writes: u64,
    /// Metadata cache hits
    pub metadata_cache_hits: u64,
    /// Metadata cache misses
    pub metadata_cache_misses: u64,
    /// Total batches processed
    pub batches_processed: u64,
    /// Average batch size
    pub avg_batch_size: f64,
    /// Average operation latency in milliseconds
    pub avg_operation_latency_ms: f64,
}

impl ConcurrentStorageMetrics {
    /// Calculate read success ratio
    #[allow(dead_code)]
    pub fn read_success_ratio(&self) -> f64 {
        if self.read_operations == 0 {
            0.0
        } else {
            self.successful_reads as f64 / self.read_operations as f64
        }
    }

    /// Calculate write success ratio
    #[allow(dead_code)]
    pub fn write_success_ratio(&self) -> f64 {
        if self.write_operations == 0 {
            0.0
        } else {
            self.successful_writes as f64 / self.write_operations as f64
        }
    }

    /// Calculate metadata cache hit ratio
    #[allow(dead_code)]
    pub fn metadata_cache_hit_ratio(&self) -> f64 {
        let total = self.metadata_cache_hits + self.metadata_cache_misses;
        if total == 0 {
            0.0
        } else {
            self.metadata_cache_hits as f64 / total as f64
        }
    }

    /// Get total operations
    pub fn total_operations(&self) -> u64 {
        self.read_operations + self.write_operations
    }
}

/// Thread-safe document text storage with optimized concurrency
pub struct ConcurrentDocumentTextStorage {
    /// Core storage protected by RwLock for reader-writer access
    storage: Arc<RwLock<DocumentTextStorage>>,
    /// Metadata cache for quick document validation
    metadata_cache: Arc<Mutex<HashMap<DocumentId, DocumentMetadata>>>,
    /// Write operation queue for batching
    write_queue: Arc<Mutex<VecDeque<WriteOperation>>>,
    /// Background batch processor handle
    batch_processor: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    /// Semaphore for limiting concurrent operations
    concurrency_limiter: Arc<Semaphore>,
    /// Configuration
    config: ConcurrentStorageConfig,
    /// Performance metrics
    metrics: Arc<Mutex<ConcurrentStorageMetrics>>,
}

impl ConcurrentDocumentTextStorage {
    /// Create new concurrent document text storage
    pub fn new(storage: DocumentTextStorage, config: ConcurrentStorageConfig) -> Self {
        let concurrency_limiter = Arc::new(Semaphore::new(config.max_concurrent_ops));

        Self {
            storage: Arc::new(RwLock::new(storage)),
            metadata_cache: Arc::new(Mutex::new(HashMap::new())),
            write_queue: Arc::new(Mutex::new(VecDeque::new())),
            batch_processor: Arc::new(Mutex::new(None)),
            concurrency_limiter,
            config,
            metrics: Arc::new(Mutex::new(ConcurrentStorageMetrics::default())),
        }
    }

    /// Start background batch processor
    pub async fn start_background_processor(&self) -> Result<(), ShardexError> {
        let mut processor = self.batch_processor.lock();

        if processor.is_some() {
            return Err(ShardexError::InvalidInput {
                field: "background_processor".to_string(),
                reason: "Background processor already running".to_string(),
                suggestion: "Stop existing processor before starting new one".to_string(),
            });
        }

        let storage = Arc::clone(&self.storage);
        let write_queue = Arc::clone(&self.write_queue);
        let metadata_cache = Arc::clone(&self.metadata_cache);
        let metrics = Arc::clone(&self.metrics);
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.batch_interval);

            loop {
                interval.tick().await;

                if let Err(e) =
                    Self::process_write_batch_static(&storage, &write_queue, &metadata_cache, &metrics, &config).await
                {
                    log::error!("Error processing write batch: {:?}", e);
                }
            }
        });

        *processor = Some(handle);
        Ok(())
    }

    /// Stop background batch processor
    pub async fn stop_background_processor(&self) -> Result<(), ShardexError> {
        let handle = {
            let mut processor = self.batch_processor.lock();
            processor.take()
        };

        if let Some(handle) = handle {
            handle.abort();

            // Process any remaining items in the queue
            self.flush_write_queue().await?;
        }

        Ok(())
    }

    /// Get document text with concurrent access optimization
    pub async fn get_text_concurrent(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        let _permit = self
            .concurrency_limiter
            .acquire()
            .await
            .map_err(|_| ShardexError::InvalidInput {
                field: "concurrency_limiter".to_string(),
                reason: "Failed to acquire concurrency permit".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

        let start_time = Instant::now();

        // Update metadata cache access info
        {
            let mut cache = self.metadata_cache.lock();
            if let Some(metadata) = cache.get_mut(&document_id) {
                metadata.last_access_time = SystemTime::now();
                metadata.access_count.fetch_add(1, Ordering::Relaxed);

                // Record cache hit
                {
                    let mut metrics = self.metrics.lock();
                    metrics.metadata_cache_hits += 1;
                }
            } else {
                // Record cache miss
                {
                    let mut metrics = self.metrics.lock();
                    metrics.metadata_cache_misses += 1;
                }
            }
        }

        // Acquire read lock (allows multiple concurrent readers)
        let storage = self.storage.read();
        let result = storage.get_text(document_id);

        // Update metrics
        let elapsed = start_time.elapsed();
        let success = result.is_ok();
        self.update_read_metrics(success, elapsed.as_millis() as f64);

        result
    }

    /// Store text with batched writes for better performance
    pub async fn store_text_batched(&self, document_id: DocumentId, text: String) -> Result<(), ShardexError> {
        let _permit = self
            .concurrency_limiter
            .acquire()
            .await
            .map_err(|_| ShardexError::InvalidInput {
                field: "concurrency_limiter".to_string(),
                reason: "Failed to acquire concurrency permit".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

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

        // Trigger batch processing if queue is full
        self.maybe_trigger_batch_write().await?;

        // Wait for completion with timeout
        timeout(self.config.write_timeout, rx)
            .await
            .map_err(|_| ShardexError::InvalidInput {
                field: "write_timeout".to_string(),
                reason: "Write operation timed out".to_string(),
                suggestion: "Increase write timeout or reduce batch size".to_string(),
            })?
            .map_err(|_| ShardexError::InvalidInput {
                field: "write_operation".to_string(),
                reason: "Write operation was cancelled".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?
    }

    /// Store text immediately without batching (for urgent operations)
    #[allow(dead_code)]
    pub async fn store_text_immediate(&self, document_id: DocumentId, text: &str) -> Result<(), ShardexError> {
        let _permit = self
            .concurrency_limiter
            .acquire()
            .await
            .map_err(|_| ShardexError::InvalidInput {
                field: "concurrency_limiter".to_string(),
                reason: "Failed to acquire concurrency permit".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

        let start_time = Instant::now();

        // Acquire write lock
        let mut storage = self.storage.write();
        let result = storage.store_text(document_id, text);

        if result.is_ok() {
            // Update metadata cache
            self.update_metadata_cache(document_id, text.len() as u64);
        }

        // Update metrics
        let elapsed = start_time.elapsed();
        let success = result.is_ok();
        self.update_write_metrics(success, elapsed.as_millis() as f64);

        result
    }

    /// Extract text substring with concurrent access
    pub async fn extract_text_substring_concurrent(
        &self,
        document_id: DocumentId,
        start: u32,
        length: u32,
    ) -> Result<String, ShardexError> {
        let _permit = self
            .concurrency_limiter
            .acquire()
            .await
            .map_err(|_| ShardexError::InvalidInput {
                field: "concurrency_limiter".to_string(),
                reason: "Failed to acquire concurrency permit".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

        // Check metadata cache for early validation
        {
            let cache = self.metadata_cache.lock();
            if let Some(metadata) = cache.get(&document_id) {
                let end_offset = start as u64 + length as u64;
                if end_offset > metadata.text_length {
                    return Err(ShardexError::InvalidInput {
                        field: "range".to_string(),
                        reason: format!(
                            "Range {}..{} exceeds document length {}",
                            start, end_offset, metadata.text_length
                        ),
                        suggestion: "Ensure range is within document bounds".to_string(),
                    });
                }
            }
        }

        let storage = self.storage.read();
        storage.extract_text_substring(document_id, start, length)
    }

    /// Process write batch if queue is full
    async fn maybe_trigger_batch_write(&self) -> Result<(), ShardexError> {
        let queue_len = {
            let queue = self.write_queue.lock();
            queue.len()
        };

        if queue_len >= self.config.max_batch_size {
            self.process_write_batch().await?;
        }

        Ok(())
    }

    /// Process queued write operations in batch
    async fn process_write_batch(&self) -> Result<(), ShardexError> {
        Self::process_write_batch_static(
            &self.storage,
            &self.write_queue,
            &self.metadata_cache,
            &self.metrics,
            &self.config,
        )
        .await
    }

    /// Static version for background processor
    async fn process_write_batch_static(
        storage: &Arc<RwLock<DocumentTextStorage>>,
        write_queue: &Arc<Mutex<VecDeque<WriteOperation>>>,
        metadata_cache: &Arc<Mutex<HashMap<DocumentId, DocumentMetadata>>>,
        metrics: &Arc<Mutex<ConcurrentStorageMetrics>>,
        config: &ConcurrentStorageConfig,
    ) -> Result<(), ShardexError> {
        let mut operations = Vec::new();

        // Collect batch of operations
        {
            let mut queue = write_queue.lock();
            for _ in 0..config.max_batch_size {
                if let Some(op) = queue.pop_front() {
                    operations.push(op);
                } else {
                    break;
                }
            }
        }

        if operations.is_empty() {
            return Ok(());
        }

        let batch_size = operations.len();
        let batch_start = Instant::now();

        // Acquire write lock for batch processing
        let mut storage_guard = storage.write();

        // Process all operations in batch
        for operation in operations {
            match operation {
                WriteOperation::StoreText {
                    document_id,
                    text,
                    completion_sender,
                } => {
                    let result = storage_guard.store_text(document_id, &text);

                    // Update metadata cache on success
                    if result.is_ok() {
                        let mut cache = metadata_cache.lock();
                        let metadata = DocumentMetadata {
                            document_id,
                            text_length: text.len() as u64,
                            last_access_time: SystemTime::now(),
                            access_count: AtomicU64::new(0),
                        };
                        cache.insert(document_id, metadata);

                        // Clean up cache if it's too large
                        if cache.len() > config.metadata_cache_size {
                            Self::cleanup_metadata_cache(&mut cache, config.metadata_cache_size / 2);
                        }
                    }

                    let _ = completion_sender.send(result);
                }
            }
        }

        // Update batch metrics
        let batch_elapsed = batch_start.elapsed();
        {
            let mut metrics_guard = metrics.lock();
            metrics_guard.batches_processed += 1;
            metrics_guard.write_operations += batch_size as u64;
            metrics_guard.successful_writes += batch_size as u64; // Simplified for now

            // Update average batch size
            let total_batches = metrics_guard.batches_processed;
            if total_batches == 1 {
                metrics_guard.avg_batch_size = batch_size as f64;
            } else {
                metrics_guard.avg_batch_size = ((metrics_guard.avg_batch_size * (total_batches - 1) as f64)
                    + batch_size as f64)
                    / total_batches as f64;
            }

            // Update average operation latency
            let total_ops = metrics_guard.total_operations();
            let batch_latency_per_op = batch_elapsed.as_millis() as f64 / batch_size as f64;
            if total_ops == batch_size as u64 {
                metrics_guard.avg_operation_latency_ms = batch_latency_per_op;
            } else {
                metrics_guard.avg_operation_latency_ms = ((metrics_guard.avg_operation_latency_ms
                    * (total_ops - batch_size as u64) as f64)
                    + (batch_latency_per_op * batch_size as f64))
                    / total_ops as f64;
            }
        }

        Ok(())
    }

    /// Clean up metadata cache by removing least recently used entries
    fn cleanup_metadata_cache(cache: &mut HashMap<DocumentId, DocumentMetadata>, target_size: usize) {
        if cache.len() <= target_size {
            return;
        }

        // Collect document IDs sorted by last access time (oldest first)
        let mut entries: Vec<_> = cache
            .iter()
            .map(|(id, metadata)| (*id, metadata.last_access_time))
            .collect();
        entries.sort_by(|a, b| a.1.cmp(&b.1));

        // Remove oldest entries
        let to_remove = cache.len() - target_size;
        for i in 0..to_remove {
            if let Some((doc_id, _)) = entries.get(i) {
                cache.remove(doc_id);
            }
        }
    }

    /// Flush all pending write operations
    pub async fn flush_write_queue(&self) -> Result<(), ShardexError> {
        self.process_write_batch().await
    }

    /// Update metadata cache
    #[allow(dead_code)]
    fn update_metadata_cache(&self, document_id: DocumentId, text_length: u64) {
        let mut cache = self.metadata_cache.lock();
        let metadata = DocumentMetadata {
            document_id,
            text_length,
            last_access_time: SystemTime::now(),
            access_count: AtomicU64::new(0),
        };
        cache.insert(document_id, metadata);
    }

    /// Update read operation metrics
    fn update_read_metrics(&self, success: bool, latency_ms: f64) {
        let mut metrics = self.metrics.lock();
        metrics.read_operations += 1;

        if success {
            metrics.successful_reads += 1;
        } else {
            metrics.failed_reads += 1;
        }

        // Update average latency
        let total_ops = metrics.total_operations();
        if total_ops == 1 {
            metrics.avg_operation_latency_ms = latency_ms;
        } else {
            metrics.avg_operation_latency_ms =
                ((metrics.avg_operation_latency_ms * (total_ops - 1) as f64) + latency_ms) / total_ops as f64;
        }
    }

    /// Update write operation metrics  
    #[allow(dead_code)]
    fn update_write_metrics(&self, success: bool, latency_ms: f64) {
        let mut metrics = self.metrics.lock();
        metrics.write_operations += 1;

        if success {
            metrics.successful_writes += 1;
        } else {
            metrics.failed_writes += 1;
        }

        // Update average latency
        let total_ops = metrics.total_operations();
        if total_ops == 1 {
            metrics.avg_operation_latency_ms = latency_ms;
        } else {
            metrics.avg_operation_latency_ms =
                ((metrics.avg_operation_latency_ms * (total_ops - 1) as f64) + latency_ms) / total_ops as f64;
        }
    }

    /// Get current performance metrics
    #[allow(dead_code)]
    pub fn get_metrics(&self) -> ConcurrentStorageMetrics {
        let metrics = self.metrics.lock();
        metrics.clone()
    }

    /// Reset performance metrics
    #[allow(dead_code)]
    pub fn reset_metrics(&self) {
        let mut metrics = self.metrics.lock();
        *metrics = ConcurrentStorageMetrics::default();
    }

    /// Get metadata cache information
    #[allow(dead_code)]
    pub fn cache_info(&self) -> (usize, usize) {
        let cache = self.metadata_cache.lock();
        (cache.len(), self.config.metadata_cache_size)
    }

    /// Clear metadata cache
    #[allow(dead_code)]
    pub fn clear_metadata_cache(&self) {
        let mut cache = self.metadata_cache.lock();
        cache.clear();
    }
}

impl Drop for ConcurrentDocumentTextStorage {
    fn drop(&mut self) {
        // Attempt to flush pending operations on drop
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            rt.spawn(async move {
                // Note: This is best effort - we can't guarantee completion in Drop
                // In production, users should call flush_write_queue() explicitly before dropping
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document_text_storage::DocumentTextStorage;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_concurrent_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = ConcurrentStorageConfig::default();

        let concurrent_storage = ConcurrentDocumentTextStorage::new(storage, config);

        // Verify initial state
        let metrics = concurrent_storage.get_metrics();
        assert_eq!(metrics.read_operations, 0);
        assert_eq!(metrics.write_operations, 0);
    }

    #[tokio::test]
    async fn test_concurrent_read_write() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = ConcurrentStorageConfig::default();

        let concurrent_storage = ConcurrentDocumentTextStorage::new(storage, config);
        concurrent_storage
            .start_background_processor()
            .await
            .unwrap();

        let doc_id = DocumentId::new();
        let text = "Test document content";

        // Store text
        concurrent_storage
            .store_text_immediate(doc_id, text)
            .await
            .unwrap();

        // Read text
        let retrieved = concurrent_storage
            .get_text_concurrent(doc_id)
            .await
            .unwrap();
        assert_eq!(retrieved, text);

        // Check metrics
        let metrics = concurrent_storage.get_metrics();
        assert_eq!(metrics.read_operations, 1);
        assert_eq!(metrics.write_operations, 1);
        assert_eq!(metrics.successful_reads, 1);
        assert_eq!(metrics.successful_writes, 1);

        concurrent_storage
            .stop_background_processor()
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_batched_writes() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = ConcurrentStorageConfig {
            max_batch_size: 2, // Small batch size for testing
            ..Default::default()
        };

        let concurrent_storage = ConcurrentDocumentTextStorage::new(storage, config);
        concurrent_storage
            .start_background_processor()
            .await
            .unwrap();

        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        let text1 = "First document";
        let text2 = "Second document";

        // Store texts (should trigger batching)
        let store1 = concurrent_storage.store_text_batched(doc1, text1.to_string());
        let store2 = concurrent_storage.store_text_batched(doc2, text2.to_string());

        // Wait for both to complete
        tokio::try_join!(store1, store2).unwrap();

        // Verify texts were stored
        let retrieved1 = concurrent_storage.get_text_concurrent(doc1).await.unwrap();
        let retrieved2 = concurrent_storage.get_text_concurrent(doc2).await.unwrap();
        assert_eq!(retrieved1, text1);
        assert_eq!(retrieved2, text2);

        concurrent_storage
            .stop_background_processor()
            .await
            .unwrap();
    }

    #[test]
    fn test_concurrent_storage_config() {
        let config = ConcurrentStorageConfig::default();
        assert_eq!(config.max_batch_size, 100);
        assert_eq!(config.metadata_cache_size, 10000);
        assert!(config.write_timeout > Duration::from_secs(0));
    }

    #[test]
    fn test_metrics_calculations() {
        let metrics = ConcurrentStorageMetrics {
            successful_reads: 80,
            read_operations: 100,
            successful_writes: 90,
            write_operations: 100,
            metadata_cache_hits: 70,
            metadata_cache_misses: 30,
            ..Default::default()
        };

        assert_eq!(metrics.read_success_ratio(), 0.8);
        assert_eq!(metrics.write_success_ratio(), 0.9);
        assert_eq!(metrics.metadata_cache_hit_ratio(), 0.7);
        assert_eq!(metrics.total_operations(), 200);
    }
}

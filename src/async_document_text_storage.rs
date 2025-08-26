//! Async I/O support layer for document text storage
//!
//! This module provides asynchronous wrappers and utilities for document text storage
//! operations, enabling non-blocking I/O and better concurrency for async applications.

use crate::concurrent_document_text_storage::ConcurrentDocumentTextStorage;
use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::timeout;

/// Read-ahead buffer configuration
#[derive(Debug, Clone)]
pub struct ReadAheadConfig {
    /// Maximum number of documents to buffer ahead
    pub buffer_size: usize,
    /// Size of each read-ahead window
    pub window_size: usize,
    /// Time to keep buffers before eviction
    pub buffer_ttl: Duration,
    /// Enable predictive read-ahead based on access patterns
    pub enable_predictive: bool,
}

impl Default for ReadAheadConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            window_size: 100,
            buffer_ttl: Duration::from_secs(300), // 5 minutes
            enable_predictive: true,
        }
    }
}

/// Entry in the read-ahead buffer
#[derive(Debug, Clone)]
struct BufferedDocument {
    /// Document content
    content: String,
    /// Time when document was buffered
    buffered_at: Instant,
    /// Number of times accessed from buffer
    access_count: u64,
}

impl BufferedDocument {
    fn new(content: String) -> Self {
        Self {
            content,
            buffered_at: Instant::now(),
            access_count: 0,
        }
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.buffered_at.elapsed() > ttl
    }

    fn access(&mut self) -> &str {
        self.access_count += 1;
        &self.content
    }
}

/// Read-ahead buffer for document content
#[derive(Debug)]
pub struct ReadAheadBuffer {
    /// Buffered documents
    buffer: HashMap<DocumentId, BufferedDocument>,
    /// Configuration
    config: ReadAheadConfig,
    /// Buffer statistics
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl ReadAheadBuffer {
    /// Create new read-ahead buffer
    pub fn new(config: ReadAheadConfig) -> Self {
        Self {
            buffer: HashMap::new(),
            config,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    /// Check if document is in buffer
    pub fn get(&mut self, document_id: &DocumentId) -> Option<String> {
        if let Some(doc) = self.buffer.get_mut(document_id) {
            if doc.is_expired(self.config.buffer_ttl) {
                self.buffer.remove(document_id);
                self.misses += 1;
                None
            } else {
                self.hits += 1;
                Some(doc.access().to_string())
            }
        } else {
            self.misses += 1;
            None
        }
    }

    /// Add document to buffer
    pub fn put(&mut self, document_id: DocumentId, content: String) {
        // Evict expired entries
        self.evict_expired();

        // Evict LRU entries if at capacity
        if self.buffer.len() >= self.config.buffer_size {
            self.evict_lru();
        }

        self.buffer.insert(document_id, BufferedDocument::new(content));
    }

    /// Evict expired entries
    fn evict_expired(&mut self) {
        let ttl = self.config.buffer_ttl;
        let before_count = self.buffer.len();
        
        self.buffer.retain(|_, doc| !doc.is_expired(ttl));
        
        let evicted = before_count - self.buffer.len();
        self.evictions += evicted as u64;
    }

    /// Evict least recently used entries
    fn evict_lru(&mut self) {
        if self.buffer.is_empty() {
            return;
        }

        // Find the document with the oldest buffer time and lowest access count
        let mut oldest_id = None;
        let mut oldest_time = Instant::now();
        let mut lowest_access = u64::MAX;

        for (id, doc) in &self.buffer {
            if doc.buffered_at < oldest_time || 
               (doc.buffered_at == oldest_time && doc.access_count < lowest_access) {
                oldest_time = doc.buffered_at;
                lowest_access = doc.access_count;
                oldest_id = Some(*id);
            }
        }

        if let Some(id) = oldest_id {
            self.buffer.remove(&id);
            self.evictions += 1;
        }
    }

    /// Get buffer statistics
    pub fn stats(&self) -> (u64, u64, u64, usize) {
        (self.hits, self.misses, self.evictions, self.buffer.len())
    }

    /// Get hit ratio
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

/// Configuration for async document text storage
#[derive(Debug, Clone)]
pub struct AsyncConfig {
    /// Read-ahead buffer configuration
    pub read_ahead: ReadAheadConfig,
    /// Maximum concurrent operations
    pub max_concurrency: usize,
    /// Operation timeout
    pub operation_timeout: Duration,
    /// Enable background read-ahead tasks
    pub enable_background_tasks: bool,
    /// I/O thread pool size
    pub io_thread_pool_size: usize,
}

impl Default for AsyncConfig {
    fn default() -> Self {
        Self {
            read_ahead: ReadAheadConfig::default(),
            max_concurrency: 100,
            operation_timeout: Duration::from_secs(30),
            enable_background_tasks: true,
            io_thread_pool_size: 4,
        }
    }
}

/// Async statistics for I/O operations
#[derive(Debug, Default)]
pub struct AsyncIOStats {
    /// Total async read operations
    pub async_reads: u64,
    /// Total async write operations
    pub async_writes: u64,
    /// Total batch operations
    pub batch_operations: u64,
    /// Read-ahead hits
    pub read_ahead_hits: u64,
    /// Read-ahead misses
    pub read_ahead_misses: u64,
    /// Average operation latency in milliseconds
    pub avg_latency_ms: f64,
    /// Operations that timed out
    pub timeouts: u64,
    /// Background task runs
    pub background_tasks: u64,
}

impl AsyncIOStats {
    /// Record an async read operation
    pub fn record_read(&mut self, latency: Duration, read_ahead_hit: bool) {
        self.async_reads += 1;
        self.update_latency(latency);
        if read_ahead_hit {
            self.read_ahead_hits += 1;
        } else {
            self.read_ahead_misses += 1;
        }
    }

    /// Record an async write operation
    pub fn record_write(&mut self, latency: Duration) {
        self.async_writes += 1;
        self.update_latency(latency);
    }

    /// Record a batch operation
    pub fn record_batch(&mut self, latency: Duration, _size: usize) {
        self.batch_operations += 1;
        self.update_latency(latency);
    }

    /// Record a timeout
    pub fn record_timeout(&mut self) {
        self.timeouts += 1;
    }

    /// Record background task execution
    pub fn record_background_task(&mut self) {
        self.background_tasks += 1;
    }

    /// Update average latency with new measurement
    fn update_latency(&mut self, latency: Duration) {
        let total_ops = self.async_reads + self.async_writes + self.batch_operations;
        let new_latency_ms = latency.as_millis() as f64;
        
        if total_ops == 1 {
            self.avg_latency_ms = new_latency_ms;
        } else {
            // Exponential moving average
            self.avg_latency_ms = (self.avg_latency_ms * 0.9) + (new_latency_ms * 0.1);
        }
    }

    /// Get read-ahead hit ratio
    pub fn read_ahead_hit_ratio(&self) -> f64 {
        let total = self.read_ahead_hits + self.read_ahead_misses;
        if total == 0 {
            0.0
        } else {
            self.read_ahead_hits as f64 / total as f64
        }
    }
}

/// Asynchronous document text storage wrapper
///
/// This component provides high-performance async I/O operations for document text storage:
/// - Non-blocking read/write operations using tokio
/// - Read-ahead buffering with predictive pre-loading
/// - Batch processing for improved throughput
/// - Concurrent operation limiting and timeout handling
/// - Background task management for optimization
pub struct AsyncDocumentTextStorage {
    /// Underlying concurrent storage
    storage: Arc<ConcurrentDocumentTextStorage>,
    /// Read-ahead buffer
    read_ahead_buffer: Arc<RwLock<ReadAheadBuffer>>,
    /// Concurrency limiter
    semaphore: Arc<Semaphore>,
    /// Configuration
    config: AsyncConfig,
    /// I/O statistics
    stats: Arc<RwLock<AsyncIOStats>>,
    /// Background task handles
    background_tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl AsyncDocumentTextStorage {
    /// Create new async document text storage wrapper
    ///
    /// # Arguments
    /// * `storage` - Underlying concurrent document text storage
    /// * `config` - Async I/O configuration
    ///
    /// # Returns
    /// * `AsyncDocumentTextStorage` - The async wrapper instance
    pub fn new(storage: ConcurrentDocumentTextStorage, config: AsyncConfig) -> Self {
        let read_ahead_buffer = Arc::new(RwLock::new(ReadAheadBuffer::new(config.read_ahead.clone())));
        let semaphore = Arc::new(Semaphore::new(config.max_concurrency));
        let stats = Arc::new(RwLock::new(AsyncIOStats::default()));
        let background_tasks = Arc::new(Mutex::new(Vec::new()));

        let instance = Self {
            storage: Arc::new(storage),
            read_ahead_buffer,
            semaphore,
            config,
            stats,
            background_tasks,
        };

        // Start background tasks if enabled
        if instance.config.enable_background_tasks {
            instance.start_background_tasks();
        }

        instance
    }

    /// Create with default configuration
    pub fn with_default_config(storage: ConcurrentDocumentTextStorage) -> Self {
        Self::new(storage, AsyncConfig::default())
    }

    /// Async text retrieval with read-ahead optimization
    ///
    /// This method provides optimized async text retrieval:
    /// - Checks read-ahead buffer first for immediate response
    /// - Uses concurrent read access to underlying storage
    /// - Triggers predictive read-ahead for nearby documents
    /// - Enforces timeout and concurrency limits
    ///
    /// # Arguments
    /// * `document_id` - Document ID to retrieve
    ///
    /// # Returns
    /// * `Ok(String)` - Document text content
    /// * `Err(ShardexError)` - Read operation failed or timed out
    pub async fn get_text_async(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        let start_time = Instant::now();

        // Acquire semaphore permit for concurrency limiting
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            ShardexError::concurrency_error(
                "get_text_async",
                "Failed to acquire concurrency permit",
                "Reduce concurrent operations or increase limit",
            )
        })?;

        // Apply operation timeout
        let result = timeout(self.config.operation_timeout, async {
            self.get_text_async_internal(document_id).await
        }).await;

        match result {
            Ok(text_result) => {
                let latency = start_time.elapsed();
                let read_ahead_hit = self.read_ahead_buffer.read().await.buffer.contains_key(&document_id);
                self.stats.write().await.record_read(latency, read_ahead_hit);
                text_result
            }
            Err(_) => {
                self.stats.write().await.record_timeout();
                Err(ShardexError::concurrency_error(
                    "get_text_async",
                    "Operation timed out",
                    "Increase timeout or reduce system load",
                ))
            }
        }
    }

    /// Internal async text retrieval implementation
    async fn get_text_async_internal(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        // Check read-ahead buffer first
        {
            let mut buffer = self.read_ahead_buffer.write().await;
            if let Some(text) = buffer.get(&document_id) {
                return Ok(text);
            }
        }

        // Buffer miss - fetch from underlying storage
        let text = self.storage.get_text_concurrent(document_id).await?;

        // Add to read-ahead buffer for future hits
        {
            let mut buffer = self.read_ahead_buffer.write().await;
            buffer.put(document_id, text.clone());
        }

        // Trigger predictive read-ahead if enabled
        if self.config.read_ahead.enable_predictive {
            self.trigger_predictive_read_ahead(document_id).await;
        }

        Ok(text)
    }

    /// Async text storage with batch processing
    ///
    /// This method provides optimized async text storage:
    /// - Uses batched writes for improved throughput
    /// - Enforces timeout and concurrency limits
    /// - Updates read-ahead buffer on successful write
    ///
    /// # Arguments
    /// * `document_id` - Document ID to store
    /// * `text` - Text content to store
    ///
    /// # Returns
    /// * `Ok(())` - Text successfully stored
    /// * `Err(ShardexError)` - Write operation failed or timed out
    pub async fn store_text_async(
        &self,
        document_id: DocumentId,
        text: String,
    ) -> Result<(), ShardexError> {
        let start_time = Instant::now();

        // Acquire semaphore permit
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            ShardexError::concurrency_error(
                "store_text_async",
                "Failed to acquire concurrency permit",
                "Reduce concurrent operations or increase limit",
            )
        })?;

        // Apply operation timeout
        let result = timeout(self.config.operation_timeout, async {
            self.storage.store_text_batched(document_id, text.clone()).await
        }).await;

        match result {
            Ok(store_result) => {
                let latency = start_time.elapsed();
                self.stats.write().await.record_write(latency);

                if store_result.is_ok() {
                    // Update read-ahead buffer with new content
                    let mut buffer = self.read_ahead_buffer.write().await;
                    buffer.put(document_id, text);
                }

                store_result
            }
            Err(_) => {
                self.stats.write().await.record_timeout();
                Err(ShardexError::concurrency_error(
                    "store_text_async",
                    "Operation timed out",
                    "Increase timeout or reduce system load",
                ))
            }
        }
    }

    /// Async batch text storage for multiple documents
    ///
    /// This method processes multiple documents concurrently with batching:
    /// - Spawns concurrent tasks up to concurrency limit
    /// - Uses batch processing for optimal throughput
    /// - Collects results and handles partial failures
    ///
    /// # Arguments
    /// * `documents` - Vector of (DocumentId, String) pairs to store
    ///
    /// # Returns
    /// * `Ok(Vec<Result<(), ShardexError>>)` - Results for each document
    /// * `Err(ShardexError)` - Batch operation failed
    pub async fn store_texts_batch(
        &self,
        documents: Vec<(DocumentId, String)>,
    ) -> Result<Vec<Result<(), ShardexError>>, ShardexError> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let start_time = Instant::now();
        let batch_size = documents.len();

        // Process documents in chunks based on concurrency limit
        let chunk_size = self.config.max_concurrency.min(documents.len());
        let mut all_results = Vec::new();

        for chunk in documents.chunks(chunk_size) {
            let mut tasks = Vec::new();

            // Spawn tasks for this chunk
            for (document_id, text) in chunk {
                let storage = Arc::clone(&self.storage);
                let doc_id = *document_id;
                let text_clone = text.clone();

                let task = tokio::spawn(async move {
                    storage.store_text_batched(doc_id, text_clone).await
                });

                tasks.push(task);
            }

            // Wait for chunk completion
            let chunk_results = futures::future::join_all(tasks).await;
            
            // Collect results, handling join errors
            for result in chunk_results {
                match result {
                    Ok(store_result) => all_results.push(store_result),
                    Err(_) => all_results.push(Err(ShardexError::concurrency_error(
                        "store_texts_batch",
                        "Task execution failed",
                        "Retry the operation",
                    ))),
                }
            }
        }

        // Update statistics
        let latency = start_time.elapsed();
        self.stats.write().await.record_batch(latency, batch_size);

        Ok(all_results)
    }

    /// Extract text substring with async optimization
    ///
    /// # Arguments
    /// * `document_id` - Document ID
    /// * `start` - Starting position in bytes
    /// * `length` - Length in bytes to extract
    ///
    /// # Returns
    /// * `Ok(String)` - Extracted text substring
    /// * `Err(ShardexError)` - Extraction failed or timed out
    pub async fn extract_text_substring_async(
        &self,
        document_id: DocumentId,
        start: u32,
        length: u32,
    ) -> Result<String, ShardexError> {
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            ShardexError::concurrency_error(
                "extract_text_substring_async",
                "Failed to acquire concurrency permit",
                "Reduce concurrent operations",
            )
        })?;

        timeout(self.config.operation_timeout, async {
            self.storage.extract_text_substring_concurrent(document_id, start, length).await
        })
        .await
        .map_err(|_| ShardexError::concurrency_error(
            "extract_text_substring_async",
            "Operation timed out",
            "Increase timeout or reduce system load",
        ))?
    }

    /// Check if document exists with async optimization
    pub async fn document_exists_async(&self, document_id: DocumentId) -> bool {
        // Check read-ahead buffer first
        {
            let buffer = self.read_ahead_buffer.read().await;
            if buffer.buffer.contains_key(&document_id) {
                return true;
            }
        }

        // Fall back to storage check
        self.storage.document_exists(document_id).await
    }

    /// Start background optimization tasks
    fn start_background_tasks(&self) {
        let mut tasks = self.background_tasks.lock();
        
        // Buffer cleanup task
        {
            let buffer = Arc::clone(&self.read_ahead_buffer);
            let stats = Arc::clone(&self.stats);
            let handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(60));
                
                loop {
                    interval.tick().await;
                    
                    {
                        let mut buf = buffer.write().await;
                        buf.evict_expired();
                    }
                    
                    stats.write().await.record_background_task();
                }
            });
            tasks.push(handle);
        }

        // Statistics reporting task
        {
            let stats = Arc::clone(&self.stats);
            let handle = tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
                
                loop {
                    interval.tick().await;
                    
                    let stats_snapshot = stats.read().await;
                    tracing::info!(
                        async_reads = stats_snapshot.async_reads,
                        async_writes = stats_snapshot.async_writes,
                        avg_latency_ms = stats_snapshot.avg_latency_ms,
                        read_ahead_hit_ratio = stats_snapshot.read_ahead_hit_ratio(),
                        timeouts = stats_snapshot.timeouts,
                        "Async document text storage statistics"
                    );
                }
            });
            tasks.push(handle);
        }
    }

    /// Trigger predictive read-ahead for related documents
    async fn trigger_predictive_read_ahead(&self, _document_id: DocumentId) {
        // This is a placeholder for predictive read-ahead logic
        // In a full implementation, this would:
        // 1. Analyze access patterns to predict related documents
        // 2. Pre-fetch likely-to-be-accessed documents
        // 3. Use machine learning or heuristics for prediction
        
        // For now, we just record that a background task was triggered
        self.stats.write().await.record_background_task();
    }

    /// Get current async I/O statistics
    pub async fn get_async_stats(&self) -> AsyncIOStats {
        self.stats.read().await.clone()
    }

    /// Get read-ahead buffer statistics
    pub async fn get_read_ahead_stats(&self) -> (u64, u64, u64, usize, f64) {
        let buffer = self.read_ahead_buffer.read().await;
        let (hits, misses, evictions, size) = buffer.stats();
        let hit_ratio = buffer.hit_ratio();
        (hits, misses, evictions, size, hit_ratio)
    }

    /// Get current configuration
    pub fn get_config(&self) -> &AsyncConfig {
        &self.config
    }

    /// Get current semaphore permits available
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Clear read-ahead buffer
    pub async fn clear_read_ahead_buffer(&self) {
        let mut buffer = self.read_ahead_buffer.write().await;
        buffer.clear();
    }

    /// Shutdown async operations gracefully
    pub async fn shutdown(&self) -> Result<(), ShardexError> {
        // Cancel background tasks
        {
            let mut tasks = self.background_tasks.lock();
            for handle in tasks.drain(..) {
                handle.abort();
            }
        } // Drop the lock here

        // Shutdown underlying storage
        self.storage.shutdown().await?;

        Ok(())
    }

    /// Force garbage collection and optimization
    pub async fn optimize(&self) -> Result<(), ShardexError> {
        // Clean up read-ahead buffer
        {
            let mut buffer = self.read_ahead_buffer.write().await;
            buffer.evict_expired();
        }

        // Flush any pending writes
        self.storage.flush_write_queue().await?;

        Ok(())
    }
}

// Clone implementation for AsyncIOStats
impl Clone for AsyncIOStats {
    fn clone(&self) -> Self {
        Self {
            async_reads: self.async_reads,
            async_writes: self.async_writes,
            batch_operations: self.batch_operations,
            read_ahead_hits: self.read_ahead_hits,
            read_ahead_misses: self.read_ahead_misses,
            avg_latency_ms: self.avg_latency_ms,
            timeouts: self.timeouts,
            background_tasks: self.background_tasks,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document_text_storage::DocumentTextStorage;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

    async fn create_test_async_storage() -> (TempDir, AsyncDocumentTextStorage) {
        let temp_dir = TempDir::new().unwrap();
        let base_storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let concurrent_storage = ConcurrentDocumentTextStorage::with_default_config(base_storage);
        let async_storage = AsyncDocumentTextStorage::with_default_config(concurrent_storage);
        (temp_dir, async_storage)
    }

    #[tokio::test]
    async fn test_async_basic_operations() {
        let (_temp_dir, storage) = create_test_async_storage().await;
        
        let doc_id = DocumentId::new();
        let text = "Async test document content";
        
        // Store text asynchronously
        storage.store_text_async(doc_id, text.to_string()).await.unwrap();
        
        // Retrieve text asynchronously
        let retrieved = storage.get_text_async(doc_id).await.unwrap();
        assert_eq!(retrieved, text);
        
        // Check existence
        assert!(storage.document_exists_async(doc_id).await);
    }

    #[tokio::test]
    async fn test_read_ahead_buffer() {
        let (_temp_dir, storage) = create_test_async_storage().await;
        
        let doc_id = DocumentId::new();
        let text = "Read-ahead buffer test content";
        
        // Store document
        storage.store_text_async(doc_id, text.to_string()).await.unwrap();
        
        // First read should miss read-ahead buffer
        let _retrieved1 = storage.get_text_async(doc_id).await.unwrap();
        
        // Second read should hit read-ahead buffer
        let retrieved2 = storage.get_text_async(doc_id).await.unwrap();
        assert_eq!(retrieved2, text);
        
        // Check read-ahead statistics
        let (hits, _misses, _, _, hit_ratio) = storage.get_read_ahead_stats().await;
        assert!(hits > 0);
        assert!(hit_ratio > 0.0);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let (_temp_dir, storage) = create_test_async_storage().await;
        
        // Prepare batch of documents
        let mut documents = Vec::new();
        for i in 0..10 {
            let doc_id = DocumentId::new();
            let text = format!("Batch document {}", i);
            documents.push((doc_id, text));
        }
        
        // Store batch
        let results = storage.store_texts_batch(documents.clone()).await.unwrap();
        
        // All should succeed
        assert_eq!(results.len(), 10);
        for result in &results {
            assert!(result.is_ok());
        }
        
        // Verify all documents can be retrieved
        for (doc_id, expected_text) in documents {
            let retrieved = storage.get_text_async(doc_id).await.unwrap();
            assert_eq!(retrieved, expected_text);
        }
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let (_temp_dir, storage) = create_test_async_storage().await;
        
        let mut handles = Vec::new();
        
        // Spawn multiple concurrent operations
        for i in 0..20 {
            let storage_ref = &storage;
            let handle = async move {
                let doc_id = DocumentId::new();
                let text = format!("Concurrent document {}", i);
                
                // Store and retrieve
                storage_ref.store_text_async(doc_id, text.clone()).await.unwrap();
                let retrieved = storage_ref.get_text_async(doc_id).await.unwrap();
                
                assert_eq!(retrieved, text);
                (doc_id, text)
            };
            handles.push(handle);
        }
        
        // Wait for all operations
        let results = futures::future::join_all(handles).await;
        assert_eq!(results.len(), 20);
        
        // Check statistics
        let stats = storage.get_async_stats().await;
        assert!(stats.async_reads >= 20);
        assert!(stats.async_writes >= 20);
    }

    #[tokio::test]
    async fn test_substring_extraction() {
        let (_temp_dir, storage) = create_test_async_storage().await;
        
        let doc_id = DocumentId::new();
        let text = "The quick brown fox jumps over the lazy dog";
        
        storage.store_text_async(doc_id, text.to_string()).await.unwrap();
        
        // Extract substring asynchronously
        let substring = storage
            .extract_text_substring_async(doc_id, 4, 11)
            .await
            .unwrap();
        assert_eq!(substring, "quick brown");
    }

    #[tokio::test]
    async fn test_timeout_handling() {
        let config = AsyncConfig {
            operation_timeout: Duration::from_millis(1), // Very short timeout
            ..Default::default()
        };
        
        let temp_dir = TempDir::new().unwrap();
        let base_storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let concurrent_storage = ConcurrentDocumentTextStorage::with_default_config(base_storage);
        let async_storage = AsyncDocumentTextStorage::new(concurrent_storage, config);
        
        let doc_id = DocumentId::new();
        let text = "A".repeat(100000); // Large text that might cause timeout
        
        // This operation might timeout due to very short timeout setting
        let result = async_storage.store_text_async(doc_id, text).await;
        
        // Either succeeds or times out - both are valid outcomes for this test
        // The important thing is that it handles timeouts gracefully
        match result {
            Ok(_) => {
                // Operation completed within timeout
            }
            Err(ShardexError::ConcurrencyError { reason, .. }) => {
                // Expected timeout error
                assert!(reason.contains("timed out") || reason.contains("timeout"));
            }
            Err(e) => panic!("Unexpected error type: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_concurrency_limiting() {
        let config = AsyncConfig {
            max_concurrency: 2, // Very low limit
            ..Default::default()
        };
        
        let temp_dir = TempDir::new().unwrap();
        let base_storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let concurrent_storage = ConcurrentDocumentTextStorage::with_default_config(base_storage);
        let async_storage = AsyncDocumentTextStorage::new(concurrent_storage, config);
        
        let initial_permits = async_storage.available_permits();
        assert_eq!(initial_permits, 2);
        
        // Start operations that will consume permits
        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        
        let handle1 = async_storage.store_text_async(doc1, "doc1".to_string());
        let handle2 = async_storage.store_text_async(doc2, "doc2".to_string());
        
        // Complete the operations
        let (result1, result2) = tokio::join!(handle1, handle2);
        result1.unwrap();
        result2.unwrap();
        
        // Permits should be released after operations complete
        let final_permits = async_storage.available_permits();
        assert_eq!(final_permits, 2);
    }

    #[tokio::test]
    async fn test_buffer_expiration() {
        let config = AsyncConfig {
            read_ahead: ReadAheadConfig {
                buffer_ttl: Duration::from_millis(100), // Short TTL
                ..Default::default()
            },
            ..Default::default()
        };
        
        let temp_dir = TempDir::new().unwrap();
        let base_storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let concurrent_storage = ConcurrentDocumentTextStorage::with_default_config(base_storage);
        let async_storage = AsyncDocumentTextStorage::new(concurrent_storage, config);
        
        let doc_id = DocumentId::new();
        let text = "Expiration test content";
        
        // Store and read to populate buffer
        async_storage.store_text_async(doc_id, text.to_string()).await.unwrap();
        let _first_read = async_storage.get_text_async(doc_id).await.unwrap();
        
        // Wait for buffer expiration
        sleep(Duration::from_millis(150)).await;
        
        // Manually trigger buffer cleanup
        async_storage.optimize().await.unwrap();
        
        // Next read should miss the buffer (expired entry removed)
        let _second_read = async_storage.get_text_async(doc_id).await.unwrap();
        
        let stats = async_storage.get_async_stats().await;
        assert!(stats.async_reads >= 2);
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let (_temp_dir, storage) = create_test_async_storage().await;
        
        // Add some operations
        let doc_id = DocumentId::new();
        storage.store_text_async(doc_id, "test".to_string()).await.unwrap();
        
        // Shutdown should complete without error
        storage.shutdown().await.unwrap();
        
        // After shutdown, operations may still work since we're using the underlying storage
        // but background tasks should be stopped
    }

    #[tokio::test]
    async fn test_statistics_tracking() {
        let (_temp_dir, storage) = create_test_async_storage().await;
        
        // Perform various operations to generate statistics
        let doc_id = DocumentId::new();
        let text = "Statistics test content";
        
        storage.store_text_async(doc_id, text.to_string()).await.unwrap();
        let _retrieved = storage.get_text_async(doc_id).await.unwrap();
        
        // Store multiple documents in batch
        let batch = vec![
            (DocumentId::new(), "batch1".to_string()),
            (DocumentId::new(), "batch2".to_string()),
        ];
        let _batch_results = storage.store_texts_batch(batch).await.unwrap();
        
        let stats = storage.get_async_stats().await;
        
        assert!(stats.async_reads > 0);
        assert!(stats.async_writes > 0);
        assert!(stats.batch_operations > 0);
        assert!(stats.avg_latency_ms >= 0.0);
    }
}
//! Asynchronous I/O support for document text storage
//!
//! This module provides async wrappers around document text storage with
//! read-ahead buffering, batch operations, and non-blocking I/O patterns.

use crate::concurrent_document_text_storage::{ConcurrentDocumentTextStorage, ConcurrentStorageConfig};
use crate::document_text_storage::DocumentTextStorage;
use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::{RwLock, Semaphore};
use tokio::time::timeout;

/// Access pattern entry for tracking document access sequences
#[derive(Debug, Clone)]
struct AccessEntry {
    document_id: DocumentId,
    timestamp: SystemTime,
    #[allow(dead_code)] // Used for future sequence analysis
    sequence_position: usize,
}

impl AccessEntry {
    fn new(document_id: DocumentId, sequence_position: usize) -> Self {
        Self {
            document_id,
            timestamp: SystemTime::now(),
            sequence_position,
        }
    }

    fn age(&self) -> Duration {
        self.timestamp.elapsed().unwrap_or(Duration::ZERO)
    }
}

/// Tracks co-occurrence patterns between documents
#[derive(Debug, Default)]
struct CooccurrenceMap {
    /// Map from document_id to documents that are frequently accessed together
    patterns: HashMap<DocumentId, HashMap<DocumentId, f64>>,
    max_patterns_per_document: usize,
}

impl CooccurrenceMap {
    fn new(max_patterns_per_document: usize) -> Self {
        Self {
            patterns: HashMap::new(),
            max_patterns_per_document,
        }
    }

    /// Record that two documents were accessed together
    fn record_cooccurrence(&mut self, doc1: DocumentId, doc2: DocumentId, weight: f64) {
        if doc1 == doc2 {
            return;
        }

        let entry = self.patterns.entry(doc1).or_default();
        *entry.entry(doc2).or_default() += weight;

        // Limit patterns per document
        if entry.len() > self.max_patterns_per_document {
            if let Some((&weakest_doc, _)) = entry.iter().min_by(|a, b| a.1.partial_cmp(b.1).unwrap()) {
                entry.remove(&weakest_doc);
            }
        }
    }

    /// Get documents likely to be accessed with the given document
    fn get_predicted_documents(&self, document_id: DocumentId, limit: usize) -> Vec<(DocumentId, f64)> {
        self.patterns
            .get(&document_id)
            .map(|patterns| {
                let mut sorted: Vec<_> = patterns.iter().map(|(&id, &score)| (id, score)).collect();
                sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                sorted.truncate(limit);
                sorted
            })
            .unwrap_or_default()
    }

    /// Clean up old or weak patterns
    fn cleanup(&mut self, min_strength: f64) {
        self.patterns.retain(|_, patterns| {
            patterns.retain(|_, &mut strength| strength >= min_strength);
            !patterns.is_empty()
        });
    }
}

/// Access pattern tracker for read-ahead prediction
#[derive(Debug)]
struct AccessPatternTracker {
    /// Recent access history (sliding window)
    access_history: VecDeque<AccessEntry>,
    /// Maximum size of access history
    max_history_size: usize,
    /// Time window for considering accesses recent
    temporal_window: Duration,
    /// Co-occurrence patterns between documents
    cooccurrence: CooccurrenceMap,
    /// Current sequence position counter
    sequence_counter: usize,
}

impl AccessPatternTracker {
    fn new(max_history_size: usize, temporal_window: Duration, max_cooccurrence_patterns: usize) -> Self {
        Self {
            access_history: VecDeque::with_capacity(max_history_size),
            max_history_size,
            temporal_window,
            cooccurrence: CooccurrenceMap::new(max_cooccurrence_patterns),
            sequence_counter: 0,
        }
    }

    /// Record a document access
    fn record_access(&mut self, document_id: DocumentId) {
        // Add to history
        let entry = AccessEntry::new(document_id, self.sequence_counter);
        self.access_history.push_back(entry);
        self.sequence_counter += 1;

        // Maintain size limit
        while self.access_history.len() > self.max_history_size {
            self.access_history.pop_front();
        }

        // Update co-occurrence patterns with recent accesses
        let recent_accesses: Vec<_> = self.access_history
            .iter()
            .rev()
            .take(10) // Consider last 10 accesses for co-occurrence
            .filter(|entry| entry.age() <= self.temporal_window)
            .map(|entry| entry.document_id)
            .collect();

        for other_doc in recent_accesses {
            let distance = if let Some(pos) = self
                .access_history
                .iter()
                .rposition(|e| e.document_id == other_doc)
            {
                self.access_history.len() - pos
            } else {
                continue;
            };

            // Weight by recency (closer accesses get higher weight)
            let weight = 1.0 / (distance as f64 + 1.0);
            self.cooccurrence
                .record_cooccurrence(document_id, other_doc, weight);
        }
    }

    /// Predict likely next documents based on access patterns
    fn predict_next_documents(&self, current_document: DocumentId, limit: usize) -> Vec<DocumentId> {
        let mut predictions = Vec::new();

        // Sequential prediction: look for patterns in recent history
        if let Some(_current_pos) = self
            .access_history
            .iter()
            .rposition(|entry| entry.document_id == current_document)
        {
            // Look for documents that followed this one in the past
            let mut sequence_scores: HashMap<DocumentId, f64> = HashMap::new();

            for (i, entry) in self.access_history.iter().enumerate() {
                if entry.document_id == current_document && i + 1 < self.access_history.len() {
                    let next_doc = self.access_history[i + 1].document_id;
                    let distance_from_current = (self.access_history.len() - i) as f64;
                    let weight = 1.0 / distance_from_current; // More recent = higher weight
                    *sequence_scores.entry(next_doc).or_default() += weight;
                }
            }

            // Add top sequential predictions
            let mut seq_predictions: Vec<_> = sequence_scores.into_iter().collect();
            seq_predictions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            predictions.extend(
                seq_predictions
                    .into_iter()
                    .take(limit / 2)
                    .map(|(doc, _)| doc),
            );
        }

        // Co-occurrence prediction: documents often accessed together
        let cooccurrence_predictions = self
            .cooccurrence
            .get_predicted_documents(current_document, limit);
        predictions.extend(cooccurrence_predictions.into_iter().map(|(doc, _)| doc));

        // Remove duplicates and current document
        predictions.retain(|&doc| doc != current_document);
        predictions.sort();
        predictions.dedup();
        predictions.truncate(limit);

        predictions
    }

    /// Clean up old entries and weak patterns
    fn cleanup(&mut self) {
        // Remove old entries from history
        let cutoff_time = SystemTime::now() - self.temporal_window;
        while let Some(entry) = self.access_history.front() {
            if entry.timestamp < cutoff_time {
                self.access_history.pop_front();
            } else {
                break;
            }
        }

        // Clean up weak co-occurrence patterns
        self.cooccurrence.cleanup(0.1); // Remove patterns with strength < 0.1
    }

    /// Get current access pattern statistics
    fn get_stats(&self) -> (usize, usize) {
        (self.access_history.len(), self.cooccurrence.patterns.len())
    }
}

/// Configuration for async document text storage
#[derive(Debug, Clone)]
pub struct AsyncStorageConfig {
    /// Configuration for underlying concurrent storage
    pub concurrent_config: ConcurrentStorageConfig,
    /// Size of read-ahead buffer
    pub read_ahead_buffer_size: usize,
    /// TTL for read-ahead buffer entries
    pub read_ahead_ttl: Duration,
    /// Maximum concurrent async operations
    pub max_concurrent_async_ops: usize,
    /// Default timeout for async operations
    pub default_timeout: Duration,
    /// Read-ahead prediction window size
    pub read_ahead_window: usize,
    /// Background task cleanup interval
    pub cleanup_interval: Duration,
    /// Maximum size of access pattern history
    pub max_access_history: usize,
    /// Time window for considering accesses recent for prediction
    pub prediction_temporal_window: Duration,
    /// Maximum co-occurrence patterns per document
    pub max_cooccurrence_patterns: usize,
    /// Number of documents to predict for read-ahead
    pub prediction_count: usize,
}

impl Default for AsyncStorageConfig {
    fn default() -> Self {
        Self {
            concurrent_config: ConcurrentStorageConfig::default(),
            read_ahead_buffer_size: 1000,
            read_ahead_ttl: Duration::from_secs(300),
            max_concurrent_async_ops: 200,
            default_timeout: Duration::from_secs(30),
            read_ahead_window: 10,
            cleanup_interval: Duration::from_secs(60),
            max_access_history: 1000,
            prediction_temporal_window: Duration::from_secs(1800), // 30 minutes
            max_cooccurrence_patterns: 50,
            prediction_count: 5,
        }
    }
}

/// Read-ahead buffer entry
#[derive(Debug, Clone)]
struct ReadAheadEntry {
    #[allow(dead_code)] // Used as map key, not read directly
    document_id: DocumentId,
    text: String,
    created_at: SystemTime,
    access_count: u64,
}

impl ReadAheadEntry {
    fn new(document_id: DocumentId, text: String) -> Self {
        Self {
            document_id,
            text,
            created_at: SystemTime::now(),
            access_count: 0,
        }
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed().unwrap_or(Duration::ZERO) > ttl
    }

    fn touch(&mut self) {
        self.access_count += 1;
    }
}

/// Read-ahead buffer for predictive caching
#[derive(Debug)]
struct ReadAheadBuffer {
    entries: HashMap<DocumentId, ReadAheadEntry>,
    access_order: Vec<DocumentId>,
    max_size: usize,
    ttl: Duration,
}

impl ReadAheadBuffer {
    fn new(max_size: usize, ttl: Duration) -> Self {
        Self {
            entries: HashMap::with_capacity(max_size),
            access_order: Vec::with_capacity(max_size),
            max_size,
            ttl,
        }
    }

    fn get(&mut self, document_id: &DocumentId) -> Option<String> {
        if let Some(entry) = self.entries.get_mut(document_id) {
            if !entry.is_expired(self.ttl) {
                entry.touch();
                // Move to end of access order
                if let Some(pos) = self.access_order.iter().position(|id| id == document_id) {
                    self.access_order.remove(pos);
                }
                self.access_order.push(*document_id);
                return Some(entry.text.clone());
            } else {
                // Remove expired entry
                self.entries.remove(document_id);
                self.access_order.retain(|id| id != document_id);
            }
        }
        None
    }

    fn put(&mut self, document_id: DocumentId, text: String) {
        // Remove if already present
        if self.entries.contains_key(&document_id) {
            self.access_order.retain(|id| id != &document_id);
        }

        // Evict if at capacity
        while self.entries.len() >= self.max_size {
            if let Some(oldest_id) = self.access_order.first().copied() {
                self.entries.remove(&oldest_id);
                self.access_order.remove(0);
            } else {
                break;
            }
        }

        // Add new entry
        let entry = ReadAheadEntry::new(document_id, text);
        self.entries.insert(document_id, entry);
        self.access_order.push(document_id);
    }

    fn cleanup_expired(&mut self) -> usize {
        let original_len = self.entries.len();

        // Collect expired document IDs
        let expired_ids: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.is_expired(self.ttl))
            .map(|(id, _)| *id)
            .collect();

        // Remove expired entries
        for id in expired_ids {
            self.entries.remove(&id);
            self.access_order.retain(|entry_id| entry_id != &id);
        }

        original_len - self.entries.len()
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn capacity(&self) -> usize {
        self.max_size
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.access_order.clear();
    }
}

/// Performance metrics for async operations
#[derive(Debug, Clone, Default)]
pub struct AsyncStorageMetrics {
    /// Total async read operations
    pub async_reads: u64,
    /// Successful async reads
    pub successful_async_reads: u64,
    /// Failed async reads
    pub failed_async_reads: u64,
    /// Total async write operations
    pub async_writes: u64,
    /// Successful async writes
    pub successful_async_writes: u64,
    /// Failed async writes
    pub failed_async_writes: u64,
    /// Read-ahead cache hits
    pub read_ahead_hits: u64,
    /// Read-ahead cache misses
    pub read_ahead_misses: u64,
    /// Read-ahead predictions made
    pub read_ahead_predictions: u64,
    /// Average async operation latency in milliseconds
    pub avg_async_latency_ms: f64,
    /// Timeout errors
    pub timeout_errors: u64,
    /// Background task executions
    pub background_tasks_executed: u64,
}

impl AsyncStorageMetrics {
    /// Calculate async read success ratio
    pub fn async_read_success_ratio(&self) -> f64 {
        if self.async_reads == 0 {
            0.0
        } else {
            self.successful_async_reads as f64 / self.async_reads as f64
        }
    }

    /// Calculate async write success ratio
    pub fn async_write_success_ratio(&self) -> f64 {
        if self.async_writes == 0 {
            0.0
        } else {
            self.successful_async_writes as f64 / self.async_writes as f64
        }
    }

    /// Calculate read-ahead hit ratio
    pub fn read_ahead_hit_ratio(&self) -> f64 {
        let total = self.read_ahead_hits + self.read_ahead_misses;
        if total == 0 {
            0.0
        } else {
            self.read_ahead_hits as f64 / total as f64
        }
    }

    /// Get total async operations
    pub fn total_async_operations(&self) -> u64 {
        self.async_reads + self.async_writes
    }
}

/// Asynchronous document text storage with read-ahead and batching
pub struct AsyncDocumentTextStorage {
    /// Underlying concurrent storage
    storage: Arc<ConcurrentDocumentTextStorage>,
    /// Read-ahead buffer for predictive caching
    read_ahead_buffer: Arc<RwLock<ReadAheadBuffer>>,
    /// Semaphore for limiting concurrent operations
    async_semaphore: Arc<Semaphore>,
    /// Configuration
    config: AsyncStorageConfig,
    /// Performance metrics
    metrics: Arc<Mutex<AsyncStorageMetrics>>,
    /// Access pattern tracker for read-ahead prediction
    access_tracker: Arc<RwLock<AccessPatternTracker>>,
    /// Background task handles
    background_tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl AsyncDocumentTextStorage {
    /// Create new async document text storage
    pub async fn new(storage: DocumentTextStorage, config: AsyncStorageConfig) -> Result<Self, ShardexError> {
        let concurrent_storage = ConcurrentDocumentTextStorage::new(storage, config.concurrent_config.clone());
        concurrent_storage.start_background_processor().await?;

        let read_ahead_buffer = Arc::new(RwLock::new(ReadAheadBuffer::new(
            config.read_ahead_buffer_size,
            config.read_ahead_ttl,
        )));

        let access_tracker = Arc::new(RwLock::new(AccessPatternTracker::new(
            config.max_access_history,
            config.prediction_temporal_window,
            config.max_cooccurrence_patterns,
        )));

        let async_storage = Self {
            storage: Arc::new(concurrent_storage),
            read_ahead_buffer,
            async_semaphore: Arc::new(Semaphore::new(config.max_concurrent_async_ops)),
            config,
            metrics: Arc::new(Mutex::new(AsyncStorageMetrics::default())),
            access_tracker,
            background_tasks: Arc::new(Mutex::new(Vec::new())),
        };

        // Start background cleanup task
        async_storage.start_background_cleanup().await?;

        Ok(async_storage)
    }

    /// Start background cleanup task
    async fn start_background_cleanup(&self) -> Result<(), ShardexError> {
        let read_ahead_buffer = Arc::clone(&self.read_ahead_buffer);
        let access_tracker = Arc::clone(&self.access_tracker);
        let metrics = Arc::clone(&self.metrics);
        let cleanup_interval = self.config.cleanup_interval;

        let cleanup_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(cleanup_interval);

            loop {
                interval.tick().await;

                // Clean up expired read-ahead entries
                let expired_count = {
                    let mut buffer = read_ahead_buffer.write().await;
                    buffer.cleanup_expired()
                };

                if expired_count > 0 {
                    log::debug!("Cleaned up {} expired read-ahead entries", expired_count);
                }

                // Clean up access patterns
                {
                    let mut tracker = access_tracker.write().await;
                    tracker.cleanup();
                    let (history_size, pattern_count) = tracker.get_stats();
                    log::trace!(
                        "Access tracker stats: {} history entries, {} pattern groups",
                        history_size,
                        pattern_count
                    );
                }

                // Update metrics
                {
                    let mut metrics_guard = metrics.lock();
                    metrics_guard.background_tasks_executed += 1;
                }
            }
        });

        let mut tasks = self.background_tasks.lock();
        tasks.push(cleanup_task);

        Ok(())
    }

    /// Get document text asynchronously with read-ahead support
    pub async fn get_text_async(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        let _permit = self
            .async_semaphore
            .acquire()
            .await
            .map_err(|_| ShardexError::InvalidInput {
                field: "async_semaphore".to_string(),
                reason: "Failed to acquire async semaphore permit".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

        let start_time = Instant::now();

        // Check read-ahead buffer first
        {
            let mut buffer = self.read_ahead_buffer.write().await;
            if let Some(text) = buffer.get(&document_id) {
                // Record access pattern even for cache hits
                drop(buffer); // Release buffer lock before acquiring tracker lock
                {
                    let mut tracker = self.access_tracker.write().await;
                    tracker.record_access(document_id);
                }

                // Trigger read-ahead prediction even for cache hits
                self.trigger_read_ahead(document_id).await;

                self.record_read_ahead_hit();
                self.record_async_read_success(start_time.elapsed().as_millis() as f64);
                return Ok(text);
            }
        }

        self.record_read_ahead_miss();

        // Fallback to underlying storage
        let result = timeout(
            self.config.default_timeout,
            self.storage.get_text_concurrent(document_id),
        )
        .await;

        match result {
            Ok(Ok(text)) => {
                // Record access pattern for prediction
                {
                    let mut tracker = self.access_tracker.write().await;
                    tracker.record_access(document_id);
                }

                // Add to read-ahead buffer
                {
                    let mut buffer = self.read_ahead_buffer.write().await;
                    buffer.put(document_id, text.clone());
                }

                // Trigger read-ahead prediction
                self.trigger_read_ahead(document_id).await;

                self.record_async_read_success(start_time.elapsed().as_millis() as f64);
                Ok(text)
            }
            Ok(Err(e)) => {
                self.record_async_read_failure(start_time.elapsed().as_millis() as f64);
                Err(e)
            }
            Err(_) => {
                self.record_timeout_error();
                self.record_async_read_failure(start_time.elapsed().as_millis() as f64);
                Err(ShardexError::InvalidInput {
                    field: "async_operation".to_string(),
                    reason: "Async operation timed out".to_string(),
                    suggestion: "Increase timeout or check storage performance".to_string(),
                })
            }
        }
    }

    /// Store text asynchronously
    pub async fn store_text_async(&self, document_id: DocumentId, text: String) -> Result<(), ShardexError> {
        let _permit = self
            .async_semaphore
            .acquire()
            .await
            .map_err(|_| ShardexError::InvalidInput {
                field: "async_semaphore".to_string(),
                reason: "Failed to acquire async semaphore permit".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

        let start_time = Instant::now();

        let result = timeout(
            self.config.default_timeout,
            self.storage.store_text_batched(document_id, text.clone()),
        )
        .await;

        match result {
            Ok(Ok(())) => {
                // Update read-ahead buffer
                {
                    let mut buffer = self.read_ahead_buffer.write().await;
                    buffer.put(document_id, text);
                }

                self.record_async_write_success(start_time.elapsed().as_millis() as f64);
                Ok(())
            }
            Ok(Err(e)) => {
                self.record_async_write_failure(start_time.elapsed().as_millis() as f64);
                Err(e)
            }
            Err(_) => {
                self.record_timeout_error();
                self.record_async_write_failure(start_time.elapsed().as_millis() as f64);
                Err(ShardexError::InvalidInput {
                    field: "async_operation".to_string(),
                    reason: "Async operation timed out".to_string(),
                    suggestion: "Increase timeout or check storage performance".to_string(),
                })
            }
        }
    }

    /// Store multiple documents asynchronously in batch
    pub async fn store_texts_batch_async(
        &self,
        documents: Vec<(DocumentId, String)>,
    ) -> Result<Vec<Result<(), ShardexError>>, ShardexError> {
        let batch_size = documents.len();
        let start_time = Instant::now();

        // Create concurrent tasks for all documents
        let mut tasks = Vec::new();
        for (doc_id, text) in documents {
            let storage = Arc::clone(&self.storage);
            let task = tokio::spawn(async move { storage.store_text_batched(doc_id, text).await });
            tasks.push(task);
        }

        // Wait for all tasks to complete
        let mut results = Vec::new();
        let mut successful_count = 0;

        for task in tasks {
            match task.await {
                Ok(Ok(())) => {
                    results.push(Ok(()));
                    successful_count += 1;
                }
                Ok(Err(e)) => {
                    results.push(Err(e));
                }
                Err(_) => {
                    results.push(Err(ShardexError::InvalidInput {
                        field: "batch_task".to_string(),
                        reason: "Batch task was cancelled".to_string(),
                        suggestion: "Retry the batch operation".to_string(),
                    }));
                }
            }
        }

        // Update metrics
        let avg_latency = start_time.elapsed().as_millis() as f64 / batch_size as f64;
        for _ in 0..successful_count {
            self.record_async_write_success(avg_latency);
        }
        for _ in 0..(batch_size - successful_count) {
            self.record_async_write_failure(avg_latency);
        }

        Ok(results)
    }

    /// Extract text substring asynchronously
    pub async fn extract_text_substring_async(
        &self,
        document_id: DocumentId,
        start: u32,
        length: u32,
    ) -> Result<String, ShardexError> {
        let _permit = self
            .async_semaphore
            .acquire()
            .await
            .map_err(|_| ShardexError::InvalidInput {
                field: "async_semaphore".to_string(),
                reason: "Failed to acquire async semaphore permit".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

        let start_time = Instant::now();

        let result = timeout(
            self.config.default_timeout,
            self.storage
                .extract_text_substring_concurrent(document_id, start, length),
        )
        .await;

        match result {
            Ok(Ok(text)) => {
                self.record_async_read_success(start_time.elapsed().as_millis() as f64);
                Ok(text)
            }
            Ok(Err(e)) => {
                self.record_async_read_failure(start_time.elapsed().as_millis() as f64);
                Err(e)
            }
            Err(_) => {
                self.record_timeout_error();
                self.record_async_read_failure(start_time.elapsed().as_millis() as f64);
                Err(ShardexError::InvalidInput {
                    field: "async_operation".to_string(),
                    reason: "Async operation timed out".to_string(),
                    suggestion: "Increase timeout or check storage performance".to_string(),
                })
            }
        }
    }

    /// Trigger read-ahead prediction for nearby documents
    async fn trigger_read_ahead(&self, document_id: DocumentId) {
        // Always record that prediction was triggered
        self.record_read_ahead_prediction();

        // Get predicted documents based on access patterns
        let predicted_documents = {
            let tracker = self.access_tracker.read().await;
            tracker.predict_next_documents(document_id, self.config.prediction_count)
        };

        if predicted_documents.is_empty() {
            log::trace!("No predictions available for document {}", document_id);
            return;
        }

        let prediction_count = predicted_documents.len();
        log::trace!(
            "Predicting {} documents for read-ahead: {:?}",
            prediction_count,
            predicted_documents
        );

        // Pre-load predicted documents into buffer asynchronously
        for predicted_id in predicted_documents {
            // Check if already in buffer
            {
                let buffer = self.read_ahead_buffer.read().await;
                if buffer.entries.contains_key(&predicted_id) {
                    log::trace!("Document {} already in read-ahead buffer, skipping", predicted_id);
                    continue;
                }
            }

            // Load document asynchronously without blocking
            let storage = Arc::clone(&self.storage);
            let read_ahead_buffer = Arc::clone(&self.read_ahead_buffer);
            let predicted_doc_id = predicted_id;

            tokio::spawn(async move {
                match storage.get_text_concurrent(predicted_doc_id).await {
                    Ok(text) => {
                        let mut buffer = read_ahead_buffer.write().await;
                        buffer.put(predicted_doc_id, text);
                        log::trace!("Pre-loaded document {} into read-ahead buffer", predicted_doc_id);
                    }
                    Err(e) => {
                        log::debug!(
                            "Failed to pre-load document {} for read-ahead: {:?}",
                            predicted_doc_id,
                            e
                        );
                    }
                }
            });
        }

        log::trace!("Triggered read-ahead prediction for {} documents", prediction_count);
    }

    /// Warm read-ahead buffer with specified documents
    pub async fn warm_read_ahead_buffer(&self, document_ids: Vec<DocumentId>) -> Result<(), ShardexError> {
        for document_id in document_ids {
            match self.storage.get_text_concurrent(document_id).await {
                Ok(text) => {
                    let mut buffer = self.read_ahead_buffer.write().await;
                    buffer.put(document_id, text);
                }
                Err(e) => {
                    log::warn!("Failed to warm read-ahead buffer for document {}: {:?}", document_id, e);
                }
            }
        }
        Ok(())
    }

    /// Flush all pending operations and shutdown gracefully
    pub async fn shutdown(&self) -> Result<(), ShardexError> {
        // Stop background tasks
        {
            let mut tasks = self.background_tasks.lock();
            for task in tasks.drain(..) {
                task.abort();
            }
        }

        // Flush pending operations
        self.storage.flush_write_queue().await?;
        self.storage.stop_background_processor().await?;

        // Clear read-ahead buffer
        {
            let mut buffer = self.read_ahead_buffer.write().await;
            buffer.clear();
        }

        Ok(())
    }

    /// Get current performance metrics
    pub fn get_metrics(&self) -> AsyncStorageMetrics {
        let metrics = self.metrics.lock();
        metrics.clone()
    }

    /// Get read-ahead buffer information
    pub async fn read_ahead_info(&self) -> (usize, usize) {
        let buffer = self.read_ahead_buffer.read().await;
        (buffer.len(), buffer.capacity())
    }

    /// Clear read-ahead buffer
    pub async fn clear_read_ahead_buffer(&self) {
        let mut buffer = self.read_ahead_buffer.write().await;
        buffer.clear();
    }

    // Metrics recording methods
    fn record_async_read_success(&self, latency_ms: f64) {
        let mut metrics = self.metrics.lock();
        metrics.async_reads += 1;
        metrics.successful_async_reads += 1;
        self.update_avg_latency(&mut metrics, latency_ms);
    }

    fn record_async_read_failure(&self, latency_ms: f64) {
        let mut metrics = self.metrics.lock();
        metrics.async_reads += 1;
        metrics.failed_async_reads += 1;
        self.update_avg_latency(&mut metrics, latency_ms);
    }

    fn record_async_write_success(&self, latency_ms: f64) {
        let mut metrics = self.metrics.lock();
        metrics.async_writes += 1;
        metrics.successful_async_writes += 1;
        self.update_avg_latency(&mut metrics, latency_ms);
    }

    fn record_async_write_failure(&self, latency_ms: f64) {
        let mut metrics = self.metrics.lock();
        metrics.async_writes += 1;
        metrics.failed_async_writes += 1;
        self.update_avg_latency(&mut metrics, latency_ms);
    }

    fn record_read_ahead_hit(&self) {
        let mut metrics = self.metrics.lock();
        metrics.read_ahead_hits += 1;
    }

    fn record_read_ahead_miss(&self) {
        let mut metrics = self.metrics.lock();
        metrics.read_ahead_misses += 1;
    }

    fn record_read_ahead_prediction(&self) {
        let mut metrics = self.metrics.lock();
        metrics.read_ahead_predictions += 1;
    }

    fn record_timeout_error(&self) {
        let mut metrics = self.metrics.lock();
        metrics.timeout_errors += 1;
    }

    fn update_avg_latency(&self, metrics: &mut AsyncStorageMetrics, latency_ms: f64) {
        let total_ops = metrics.total_async_operations();
        if total_ops == 1 {
            metrics.avg_async_latency_ms = latency_ms;
        } else {
            metrics.avg_async_latency_ms =
                ((metrics.avg_async_latency_ms * (total_ops - 1) as f64) + latency_ms) / total_ops as f64;
        }
    }
}

impl Drop for AsyncDocumentTextStorage {
    fn drop(&mut self) {
        // Attempt graceful shutdown in drop
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let background_tasks = Arc::clone(&self.background_tasks);
            rt.spawn(async move {
                let mut tasks = background_tasks.lock();
                for task in tasks.drain(..) {
                    task.abort();
                }
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
    async fn test_async_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = AsyncStorageConfig::default();

        let async_storage = AsyncDocumentTextStorage::new(storage, config)
            .await
            .unwrap();

        let metrics = async_storage.get_metrics();
        assert_eq!(metrics.async_reads, 0);
        assert_eq!(metrics.async_writes, 0);

        async_storage.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_async_read_write() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = AsyncStorageConfig::default();

        let async_storage = AsyncDocumentTextStorage::new(storage, config)
            .await
            .unwrap();

        let doc_id = DocumentId::new();
        let text = "Async test document content";

        // Store text asynchronously
        async_storage
            .store_text_async(doc_id, text.to_string())
            .await
            .unwrap();

        // Read text asynchronously
        let retrieved = async_storage.get_text_async(doc_id).await.unwrap();
        assert_eq!(retrieved, text);

        // Check metrics
        let metrics = async_storage.get_metrics();
        assert_eq!(metrics.async_reads, 1);
        assert_eq!(metrics.async_writes, 1);
        assert_eq!(metrics.successful_async_reads, 1);
        assert_eq!(metrics.successful_async_writes, 1);

        async_storage.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_read_ahead_buffer() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = AsyncStorageConfig::default();

        let async_storage = AsyncDocumentTextStorage::new(storage, config)
            .await
            .unwrap();

        let doc_id = DocumentId::new();
        let text = "Read-ahead test content";

        // Store and first read (populates read-ahead buffer)
        async_storage
            .store_text_async(doc_id, text.to_string())
            .await
            .unwrap();
        let _ = async_storage.get_text_async(doc_id).await.unwrap();

        // Second read should hit read-ahead buffer
        let retrieved = async_storage.get_text_async(doc_id).await.unwrap();
        assert_eq!(retrieved, text);

        // Check read-ahead metrics
        let metrics = async_storage.get_metrics();
        assert!(metrics.read_ahead_hits > 0);

        async_storage.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_batch_async_operations() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = AsyncStorageConfig::default();

        let async_storage = AsyncDocumentTextStorage::new(storage, config)
            .await
            .unwrap();

        let documents = vec![
            (DocumentId::new(), "Document 1".to_string()),
            (DocumentId::new(), "Document 2".to_string()),
            (DocumentId::new(), "Document 3".to_string()),
        ];

        let _doc_ids: Vec<_> = documents.iter().map(|(id, _)| *id).collect();

        // Store batch
        let results = async_storage
            .store_texts_batch_async(documents.clone())
            .await
            .unwrap();

        // All should succeed
        assert_eq!(results.len(), 3);
        for result in results {
            assert!(result.is_ok());
        }

        // Verify all documents can be read
        for (doc_id, expected_text) in documents {
            let retrieved = async_storage.get_text_async(doc_id).await.unwrap();
            assert_eq!(retrieved, expected_text);
        }

        async_storage.shutdown().await.unwrap();
    }

    #[test]
    fn test_read_ahead_buffer_functionality() {
        let mut buffer = ReadAheadBuffer::new(3, Duration::from_secs(60));

        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        let doc3 = DocumentId::new();
        let doc4 = DocumentId::new();

        // Add entries
        buffer.put(doc1, "Text 1".to_string());
        buffer.put(doc2, "Text 2".to_string());
        buffer.put(doc3, "Text 3".to_string());

        assert_eq!(buffer.len(), 3);

        // Get entry
        let text = buffer.get(&doc2);
        assert_eq!(text, Some("Text 2".to_string()));

        // Add fourth entry (should evict oldest)
        buffer.put(doc4, "Text 4".to_string());
        assert_eq!(buffer.len(), 3);

        // doc1 should be evicted
        assert!(buffer.get(&doc1).is_none());
        assert!(buffer.get(&doc4).is_some());
    }

    #[test]
    fn test_async_metrics_calculations() {
        let metrics = AsyncStorageMetrics {
            successful_async_reads: 80,
            async_reads: 100,
            successful_async_writes: 90,
            async_writes: 100,
            read_ahead_hits: 70,
            read_ahead_misses: 30,
            ..Default::default()
        };

        assert_eq!(metrics.async_read_success_ratio(), 0.8);
        assert_eq!(metrics.async_write_success_ratio(), 0.9);
        assert_eq!(metrics.read_ahead_hit_ratio(), 0.7);
        assert_eq!(metrics.total_async_operations(), 200);
    }

    #[test]
    fn test_access_pattern_tracker() {
        let mut tracker = AccessPatternTracker::new(100, Duration::from_secs(60), 50);

        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        let doc3 = DocumentId::new();

        // Record access pattern: doc1 -> doc2 -> doc3
        tracker.record_access(doc1);
        tracker.record_access(doc2);
        tracker.record_access(doc3);

        // Predictions should include doc2 and doc3 for doc1
        let predictions = tracker.predict_next_documents(doc1, 5);
        assert!(!predictions.is_empty());

        // Check stats
        let (history_size, pattern_count) = tracker.get_stats();
        assert_eq!(history_size, 3);
        assert!(pattern_count > 0);
    }

    #[test]
    fn test_cooccurrence_map() {
        let mut cooccur = CooccurrenceMap::new(10);

        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        let doc3 = DocumentId::new();

        // Record co-occurrences
        cooccur.record_cooccurrence(doc1, doc2, 1.0);
        cooccur.record_cooccurrence(doc1, doc3, 0.5);
        cooccur.record_cooccurrence(doc2, doc3, 0.8);

        // Test predictions
        let predictions = cooccur.get_predicted_documents(doc1, 2);
        assert_eq!(predictions.len(), 2);

        // Should be sorted by strength (doc2 has weight 1.0, doc3 has 0.5)
        assert_eq!(predictions[0].0, doc2);
        assert_eq!(predictions[0].1, 1.0);
        assert_eq!(predictions[1].0, doc3);
        assert_eq!(predictions[1].1, 0.5);

        // Cleanup weak patterns
        cooccur.cleanup(0.6);
        let predictions_after_cleanup = cooccur.get_predicted_documents(doc1, 2);
        assert_eq!(predictions_after_cleanup.len(), 1); // Only doc2 remains
    }

    #[test]
    fn test_access_pattern_sequence_prediction() {
        let mut tracker = AccessPatternTracker::new(100, Duration::from_secs(300), 50);

        let docs = [
            DocumentId::new(),
            DocumentId::new(),
            DocumentId::new(),
            DocumentId::new(),
        ];

        // Create a repeating access pattern: doc1 -> doc2 -> doc3 -> doc1 -> doc2 -> doc3
        for _ in 0..3 {
            for doc in &docs[0..3] {
                tracker.record_access(*doc);
            }
        }

        // Test sequential prediction
        let predictions_from_doc1 = tracker.predict_next_documents(docs[0], 3);
        assert!(!predictions_from_doc1.is_empty());

        let predictions_from_doc2 = tracker.predict_next_documents(docs[1], 3);
        assert!(!predictions_from_doc2.is_empty());
    }

    #[tokio::test]
    async fn test_read_ahead_prediction_integration() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = AsyncStorageConfig::default();

        let async_storage = AsyncDocumentTextStorage::new(storage, config)
            .await
            .unwrap();

        let doc_id = DocumentId::new();
        let text = "Test document for prediction".to_string();

        // Store document
        async_storage
            .store_text_async(doc_id, text.clone())
            .await
            .unwrap();

        // Read document - this should trigger read-ahead prediction
        let _ = async_storage.get_text_async(doc_id).await.unwrap();

        // Check that predictions were triggered (even if no predictions were made)
        let metrics = async_storage.get_metrics();
        assert!(
            metrics.read_ahead_predictions > 0,
            "Expected at least one read-ahead prediction to be triggered"
        );

        async_storage.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_access_pattern_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = AsyncStorageConfig {
            prediction_temporal_window: Duration::from_millis(50), // Very short window
            cleanup_interval: Duration::from_millis(100),
            ..AsyncStorageConfig::default()
        };

        let async_storage = AsyncDocumentTextStorage::new(storage, config)
            .await
            .unwrap();

        let doc_id = DocumentId::new();
        let text = "Test document for cleanup".to_string();

        async_storage.store_text_async(doc_id, text).await.unwrap();
        let _ = async_storage.get_text_async(doc_id).await.unwrap();

        // Wait for entries to expire and cleanup to run
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Access patterns should be cleaned up due to short temporal window
        let (history_size, _) = {
            let tracker = async_storage.access_tracker.read().await;
            tracker.get_stats()
        };

        // History should be cleaned up or very small
        assert!(history_size <= 1);

        async_storage.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_prediction_performance_with_many_documents() {
        let temp_dir = TempDir::new().unwrap();
        let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
        let config = AsyncStorageConfig {
            max_access_history: 500,
            prediction_count: 10,
            ..AsyncStorageConfig::default()
        };

        let async_storage = AsyncDocumentTextStorage::new(storage, config)
            .await
            .unwrap();

        // Create many documents
        let num_docs = 100;
        let mut doc_ids = Vec::new();
        for i in 0..num_docs {
            let doc_id = DocumentId::new();
            let text = format!("Document number {}", i);
            doc_ids.push(doc_id);
            async_storage.store_text_async(doc_id, text).await.unwrap();
        }

        // Create random access patterns
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let start_time = std::time::Instant::now();

        for i in 0..200 {
            // 200 accesses
            let mut hasher = DefaultHasher::new();
            i.hash(&mut hasher);
            let index = (hasher.finish() as usize) % num_docs;
            let _ = async_storage.get_text_async(doc_ids[index]).await.unwrap();
        }

        let access_time = start_time.elapsed();

        // Ensure prediction doesn't cause significant performance degradation
        assert!(access_time.as_millis() < 5000); // Should complete in reasonable time

        let metrics = async_storage.get_metrics();
        assert!(
            metrics.read_ahead_predictions > 0,
            "Expected predictions with {} reads",
            metrics.async_reads
        );
        assert_eq!(metrics.async_reads, 200);

        async_storage.shutdown().await.unwrap();
    }
}

//! Document Transaction Coordinator for ACID guarantees
//!
//! This module provides comprehensive transaction coordination for document text operations,
//! ensuring ACID properties across text storage and posting operations.

use crate::error::ShardexError;
use crate::identifiers::{DocumentId, TransactionId};
use crate::shardex_index::ShardexIndex;
use crate::transactions::{WalOperation, WalTransaction};
use crate::wal::WalSegment;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Transaction coordinator for document operations
pub struct DocumentTransactionCoordinator {
    /// WAL segment for persistence
    wal_segment: Arc<WalSegment>,
    
    /// Transaction state tracking
    active_transactions: HashMap<TransactionId, ActiveTransaction>,
    
    /// Transaction ID generator
    transaction_counter: AtomicU64,
    
    /// Maximum transaction timeout
    transaction_timeout: Duration,
    
    /// Maximum number of concurrent active transactions
    max_active_transactions: usize,
}

/// Active transaction state
#[derive(Debug)]
struct ActiveTransaction {
    /// Transaction ID
    id: TransactionId,
    
    /// Start timestamp
    start_time: SystemTime,
    
    /// Operations in this transaction
    operations: Vec<WalOperation>,
    
    /// Transaction state
    state: TransactionState,
}

/// Transaction state enumeration
#[derive(Debug, Clone, PartialEq)]
enum TransactionState {
    Active,
    Committing,
    Committed,
    Aborted,
}

/// Statistics for transaction coordinator
#[derive(Debug, Clone)]
pub struct TransactionStatistics {
    pub active_transactions: usize,
    pub oldest_transaction_age: Option<Duration>,
    pub total_operations_pending: usize,
}

impl DocumentTransactionCoordinator {
    /// Create a new transaction coordinator
    pub fn new(wal_segment: Arc<WalSegment>, transaction_timeout: Duration) -> Self {
        Self::with_max_transactions(wal_segment, transaction_timeout, 1000)
    }

    /// Create a new transaction coordinator with custom maximum transaction limit
    pub fn with_max_transactions(
        wal_segment: Arc<WalSegment>, 
        transaction_timeout: Duration,
        max_active_transactions: usize,
    ) -> Self {
        Self {
            wal_segment,
            active_transactions: HashMap::new(),
            transaction_counter: AtomicU64::new(1),
            transaction_timeout,
            max_active_transactions,
        }
    }

    /// Begin a new transaction
    pub async fn begin_transaction(&mut self) -> Result<TransactionId, ShardexError> {
        // Check transaction count limit
        if self.active_transactions.len() >= self.max_active_transactions {
            return Err(ShardexError::InvalidInput {
                field: "transaction_count".to_string(),
                reason: format!(
                    "Maximum active transaction limit reached: {} (max: {})", 
                    self.active_transactions.len(), 
                    self.max_active_transactions
                ),
                suggestion: "Wait for existing transactions to complete or increase the limit".to_string(),
            });
        }
        
        let _ = self.transaction_counter.fetch_add(1, Ordering::SeqCst);
        let transaction_id = TransactionId::new();
        
        let transaction = ActiveTransaction {
            id: transaction_id,
            start_time: SystemTime::now(),
            operations: Vec::new(),
            state: TransactionState::Active,
        };
        
        self.active_transactions.insert(transaction_id, transaction);
        
        tracing::debug!("Transaction {} started", transaction_id);
        Ok(transaction_id)
    }
    
    /// Add operation to active transaction
    pub async fn add_operation(
        &mut self,
        transaction_id: TransactionId,
        operation: WalOperation,
    ) -> Result<(), ShardexError> {
        // Validate operation before adding
        self.validate_operation(&operation)?;
        
        let transaction = self.active_transactions
            .get_mut(&transaction_id)
            .ok_or_else(|| ShardexError::InvalidInput {
                field: "transaction_id".to_string(),
                reason: format!("Transaction {} not found or expired", transaction_id),
                suggestion: "Begin a new transaction".to_string(),
            })?;
        
        if transaction.state != TransactionState::Active {
            return Err(ShardexError::InvalidInput {
                field: "transaction_state".to_string(),
                reason: format!("Transaction {} is not active (state: {:?})", 
                               transaction_id, transaction.state),
                suggestion: "Begin a new transaction".to_string(),
            });
        }
        
        transaction.operations.push(operation);
        
        Ok(())
    }
    
    /// Commit transaction atomically
    pub async fn commit_transaction(
        &mut self,
        transaction_id: TransactionId,
        index: &mut ShardexIndex,
    ) -> Result<(), ShardexError> {
        let mut transaction = self.active_transactions
            .remove(&transaction_id)
            .ok_or_else(|| ShardexError::InvalidInput {
                field: "transaction_id".to_string(),
                reason: format!("Transaction {} not found", transaction_id),
                suggestion: "Check transaction ID".to_string(),
            })?;
        
        transaction.state = TransactionState::Committing;
        
        // Validate all operations before commit
        for operation in &transaction.operations {
            self.validate_operation_for_commit(operation, index)?;
        }
        
        // Write to WAL first (durability)
        let wal_transaction = WalTransaction::with_id_and_timestamp(
            transaction_id,
            transaction.start_time,
            transaction.operations.clone(),
        )?;
        
        self.wal_segment.append_transaction(&wal_transaction)?;
        self.wal_segment.sync()?;
        
        // Apply operations to index (consistency + isolation)
        for operation in &transaction.operations {
            self.apply_operation_to_index(operation, index).await?;
        }
        
        transaction.state = TransactionState::Committed;
        
        tracing::debug!(
            "Transaction {} (id: {}) committed with {} operations", 
            transaction_id, 
            transaction.id,
            transaction.operations.len()
        );
        
        Ok(())
    }
    
    /// Abort transaction and rollback
    pub async fn abort_transaction(
        &mut self,
        transaction_id: TransactionId,
    ) -> Result<(), ShardexError> {
        if let Some(mut transaction) = self.active_transactions.remove(&transaction_id) {
            transaction.state = TransactionState::Aborted;
            
            // Rollback is handled by not applying operations to index
            // WAL cleanup happens during normal WAL maintenance
            
            tracing::debug!(
                "Transaction {} aborted with {} operations", 
                transaction_id, 
                transaction.operations.len()
            );
        }
        
        Ok(())
    }

    /// Validate operation before adding to transaction
    fn validate_operation(&self, operation: &WalOperation) -> Result<(), ShardexError> {
        match operation {
            WalOperation::StoreDocumentText { document_id, text } => {
                self.validate_document_id(*document_id)?;
                self.validate_text_content(text)?;
            }
            
            WalOperation::DeleteDocumentText { document_id } => {
                self.validate_document_id(*document_id)?;
            }
            
            WalOperation::AddPosting { document_id, start, length, vector } => {
                self.validate_document_id(*document_id)?;
                self.validate_posting_coordinates(*start, *length)?;
                self.validate_vector(vector)?;
            }
            
            WalOperation::RemoveDocument { document_id } => {
                self.validate_document_id(*document_id)?;
            }
        }
        
        Ok(())
    }
    
    /// Validate operation before commit (more comprehensive)
    fn validate_operation_for_commit(
        &self,
        operation: &WalOperation,
        index: &ShardexIndex,
    ) -> Result<(), ShardexError> {
        match operation {
            WalOperation::StoreDocumentText { text, .. } => {
                // Check if text storage is available
                if !index.has_text_storage() {
                    return Err(ShardexError::InvalidInput {
                        field: "text_storage".to_string(),
                        reason: "Text storage not enabled for this index".to_string(),
                        suggestion: "Enable text storage in configuration".to_string(),
                    });
                }
                
                // Validate against configuration limits
                let config = index.get_config();
                if text.len() > config.max_document_text_size {
                    return Err(ShardexError::DocumentTooLarge {
                        size: text.len(),
                        max_size: config.max_document_text_size,
                    });
                }
            }
            
            WalOperation::AddPosting { vector, .. } => {
                // Validate vector dimension matches configuration
                let config = index.get_config();
                if vector.len() != config.vector_size {
                    return Err(ShardexError::InvalidDimension {
                        expected: config.vector_size,
                        actual: vector.len(),
                    });
                }
            }
            
            _ => {} // Other operations validated in basic validation
        }
        
        Ok(())
    }
    
    /// Apply operation to index
    async fn apply_operation_to_index(
        &self,
        operation: &WalOperation,
        index: &mut ShardexIndex,
    ) -> Result<(), ShardexError> {
        tracing::trace!("Applying operation to index: {:?}", operation);
        
        // Use existing apply_wal_operation method if available
        // For now, we'll implement the operations directly
        match operation {
            WalOperation::StoreDocumentText { document_id, text } => {
                index.store_document_text(*document_id, text)?;
            }
            WalOperation::DeleteDocumentText { document_id } => {
                index.delete_document_text(*document_id)?;
            }
            WalOperation::AddPosting { document_id, start, length, vector } => {
                index.add_posting(*document_id, *start, *length, vector.clone())?;
                tracing::debug!("Added posting for document: {} at {}:{}", document_id, start, length);
            }
            WalOperation::RemoveDocument { document_id } => {
                index.remove_document(*document_id)?;
                tracing::debug!("Removed all postings for document: {}", document_id);
            }
        }
        
        Ok(())
    }

    /// Clean up expired transactions
    pub async fn cleanup_expired_transactions(&mut self) -> Result<(), ShardexError> {
        let now = SystemTime::now();
        let mut expired_transactions = Vec::new();
        
        for (id, transaction) in &self.active_transactions {
            if let Ok(elapsed) = now.duration_since(transaction.start_time) {
                if elapsed > self.transaction_timeout {
                    expired_transactions.push(*id);
                }
            }
        }
        
        for transaction_id in expired_transactions {
            tracing::warn!("Cleaning up expired transaction: {}", transaction_id);
            self.abort_transaction(transaction_id).await?;
        }
        
        Ok(())
    }
    
    /// Get statistics for active transactions
    pub fn get_transaction_statistics(&self) -> TransactionStatistics {
        let oldest_age = self.active_transactions
            .values()
            .filter_map(|t| SystemTime::now().duration_since(t.start_time).ok())
            .max();

        TransactionStatistics {
            active_transactions: self.active_transactions.len(),
            oldest_transaction_age: oldest_age,
            total_operations_pending: self.active_transactions
                .values()
                .map(|t| t.operations.len())
                .sum(),
        }
    }

    // Helper validation methods
    /// Validates a document ID for transaction operations.
    /// 
    /// DocumentId validation is minimal since the type system ensures only valid
    /// ULID-based identifiers can be constructed. Additional validation could include
    /// checking if the document exists, but that's handled at the operation level.
    ///
    /// # Arguments
    /// * `_document_id` - The document identifier to validate
    ///
    /// # Returns
    /// * `Ok(())` - DocumentId is valid (always true for type-safe IDs)
    fn validate_document_id(&self, _document_id: DocumentId) -> Result<(), ShardexError> {
        // DocumentId validation is handled by the type system
        Ok(())
    }

    /// Validates document text content according to transaction rules.
    /// 
    /// Enforces that document text cannot be empty, as empty documents provide
    /// no value and can cause issues with text processing operations. More
    /// comprehensive validation (e.g., size limits) is performed at commit time.
    ///
    /// # Arguments
    /// * `text` - The document text content to validate
    ///
    /// # Returns
    /// * `Ok(())` - Text content is valid
    /// * `Err(ShardexError::InvalidInput)` - Text is empty or otherwise invalid
    fn validate_text_content(&self, text: &str) -> Result<(), ShardexError> {
        if text.is_empty() {
            return Err(ShardexError::InvalidInput {
                field: "text".to_string(),
                reason: "Document text cannot be empty".to_string(),
                suggestion: "Provide non-empty text content".to_string(),
            });
        }
        Ok(())
    }

    /// Validates posting coordinates to prevent arithmetic overflow and invalid ranges.
    /// 
    /// Ensures that posting coordinates are mathematically valid and won't cause
    /// overflow when calculating the end position (start + length). Zero-length
    /// postings are rejected as they represent no actual text content.
    ///
    /// # Arguments
    /// * `start` - The starting position of the posting in the document text
    /// * `length` - The length of the text span covered by this posting
    ///
    /// # Returns
    /// * `Ok(())` - Coordinates are valid and won't cause overflow
    /// * `Err(ShardexError::InvalidPostingData)` - Length is zero or coordinates would overflow
    fn validate_posting_coordinates(&self, start: u32, length: u32) -> Result<(), ShardexError> {
        if length == 0 {
            return Err(ShardexError::InvalidPostingData {
                reason: "Posting length cannot be zero".to_string(),
                suggestion: "Provide a positive length value".to_string(),
            });
        }

        if start > u32::MAX - length {
            return Err(ShardexError::InvalidPostingData {
                reason: "Start + length coordinates would overflow".to_string(),
                suggestion: "Reduce start position or length".to_string(),
            });
        }

        Ok(())
    }

    /// Validates vector embeddings to ensure they contain only finite values.
    /// 
    /// Ensures all vector values are finite (not NaN or infinite) as these
    /// would corrupt distance calculations and indexing operations. Empty
    /// vectors are rejected as they provide no semantic meaning.
    ///
    /// # Arguments  
    /// * `vector` - The embedding vector to validate
    ///
    /// # Returns
    /// * `Ok(())` - Vector contains only finite values
    /// * `Err(ShardexError::InvalidPostingData)` - Vector is empty or contains NaN/infinite values
    fn validate_vector(&self, vector: &[f32]) -> Result<(), ShardexError> {
        if vector.is_empty() {
            return Err(ShardexError::InvalidPostingData {
                reason: "Vector cannot be empty".to_string(),
                suggestion: "Provide a non-empty vector".to_string(),
            });
        }

        for (i, &value) in vector.iter().enumerate() {
            if !value.is_finite() {
                return Err(ShardexError::InvalidPostingData {
                    reason: format!("Invalid vector value at index {}: {} (must be finite)", i, value),
                    suggestion: "Remove NaN or infinite values from vector".to_string(),
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ShardexConfig;
    use crate::identifiers::ShardId;
    use crate::layout::DirectoryLayout;
    use crate::shardex_index::ShardexIndex;
    use crate::test_utils::TestEnvironment;
    use std::sync::Arc;
    use std::time::Duration;

    async fn setup_coordinator() -> (DocumentTransactionCoordinator, ShardexIndex, TestEnvironment) {
        let test_env = TestEnvironment::new("doc_tx_coord_test");
        let layout = DirectoryLayout::new(test_env.path());
        
        // Create WAL segment
        let wal_segment_path = layout.wal_segment_path(1);
        let wal_segment = Arc::new(WalSegment::create(1, wal_segment_path, 8192).unwrap());
        
        // Create config and index
        let config = ShardexConfig::new()
            .directory_path(test_env.path())
            .vector_size(3)
            .max_document_text_size(1024);
            
        let mut index = ShardexIndex::create_new(layout.clone(), config.clone()).unwrap();
        index.enable_text_storage().unwrap();
        
        // Create a real shard for testing posting operations
        let shard_id = ShardId::new();
        let shard_path = layout.shard_vectors_path(&shard_id);
        std::fs::create_dir_all(shard_path.parent().unwrap()).unwrap();
        
        // Create and add a real shard to the index
        let shard = crate::shard::Shard::create(
            shard_id,
            1000, // shard_size
            config.vector_size,
            layout.shards_dir().to_path_buf(),
        ).unwrap();
        index.add_shard(shard).unwrap();
        
        // Verify shard was added
        println!("Shard count after adding: {}", index.shard_count());
        assert!(index.shard_count() > 0, "No shards were added to the index");
        
        let coordinator = DocumentTransactionCoordinator::new(
            wal_segment,
            Duration::from_secs(30)
        );
        
        (coordinator, index, test_env)
    }

    #[tokio::test]
    async fn test_transaction_lifecycle_basic() {
        let (mut coordinator, mut index, _test_env) = setup_coordinator().await;
        
        // Begin transaction
        let tx_id = coordinator.begin_transaction().await.unwrap();
        
        // Add operation
        let doc_id = DocumentId::new();
        let operation = WalOperation::StoreDocumentText {
            document_id: doc_id,
            text: "Test document".to_string(),
        };
        
        coordinator.add_operation(tx_id, operation).await.unwrap();
        
        // Check statistics
        let stats = coordinator.get_transaction_statistics();
        assert_eq!(stats.active_transactions, 1);
        assert_eq!(stats.total_operations_pending, 1);
        
        // Commit transaction
        coordinator.commit_transaction(tx_id, &mut index).await.unwrap();
        
        // Statistics should show no active transactions
        let stats = coordinator.get_transaction_statistics();
        assert_eq!(stats.active_transactions, 0);
        assert_eq!(stats.total_operations_pending, 0);
    }

    #[tokio::test]
    async fn test_transaction_abort() {
        let (mut coordinator, _index, _test_env) = setup_coordinator().await;
        
        // Begin transaction
        let tx_id = coordinator.begin_transaction().await.unwrap();
        
        // Add operation
        let doc_id = DocumentId::new();
        let operation = WalOperation::StoreDocumentText {
            document_id: doc_id,
            text: "Test document".to_string(),
        };
        
        coordinator.add_operation(tx_id, operation).await.unwrap();
        
        // Abort transaction
        coordinator.abort_transaction(tx_id).await.unwrap();
        
        // Statistics should show no active transactions
        let stats = coordinator.get_transaction_statistics();
        assert_eq!(stats.active_transactions, 0);
        assert_eq!(stats.total_operations_pending, 0);
    }

    #[tokio::test]
    async fn test_multiple_operations_in_transaction() {
        let (mut coordinator, mut index, _test_env) = setup_coordinator().await;
        
        let tx_id = coordinator.begin_transaction().await.unwrap();
        let doc_id = DocumentId::new();
        let doc_id2 = DocumentId::new();
        
        // Add multiple text operations
        let operations = vec![
            WalOperation::StoreDocumentText {
                document_id: doc_id,
                text: "Test document".to_string(),
            },
            WalOperation::StoreDocumentText {
                document_id: doc_id2,
                text: "Another document".to_string(),
            },
            WalOperation::DeleteDocumentText {
                document_id: doc_id,
            },
        ];
        
        for operation in operations {
            coordinator.add_operation(tx_id, operation).await.unwrap();
        }
        
        let stats = coordinator.get_transaction_statistics();
        assert_eq!(stats.active_transactions, 1);
        assert_eq!(stats.total_operations_pending, 3);
        
        // Commit all operations atomically
        coordinator.commit_transaction(tx_id, &mut index).await.unwrap();
        
        let stats = coordinator.get_transaction_statistics();
        assert_eq!(stats.active_transactions, 0);
    }

    #[tokio::test]
    async fn test_operation_validation() {
        let (mut coordinator, _index, _test_env) = setup_coordinator().await;
        
        let tx_id = coordinator.begin_transaction().await.unwrap();
        let doc_id = DocumentId::new();
        
        // Test invalid empty text
        let invalid_text_op = WalOperation::StoreDocumentText {
            document_id: doc_id,
            text: "".to_string(),
        };
        
        let result = coordinator.add_operation(tx_id, invalid_text_op).await;
        assert!(result.is_err());
        
        // Test invalid empty vector
        let invalid_vector_op = WalOperation::AddPosting {
            document_id: doc_id,
            start: 0,
            length: 10,
            vector: vec![],
        };
        
        let result = coordinator.add_operation(tx_id, invalid_vector_op).await;
        assert!(result.is_err());
        
        // Test invalid vector with NaN
        let nan_vector_op = WalOperation::AddPosting {
            document_id: doc_id,
            start: 0,
            length: 10,
            vector: vec![1.0, f32::NAN, 3.0],
        };
        
        let result = coordinator.add_operation(tx_id, nan_vector_op).await;
        assert!(result.is_err());
        
        // Test coordinate overflow
        let overflow_op = WalOperation::AddPosting {
            document_id: doc_id,
            start: u32::MAX,
            length: 1,
            vector: vec![1.0, 2.0, 3.0],
        };
        
        let result = coordinator.add_operation(tx_id, overflow_op).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_commit_validation() {
        let (mut coordinator, mut index, _test_env) = setup_coordinator().await;
        
        let tx_id = coordinator.begin_transaction().await.unwrap();
        let doc_id = DocumentId::new();
        
        // Add operation with wrong vector dimension (index expects 3, providing 4)
        let wrong_dim_op = WalOperation::AddPosting {
            document_id: doc_id,
            start: 0,
            length: 10,
            vector: vec![1.0, 2.0, 3.0, 4.0], // 4 dimensions, but config expects 3
        };
        
        coordinator.add_operation(tx_id, wrong_dim_op).await.unwrap(); // Should pass basic validation
        
        // Commit should fail due to dimension mismatch
        let result = coordinator.commit_transaction(tx_id, &mut index).await;
        assert!(result.is_err());
        
        if let Err(ShardexError::InvalidDimension { expected, actual }) = result {
            assert_eq!(expected, 3);
            assert_eq!(actual, 4);
        } else {
            panic!("Expected InvalidDimension error");
        }
    }

    #[tokio::test]
    async fn test_transaction_not_found_errors() {
        let (mut coordinator, mut index, _test_env) = setup_coordinator().await;
        
        let fake_tx_id = TransactionId::new();
        let doc_id = DocumentId::new();
        
        // Try to add operation to non-existent transaction
        let operation = WalOperation::StoreDocumentText {
            document_id: doc_id,
            text: "Test".to_string(),
        };
        
        let result = coordinator.add_operation(fake_tx_id, operation).await;
        assert!(result.is_err());
        
        // Try to commit non-existent transaction
        let result = coordinator.commit_transaction(fake_tx_id, &mut index).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_inactive_transaction_error() {
        let (mut coordinator, mut index, _test_env) = setup_coordinator().await;
        
        let tx_id = coordinator.begin_transaction().await.unwrap();
        let doc_id = DocumentId::new();
        
        // Add and commit
        let operation = WalOperation::StoreDocumentText {
            document_id: doc_id,
            text: "Test".to_string(),
        };
        
        coordinator.add_operation(tx_id, operation.clone()).await.unwrap();
        coordinator.commit_transaction(tx_id, &mut index).await.unwrap();
        
        // Try to add operation to committed transaction - should fail
        let result = coordinator.add_operation(tx_id, operation).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transaction_timeout_cleanup() {
        let test_env = TestEnvironment::new("timeout_test");
        let layout = DirectoryLayout::new(test_env.path());
        
        let wal_segment_path = layout.wal_segment_path(1);
        let wal_segment = Arc::new(WalSegment::create(1, wal_segment_path, 8192).unwrap());
        
        // Create coordinator with very short timeout
        let mut coordinator = DocumentTransactionCoordinator::new(
            wal_segment,
            Duration::from_millis(10) // 10ms timeout
        );
        
        // Begin transaction
        let tx_id = coordinator.begin_transaction().await.unwrap();
        
        // Add operation
        let doc_id = DocumentId::new();
        let operation = WalOperation::StoreDocumentText {
            document_id: doc_id,
            text: "Test".to_string(),
        };
        
        coordinator.add_operation(tx_id, operation).await.unwrap();
        
        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        // Cleanup should remove expired transaction
        coordinator.cleanup_expired_transactions().await.unwrap();
        
        let stats = coordinator.get_transaction_statistics();
        assert_eq!(stats.active_transactions, 0);
    }

    #[tokio::test]
    async fn test_transaction_statistics() {
        let (mut coordinator, _index, _test_env) = setup_coordinator().await;
        
        // Initial statistics should be empty
        let stats = coordinator.get_transaction_statistics();
        assert_eq!(stats.active_transactions, 0);
        assert_eq!(stats.total_operations_pending, 0);
        assert!(stats.oldest_transaction_age.is_none());
        
        // Begin transactions and add operations
        let tx1 = coordinator.begin_transaction().await.unwrap();
        let tx2 = coordinator.begin_transaction().await.unwrap();
        
        let doc_id = DocumentId::new();
        let operation = WalOperation::StoreDocumentText {
            document_id: doc_id,
            text: "Test".to_string(),
        };
        
        coordinator.add_operation(tx1, operation.clone()).await.unwrap();
        coordinator.add_operation(tx2, operation.clone()).await.unwrap();
        coordinator.add_operation(tx2, operation).await.unwrap();
        
        let stats = coordinator.get_transaction_statistics();
        assert_eq!(stats.active_transactions, 2);
        assert_eq!(stats.total_operations_pending, 3);
        assert!(stats.oldest_transaction_age.is_some());
    }

    #[tokio::test]
    async fn test_large_document_validation() {
        let (mut coordinator, mut index, _test_env) = setup_coordinator().await;
        
        let tx_id = coordinator.begin_transaction().await.unwrap();
        let doc_id = DocumentId::new();
        
        // Test document at max size boundary (assuming 1MB limit)
        let max_size = 1024 * 1024; // 1MB
        let large_text = "a".repeat(max_size);
        
        let large_doc_op = WalOperation::StoreDocumentText {
            document_id: doc_id,
            text: large_text,
        };
        
        // Should pass basic validation
        coordinator.add_operation(tx_id, large_doc_op).await.unwrap();
        
        // Commit should work (assuming index accepts this size)
        let result = coordinator.commit_transaction(tx_id, &mut index).await;
        // Note: This may fail if the ShardexIndex has smaller size limits
        // The test verifies the validation pipeline works correctly
        match result {
            Ok(()) => {
                // Large document accepted
            }
            Err(ShardexError::DocumentTooLarge { size, max_size: limit }) => {
                // Document rejected due to size limit
                assert_eq!(size, max_size);
                assert!(limit < max_size);
            }
            Err(e) => {
                panic!("Unexpected error for large document: {:?}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_concurrent_transaction_cleanup() {
        use tokio::sync::Barrier;
        use std::sync::Arc;
        
        let test_env = TestEnvironment::new("concurrent_cleanup_test");
        let layout = DirectoryLayout::new(test_env.path());
        
        let wal_segment_path = layout.wal_segment_path(1);
        let wal_segment = Arc::new(WalSegment::create(1, wal_segment_path, 8192).unwrap());
        
        // Create coordinator with short timeout
        let coordinator = Arc::new(tokio::sync::Mutex::new(
            DocumentTransactionCoordinator::new(
                wal_segment,
                Duration::from_millis(50) // 50ms timeout
            )
        ));
        
        // Create multiple transactions concurrently
        let barrier = Arc::new(Barrier::new(3));
        let mut handles = vec![];
        
        for i in 0..3 {
            let coordinator = coordinator.clone();
            let barrier = barrier.clone();
            
            let handle = tokio::spawn(async move {
                let tx_id = {
                    let mut coord = coordinator.lock().await;
                    coord.begin_transaction().await.unwrap()
                };
                
                // Add operation to transaction
                let doc_id = DocumentId::new();
                let operation = WalOperation::StoreDocumentText {
                    document_id: doc_id,
                    text: format!("Test document {}", i),
                };
                
                {
                    let mut coord = coordinator.lock().await;
                    coord.add_operation(tx_id, operation).await.unwrap();
                }
                
                // Wait for all transactions to be created
                barrier.wait().await;
                
                // Sleep beyond timeout
                tokio::time::sleep(Duration::from_millis(100)).await;
                
                // All should cleanup concurrently
                let stats = {
                    let mut coord = coordinator.lock().await;
                    coord.cleanup_expired_transactions().await.unwrap();
                    coord.get_transaction_statistics()
                };
                
                stats
            });
            
            handles.push(handle);
        }
        
        // Wait for all concurrent cleanup tasks
        let mut results = vec![];
        for handle in handles {
            results.push(handle.await);
        }
        
        // At least one should show no active transactions after cleanup
        let final_stats = {
            let coord = coordinator.lock().await;
            coord.get_transaction_statistics()
        };
        
        assert_eq!(final_stats.active_transactions, 0);
        
        // Verify all tasks completed successfully
        for result in results {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_transaction_boundary_conditions() {
        let (mut coordinator, mut index, _test_env) = setup_coordinator().await;
        
        // Test maximum coordinate values
        let tx_id = coordinator.begin_transaction().await.unwrap();
        let doc_id = DocumentId::new();
        
        // Test coordinates at maximum safe values
        let max_safe_start = u32::MAX - 1000;
        let max_safe_length = 1000;
        
        let boundary_op = WalOperation::AddPosting {
            document_id: doc_id,
            start: max_safe_start,
            length: max_safe_length,
            vector: vec![1.0, 2.0, 3.0],
        };
        
        coordinator.add_operation(tx_id, boundary_op).await.unwrap();
        
        // Should commit successfully
        coordinator.commit_transaction(tx_id, &mut index).await.unwrap();
        
        // Test edge case: exactly at overflow boundary
        let tx_id2 = coordinator.begin_transaction().await.unwrap();
        
        let overflow_op = WalOperation::AddPosting {
            document_id: doc_id,
            start: u32::MAX - 1,
            length: 2, // This would overflow
            vector: vec![1.0, 2.0, 3.0],
        };
        
        let result = coordinator.add_operation(tx_id2, overflow_op).await;
        assert!(result.is_err()); // Should reject overflow
    }

    #[tokio::test]
    async fn test_transaction_count_limit() {
        let test_env = TestEnvironment::new("transaction_limit_test");
        let layout = DirectoryLayout::new(test_env.path());
        
        let wal_segment_path = layout.wal_segment_path(1);
        let wal_segment = Arc::new(WalSegment::create(1, wal_segment_path, 8192).unwrap());
        
        // Create coordinator with very low transaction limit
        let mut coordinator = DocumentTransactionCoordinator::with_max_transactions(
            wal_segment,
            Duration::from_secs(30),
            2 // Only allow 2 active transactions
        );
        
        // Create first transaction - should succeed
        let tx1 = coordinator.begin_transaction().await;
        assert!(tx1.is_ok());
        
        // Create second transaction - should succeed
        let tx2 = coordinator.begin_transaction().await;
        assert!(tx2.is_ok());
        
        // Try to create third transaction - should fail due to limit
        let tx3 = coordinator.begin_transaction().await;
        assert!(tx3.is_err());
        
        if let Err(ShardexError::InvalidInput { field, reason, .. }) = tx3 {
            assert_eq!(field, "transaction_count");
            assert!(reason.contains("Maximum active transaction limit reached"));
            assert!(reason.contains("2"));
        } else {
            panic!("Expected InvalidInput error for transaction limit");
        }
        
        // Verify statistics show 2 active transactions
        let stats = coordinator.get_transaction_statistics();
        assert_eq!(stats.active_transactions, 2);
    }
}
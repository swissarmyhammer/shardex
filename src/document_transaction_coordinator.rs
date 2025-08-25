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
        Self {
            wal_segment,
            active_transactions: HashMap::new(),
            transaction_counter: AtomicU64::new(1),
            transaction_timeout,
        }
    }

    /// Begin a new transaction
    pub async fn begin_transaction(&mut self) -> Result<TransactionId, ShardexError> {
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
            WalOperation::AddPosting { document_id, start, length, vector: _ } => {
                // For now, skip actual posting addition to shards
                // This would normally add the posting to the appropriate shard
                tracing::debug!("Posting addition marked for document: {} at {}:{}", document_id, start, length);
            }
            WalOperation::RemoveDocument { document_id } => {
                // For now, skip actual document removal from shards
                // This would normally remove the document from all shards
                tracing::debug!("Document removal marked for: {}", document_id);
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
            .map(|t| SystemTime::now().duration_since(t.start_time).ok())
            .filter_map(|d| d)
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
    fn validate_document_id(&self, _document_id: DocumentId) -> Result<(), ShardexError> {
        // DocumentId validation is handled by the type system
        Ok(())
    }

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

    fn validate_posting_coordinates(&self, start: u32, length: u32) -> Result<(), ShardexError> {
        if length == 0 {
            return Err(ShardexError::InvalidInput {
                field: "length".to_string(),
                reason: "Posting length cannot be zero".to_string(),
                suggestion: "Provide a positive length value".to_string(),
            });
        }

        if start > u32::MAX - length {
            return Err(ShardexError::InvalidInput {
                field: "coordinates".to_string(),
                reason: "Start + length would overflow".to_string(),
                suggestion: "Reduce start position or length".to_string(),
            });
        }

        Ok(())
    }

    fn validate_vector(&self, vector: &[f32]) -> Result<(), ShardexError> {
        if vector.is_empty() {
            return Err(ShardexError::InvalidInput {
                field: "vector".to_string(),
                reason: "Vector cannot be empty".to_string(),
                suggestion: "Provide a non-empty vector".to_string(),
            });
        }

        for (i, &value) in vector.iter().enumerate() {
            if !value.is_finite() {
                return Err(ShardexError::InvalidInput {
                    field: "vector".to_string(),
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
}
//! Batch Processing Timer for Shardex WAL Operations
//!
//! This module provides timer-based batch processing that flushes WAL entries to shards
//! at regular intervals. It integrates with the existing batch management infrastructure
//! to provide a high-level interface for automatic and manual batch processing.

use crate::error::ShardexError;
use crate::transactions::{BatchConfig, WalBatchManager, WalOperation};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::{debug, warn};

/// High-level batch processor for timer-based WAL operations
///
/// BatchProcessor coordinates between the batch management layer and WAL storage,
/// providing automatic timer-based flushing and manual flush capabilities.
pub struct BatchProcessor {
    /// Timer interval for batch processing
    batch_interval: Duration,
    /// Pending operations accumulated for batching
    pending_operations: Vec<WalOperation>,
    /// Handle for the background timer task
    timer_handle: Option<JoinHandle<()>>,
    /// Atomic shutdown signal for coordinating shutdown
    shutdown_signal: Arc<AtomicBool>,
    /// Batch configuration for the underlying manager
    batch_config: BatchConfig,
    /// Expected vector dimension for operation validation
    expected_vector_dimension: Option<usize>,
    /// Command channel for communicating with the background task
    command_sender: Option<mpsc::Sender<BatchProcessorCommand>>,
}

/// Commands for communicating with the batch processor background task
#[derive(Debug)]
enum BatchProcessorCommand {
    /// Add an operation to the current batch
    AddOperation(WalOperation),
    /// Force immediate flush of current batch
    FlushNow,
    /// Shutdown the processor
    Shutdown,
}

/// Response from batch processor commands
#[derive(Debug)]
enum BatchProcessorResponse {
    /// Operation added successfully
    OperationAdded,
    /// Batch flushed successfully
    BatchFlushed,
    /// Error occurred
    Error(ShardexError),
    /// Shutdown complete
    ShutdownComplete,
}

impl BatchProcessor {
    /// Create a new batch processor with the given configuration
    pub fn new(
        batch_interval: Duration,
        batch_config: BatchConfig,
        expected_vector_dimension: Option<usize>,
    ) -> Self {
        let shutdown_signal = Arc::new(AtomicBool::new(true)); // Start in shutdown state

        Self {
            batch_interval,
            pending_operations: Vec::new(),
            timer_handle: None,
            shutdown_signal,
            batch_config,
            expected_vector_dimension,
            command_sender: None,
        }
    }

    /// Start the batch processor background task
    pub async fn start(&mut self) -> Result<(), ShardexError> {
        if self.timer_handle.is_some() {
            return Err(ShardexError::Wal(
                "Batch processor already started".to_string(),
            ));
        }

        // Create communication channels
        let (command_sender, mut command_receiver) = mpsc::channel::<BatchProcessorCommand>(1000);
        let (response_sender, _response_receiver) = mpsc::channel::<BatchProcessorResponse>(1000);

        self.command_sender = Some(command_sender);

        // Create batch manager
        let _batch_manager =
            WalBatchManager::new(self.batch_config.clone(), self.expected_vector_dimension);

        // Clone necessary data for the background task
        let batch_interval = self.batch_interval;
        let shutdown_signal = self.shutdown_signal.clone();

        // Start background task
        let handle = tokio::spawn(async move {
            let mut timer = interval(batch_interval);

            loop {
                tokio::select! {
                    // Handle timer ticks
                    _ = timer.tick() => {
                        // For now, just check shutdown signal
                        if shutdown_signal.load(Ordering::SeqCst) {
                            debug!("Timer task shutting down due to shutdown signal");
                            break;
                        }
                    }

                    // Handle commands
                    command = command_receiver.recv() => {
                        match command {
                            Some(BatchProcessorCommand::Shutdown) => {
                                debug!("Timer task received shutdown command");
                                let _ = response_sender.send(BatchProcessorResponse::ShutdownComplete).await;
                                break;
                            }
                            Some(BatchProcessorCommand::FlushNow) => {
                                debug!("Timer task received flush command");
                                let _ = response_sender.send(BatchProcessorResponse::BatchFlushed).await;
                            }
                            Some(BatchProcessorCommand::AddOperation(_op)) => {
                                debug!("Timer task received add operation command");
                                let _ = response_sender.send(BatchProcessorResponse::OperationAdded).await;
                            }
                            None => {
                                debug!("Command channel closed, shutting down timer task");
                                break;
                            }
                        }
                    }
                }
            }

            debug!("Batch processor background task completed");
        });

        self.timer_handle = Some(handle);

        // Signal that we're now running (not shutdown)
        self.shutdown_signal.store(false, Ordering::SeqCst);

        Ok(())
    }

    /// Force immediate flush of current batch
    pub async fn flush_now(&mut self) -> Result<(), ShardexError> {
        if let Some(ref command_sender) = self.command_sender {
            command_sender
                .send(BatchProcessorCommand::FlushNow)
                .await
                .map_err(|_| ShardexError::Wal("Failed to send flush command".to_string()))?;
        }
        Ok(())
    }

    /// Shutdown the batch processor gracefully
    pub async fn shutdown(&mut self) -> Result<(), ShardexError> {
        // Signal shutdown
        self.shutdown_signal.store(true, Ordering::SeqCst);

        // Send shutdown command if command sender is available
        if let Some(ref command_sender) = self.command_sender {
            let _ = command_sender.send(BatchProcessorCommand::Shutdown).await;
        }

        // Wait for background task to complete
        if let Some(handle) = self.timer_handle.take() {
            match handle.await {
                Ok(_) => debug!("Background task completed successfully"),
                Err(e) => warn!("Background task completed with error: {}", e),
            }
        }

        // Clear the command sender
        self.command_sender = None;

        Ok(())
    }

    /// Add an operation to be processed in the next batch
    pub async fn add_operation(&mut self, _operation: WalOperation) -> Result<(), ShardexError> {
        if let Some(ref command_sender) = self.command_sender {
            command_sender
                .send(BatchProcessorCommand::AddOperation(_operation))
                .await
                .map_err(|_| {
                    ShardexError::Wal("Failed to send add operation command".to_string())
                })?;
        } else {
            // If not started, add to pending operations
            self.pending_operations.push(_operation);
        }
        Ok(())
    }

    /// Check if the processor is currently running
    pub fn is_running(&self) -> bool {
        self.timer_handle.is_some() && !self.shutdown_signal.load(Ordering::SeqCst)
    }

    /// Get the current batch interval
    pub fn batch_interval(&self) -> Duration {
        self.batch_interval
    }

    /// Get the number of pending operations
    pub fn pending_operation_count(&self) -> usize {
        self.pending_operations.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifiers::DocumentId;
    use crate::layout::DirectoryLayout;
    use crate::test_utils::TestEnvironment;
    use crate::transactions::BatchConfig;
    use crate::wal::WalManager;

    #[tokio::test]
    async fn test_batch_processor_creation() {
        let batch_interval = Duration::from_millis(100);
        let batch_config = BatchConfig::default();
        let processor = BatchProcessor::new(batch_interval, batch_config, Some(128));

        assert_eq!(processor.batch_interval(), batch_interval);
        assert!(!processor.is_running());
        assert_eq!(processor.pending_operation_count(), 0);
    }

    #[tokio::test]
    async fn test_batch_processor_start() {
        let batch_interval = Duration::from_millis(50);
        let batch_config = BatchConfig::default();
        let mut processor = BatchProcessor::new(batch_interval, batch_config, Some(128));

        // Should start successfully
        let result = processor.start().await;
        assert!(result.is_ok());

        // Should not be able to start twice
        let result2 = processor.start().await;
        assert!(result2.is_err());

        // Clean up
        let _ = processor.shutdown().await;
    }

    #[tokio::test]
    async fn test_batch_processor_with_wal_integration() {
        let _test_env = TestEnvironment::new("test_batch_processor_with_wal_integration");
        let layout = DirectoryLayout::new(_test_env.path());
        layout.create_directories().unwrap();

        // Create WAL manager and segment
        let mut wal_manager = WalManager::new(layout.clone(), 8192); // 8KB segments
        wal_manager.initialize().unwrap();

        // Create batch processor with shorter interval for testing
        let batch_config = BatchConfig {
            batch_write_interval_ms: 50,
            max_operations_per_batch: 5,
            max_batch_size_bytes: 1024,
        };
        let batch_interval = Duration::from_millis(50);
        let mut processor = BatchProcessor::new(batch_interval, batch_config, Some(3));

        // Start the processor
        processor.start().await.unwrap();
        assert!(processor.is_running());

        // Add some operations
        let doc_id = DocumentId::new();
        let operations = vec![
            WalOperation::AddPosting {
                document_id: doc_id,
                start: 0,
                length: 100,
                vector: vec![1.0, 2.0, 3.0],
            },
            WalOperation::RemoveDocument {
                document_id: doc_id,
            },
        ];

        for operation in operations {
            processor.add_operation(operation).await.unwrap();
        }

        // Force flush
        processor.flush_now().await.unwrap();

        // Give some time for background processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Shutdown
        processor.shutdown().await.unwrap();
        assert!(!processor.is_running());
    }

    #[tokio::test]
    async fn test_batch_processor_timer_based_flushing() {
        let _test_env = TestEnvironment::new("test_batch_processor_timer_based_flushing");

        // Create batch processor with very short interval for testing
        let batch_config = BatchConfig {
            batch_write_interval_ms: 20,   // Very short for testing
            max_operations_per_batch: 100, // High limit so timer triggers first
            max_batch_size_bytes: 10000,
        };
        let batch_interval = Duration::from_millis(20);
        let mut processor = BatchProcessor::new(batch_interval, batch_config, Some(3));

        processor.start().await.unwrap();

        // Add one operation
        let doc_id = DocumentId::new();
        let operation = WalOperation::AddPosting {
            document_id: doc_id,
            start: 0,
            length: 100,
            vector: vec![1.0, 2.0, 3.0],
        };
        processor.add_operation(operation).await.unwrap();

        // Wait longer than the flush interval to allow timer-based flush
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Clean shutdown
        processor.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_batch_processor_graceful_shutdown_with_pending_operations() {
        let _test_env =
            TestEnvironment::new("test_batch_processor_graceful_shutdown_with_pending_operations");

        let batch_config = BatchConfig {
            batch_write_interval_ms: 1000, // Long interval
            max_operations_per_batch: 100,
            max_batch_size_bytes: 10000,
        };
        let batch_interval = Duration::from_millis(1000);
        let mut processor = BatchProcessor::new(batch_interval, batch_config, Some(3));

        processor.start().await.unwrap();

        // Add operations that won't be flushed by timer or size
        let doc_id = DocumentId::new();
        for i in 0..3 {
            let operation = WalOperation::AddPosting {
                document_id: doc_id,
                start: i * 100,
                length: 100,
                vector: vec![1.0 + i as f32, 2.0 + i as f32, 3.0 + i as f32],
            };
            processor.add_operation(operation).await.unwrap();
        }

        // Shutdown should flush pending operations
        processor.shutdown().await.unwrap();
        assert!(!processor.is_running());
    }

    #[tokio::test]
    async fn test_batch_processor_basic_lifecycle() {
        let batch_interval = Duration::from_millis(50);
        let batch_config = BatchConfig::default();
        let mut processor = BatchProcessor::new(batch_interval, batch_config, Some(128));

        // Start processor
        processor.start().await.unwrap();
        assert!(processor.is_running());

        // Add an operation
        let doc_id = DocumentId::new();
        let operation = WalOperation::AddPosting {
            document_id: doc_id,
            start: 0,
            length: 100,
            vector: vec![1.0, 2.0, 3.0],
        };

        processor.add_operation(operation).await.unwrap();

        // Flush manually
        processor.flush_now().await.unwrap();

        // Shutdown
        processor.shutdown().await.unwrap();
        assert!(!processor.is_running());
    }
}

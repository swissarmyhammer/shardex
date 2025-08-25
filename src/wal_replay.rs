//! WAL Replay for Recovery
//!
//! This module provides WAL replay functionality for crash recovery and startup.
//! It processes transaction records from WAL segments, applies them to the index
//! with idempotent operation handling, and provides recovery validation.

use crate::error::ShardexError;
use crate::identifiers::{ShardId, TransactionId};
use crate::layout::{DirectoryLayout, FileDiscovery};
use crate::shard::Shard;
use crate::shardex_index::ShardexIndex;
use crate::structures::Posting;
use crate::transactions::{WalOperation, WalTransaction};
use crate::wal::{WalRecordHeader, WalSegment};
use std::collections::HashSet;
use std::path::PathBuf;
use tracing::{info, warn};

/// Statistics and progress information for WAL replay operations
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecoveryStats {
    /// Number of WAL segments processed
    pub segments_processed: usize,
    /// Total number of transactions replayed successfully
    pub transactions_replayed: usize,
    /// Number of transactions skipped due to duplicates
    pub transactions_skipped: usize,
    /// Total number of operations applied to the index
    pub operations_applied: usize,
    /// List of error messages encountered during replay
    pub errors_encountered: Vec<String>,
    /// Number of AddPosting operations applied
    pub add_posting_operations: usize,
    /// Number of RemoveDocument operations applied
    pub remove_document_operations: usize,
}

impl RecoveryStats {
    /// Create new empty recovery statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an error message to the statistics
    pub fn add_error<S: Into<String>>(&mut self, error: S) {
        self.errors_encountered.push(error.into());
    }

    /// Check if any errors were encountered during recovery
    pub fn has_errors(&self) -> bool {
        !self.errors_encountered.is_empty()
    }

    /// Get the total number of operations (applied + skipped from duplicates)
    pub fn total_operations_processed(&self) -> usize {
        self.add_posting_operations + self.remove_document_operations
    }

    /// Calculate success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        let total_transactions = self.transactions_replayed + self.transactions_skipped;
        if total_transactions == 0 {
            100.0
        } else {
            (self.transactions_replayed as f64 / total_transactions as f64) * 100.0
        }
    }
}

/// WAL replay engine for processing transaction records and reconstructing index state
pub struct WalReplayer {
    /// Directory layout for finding WAL segments
    wal_directory: PathBuf,
    /// Target index to apply operations to
    shardex_index: ShardexIndex,
    /// Set of transaction IDs that have been processed (for idempotency)
    processed_transactions: HashSet<TransactionId>,
    /// Recovery progress and statistics
    recovery_stats: RecoveryStats,
}

impl WalReplayer {
    /// Create a new WAL replayer with the given WAL directory and target index
    pub fn new(wal_directory: PathBuf, shardex_index: ShardexIndex) -> Self {
        Self {
            wal_directory,
            shardex_index,
            processed_transactions: HashSet::new(),
            recovery_stats: RecoveryStats::new(),
        }
    }

    /// Get the current recovery statistics
    pub fn recovery_stats(&self) -> &RecoveryStats {
        &self.recovery_stats
    }

    /// Get a mutable reference to recovery statistics (for testing)
    pub fn recovery_stats_mut(&mut self) -> &mut RecoveryStats {
        &mut self.recovery_stats
    }

    /// Replay all WAL segments in the configured directory
    pub async fn replay_all_segments(&mut self) -> Result<(), ShardexError> {
        // Create a DirectoryLayout from our WAL directory path
        // The WAL directory should be the parent's wal subdirectory
        let parent_dir = self.wal_directory.parent().ok_or_else(|| {
            ShardexError::Wal("WAL directory has no parent directory".to_string())
        })?;

        let layout = DirectoryLayout::new(parent_dir);
        let discovery = FileDiscovery::new(layout);

        // Discover all WAL segments
        let wal_segments = discovery.discover_wal_segments()?;

        if wal_segments.is_empty() {
            // No WAL segments found, nothing to replay
            return Ok(());
        }

        // Replay segments in order (they're already sorted by segment number)
        let mut total_transactions = 0;
        for segment_info in &wal_segments {
            // Open the WAL segment
            let segment = WalSegment::open(segment_info.path.clone())?;

            // Replay this segment
            match self.replay_segment(&segment).await {
                Ok(transactions_processed) => {
                    total_transactions += transactions_processed;
                }
                Err(e) => {
                    self.recovery_stats.add_error(format!(
                        "Failed to replay segment {}: {}",
                        segment_info.path.display(),
                        e
                    ));
                    // Continue with other segments even if one fails
                }
            }
        }

        // Log final statistics
        if total_transactions > 0 {
            info!(
                segments = wal_segments.len(),
                transactions = total_transactions,
                "WAL replay completed"
            );
        }

        Ok(())
    }

    /// Replay a specific WAL segment and return the number of transactions processed
    pub async fn replay_segment(&mut self, segment: &WalSegment) -> Result<usize, ShardexError> {
        let mut transactions_processed = 0;
        let initial_write_pos = crate::wal::initial_write_position();

        // Get the memory-mapped segment data
        let segment_data = segment.read_segment_data()?;

        let mut current_pos = initial_write_pos;
        let write_pointer = segment.write_pointer();

        // Read all records from the segment
        while current_pos < write_pointer {
            // Check if we have enough space for a record header
            if current_pos + WalRecordHeader::SIZE > segment_data.len() {
                break;
            }

            // Read the record header manually to avoid alignment issues
            let header_bytes = &segment_data[current_pos..current_pos + WalRecordHeader::SIZE];
            let data_length = u32::from_le_bytes([
                header_bytes[0],
                header_bytes[1],
                header_bytes[2],
                header_bytes[3],
            ]);
            let checksum = u32::from_le_bytes([
                header_bytes[4],
                header_bytes[5],
                header_bytes[6],
                header_bytes[7],
            ]);

            let data_length_usize = data_length as usize;
            let record_data_start = current_pos + WalRecordHeader::SIZE;
            let record_data_end = record_data_start + data_length_usize;

            // Check bounds and validate record
            if record_data_end > segment_data.len() || record_data_end > write_pointer {
                self.recovery_stats
                    .add_error(format!("Truncated record at position {}", current_pos));
                break;
            }

            let record_data = &segment_data[record_data_start..record_data_end];

            // Validate checksum
            let expected_checksum = crc32fast::hash(record_data);
            if checksum != expected_checksum {
                self.recovery_stats
                    .add_error(format!("Invalid checksum at position {}", current_pos));
                current_pos = record_data_end;
                continue;
            }

            // The record_data contains the serialized transaction
            // Try to deserialize the transaction directly
            match WalTransaction::deserialize(record_data) {
                Ok(transaction) => {
                    transactions_processed += 1;

                    // Check if we've already processed this transaction (idempotency)
                    if self.is_transaction_processed(&transaction.id) {
                        self.recovery_stats.transactions_skipped += 1;
                    } else {
                        // Apply the transaction
                        match self.apply_transaction(&transaction).await {
                            Ok(operations_applied) => {
                                self.recovery_stats.transactions_replayed += 1;
                                self.recovery_stats.operations_applied += operations_applied;
                                self.mark_transaction_processed(transaction.id);
                            }
                            Err(e) => {
                                self.recovery_stats.add_error(format!(
                                    "Failed to apply transaction {}: {}",
                                    transaction.id, e
                                ));
                            }
                        }
                    }
                }
                Err(e) => {
                    self.recovery_stats.add_error(format!(
                        "Failed to deserialize transaction at position {}: {}",
                        current_pos, e
                    ));
                }
            }

            current_pos = record_data_end;
        }

        self.recovery_stats.segments_processed += 1;
        Ok(transactions_processed)
    }

    /// Apply a transaction's operations to the index
    async fn apply_transaction(
        &mut self,
        transaction: &WalTransaction,
    ) -> Result<usize, ShardexError> {
        let mut operations_applied = 0;

        for operation in &transaction.operations {
            self.apply_operation(operation)?;
            operations_applied += 1;

            // Update operation type counters
            match operation {
                WalOperation::AddPosting { .. } => {
                    self.recovery_stats.add_posting_operations += 1;
                }
                WalOperation::RemoveDocument { .. } => {
                    self.recovery_stats.remove_document_operations += 1;
                }
                WalOperation::StoreDocumentText { .. } => {
                    // TODO: Add counter for document text operations when recovery stats are extended
                }
                WalOperation::DeleteDocumentText { .. } => {
                    // TODO: Add counter for document text operations when recovery stats are extended
                }
            }
        }

        Ok(operations_applied)
    }

    /// Apply a single WAL operation to the index (idempotently)
    fn apply_operation(&mut self, op: &WalOperation) -> Result<(), ShardexError> {
        match op {
            WalOperation::AddPosting {
                document_id,
                start,
                length,
                vector,
            } => {
                // Validate the operation
                if vector.is_empty() {
                    return Err(ShardexError::Wal(
                        "Cannot add posting with empty vector".to_string(),
                    ));
                }
                if *length == 0 {
                    return Err(ShardexError::Wal(
                        "Cannot add posting with zero length".to_string(),
                    ));
                }

                // Create a posting from the operation
                let posting = Posting {
                    document_id: *document_id,
                    start: *start,
                    length: *length,
                    vector: vector.clone(),
                };

                // Find the nearest shard for this posting's vector
                let shard_id = match self.shardex_index.find_nearest_shard(&posting.vector)? {
                    Some(shard_id) => shard_id,
                    None => {
                        // No shards available - create a default shard for recovery
                        info!("No shards found during WAL replay - creating default shard for recovery");
                        self.create_default_shard_for_recovery(&posting.vector)?
                    }
                };

                // Get mutable reference to the shard and add the posting
                let shard = self.shardex_index.get_shard_mut(shard_id)?;
                match shard.add_posting(posting) {
                    Ok(_) => {
                        // Successfully added posting
                        Ok(())
                    }
                    Err(e) => {
                        warn!(
                            document_id = %document_id,
                            shard_id = %shard_id,
                            error = %e,
                            "Failed to add posting to shard during WAL replay"
                        );
                        Err(e)
                    }
                }
            }
            WalOperation::RemoveDocument { document_id } => {
                // Remove the document from all shards that might contain it
                // We need to check all shards since we don't know which ones contain this document
                let mut total_removed = 0;
                let shard_ids = self.shardex_index.shard_ids();

                for shard_id in shard_ids {
                    let shard = self.shardex_index.get_shard_mut(shard_id)?;
                    match shard.remove_document(*document_id) {
                        Ok(removed_count) => {
                            total_removed += removed_count;
                        }
                        Err(e) => {
                            warn!(
                                document_id = %document_id,
                                shard_id = %shard_id,
                                error = %e,
                                "Failed to remove document from shard during WAL replay"
                            );
                            // Continue with other shards even if one fails
                        }
                    }
                }

                // Log if no postings were found to remove (might be expected in some cases)
                if total_removed == 0 {
                    warn!(
                        document_id = %document_id,
                        "No postings found to remove for document during WAL replay"
                    );
                }

                Ok(())
            }
            WalOperation::StoreDocumentText { document_id, text: _ } => {
                // Document text storage operations will be handled at the index level
                // For now, we'll just log and ignore these operations during WAL replay
                info!(
                    document_id = %document_id,
                    "StoreDocumentText operation during WAL replay - document text storage not yet implemented"
                );
                Ok(())
            }
            WalOperation::DeleteDocumentText { document_id } => {
                // Document text deletion operations will be handled at the index level
                // For now, we'll just log and ignore these operations during WAL replay
                info!(
                    document_id = %document_id,
                    "DeleteDocumentText operation during WAL replay - document text storage not yet implemented"
                );
                Ok(())
            }
        }
    }

    /// Check if a transaction has already been processed
    pub fn is_transaction_processed(&self, transaction_id: &TransactionId) -> bool {
        self.processed_transactions.contains(transaction_id)
    }

    /// Mark a transaction as processed
    pub fn mark_transaction_processed(&mut self, transaction_id: TransactionId) {
        self.processed_transactions.insert(transaction_id);
    }

    /// Get the number of processed transactions
    pub fn processed_transaction_count(&self) -> usize {
        self.processed_transactions.len()
    }

    /// Consume the replayer and return the ShardexIndex
    pub fn into_index(self) -> ShardexIndex {
        self.shardex_index
    }

    /// Create a default shard for recovery when no shards exist
    /// This is a recovery scenario where we need to replay operations but the index is empty
    fn create_default_shard_for_recovery(
        &mut self,
        sample_vector: &[f32],
    ) -> Result<ShardId, ShardexError> {
        let shard_id = ShardId::new();
        let vector_size = sample_vector.len();
        let default_capacity = 1000; // Default capacity for recovery shard

        // Create the shard in the same directory as the index
        let shard = Shard::create(
            shard_id,
            default_capacity,
            vector_size,
            self.wal_directory
                .parent()
                .ok_or_else(|| {
                    ShardexError::Wal("WAL directory has no parent for shard creation".to_string())
                })?
                .to_path_buf(),
        )?;

        // Add the shard to the index
        self.shardex_index.add_shard(shard)?;

        info!(
            shard_id = %shard_id,
            vector_size = vector_size,
            capacity = default_capacity,
            "Created default shard for WAL replay recovery"
        );

        Ok(shard_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ShardexConfig;
    use crate::test_utils::TestEnvironment;

    #[test]
    fn test_recovery_stats_basic() {
        let mut stats = RecoveryStats::new();

        assert_eq!(stats.segments_processed, 0);
        assert_eq!(stats.transactions_replayed, 0);
        assert_eq!(stats.transactions_skipped, 0);
        assert_eq!(stats.operations_applied, 0);
        assert!(stats.errors_encountered.is_empty());
        assert!(!stats.has_errors());
        assert_eq!(stats.success_rate(), 100.0);

        // Add an error
        stats.add_error("Test error");
        assert!(stats.has_errors());
        assert_eq!(stats.errors_encountered.len(), 1);
    }

    #[test]
    fn test_recovery_stats_success_rate() {
        let mut stats = RecoveryStats::new();

        stats.transactions_replayed = 8;
        stats.transactions_skipped = 2;
        assert_eq!(stats.success_rate(), 80.0);

        stats.transactions_replayed = 10;
        stats.transactions_skipped = 0;
        assert_eq!(stats.success_rate(), 100.0);
    }

    #[test]
    fn test_wal_replayer_creation() {
        let _test_env = TestEnvironment::new("test_wal_replayer_creation");
        let config = ShardexConfig::new()
            .directory_path(_test_env.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).unwrap();
        let wal_directory = _test_env.path().join("wal");

        let replayer = WalReplayer::new(wal_directory.clone(), index);

        assert_eq!(replayer.wal_directory, wal_directory);
        assert_eq!(replayer.processed_transaction_count(), 0);
        assert!(!replayer.recovery_stats().has_errors());
    }

    #[test]
    fn test_transaction_tracking() {
        let _test_env = TestEnvironment::new("test_transaction_tracking");
        let config = ShardexConfig::new()
            .directory_path(_test_env.path())
            .vector_size(128);

        let index = ShardexIndex::create(config).unwrap();
        let wal_directory = _test_env.path().join("wal");
        let mut replayer = WalReplayer::new(wal_directory, index);

        let transaction_id = TransactionId::new();

        // Initially not processed
        assert!(!replayer.is_transaction_processed(&transaction_id));
        assert_eq!(replayer.processed_transaction_count(), 0);

        // Mark as processed
        replayer.mark_transaction_processed(transaction_id);

        // Should now be processed
        assert!(replayer.is_transaction_processed(&transaction_id));
        assert_eq!(replayer.processed_transaction_count(), 1);
    }
}

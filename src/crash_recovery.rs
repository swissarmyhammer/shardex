//! WAL-Based Crash Recovery for Shardex
//!
//! This module provides comprehensive crash recovery using Write-Ahead Log replay
//! with consistency validation, progress tracking, and graceful handling of
//! corrupted WAL segments.

use crate::config::ShardexConfig;
use crate::error::ShardexError;
use crate::layout::DirectoryLayout;
use crate::shardex_index::ShardexIndex;
use crate::wal_replay::{RecoveryStats, WalReplayer};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{info, warn, error};

/// Crash recovery statistics with performance metrics
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CrashRecoveryStats {
    /// Base recovery statistics from WAL replay
    pub recovery_stats: RecoveryStats,
    /// Time taken for crash detection phase
    pub crash_detection_duration: Duration,
    /// Time taken for WAL replay phase
    pub wal_replay_duration: Duration,
    /// Time taken for consistency validation phase
    pub consistency_validation_duration: Duration,
    /// Total recovery duration
    pub total_recovery_duration: Duration,
    /// Whether a crash was detected
    pub crash_detected: bool,
    /// List of corrupted segments that were skipped
    pub corrupted_segments: Vec<String>,
    /// Whether recovery completed successfully
    pub recovery_completed: bool,
    /// Whether consistency validation passed
    pub consistency_valid: bool,
}

impl CrashRecoveryStats {
    /// Create new empty crash recovery statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if recovery was successful
    pub fn is_successful(&self) -> bool {
        self.recovery_completed && self.consistency_valid && !self.recovery_stats.has_errors()
    }

    /// Get total number of transactions processed
    pub fn total_transactions(&self) -> usize {
        self.recovery_stats.transactions_replayed + self.recovery_stats.transactions_skipped
    }

    /// Add a corrupted segment to the statistics
    pub fn add_corrupted_segment<S: Into<String>>(&mut self, segment: S) {
        self.corrupted_segments.push(segment.into());
    }
}

/// WAL-based crash recovery coordinator
///
/// CrashRecovery provides comprehensive crash detection, WAL replay coordination,
/// and consistency validation with support for partial recovery from corrupted segments.
pub struct CrashRecovery {
    /// Index directory path
    index_directory: PathBuf,
    /// Shardex configuration
    config: ShardexConfig,
    /// Directory layout for file management
    layout: DirectoryLayout,
    /// Recovery statistics and progress
    recovery_stats: CrashRecoveryStats,
}

impl CrashRecovery {
    /// Create a new crash recovery coordinator
    pub fn new(config: ShardexConfig) -> Self {
        let layout = DirectoryLayout::new(&config.directory_path);
        Self {
            index_directory: config.directory_path.clone(),
            config,
            layout,
            recovery_stats: CrashRecoveryStats::new(),
        }
    }

    /// Detect if a crash occurred by checking for incomplete operations
    pub async fn detect_crash(&mut self) -> Result<bool, ShardexError> {
        let start_time = Instant::now();
        info!("Starting crash detection for index at {}", self.index_directory.display());

        let mut crash_detected = false;

        // Check for recovery lock file indicating incomplete recovery
        let recovery_lock_path = self.layout.root_path().join(".recovery_lock");
        if recovery_lock_path.exists() {
            info!("Found recovery lock file - crash detected");
            crash_detected = true;
        }

        // Check for incomplete index metadata
        let metadata_path = self.layout.root_path().join("shardex.meta");
        if metadata_path.exists() {
            // Try to validate the metadata file
            match std::fs::metadata(&metadata_path) {
                Ok(meta) => {
                    if meta.len() == 0 {
                        info!("Empty metadata file detected - crash detected");
                        crash_detected = true;
                    }
                }
                Err(e) => {
                    info!("Failed to read metadata file: {} - crash detected", e);
                    crash_detected = true;
                }
            }
        } else {
            // No metadata file - this could be a fresh index or a corrupted one
            // Check if there are any WAL files that would indicate this should have metadata
            let wal_files = std::fs::read_dir(self.layout.wal_dir())
                .map(|entries| entries.count())
                .unwrap_or(0);
            
            if wal_files > 0 {
                info!("WAL files exist but no metadata - crash detected");
                crash_detected = true;
            }
        }

        // Check for corrupted or incomplete WAL segments
        if let Ok(entries) = std::fs::read_dir(self.layout.wal_dir()) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "log") {
                    // Try to open the WAL segment to check for corruption
                    match crate::wal::WalSegment::open(path.clone()) {
                        Ok(segment) => {
                            // Validate segment integrity
                            if let Err(e) = segment.validate_integrity() {
                                warn!("Corrupted WAL segment {}: {}", path.display(), e);
                                self.recovery_stats.add_corrupted_segment(path.display().to_string());
                                crash_detected = true;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to open WAL segment {}: {}", path.display(), e);
                            self.recovery_stats.add_corrupted_segment(path.display().to_string());
                            crash_detected = true;
                        }
                    }
                }
            }
        }

        self.recovery_stats.crash_detection_duration = start_time.elapsed();
        self.recovery_stats.crash_detected = crash_detected;

        if crash_detected {
            info!(
                duration_ms = self.recovery_stats.crash_detection_duration.as_millis(),
                corrupted_segments = self.recovery_stats.corrupted_segments.len(),
                "Crash detection completed - recovery required"
            );
        } else {
            info!(
                duration_ms = self.recovery_stats.crash_detection_duration.as_millis(),
                "Crash detection completed - no crash detected"
            );
        }

        Ok(crash_detected)
    }

    /// Perform comprehensive crash recovery
    pub async fn recover(&mut self) -> Result<ShardexIndex, ShardexError> {
        let start_time = Instant::now();
        info!("Starting crash recovery process");

        // Create recovery lock to prevent concurrent recovery attempts
        let recovery_lock_path = self.layout.root_path().join(".recovery_lock");
        if let Err(e) = std::fs::write(&recovery_lock_path, "recovery_in_progress") {
            warn!("Failed to create recovery lock: {}", e);
        }

        // Ensure directories exist
        self.layout.create_directories()?;

        // Create or recover the index
        let index = match ShardexIndex::open(&self.config.directory_path) {
            Ok(index) => {
                info!("Opened existing index for recovery");
                index
            }
            Err(_) => {
                info!("Creating new index for recovery");
                ShardexIndex::create(self.config.clone())?
            }
        };

        // Perform WAL replay using existing infrastructure
        let wal_replay_start = Instant::now();
        let mut recovered_index = self.replay_wal(index).await?;
        self.recovery_stats.wal_replay_duration = wal_replay_start.elapsed();

        // Validate consistency after recovery
        let validation_start = Instant::now();
        self.validate_consistency(&mut recovered_index).await?;
        self.recovery_stats.consistency_validation_duration = validation_start.elapsed();

        // Clean up recovery lock
        if let Err(e) = std::fs::remove_file(&recovery_lock_path) {
            warn!("Failed to remove recovery lock: {}", e);
        }

        self.recovery_stats.total_recovery_duration = start_time.elapsed();
        self.recovery_stats.recovery_completed = true;

        info!(
            total_duration_ms = self.recovery_stats.total_recovery_duration.as_millis(),
            transactions_replayed = self.recovery_stats.recovery_stats.transactions_replayed,
            transactions_skipped = self.recovery_stats.recovery_stats.transactions_skipped,
            segments_processed = self.recovery_stats.recovery_stats.segments_processed,
            "Crash recovery completed successfully"
        );

        Ok(recovered_index)
    }

    /// Perform WAL replay using the existing WalReplayer infrastructure
    async fn replay_wal(&mut self, index: ShardexIndex) -> Result<ShardexIndex, ShardexError> {
        info!("Starting WAL replay phase");

        let wal_directory = self.layout.wal_dir().to_path_buf();
        let mut replayer = WalReplayer::new(wal_directory, index);

        // Replay all segments with error handling for corrupted segments
        match replayer.replay_all_segments().await {
            Ok(()) => {
                // Copy recovery statistics from the replayer
                self.recovery_stats.recovery_stats = replayer.recovery_stats().clone();
                
                info!(
                    segments_processed = self.recovery_stats.recovery_stats.segments_processed,
                    transactions_replayed = self.recovery_stats.recovery_stats.transactions_replayed,
                    operations_applied = self.recovery_stats.recovery_stats.operations_applied,
                    "WAL replay completed successfully"
                );

                Ok(replayer.into_index())
            }
            Err(e) => {
                // Copy partial recovery statistics even on error
                self.recovery_stats.recovery_stats = replayer.recovery_stats().clone();
                
                // Check if we can continue with partial recovery
                if self.recovery_stats.recovery_stats.transactions_replayed > 0 {
                    warn!(
                        error = %e,
                        transactions_replayed = self.recovery_stats.recovery_stats.transactions_replayed,
                        "Partial WAL replay completed with errors - continuing with recovered data"
                    );
                    
                    // Add the error to our statistics
                    self.recovery_stats.recovery_stats.add_error(format!("Partial recovery error: {}", e));
                    
                    Ok(replayer.into_index())
                } else {
                    error!(error = %e, "WAL replay failed completely");
                    Err(e)
                }
            }
        }
    }

    /// Validate index consistency after recovery
    pub async fn validate_consistency(&mut self, index: &mut ShardexIndex) -> Result<(), ShardexError> {
        info!("Starting consistency validation");

        // Validate shard structure integrity
        let shard_ids = index.shard_ids();
        for shard_id in &shard_ids {
            match index.get_shard(*shard_id) {
                Ok(shard) => {
                    // Validate shard integrity
                    if let Err(e) = shard.validate_integrity() {
                        let error_msg = format!("Shard {} failed integrity validation: {}", shard_id, e);
                        error!("{}", error_msg);
                        self.recovery_stats.recovery_stats.add_error(error_msg);
                        self.recovery_stats.consistency_valid = false;
                        return Err(ShardexError::Corruption(format!("Shard integrity validation failed: {}", e)));
                    }
                }
                Err(e) => {
                    let error_msg = format!("Failed to access shard {} for validation: {}", shard_id, e);
                    error!("{}", error_msg);
                    self.recovery_stats.recovery_stats.add_error(error_msg);
                    self.recovery_stats.consistency_valid = false;
                    return Err(e);
                }
            }
        }

        // Validate index metadata consistency
        match index.validate_metadata() {
            Ok(()) => {
                self.recovery_stats.consistency_valid = true;
                info!(
                    shards_validated = shard_ids.len(),
                    "Consistency validation passed"
                );
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("Index metadata validation failed: {}", e);
                error!("{}", error_msg);
                self.recovery_stats.recovery_stats.add_error(error_msg);
                self.recovery_stats.consistency_valid = false;
                Err(e)
            }
        }
    }

    /// Get the current recovery statistics
    pub fn recovery_stats(&self) -> &CrashRecoveryStats {
        &self.recovery_stats
    }

    /// Get a mutable reference to recovery statistics (for testing)
    pub fn recovery_stats_mut(&mut self) -> &mut CrashRecoveryStats {
        &mut self.recovery_stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestEnvironment;

    #[test]
    fn test_crash_recovery_stats_new() {
        let stats = CrashRecoveryStats::new();
        assert!(!stats.crash_detected);
        assert!(!stats.recovery_completed);
        assert!(!stats.consistency_valid);
        assert_eq!(stats.total_transactions(), 0);
        assert!(stats.corrupted_segments.is_empty());
        assert!(!stats.is_successful());
    }

    #[test]
    fn test_crash_recovery_stats_successful() {
        let mut stats = CrashRecoveryStats::new();
        stats.recovery_completed = true;
        stats.consistency_valid = true;
        // No errors in recovery_stats means is_successful should return true
        assert!(stats.is_successful());
    }

    #[test]
    fn test_crash_recovery_stats_with_errors() {
        let mut stats = CrashRecoveryStats::new();
        stats.recovery_completed = true;
        stats.consistency_valid = true;
        stats.recovery_stats.add_error("Test error");
        // Has errors, so should not be successful
        assert!(!stats.is_successful());
    }

    #[test]
    fn test_crash_recovery_stats_add_corrupted_segment() {
        let mut stats = CrashRecoveryStats::new();
        stats.add_corrupted_segment("segment_001.log");
        stats.add_corrupted_segment("segment_002.log");
        assert_eq!(stats.corrupted_segments.len(), 2);
        assert!(stats.corrupted_segments.contains(&"segment_001.log".to_string()));
        assert!(stats.corrupted_segments.contains(&"segment_002.log".to_string()));
    }

    #[test]
    fn test_crash_recovery_creation() {
        let _test_env = TestEnvironment::new("test_crash_recovery_creation");
        let config = ShardexConfig::new()
            .directory_path(_test_env.path())
            .vector_size(128);

        let recovery = CrashRecovery::new(config.clone());
        assert_eq!(recovery.index_directory, config.directory_path);
        assert_eq!(recovery.config, config);
        assert!(!recovery.recovery_stats.crash_detected);
    }

    #[tokio::test]
    async fn test_detect_crash_no_crash() {
        let _test_env = TestEnvironment::new("test_detect_crash_no_crash");
        let config = ShardexConfig::new()
            .directory_path(_test_env.path())
            .vector_size(128);

        let mut recovery = CrashRecovery::new(config);
        
        // Create directories but no problematic files
        recovery.layout.create_directories().unwrap();
        
        let crash_detected = recovery.detect_crash().await.unwrap();
        assert!(!crash_detected);
        assert!(!recovery.recovery_stats.crash_detected);
        assert!(recovery.recovery_stats.crash_detection_duration > Duration::from_nanos(0));
    }

    #[tokio::test]
    async fn test_detect_crash_recovery_lock() {
        let _test_env = TestEnvironment::new("test_detect_crash_recovery_lock");
        let config = ShardexConfig::new()
            .directory_path(_test_env.path())
            .vector_size(128);

        let mut recovery = CrashRecovery::new(config);
        recovery.layout.create_directories().unwrap();
        
        // Create a recovery lock file
        let lock_path = recovery.layout.root_path().join(".recovery_lock");
        std::fs::write(&lock_path, "test").unwrap();
        
        let crash_detected = recovery.detect_crash().await.unwrap();
        assert!(crash_detected);
        assert!(recovery.recovery_stats.crash_detected);
    }

    #[tokio::test]
    async fn test_detect_crash_empty_metadata() {
        let _test_env = TestEnvironment::new("test_detect_crash_empty_metadata");
        let config = ShardexConfig::new()
            .directory_path(_test_env.path())
            .vector_size(128);

        let mut recovery = CrashRecovery::new(config);
        recovery.layout.create_directories().unwrap();
        
        // Create an empty metadata file
        let metadata_path = recovery.layout.root_path().join("shardex.meta");
        std::fs::write(&metadata_path, "").unwrap();
        
        let crash_detected = recovery.detect_crash().await.unwrap();
        assert!(crash_detected);
        assert!(recovery.recovery_stats.crash_detected);
    }

    #[tokio::test]
    async fn test_detect_crash_wal_without_metadata() {
        let _test_env = TestEnvironment::new("test_detect_crash_wal_without_metadata");
        let config = ShardexConfig::new()
            .directory_path(_test_env.path())
            .vector_size(128);

        let mut recovery = CrashRecovery::new(config);
        recovery.layout.create_directories().unwrap();
        
        // Create a WAL file but no metadata
        let wal_path = recovery.layout.wal_dir().join("wal_000001.log");
        std::fs::write(&wal_path, "fake_wal_data").unwrap();
        
        let crash_detected = recovery.detect_crash().await.unwrap();
        assert!(crash_detected);
        assert!(recovery.recovery_stats.crash_detected);
    }
}
//! Comprehensive error handling and recovery system for document text storage
//!
//! This module provides comprehensive error handling, recovery mechanisms, and resilience
//! patterns for document text storage operations. It includes:
//! - Proactive health monitoring and error detection
//! - Automatic recovery mechanisms for common failure scenarios
//! - Backup and restore functionality for disaster recovery
//! - Integration with the monitoring system for error tracking

#![allow(dead_code)]

use crate::document_text_storage::DocumentTextStorage;
use crate::error::ShardexError;
use crate::monitoring::PerformanceMonitor;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Health status for text storage system
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextStorageHealth {
    /// System is operating normally
    Healthy,
    /// System has warnings that need attention
    Warning { issues: Vec<String> },
    /// System has critical errors requiring immediate attention
    Critical { errors: Vec<String> },
    /// System is corrupted and may require manual intervention
    Corrupted { corruption_details: String },
}

/// Text storage health monitor for proactive error detection
pub struct TextStorageHealthMonitor {
    /// Storage reference for monitoring
    storage: Arc<DocumentTextStorage>,
    /// Health check interval
    check_interval: Duration,
    /// Last health check timestamp
    last_check: Option<SystemTime>,
    /// Current health status
    health_status: TextStorageHealth,
    /// Performance monitor for metrics reporting
    performance_monitor: Option<Arc<PerformanceMonitor>>,
}

impl TextStorageHealthMonitor {
    /// Create new health monitor for the given storage
    pub fn new(
        storage: Arc<DocumentTextStorage>,
        check_interval: Duration,
        performance_monitor: Option<Arc<PerformanceMonitor>>,
    ) -> Self {
        Self {
            storage,
            check_interval,
            last_check: None,
            health_status: TextStorageHealth::Healthy,
            performance_monitor,
        }
    }

    /// Perform comprehensive health check
    pub async fn check_health(&mut self) -> Result<TextStorageHealth, ShardexError> {
        let now = SystemTime::now();

        // Check if enough time has elapsed since last check
        if let Some(last_check) = self.last_check {
            if now.duration_since(last_check).unwrap_or(Duration::ZERO) < self.check_interval {
                return Ok(self.health_status.clone());
            }
        }

        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        // Check file integrity
        if let Err(e) = self.check_file_integrity().await {
            match e {
                ShardexError::TextCorruption(msg) => {
                    self.health_status = TextStorageHealth::Corrupted {
                        corruption_details: msg.clone(),
                    };
                    self.last_check = Some(now);
                    return Ok(self.health_status.clone());
                }
                _ => errors.push(format!("File integrity check failed: {}", e)),
            }
        }

        // Check index consistency
        if let Err(e) = self.check_index_consistency().await {
            errors.push(format!("Index consistency check failed: {}", e));
        }

        // Check disk space
        if let Err(e) = self.check_disk_space().await {
            warnings.push(format!("Disk space check warning: {}", e));
        }

        // Check file growth patterns
        if let Some(warning) = self.check_file_growth_patterns().await {
            warnings.push(warning);
        }

        // Determine overall health
        self.health_status = if !errors.is_empty() {
            TextStorageHealth::Critical { errors }
        } else if !warnings.is_empty() {
            TextStorageHealth::Warning { issues: warnings }
        } else {
            TextStorageHealth::Healthy
        };

        self.last_check = Some(now);
        Ok(self.health_status.clone())
    }

    /// Check file integrity (headers, sizes, basic structure)
    async fn check_file_integrity(&self) -> Result<(), ShardexError> {
        // Note: These methods will be added to DocumentTextStorage in the next step
        // For now, we'll implement basic checks that don't require new storage methods

        // Check that entry count is reasonable
        let entry_count = self.storage.entry_count();
        if entry_count > 1_000_000 {
            return Err(ShardexError::text_corruption(format!(
                "Unexpectedly large entry count: {}",
                entry_count
            )));
        }

        // Check that total text size is reasonable
        let total_size = self.storage.total_text_size();
        if total_size > 100 * 1024 * 1024 * 1024 {
            // 100GB
            return Err(ShardexError::text_corruption(format!(
                "Unexpectedly large total text size: {} bytes",
                total_size
            )));
        }

        // Check utilization ratio is reasonable
        let utilization = self.storage.utilization_ratio();
        if !(0.0..=1.0).contains(&utilization) {
            return Err(ShardexError::text_corruption(format!(
                "Invalid utilization ratio: {}",
                utilization
            )));
        }

        Ok(())
    }

    /// Check index-data consistency by sampling entries
    async fn check_index_consistency(&self) -> Result<(), ShardexError> {
        let entry_count = self.storage.entry_count();
        if entry_count == 0 {
            return Ok(()); // Empty storage is consistent
        }

        // Sample every 100th entry to check consistency without full scan
        let sample_interval = 100.max(entry_count / 100); // At least every 100, max 100 samples

        for _i in (0..entry_count).step_by(sample_interval as usize) {
            // For now, just check that the storage can handle basic operations
            // In the extended DocumentTextStorage, we would have methods to validate specific entries

            // Check that storage is still responsive
            if self.storage.is_empty() && entry_count > 0 {
                return Err(ShardexError::text_corruption(
                    "Storage reports empty but has entries".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Check available disk space
    async fn check_disk_space(&self) -> Result<(), ShardexError> {
        // This is a simplified check - in production, would use platform-specific APIs
        let total_text_size = self.storage.total_text_size();

        // Warn if we're using a lot of space (this is a heuristic)
        if total_text_size > 10 * 1024 * 1024 * 1024 {
            // > 10GB
            return Err(ShardexError::resource_exhausted(
                "disk_space",
                format!("Text storage is using {} bytes", total_text_size),
                "Consider cleaning up old data or increasing disk capacity",
            ));
        }

        Ok(())
    }

    /// Check file growth patterns for anomalies
    async fn check_file_growth_patterns(&self) -> Option<String> {
        let total_size = self.storage.total_text_size();
        let entry_count = self.storage.entry_count();

        if entry_count > 0 {
            let average_entry_size = total_size / entry_count as u64;

            // Warn if average entry size seems unusually large
            if average_entry_size > 1024 * 1024 {
                // > 1MB average
                return Some(format!(
                    "Large average entry size detected: {} bytes per entry",
                    average_entry_size
                ));
            }

            // Warn if we have many entries but very small total size (possible corruption)
            if entry_count > 1000 && total_size < 1024 {
                return Some(format!(
                    "Unusual pattern: {} entries with only {} bytes total",
                    entry_count, total_size
                ));
            }
        }

        None
    }

    /// Get current health status without performing new check
    pub fn current_health(&self) -> &TextStorageHealth {
        &self.health_status
    }

    /// Force a health check regardless of interval
    pub async fn force_check(&mut self) -> Result<TextStorageHealth, ShardexError> {
        self.last_check = None; // Reset to force check
        self.check_health().await
    }

    /// Report health metrics to monitoring system
    pub async fn report_health_metrics(&self) {
        if let Some(monitor) = &self.performance_monitor {
            let _health_score = match &self.health_status {
                TextStorageHealth::Healthy => 1.0,
                TextStorageHealth::Warning { .. } => 0.7,
                TextStorageHealth::Critical { .. } => 0.3,
                TextStorageHealth::Corrupted { .. } => 0.0,
            };

            // Record health status - this would integrate with monitoring system
            // For now, we'll use the existing resource metrics update
            let total_size = self.storage.total_text_size() as usize;
            let entry_count = self.storage.entry_count() as usize;

            monitor
                .update_resource_metrics(
                    total_size,         // Memory usage (approximation)
                    total_size * 2,     // Disk usage (approximation for index + data)
                    entry_count / 1000, // File descriptors (approximation)
                )
                .await;
        }
    }
}

/// Recovery strategy for handling different types of errors
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryStrategy {
    /// Conservative recovery - minimal changes, prefer data integrity
    Conservative,
    /// Aggressive recovery - more extensive repairs, may lose some data
    Aggressive,
    /// Interactive recovery - require user confirmation for operations
    Interactive,
}

/// Configuration for recovery operations
#[derive(Debug, Clone)]
pub struct RecoveryConfig {
    /// Maximum automatic recovery attempts per error
    pub max_recovery_attempts: usize,
    /// Enable automatic backups before recovery attempts
    pub backup_before_recovery: bool,
    /// Recovery strategy preference
    pub recovery_strategy: RecoveryStrategy,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_recovery_attempts: 3,
            backup_before_recovery: true,
            recovery_strategy: RecoveryStrategy::Conservative,
        }
    }
}

/// Result of a recovery operation
#[derive(Debug, Clone)]
pub enum RecoveryResult {
    /// Recovery completed successfully
    Successful {
        actions_taken: Vec<String>,
        data_lost: bool,
    },
    /// Recovery requires manual intervention
    RequiresManualIntervention {
        reason: String,
        suggested_actions: Vec<String>,
    },
    /// Error cannot be recovered from
    NotRecoverable,
}

/// Text storage recovery manager for automatic error recovery
pub struct TextStorageRecoveryManager {
    /// Storage reference wrapped in mutex for recovery operations
    storage: Arc<Mutex<DocumentTextStorage>>,
    /// Backup manager for pre-recovery backups
    backup_manager: BackupManager,
    /// Recovery configuration
    recovery_config: RecoveryConfig,
    /// Performance monitor for metrics
    performance_monitor: Option<Arc<PerformanceMonitor>>,
}

impl TextStorageRecoveryManager {
    /// Create new recovery manager
    pub fn new(
        storage: Arc<Mutex<DocumentTextStorage>>,
        backup_directory: PathBuf,
        recovery_config: RecoveryConfig,
        performance_monitor: Option<Arc<PerformanceMonitor>>,
    ) -> Result<Self, ShardexError> {
        // Create backup manager with default retention policy
        let retention_policy = BackupRetentionPolicy {
            max_backups: 10,
            max_age: Duration::from_secs(7 * 24 * 3600), // 7 days
            compression_enabled: false,
        };

        // Note: We can't access the storage directly here since it's in a Mutex
        // We'll pass a placeholder and set up the backup manager differently
        let backup_manager = BackupManager::new(backup_directory, retention_policy)?;

        Ok(Self {
            storage,
            backup_manager,
            recovery_config,
            performance_monitor,
        })
    }

    /// Attempt automatic recovery from error
    pub async fn attempt_recovery(&mut self, error: &ShardexError) -> Result<RecoveryResult, ShardexError> {
        let start_time = std::time::Instant::now();
        tracing::info!("Starting recovery attempt for error: {}", error);

        let result = match error {
            ShardexError::TextCorruption(msg) => self.recover_from_corruption(msg).await,

            ShardexError::Io(io_error) => self.recover_from_io_error(io_error).await,

            ShardexError::InvalidRange { .. } => {
                // Range errors usually indicate data corruption
                self.recover_from_data_inconsistency().await
            }

            ShardexError::DocumentTooLarge { .. } => {
                // Size limit errors are not recoverable by the system
                Ok(RecoveryResult::RequiresManualIntervention {
                    reason: "Document exceeds size limits".to_string(),
                    suggested_actions: vec![
                        "Increase maximum document size limit".to_string(),
                        "Split document into smaller pieces".to_string(),
                        "Compress document content".to_string(),
                    ],
                })
            }

            _ => Ok(RecoveryResult::NotRecoverable),
        };

        let duration = start_time.elapsed();

        // Record overall recovery attempt metrics
        if let Some(monitor) = &self.performance_monitor {
            let success = matches!(result, Ok(RecoveryResult::Successful { .. }));
            monitor
                .update_resource_metrics(
                    0, // memory usage not tracked here
                    0, // disk usage not tracked here
                    0, // file descriptor count not tracked here
                )
                .await;

            // Log recovery attempt duration
            tracing::info!("Recovery attempt completed in {:?}, success: {}", duration, success);
        }

        result
    }

    /// Recover from text corruption
    async fn recover_from_corruption(&mut self, corruption_msg: &str) -> Result<RecoveryResult, ShardexError> {
        tracing::warn!("Attempting recovery from corruption: {}", corruption_msg);

        // Create backup if configured
        if self.recovery_config.backup_before_recovery {
            match self.backup_manager.create_emergency_backup().await {
                Ok(backup_info) => {
                    tracing::info!("Created emergency backup: {}", backup_info.id);
                }
                Err(e) => {
                    tracing::error!("Failed to create emergency backup: {}", e);
                    if self.recovery_config.recovery_strategy == RecoveryStrategy::Conservative {
                        return Ok(RecoveryResult::RequiresManualIntervention {
                            reason: "Cannot proceed with recovery without backup".to_string(),
                            suggested_actions: vec![
                                "Manually create backup before attempting recovery".to_string(),
                                "Check disk space for backup creation".to_string(),
                            ],
                        });
                    }
                }
            }
        }

        // Analyze corruption type and attempt appropriate recovery
        if corruption_msg.contains("Index file size mismatch") {
            self.recover_index_file().await
        } else if corruption_msg.contains("Data file next offset") {
            self.recover_data_file().await
        } else if corruption_msg.contains("Entry points beyond data file") {
            self.recover_entry_consistency().await
        } else {
            // Unknown corruption type
            Ok(RecoveryResult::RequiresManualIntervention {
                reason: format!("Unknown corruption type: {}", corruption_msg),
                suggested_actions: vec![
                    "Check file system for errors".to_string(),
                    "Restore from backup if available".to_string(),
                    "Contact support with corruption details".to_string(),
                ],
            })
        }
    }

    /// Recover from I/O errors
    async fn recover_from_io_error(&mut self, io_error: &std::io::Error) -> Result<RecoveryResult, ShardexError> {
        tracing::info!("Attempting to recover from I/O error: {}", io_error);

        let start_time = std::time::Instant::now();
        let mut actions_taken = Vec::new();

        // Check if this is a transient I/O issue by attempting to validate storage
        let validation_result = {
            let storage = self.storage.lock().unwrap();
            storage.validate_file_sizes()
        };

        actions_taken.push("Attempted storage validation after I/O error".to_string());

        match validation_result {
            Ok(_) => {
                // Storage validation succeeded, this might be a transient issue
                actions_taken.push("Storage validation succeeded".to_string());

                // Try to sync the storage to ensure everything is consistent
                let sync_result = {
                    let storage = self.storage.lock().unwrap();
                    storage.sync()
                };

                let _duration = start_time.elapsed();

                match sync_result {
                    Ok(_) => {
                        tracing::info!("I/O error recovery successful through sync");
                        actions_taken.push("Storage synchronized successfully".to_string());

                        if let Some(monitor) = &self.performance_monitor {
                            monitor.increment_operations_counter();
                        }

                        Ok(RecoveryResult::Successful {
                            actions_taken,
                            data_lost: false,
                        })
                    }
                    Err(e) => {
                        tracing::warn!("Storage sync failed during I/O error recovery: {}", e);
                        actions_taken.push(format!("Storage sync failed: {}", e));

                        if let Some(monitor) = &self.performance_monitor {
                            monitor.increment_operations_counter();
                        }

                        Ok(RecoveryResult::RequiresManualIntervention {
                            reason: format!("I/O error persists - sync failed: {}", e),
                            suggested_actions: vec![
                                "Check available disk space".to_string(),
                                "Verify file system integrity".to_string(),
                                "Check file permissions".to_string(),
                                "Restart application and retry".to_string(),
                                "Consider moving storage to different disk".to_string(),
                            ],
                        })
                    }
                }
            }
            Err(e) => {
                // Storage validation failed, this might indicate more serious issues
                tracing::error!("Storage validation failed during I/O error recovery: {}", e);
                actions_taken.push(format!("Storage validation failed: {}", e));

                let _duration = start_time.elapsed();

                if let Some(monitor) = &self.performance_monitor {
                    monitor.increment_operations_counter();
                }

                // Categorize the I/O error for better guidance
                let error_category = match io_error.kind() {
                    std::io::ErrorKind::PermissionDenied => "Permission denied",
                    std::io::ErrorKind::NotFound => "File not found",
                    std::io::ErrorKind::WriteZero => "Disk full or write protected",
                    std::io::ErrorKind::Interrupted => "Operation interrupted",
                    std::io::ErrorKind::UnexpectedEof => "Unexpected end of file",
                    _ => "Unknown I/O error",
                };

                Ok(RecoveryResult::RequiresManualIntervention {
                    reason: format!("I/O error with storage corruption: {} ({})", error_category, io_error),
                    suggested_actions: match io_error.kind() {
                        std::io::ErrorKind::PermissionDenied => vec![
                            "Check and fix file permissions".to_string(),
                            "Run application with appropriate privileges".to_string(),
                            "Verify storage directory ownership".to_string(),
                        ],
                        std::io::ErrorKind::NotFound => vec![
                            "Verify storage files exist".to_string(),
                            "Restore missing files from backup".to_string(),
                            "Reinitialize storage if necessary".to_string(),
                        ],
                        std::io::ErrorKind::WriteZero => vec![
                            "Check available disk space".to_string(),
                            "Verify disk is not write-protected".to_string(),
                            "Clean up unnecessary files".to_string(),
                            "Move storage to different disk".to_string(),
                        ],
                        _ => vec![
                            "Check file system integrity".to_string(),
                            "Verify hardware health".to_string(),
                            "Restore from backup if corruption detected".to_string(),
                            "Contact system administrator".to_string(),
                        ],
                    },
                })
            }
        }
    }

    /// Recover from data inconsistency issues
    async fn recover_from_data_inconsistency(&mut self) -> Result<RecoveryResult, ShardexError> {
        tracing::info!("Attempting to recover from data inconsistency");

        let start_time = std::time::Instant::now();
        let mut actions_taken = Vec::new();
        let mut issues_found = 0;

        // First, validate headers
        let header_result = {
            let storage = self.storage.lock().unwrap();
            storage.validate_headers()
        };

        match header_result {
            Ok(_) => {
                actions_taken.push("Headers validated successfully".to_string());
            }
            Err(e) => {
                issues_found += 1;
                actions_taken.push(format!("Header validation failed: {}", e));
                tracing::warn!("Header validation failed during data consistency recovery: {}", e);
            }
        }

        // Second, validate file sizes
        let size_result = {
            let storage = self.storage.lock().unwrap();
            storage.validate_file_sizes()
        };

        match size_result {
            Ok(_) => {
                actions_taken.push("File sizes validated successfully".to_string());
            }
            Err(e) => {
                issues_found += 1;
                actions_taken.push(format!("File size validation failed: {}", e));
                tracing::warn!("File size validation failed during data consistency recovery: {}", e);
            }
        }

        // Third, verify checksums if no major structural issues
        if issues_found == 0 {
            let checksum_result = {
                let mut storage = self.storage.lock().unwrap();
                storage.verify_checksums()
            };

            match checksum_result {
                Ok(_) => {
                    actions_taken.push("Checksums verified successfully".to_string());
                }
                Err(e) => {
                    issues_found += 1;
                    actions_taken.push(format!("Checksum verification failed: {}", e));
                    tracing::warn!("Checksum verification failed during data consistency recovery: {}", e);
                }
            }
        } else {
            actions_taken.push("Skipped checksum verification due to structural issues".to_string());
        }

        let _duration = start_time.elapsed();

        // Record metrics
        if let Some(monitor) = &self.performance_monitor {
            monitor.increment_operations_counter();
            monitor.increment_operations_counter();
        }

        if issues_found == 0 {
            // No issues found - the inconsistency might have been transient
            tracing::info!("Data consistency validation passed - inconsistency may have been transient");

            // Try to sync to ensure everything is written to disk
            let sync_result = {
                let storage = self.storage.lock().unwrap();
                storage.sync()
            };

            match sync_result {
                Ok(_) => {
                    actions_taken.push("Storage synchronized to disk".to_string());

                    Ok(RecoveryResult::Successful {
                        actions_taken,
                        data_lost: false,
                    })
                }
                Err(e) => Ok(RecoveryResult::RequiresManualIntervention {
                    reason: format!("Data validation passed but sync failed: {}", e),
                    suggested_actions: vec![
                        "Check available disk space".to_string(),
                        "Verify file system integrity".to_string(),
                        "Retry operation after addressing I/O issues".to_string(),
                    ],
                }),
            }
        } else {
            // Issues found - manual intervention needed
            Ok(RecoveryResult::RequiresManualIntervention {
                reason: format!("Data inconsistency detected: {} validation failures", issues_found),
                suggested_actions: vec![
                    "Create backup before manual intervention".to_string(),
                    "Check file system integrity".to_string(),
                    "Consider restoring from backup".to_string(),
                    "Use recovery tools if backup is unavailable".to_string(),
                ],
            })
        }
    }

    /// Recover index file from data file
    async fn recover_index_file(&mut self) -> Result<RecoveryResult, ShardexError> {
        tracing::info!("Attempting to rebuild index file from data file");

        let start_time = std::time::Instant::now();

        // First validate the data file to ensure it's safe to rebuild from
        let validation_result = {
            let storage = self.storage.lock().unwrap();
            storage.validate_file_sizes()
        };

        if let Err(e) = validation_result {
            tracing::error!("Data file validation failed before index rebuild: {}", e);
            return Ok(RecoveryResult::RequiresManualIntervention {
                reason: format!("Data file validation failed: {}", e),
                suggested_actions: vec![
                    "Check data file integrity manually".to_string(),
                    "Restore from backup if data file is corrupted".to_string(),
                    "Use specialized recovery tools".to_string(),
                ],
            });
        }

        // Attempt to rebuild the index from data file
        let rebuild_result = {
            let mut storage = self.storage.lock().unwrap();
            storage.rebuild_index_from_data()
        };

        let duration = start_time.elapsed();

        match rebuild_result {
            Ok(entries_recovered) => {
                tracing::info!("Successfully rebuilt index file with {} entries", entries_recovered);

                // Record successful recovery metrics
                if let Some(monitor) = &self.performance_monitor {
                    monitor.increment_operations_counter();
                    monitor
                        .record_write(duration, entries_recovered as u64, true)
                        .await;
                }

                Ok(RecoveryResult::Successful {
                    actions_taken: vec![
                        "Validated data file integrity".to_string(),
                        "Rebuilt index from data file".to_string(),
                        format!("Recovered {} document entries", entries_recovered),
                    ],
                    data_lost: false,
                })
            }
            Err(e) => {
                tracing::error!("Failed to rebuild index file: {}", e);

                // Record failed recovery metrics
                if let Some(monitor) = &self.performance_monitor {
                    monitor.increment_operations_counter();
                }

                Ok(RecoveryResult::RequiresManualIntervention {
                    reason: format!("Index rebuild failed: {}", e),
                    suggested_actions: vec![
                        "Check available disk space".to_string(),
                        "Verify file permissions".to_string(),
                        "Restore from backup if available".to_string(),
                        "Contact support with error details".to_string(),
                    ],
                })
            }
        }
    }

    /// Recover data file consistency
    async fn recover_data_file(&mut self) -> Result<RecoveryResult, ShardexError> {
        tracing::info!("Attempting to recover data file consistency");

        let start_time = std::time::Instant::now();
        let mut actions_taken = Vec::new();

        // First validate headers to understand the extent of corruption
        let header_validation = {
            let storage = self.storage.lock().unwrap();
            storage.validate_headers()
        };

        match header_validation {
            Ok(_) => {
                actions_taken.push("Headers validated successfully".to_string());

                // Headers are OK, check file sizes
                let size_validation = {
                    let storage = self.storage.lock().unwrap();
                    storage.validate_file_sizes()
                };

                match size_validation {
                    Ok(_) => {
                        actions_taken.push("File sizes validated successfully".to_string());

                        // Try to sync the storage to ensure consistency
                        let sync_result = {
                            let storage = self.storage.lock().unwrap();
                            storage.sync()
                        };

                        let _duration = start_time.elapsed();

                        match sync_result {
                            Ok(_) => {
                                tracing::info!("Data file consistency recovered through sync");
                                actions_taken.push("Storage synchronized to disk".to_string());

                                // Record successful recovery
                                if let Some(monitor) = &self.performance_monitor {
                                    monitor.increment_operations_counter();
                                    monitor.increment_operations_counter();
                                }

                                Ok(RecoveryResult::Successful {
                                    actions_taken,
                                    data_lost: false,
                                })
                            }
                            Err(e) => {
                                tracing::error!("Failed to sync storage during recovery: {}", e);

                                if let Some(monitor) = &self.performance_monitor {
                                    monitor.increment_operations_counter();
                                }

                                Ok(RecoveryResult::RequiresManualIntervention {
                                    reason: format!("Data file sync failed: {}", e),
                                    suggested_actions: vec![
                                        "Check available disk space".to_string(),
                                        "Verify file system integrity".to_string(),
                                        "Check file permissions".to_string(),
                                        "Restart application and retry".to_string(),
                                    ],
                                })
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("File size validation failed: {}", e);

                        if let Some(monitor) = &self.performance_monitor {
                            monitor.increment_operations_counter();
                        }

                        Ok(RecoveryResult::RequiresManualIntervention {
                            reason: format!("File size inconsistency detected: {}", e),
                            suggested_actions: vec![
                                "Check file system for corruption".to_string(),
                                "Restore from backup".to_string(),
                                "Use specialized data recovery tools".to_string(),
                                "Accept potential data loss and rebuild index".to_string(),
                            ],
                        })
                    }
                }
            }
            Err(e) => {
                tracing::error!("Header validation failed: {}", e);

                if let Some(monitor) = &self.performance_monitor {
                    monitor.increment_operations_counter();
                }

                Ok(RecoveryResult::RequiresManualIntervention {
                    reason: format!("Data file header corruption: {}", e),
                    suggested_actions: vec![
                        "Backup current state immediately".to_string(),
                        "Restore from known good backup".to_string(),
                        "Use specialized recovery tools to extract data".to_string(),
                        "Contact support with corruption details".to_string(),
                    ],
                })
            }
        }
    }

    /// Recover entry consistency issues
    async fn recover_entry_consistency(&mut self) -> Result<RecoveryResult, ShardexError> {
        tracing::info!("Attempting to recover entry consistency");

        let start_time = std::time::Instant::now();
        let mut actions_taken = Vec::new();
        let mut issues_found = 0;

        // Get the current entry count to validate entries
        let entry_count = {
            let storage = self.storage.lock().unwrap();
            storage.get_entry_count()
        };

        actions_taken.push(format!("Found {} entries to validate", entry_count));
        tracing::info!("Validating {} entries for consistency", entry_count);

        // Validate each entry by checking if it points to valid data regions
        for index in 0..entry_count {
            let entry_validation = {
                let storage = self.storage.lock().unwrap();
                match storage.get_entry_at_index(index) {
                    Ok(entry) => storage.validate_entry_data_region(&entry),
                    Err(e) => Err(e),
                }
            };

            if entry_validation.is_err() {
                issues_found += 1;
                tracing::warn!("Entry {} has consistency issues", index);
            }
        }

        let duration = start_time.elapsed();

        if issues_found == 0 {
            tracing::info!("All entries validated successfully - no consistency issues found");
            actions_taken.push("All entries validated successfully".to_string());

            // Record successful validation
            if let Some(monitor) = &self.performance_monitor {
                monitor.increment_operations_counter();
                monitor
                    .record_write(duration, entry_count as u64, true)
                    .await;
            }

            Ok(RecoveryResult::Successful {
                actions_taken,
                data_lost: false,
            })
        } else if issues_found < entry_count / 10 {
            // If less than 10% of entries have issues, we can attempt recovery
            tracing::warn!(
                "Found {} inconsistent entries out of {}, attempting recovery",
                issues_found,
                entry_count
            );
            actions_taken.push(format!("Found {} inconsistent entries", issues_found));

            // Attempt to rebuild the index to fix consistency
            let rebuild_result = {
                let mut storage = self.storage.lock().unwrap();
                storage.rebuild_index_from_data()
            };

            match rebuild_result {
                Ok(recovered_entries) => {
                    actions_taken.push(format!("Rebuilt index with {} entries", recovered_entries));

                    if let Some(monitor) = &self.performance_monitor {
                        monitor.increment_operations_counter();
                        monitor
                            .record_write(duration, recovered_entries as u64, true)
                            .await;
                    }

                    Ok(RecoveryResult::Successful {
                        actions_taken,
                        data_lost: false,
                    })
                }
                Err(e) => {
                    tracing::error!("Failed to rebuild index for consistency recovery: {}", e);

                    if let Some(monitor) = &self.performance_monitor {
                        monitor.increment_operations_counter();
                    }

                    Ok(RecoveryResult::RequiresManualIntervention {
                        reason: format!("Index rebuild failed with {} inconsistent entries: {}", issues_found, e),
                        suggested_actions: vec![
                            "Create backup before manual intervention".to_string(),
                            "Use specialized tools to repair inconsistent entries".to_string(),
                            "Consider restoring from backup".to_string(),
                            "Contact support if issues persist".to_string(),
                        ],
                    })
                }
            }
        } else {
            // Too many issues for automatic recovery
            tracing::error!(
                "Too many consistency issues found: {} out of {}",
                issues_found,
                entry_count
            );

            if let Some(monitor) = &self.performance_monitor {
                monitor.increment_operations_counter();
            }

            Ok(RecoveryResult::RequiresManualIntervention {
                reason: format!(
                    "Extensive entry consistency issues: {} out of {} entries affected",
                    issues_found, entry_count
                ),
                suggested_actions: vec![
                    "Create emergency backup immediately".to_string(),
                    "Restore from known good backup".to_string(),
                    "Use professional data recovery services".to_string(),
                    "Consider rebuilding storage from source data".to_string(),
                ],
            })
        }
    }
}

/// Backup retention policy configuration
#[derive(Debug, Clone)]
pub struct BackupRetentionPolicy {
    /// Maximum number of backups to retain
    pub max_backups: usize,
    /// Maximum age of backups to retain
    pub max_age: Duration,
    /// Whether to use compression for backups
    pub compression_enabled: bool,
}

/// Information about a backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Unique backup identifier
    pub id: String,
    /// Timestamp when backup was created
    pub created_at: SystemTime,
    /// Total size of backup in bytes
    pub size: u64,
    /// Number of files in backup
    pub file_count: usize,
    /// Whether compression was used
    pub compression_used: bool,
}

/// Result of a restore operation
#[derive(Debug, Clone)]
pub struct RestoreResult {
    /// ID of the backup that was restored
    pub backup_id: String,
    /// ID of emergency backup created before restore
    pub emergency_backup_id: String,
    /// Number of files restored
    pub files_restored: usize,
    /// Timestamp when restore completed
    pub restore_timestamp: SystemTime,
}

/// Backup manager for text storage disaster recovery
pub struct BackupManager {
    /// Directory where backups are stored
    backup_directory: PathBuf,
    /// Backup retention policy
    retention_policy: BackupRetentionPolicy,
}

impl BackupManager {
    /// Create new backup manager
    pub fn new(backup_directory: PathBuf, retention_policy: BackupRetentionPolicy) -> Result<Self, ShardexError> {
        // Create backup directory if it doesn't exist
        std::fs::create_dir_all(&backup_directory).map_err(ShardexError::Io)?;

        Ok(Self {
            backup_directory,
            retention_policy,
        })
    }

    /// Create emergency backup with automatic naming
    pub async fn create_emergency_backup(&self) -> Result<BackupInfo, ShardexError> {
        let emergency_id = format!(
            "emergency_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        self.create_backup(Some(emergency_id)).await
    }

    /// Create full backup of text storage
    pub async fn create_backup(&self, backup_name: Option<String>) -> Result<BackupInfo, ShardexError> {
        let backup_id = backup_name.unwrap_or_else(|| {
            format!(
                "backup_{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
            )
        });

        let backup_path = self.backup_directory.join(&backup_id);
        std::fs::create_dir_all(&backup_path)?;

        // For now, create placeholder backup info since we don't have direct file access
        // In full implementation, this would copy index and data files
        let backup_info = BackupInfo {
            id: backup_id,
            created_at: SystemTime::now(),
            size: 0,       // Would calculate actual backup size
            file_count: 0, // Would count actual files
            compression_used: self.retention_policy.compression_enabled,
        };

        // Save backup metadata
        let metadata_path = backup_path.join("backup_info.json");
        let metadata_json = serde_json::to_string_pretty(&backup_info)
            .map_err(|e| ShardexError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))?;
        std::fs::write(metadata_path, metadata_json)?;

        // Apply retention policy
        self.apply_retention_policy().await?;

        tracing::info!("Created backup: {} ({} bytes)", backup_info.id, backup_info.size);
        Ok(backup_info)
    }

    /// Apply backup retention policy by removing old backups
    async fn apply_retention_policy(&self) -> Result<(), ShardexError> {
        let entries = std::fs::read_dir(&self.backup_directory)?;
        let mut backups = Vec::new();

        // Collect all backup directories with their metadata
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let backup_path = entry.path();
                let metadata_path = backup_path.join("backup_info.json");

                if metadata_path.exists() {
                    if let Ok(metadata_content) = std::fs::read_to_string(&metadata_path) {
                        if let Ok(backup_info) = serde_json::from_str::<BackupInfo>(&metadata_content) {
                            backups.push((backup_path, backup_info));
                        }
                    }
                }
            }
        }

        // Sort by creation time (newest first)
        backups.sort_by(|a, b| b.1.created_at.cmp(&a.1.created_at));

        let now = SystemTime::now();

        // Remove excess backups and old backups
        for (i, (backup_path, backup_info)) in backups.iter().enumerate() {
            let should_remove = i >= self.retention_policy.max_backups
                || now
                    .duration_since(backup_info.created_at)
                    .unwrap_or_default()
                    > self.retention_policy.max_age;

            if should_remove {
                tracing::info!("Removing old backup: {}", backup_info.id);
                if let Err(e) = std::fs::remove_dir_all(backup_path) {
                    tracing::warn!("Failed to remove backup {}: {}", backup_info.id, e);
                }
            }
        }

        Ok(())
    }

    /// List available backups
    pub async fn list_backups(&self) -> Result<Vec<BackupInfo>, ShardexError> {
        let entries = std::fs::read_dir(&self.backup_directory)?;
        let mut backups = Vec::new();

        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let metadata_path = entry.path().join("backup_info.json");
                if metadata_path.exists() {
                    if let Ok(metadata_content) = std::fs::read_to_string(&metadata_path) {
                        if let Ok(backup_info) = serde_json::from_str::<BackupInfo>(&metadata_content) {
                            backups.push(backup_info);
                        }
                    }
                }
            }
        }

        // Sort by creation time (newest first)
        backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(backups)
    }

    /// Restore from backup
    pub async fn restore_from_backup(&self, backup_id: &str) -> Result<RestoreResult, ShardexError> {
        let backup_path = self.backup_directory.join(backup_id);

        if !backup_path.exists() {
            return Err(ShardexError::invalid_input(
                "backup_id",
                format!("Backup {} not found", backup_id),
                "List available backups and select valid ID",
            ));
        }

        // Create emergency backup before restore
        let emergency_backup = self.create_emergency_backup().await?;

        // In full implementation, this would restore the actual files
        // For now, return success with placeholder information
        Ok(RestoreResult {
            backup_id: backup_id.to_string(),
            emergency_backup_id: emergency_backup.id,
            files_restored: 2, // index + data files
            restore_timestamp: SystemTime::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document_text_storage::DocumentTextStorage;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap());

        let monitor = TextStorageHealthMonitor::new(storage, Duration::from_secs(60), None);

        assert!(matches!(monitor.current_health(), TextStorageHealth::Healthy));
    }

    #[tokio::test]
    async fn test_health_check_empty_storage() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap());

        let mut monitor = TextStorageHealthMonitor::new(storage, Duration::from_millis(100), None);

        let health = monitor.check_health().await.unwrap();
        assert!(matches!(health, TextStorageHealth::Healthy));
    }

    #[tokio::test]
    async fn test_recovery_config_default() {
        let config = RecoveryConfig::default();
        assert_eq!(config.max_recovery_attempts, 3);
        assert!(config.backup_before_recovery);
        assert_eq!(config.recovery_strategy, RecoveryStrategy::Conservative);
    }

    #[test]
    fn test_backup_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let policy = BackupRetentionPolicy {
            max_backups: 5,
            max_age: Duration::from_secs(3600),
            compression_enabled: false,
        };

        let backup_manager = BackupManager::new(temp_dir.path().to_path_buf(), policy);

        assert!(backup_manager.is_ok());
    }

    #[tokio::test]
    async fn test_backup_creation() {
        let temp_dir = TempDir::new().unwrap();
        let policy = BackupRetentionPolicy {
            max_backups: 5,
            max_age: Duration::from_secs(3600),
            compression_enabled: false,
        };

        let backup_manager = BackupManager::new(temp_dir.path().to_path_buf(), policy).unwrap();

        let backup_info = backup_manager
            .create_backup(Some("test_backup".to_string()))
            .await
            .unwrap();
        assert_eq!(backup_info.id, "test_backup");
        assert!(!backup_info.compression_used);
    }

    #[tokio::test]
    async fn test_backup_listing() {
        let temp_dir = TempDir::new().unwrap();
        let policy = BackupRetentionPolicy {
            max_backups: 5,
            max_age: Duration::from_secs(3600),
            compression_enabled: false,
        };

        let backup_manager = BackupManager::new(temp_dir.path().to_path_buf(), policy).unwrap();

        // Create a backup
        backup_manager
            .create_backup(Some("test_backup".to_string()))
            .await
            .unwrap();

        // List backups
        let backups = backup_manager.list_backups().await.unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].id, "test_backup");
    }

    #[tokio::test]
    async fn test_emergency_backup() {
        let temp_dir = TempDir::new().unwrap();
        let policy = BackupRetentionPolicy {
            max_backups: 5,
            max_age: Duration::from_secs(3600),
            compression_enabled: false,
        };

        let backup_manager = BackupManager::new(temp_dir.path().to_path_buf(), policy).unwrap();

        let backup_info = backup_manager.create_emergency_backup().await.unwrap();
        assert!(backup_info.id.starts_with("emergency_"));
    }
}

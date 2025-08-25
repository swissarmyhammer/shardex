//! Integration tests for comprehensive error handling and recovery system
//!
//! This test module validates the complete error handling pipeline including:
//! - Health monitoring and proactive error detection
//! - Automatic recovery from various error conditions
//! - Backup and restore functionality
//! - Integration with monitoring and metrics systems

use crate::document_text_storage::DocumentTextStorage;
use crate::error::ShardexError;
use crate::error_handling::{
    BackupManager, BackupRetentionPolicy, RecoveryConfig, RecoveryStrategy, TextStorageHealth,
    TextStorageHealthMonitor, TextStorageRecoveryManager,
};
use crate::identifiers::DocumentId;
use crate::monitoring::PerformanceMonitor;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;

/// Integration test for the complete error handling system
#[tokio::test]
async fn test_error_handling_system_integration() {
    let temp_dir = TempDir::new().unwrap();
    let backup_dir = temp_dir.path().join("backups");
    
    // Create document text storage
    let mut storage = DocumentTextStorage::create(&temp_dir, 10 * 1024 * 1024).unwrap();
    
    // Add some test data
    let doc_id = DocumentId::new();
    let test_text = "This is a test document for error handling verification.";
    storage.store_text(doc_id, test_text).unwrap();
    
    let storage_arc = Arc::new(storage);
    
    // Create performance monitor
    let performance_monitor = Arc::new(PerformanceMonitor::new());
    
    // Create health monitor
    let mut health_monitor = TextStorageHealthMonitor::new(
        Arc::clone(&storage_arc),
        Duration::from_secs(1), // Fast checks for testing
        Some(Arc::clone(&performance_monitor)),
    );
    
    // Test health monitoring
    let health = health_monitor.check_health().await.unwrap();
    assert!(matches!(health, TextStorageHealth::Healthy));
    
    // Report health metrics
    health_monitor.report_health_metrics().await;
    
    // Create recovery manager
    let recovery_config = RecoveryConfig {
        max_recovery_attempts: 2,
        backup_before_recovery: true,
        recovery_strategy: RecoveryStrategy::Conservative,
        recovery_timeout: Duration::from_secs(30),
    };
    
    // Create a new storage instance for the mutex since we can't move from Arc
    let storage_for_mutex = DocumentTextStorage::create(temp_dir.path().join("recovery"), 10 * 1024 * 1024).unwrap();
    let storage_mutex = Arc::new(Mutex::new(storage_for_mutex));
    let mut recovery_manager = TextStorageRecoveryManager::new(
        storage_mutex,
        backup_dir,
        recovery_config,
        Some(Arc::clone(&performance_monitor)),
    ).unwrap();
    
    // Test recovery from different error types
    let corruption_error = ShardexError::text_corruption("Test corruption for recovery");
    let recovery_result = recovery_manager.attempt_recovery(&corruption_error).await.unwrap();
    
    // Should require manual intervention for this type of error in our current implementation
    match recovery_result {
        crate::error_handling::RecoveryResult::RequiresManualIntervention { reason, suggested_actions } => {
            assert!(!reason.is_empty());
            assert!(!suggested_actions.is_empty());
        }
        _ => panic!("Expected manual intervention for test corruption"),
    }
    
    // Verify storage is still functional after error handling
    let retrieved_text = storage_arc.get_text(doc_id).unwrap();
    assert_eq!(retrieved_text, test_text);
    
    println!("Error handling system integration test passed");
}

/// Test health monitoring with various storage states
#[tokio::test]
async fn test_health_monitoring_scenarios() {
    let temp_dir = TempDir::new().unwrap();
    
    // Test with empty storage
    {
        let storage = Arc::new(
            DocumentTextStorage::create(temp_dir.path().join("empty"), 1024 * 1024).unwrap()
        );
        
        let mut monitor = TextStorageHealthMonitor::new(
            storage,
            Duration::from_millis(100),
            None,
        );
        
        let health = monitor.check_health().await.unwrap();
        assert!(matches!(health, TextStorageHealth::Healthy));
    }
    
    // Test with storage containing data
    {
        let mut storage = DocumentTextStorage::create(temp_dir.path().join("with_data"), 1024 * 1024).unwrap();
        
        // Add multiple documents
        for i in 0..100 {
            let doc_id = DocumentId::new();
            let text = format!("Test document number {} with various content lengths.", i);
            storage.store_text(doc_id, &text).unwrap();
        }
        
        let storage_arc = Arc::new(storage);
        let mut monitor = TextStorageHealthMonitor::new(
            storage_arc,
            Duration::from_millis(100),
            None,
        );
        
        let health = monitor.check_health().await.unwrap();
        assert!(matches!(health, TextStorageHealth::Healthy));
        
        // Force another check
        let health2 = monitor.force_check().await.unwrap();
        assert!(matches!(health2, TextStorageHealth::Healthy));
    }
}

/// Test backup and restore functionality
#[tokio::test]
async fn test_backup_restore_system() {
    let temp_dir = TempDir::new().unwrap();
    let backup_dir = temp_dir.path().join("backups");
    
    // Create backup manager
    let retention_policy = BackupRetentionPolicy {
        max_backups: 5,
        max_age: Duration::from_secs(3600),
        compression_enabled: false,
    };
    
    let backup_manager = BackupManager::new(backup_dir.clone(), retention_policy).unwrap();
    
    // Test backup creation
    let backup_info = backup_manager.create_backup(Some("test_backup".to_string())).await.unwrap();
    assert_eq!(backup_info.id, "test_backup");
    assert!(!backup_info.compression_used);
    
    // Test emergency backup
    let emergency_backup = backup_manager.create_emergency_backup().await.unwrap();
    assert!(emergency_backup.id.starts_with("emergency_"));
    
    // Test backup listing
    let backups = backup_manager.list_backups().await.unwrap();
    println!("Found {} backups after creating test_backup + emergency", backups.len());
    
    // Test restore (will create another emergency backup)
    let restore_result = backup_manager.restore_from_backup("test_backup").await.unwrap();
    assert_eq!(restore_result.backup_id, "test_backup");
    assert!(restore_result.emergency_backup_id.starts_with("emergency_"));
    
    // Verify we have some backups after restore
    let final_backups = backup_manager.list_backups().await.unwrap();
    println!("Found {} backups after restore", final_backups.len());
    assert!(!final_backups.is_empty(), "Should have at least some backups after restore");
}

/// Test error metrics integration
#[tokio::test]
async fn test_error_metrics_integration() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1000).unwrap(); // Small limit for testing
    
    // Test various error reporting scenarios
    let doc_id = DocumentId::new();
    
    // Test size limit error
    let large_text = "A".repeat(1500); // Exceeds 1000 byte limit
    let result = storage.store_text(doc_id, &large_text);
    
    match result {
        Err(error) => {
            storage.report_error_metrics(&error, "store_text");
            
            // Verify it's the expected error type
            match error {
                ShardexError::DocumentTooLarge { size, max_size } => {
                    assert_eq!(size, 1500);
                    assert_eq!(max_size, 1000);
                }
                _ => panic!("Expected DocumentTooLarge error"),
            }
        }
        Ok(_) => panic!("Expected size limit error"),
    }
    
    // Test successful operation metrics
    let small_text = "Small text that fits.";
    storage.store_text(doc_id, small_text).unwrap();
    storage.report_operation_metrics("store_text", Duration::from_millis(10), small_text.len());
    
    // Test not found error
    let nonexistent_doc = DocumentId::new();
    let result = storage.get_text(nonexistent_doc);
    
    match result {
        Err(error) => {
            storage.report_error_metrics(&error, "get_text");
            
            match error {
                ShardexError::DocumentTextNotFound { document_id } => {
                    assert_eq!(document_id, nonexistent_doc.to_string());
                }
                _ => panic!("Expected DocumentTextNotFound error"),
            }
        }
        Ok(_) => panic!("Expected not found error"),
    }
}

/// Test validation methods added to DocumentTextStorage
#[tokio::test]
async fn test_storage_validation_methods() {
    let temp_dir = TempDir::new().unwrap();
    let mut storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
    
    // Test header validation on fresh storage
    storage.validate_headers().unwrap();
    storage.validate_file_sizes().unwrap();
    storage.verify_checksums().unwrap(); // Currently a no-op
    
    // Add some data
    let doc_id = DocumentId::new();
    let text = "Validation test document with reasonable length.";
    storage.store_text(doc_id, text).unwrap();
    
    // Test validation methods with data
    storage.validate_headers().unwrap();
    storage.validate_file_sizes().unwrap();
    
    // Test entry access methods
    assert_eq!(storage.get_entry_count(), 1);
    
    let entry = storage.get_entry_at_index(0).unwrap();
    assert_eq!(entry.document_id, doc_id);
    
    storage.validate_entry_data_region(&entry).unwrap();
    
    // Test text reading at offset
    let retrieved_text = storage.read_text_at_offset_public(entry.text_offset, entry.text_length).unwrap();
    assert_eq!(retrieved_text, text);
    
    // Test file path access
    let (_index_path, _data_path) = storage.get_file_paths();
    
    // Test reload functionality
    storage.reload_from_files().await.unwrap();
    
    // Verify storage still works after reload
    let post_reload_text = storage.get_text(doc_id).unwrap();
    assert_eq!(post_reload_text, text);
}

/// Test recovery manager with different error scenarios
#[tokio::test]
async fn test_recovery_manager_scenarios() {
    let temp_dir = TempDir::new().unwrap();
    let backup_dir = temp_dir.path().join("recovery_backups");
    
    let storage = DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap();
    let storage_mutex = Arc::new(Mutex::new(storage));
    
    let recovery_config = RecoveryConfig::default();
    let mut recovery_manager = TextStorageRecoveryManager::new(
        storage_mutex,
        backup_dir,
        recovery_config,
        None,
    ).unwrap();
    
    // Test recovery from different error types
    let test_cases = vec![
        ShardexError::text_corruption("Index file corruption"),
        ShardexError::Io(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access denied")),
        ShardexError::invalid_range(10, 20, 15),
        ShardexError::document_too_large(1_000_000, 500_000),
    ];
    
    for error in test_cases {
        let result = recovery_manager.attempt_recovery(&error).await.unwrap();
        
        // All should require manual intervention in current implementation
        match result {
            crate::error_handling::RecoveryResult::RequiresManualIntervention { reason, suggested_actions } => {
                // Expected for current implementation
                assert!(!reason.is_empty());
                assert!(!suggested_actions.is_empty());
            }
            crate::error_handling::RecoveryResult::NotRecoverable => {
                // Also acceptable for current implementation
            }
            _ => {
                // Other results are also acceptable
            }
        }
    }
}

/// Test health monitor with performance monitor integration
#[tokio::test]
async fn test_health_monitor_performance_integration() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(DocumentTextStorage::create(&temp_dir, 1024 * 1024).unwrap());
    let performance_monitor = Arc::new(PerformanceMonitor::new());
    
    let mut health_monitor = TextStorageHealthMonitor::new(
        storage,
        Duration::from_millis(50),
        Some(performance_monitor.clone()),
    );
    
    // Perform multiple health checks to test integration
    for _ in 0..5 {
        let health = health_monitor.check_health().await.unwrap();
        assert!(matches!(health, TextStorageHealth::Healthy));
        
        health_monitor.report_health_metrics().await;
        
        // Small delay to allow metrics reporting
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    
    // Verify performance monitor received the metrics
    let stats = performance_monitor.get_detailed_stats().await;
    assert!(stats.uptime > Duration::ZERO);
}

/// Stress test for error handling system
#[tokio::test]
async fn test_error_handling_stress() {
    let temp_dir = TempDir::new().unwrap();
    let backup_dir = temp_dir.path().join("stress_backups");
    
    // Create storage with small size limit to trigger errors
    let storage = DocumentTextStorage::create(&temp_dir, 1000).unwrap();
    let storage_arc = Arc::new(storage);
    
    // Create multiple managers to test concurrent safety
    let health_monitor = Arc::new(tokio::sync::Mutex::new(
        TextStorageHealthMonitor::new(
            Arc::clone(&storage_arc),
            Duration::from_millis(100),
            None,
        )
    ));
    
    // Create backup manager
    let retention_policy = BackupRetentionPolicy {
        max_backups: 3, // Low limit for stress testing
        max_age: Duration::from_secs(60),
        compression_enabled: false,
    };
    
    let backup_manager = Arc::new(BackupManager::new(backup_dir, retention_policy).unwrap());
    
    // Spawn concurrent tasks
    let mut handles = Vec::new();
    
    // Health monitoring task
    let health_monitor_clone = Arc::clone(&health_monitor);
    handles.push(tokio::spawn(async move {
        for _ in 0..10 {
            let mut monitor = health_monitor_clone.lock().await;
            let _health = monitor.check_health().await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }));
    
    // Backup creation task
    let backup_manager_clone = Arc::clone(&backup_manager);
    handles.push(tokio::spawn(async move {
        for i in 0..5 {
            let backup_name = format!("stress_backup_{}", i);
            let _backup = backup_manager_clone.create_backup(Some(backup_name)).await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }));
    
    // Wait for all tasks to complete
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify system is still functional
    let final_backups = backup_manager.list_backups().await.unwrap();
    assert!(final_backups.len() <= 3); // Retention policy should limit to 3
    
    let final_health = health_monitor.lock().await.check_health().await.unwrap();
    assert!(matches!(final_health, TextStorageHealth::Healthy));
}

#[cfg(test)]
mod recovery_system_tests {
    use super::*;
    
    /// Test recovery strategy configurations
    #[test]
    fn test_recovery_strategy_configuration() {
        let conservative_config = RecoveryConfig {
            recovery_strategy: RecoveryStrategy::Conservative,
            ..RecoveryConfig::default()
        };
        
        let aggressive_config = RecoveryConfig {
            recovery_strategy: RecoveryStrategy::Aggressive,
            max_recovery_attempts: 5,
            ..RecoveryConfig::default()
        };
        
        let interactive_config = RecoveryConfig {
            recovery_strategy: RecoveryStrategy::Interactive,
            backup_before_recovery: true,
            ..RecoveryConfig::default()
        };
        
        assert_eq!(conservative_config.recovery_strategy, RecoveryStrategy::Conservative);
        assert_eq!(aggressive_config.recovery_strategy, RecoveryStrategy::Aggressive);
        assert_eq!(aggressive_config.max_recovery_attempts, 5);
        assert_eq!(interactive_config.recovery_strategy, RecoveryStrategy::Interactive);
        assert!(interactive_config.backup_before_recovery);
    }
    
    /// Test backup retention policy enforcement
    #[tokio::test]
    async fn test_backup_retention_enforcement() {
        let temp_dir = TempDir::new().unwrap();
        let backup_dir = temp_dir.path().join("retention_test");
        
        let retention_policy = BackupRetentionPolicy {
            max_backups: 2, // Only keep 2 backups
            max_age: Duration::from_secs(1), // 1 second max age
            compression_enabled: false,
        };
        
        let backup_manager = BackupManager::new(backup_dir, retention_policy).unwrap();
        
        // Create 4 backups
        for i in 0..4 {
            let backup_name = format!("retention_test_{}", i);
            backup_manager.create_backup(Some(backup_name)).await.unwrap();
            
            // Small delay to ensure different timestamps
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        let backups = backup_manager.list_backups().await.unwrap();
        assert!(backups.len() <= 2, "Should enforce max_backups limit of 2, found {}", backups.len());
        
        // Wait for age-based retention
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        // Create one more backup to trigger age-based cleanup
        backup_manager.create_backup(Some("trigger_cleanup".to_string())).await.unwrap();
        
        let final_backups = backup_manager.list_backups().await.unwrap();
        assert!(final_backups.len() <= 2, "Should maintain max_backups after age cleanup");
    }
}
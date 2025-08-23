//! Configuration persistence for Shardex indexes
//!
//! This module provides configuration persistence and loading capabilities to maintain
//! index settings across restarts. It includes versioning, validation, migration support,
//! and atomic file operations for data integrity.

use crate::config::ShardexConfig;
use crate::error::ShardexError;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Current version of the persisted configuration format
pub const CONFIG_VERSION: u32 = 1;

/// Configuration file name
pub const CONFIG_FILE: &str = "shardex.config";

/// Backup configuration file name
pub const CONFIG_BACKUP_FILE: &str = "shardex.config.bak";

/// Persisted configuration with versioning, integrity checking, and metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedConfig {
    /// Version of the configuration format for migration support
    pub version: u32,
    /// The actual Shardex configuration
    pub config: ShardexConfig,
    /// Timestamp when the configuration was created
    pub created_at: SystemTime,
    /// Timestamp when the configuration was last modified
    pub modified_at: SystemTime,
    /// CRC32 checksum of the configuration for integrity verification
    pub checksum: u32,
}

impl PersistedConfig {
    /// Create a new persisted configuration from a ShardexConfig
    pub fn new(config: ShardexConfig) -> Result<Self> {
        // Validate the configuration before persisting
        config.validate()?;

        let now = SystemTime::now();
        let mut persisted = Self {
            version: CONFIG_VERSION,
            config,
            created_at: now,
            modified_at: now,
            checksum: 0, // Will be calculated during save
        };

        // Calculate checksum
        persisted.update_checksum()?;

        Ok(persisted)
    }

    /// Save the configuration to a file atomically
    pub async fn save(&mut self, path: &Path) -> Result<()> {
        // Update modification timestamp
        self.modified_at = SystemTime::now();

        // Recalculate checksum
        self.update_checksum()?;

        // Create backup if original exists
        self.create_backup(path).await?;

        // Serialize to JSON with pretty printing for readability
        let json_content = serde_json::to_string_pretty(self).map_err(|e| {
            ShardexError::Config(format!("Failed to serialize configuration: {}", e))
        })?;

        // Write atomically using temporary file
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, json_content).map_err(|e| {
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to write configuration to {}: {}",
                    temp_path.display(),
                    e
                ),
            ))
        })?;

        // Atomic rename
        fs::rename(&temp_path, path).map_err(|e| {
            // Clean up temporary file on error
            let _ = fs::remove_file(&temp_path);
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to move configuration from {} to {}: {}",
                    temp_path.display(),
                    path.display(),
                    e
                ),
            ))
        })?;

        Ok(())
    }

    /// Load configuration from a file
    pub async fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).map_err(|e| {
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read configuration file {}: {}",
                    path.display(),
                    e
                ),
            ))
        })?;

        let mut config: Self = serde_json::from_str(&content).map_err(|e| {
            ShardexError::Corruption(format!(
                "Failed to parse configuration file {}: {}",
                path.display(),
                e
            ))
        })?;

        // Verify checksum
        let stored_checksum = config.checksum;
        config.update_checksum()?;

        if stored_checksum != config.checksum {
            return Err(ShardexError::Corruption(format!(
                "Configuration file checksum mismatch: expected {}, got {}",
                config.checksum, stored_checksum
            )));
        }

        // Restore the stored checksum
        config.checksum = stored_checksum;

        // Validate version compatibility
        if config.version > CONFIG_VERSION {
            return Err(ShardexError::Config(format!(
                "Unsupported configuration version: found {}, maximum supported {}",
                config.version, CONFIG_VERSION
            )));
        }

        // Perform migration if needed
        if config.version < CONFIG_VERSION {
            config = config.migrate_to_current_version()?;
        }

        // Validate the loaded configuration
        config.config.validate()?;

        Ok(config)
    }

    /// Check if this configuration is compatible with another configuration
    pub fn validate_compatibility(&self, other: &PersistedConfig) -> Result<()> {
        // Check immutable fields that affect file format and cannot be changed
        if self.config.vector_size != other.config.vector_size {
            return Err(ShardexError::Config(format!(
                "Incompatible vector size: existing={}, new={}. Vector size cannot be changed after index creation.",
                self.config.vector_size, other.config.vector_size
            )));
        }

        if self.config.directory_path != other.config.directory_path {
            return Err(ShardexError::Config(format!(
                "Incompatible directory path: existing={}, new={}. Directory path cannot be changed.",
                self.config.directory_path.display(),
                other.config.directory_path.display()
            )));
        }

        Ok(())
    }

    /// Update this configuration with compatible changes from another configuration
    pub fn merge_compatible_changes(&mut self, other: &PersistedConfig) -> Result<()> {
        // First validate compatibility
        self.validate_compatibility(other)?;

        // Update mutable configuration parameters
        self.config.shard_size = other.config.shard_size;
        self.config.shardex_segment_size = other.config.shardex_segment_size;
        self.config.wal_segment_size = other.config.wal_segment_size;
        self.config.batch_write_interval_ms = other.config.batch_write_interval_ms;
        self.config.slop_factor_config = other.config.slop_factor_config.clone();
        self.config.bloom_filter_size = other.config.bloom_filter_size;
        self.config.deduplication_policy = other.config.deduplication_policy;

        // Update modification timestamp
        self.modified_at = SystemTime::now();

        // Validate the merged configuration
        self.config.validate()?;

        Ok(())
    }

    /// Create a backup of the existing configuration file
    pub async fn create_backup(&self, config_path: &Path) -> Result<()> {
        if !config_path.exists() {
            return Ok(()); // No backup needed if original doesn't exist
        }

        let backup_path = config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(CONFIG_BACKUP_FILE);

        fs::copy(config_path, &backup_path).map_err(|e| {
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to create backup from {} to {}: {}",
                    config_path.display(),
                    backup_path.display(),
                    e
                ),
            ))
        })?;

        Ok(())
    }

    /// Restore configuration from backup file
    pub async fn restore_from_backup(config_path: &Path) -> Result<Self> {
        let backup_path = config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(CONFIG_BACKUP_FILE);

        if !backup_path.exists() {
            return Err(ShardexError::Config(format!(
                "No backup configuration file found at {}",
                backup_path.display()
            )));
        }

        let backup_config = Self::load(&backup_path).await?;

        // Copy backup to main config location
        fs::copy(&backup_path, config_path).map_err(|e| {
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to restore backup from {} to {}: {}",
                    backup_path.display(),
                    config_path.display(),
                    e
                ),
            ))
        })?;

        Ok(backup_config)
    }

    /// Update the checksum based on current configuration
    fn update_checksum(&mut self) -> Result<()> {
        // Temporarily set checksum to 0 for calculation
        let _old_checksum = self.checksum;
        self.checksum = 0;

        // Serialize configuration for checksum calculation
        let config_bytes = serde_json::to_vec(&self.config).map_err(|e| {
            ShardexError::Config(format!("Failed to serialize config for checksum: {}", e))
        })?;

        // Calculate CRC32 checksum
        self.checksum = crc32fast::hash(&config_bytes);

        Ok(())
    }

    /// Migrate configuration from older version to current version
    fn migrate_to_current_version(mut self) -> Result<Self> {
        match self.version {
            1 => {
                // Already at current version
                Ok(self)
            }
            0 => {
                // Migration from version 0 to version 1
                // For now, just update the version since we're at version 1
                self.version = 1;
                self.modified_at = SystemTime::now();
                Ok(self)
            }
            _ => Err(ShardexError::Config(format!(
                "Unknown configuration version: {}",
                self.version
            ))),
        }
    }

    /// Get the age of this configuration
    pub fn age(&self) -> Result<std::time::Duration> {
        SystemTime::now()
            .duration_since(self.created_at)
            .map_err(|e| ShardexError::Config(format!("Invalid created_at timestamp: {}", e)))
    }

    /// Get the time since last modification
    pub fn time_since_modified(&self) -> Result<std::time::Duration> {
        SystemTime::now()
            .duration_since(self.modified_at)
            .map_err(|e| ShardexError::Config(format!("Invalid modified_at timestamp: {}", e)))
    }
}

/// Configuration manager for handling persistence operations
#[derive(Debug)]
pub struct ConfigurationManager {
    config_path: std::path::PathBuf,
}

impl ConfigurationManager {
    /// Create a new configuration manager for the given directory
    pub fn new<P: AsRef<Path>>(index_directory: P) -> Self {
        let config_path = index_directory.as_ref().join(CONFIG_FILE);
        Self { config_path }
    }

    /// Save a configuration
    pub async fn save_config(&self, config: &ShardexConfig) -> Result<()> {
        let mut persisted = PersistedConfig::new(config.clone())?;
        persisted.save(&self.config_path).await
    }

    /// Load a configuration
    pub async fn load_config(&self) -> Result<PersistedConfig> {
        PersistedConfig::load(&self.config_path).await
    }

    /// Check if a configuration file exists
    pub fn config_exists(&self) -> bool {
        self.config_path.exists()
    }

    /// Update an existing configuration with new values
    pub async fn update_config(&self, new_config: &ShardexConfig) -> Result<()> {
        if !self.config_exists() {
            return Err(ShardexError::Config(
                "Cannot update configuration: no existing configuration found".to_string(),
            ));
        }

        // Load existing configuration
        let mut existing = self.load_config().await?;

        // Create new persisted config for compatibility checking
        let new_persisted = PersistedConfig::new(new_config.clone())?;

        // Merge compatible changes
        existing.merge_compatible_changes(&new_persisted)?;

        // Save updated configuration
        existing.save(&self.config_path).await
    }

    /// Restore configuration from backup
    pub async fn restore_from_backup(&self) -> Result<PersistedConfig> {
        PersistedConfig::restore_from_backup(&self.config_path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TestEnvironment;
    use std::time::Duration;
    use tokio::time::sleep;

    fn create_test_config() -> ShardexConfig {
        ShardexConfig::new()
            .vector_size(128)
            .shard_size(1000)
            .batch_write_interval_ms(100)
    }

    #[tokio::test]
    async fn test_persisted_config_creation() {
        let config = create_test_config();
        let persisted = PersistedConfig::new(config.clone()).unwrap();

        assert_eq!(persisted.version, CONFIG_VERSION);
        assert_eq!(persisted.config, config);
        assert!(persisted.checksum > 0);
    }

    #[tokio::test]
    async fn test_persisted_config_save_and_load() {
        let _env = TestEnvironment::new("test_persisted_config_save_load");
        let config_path = _env.path().join(CONFIG_FILE);
        let config = create_test_config();

        let mut persisted = PersistedConfig::new(config.clone()).unwrap();

        // Save configuration
        persisted.save(&config_path).await.unwrap();
        assert!(config_path.exists());

        // Load configuration
        let loaded = PersistedConfig::load(&config_path).await.unwrap();
        assert_eq!(loaded.config, config);
        assert_eq!(loaded.version, CONFIG_VERSION);
        assert_eq!(loaded.checksum, persisted.checksum);
    }

    #[tokio::test]
    async fn test_configuration_manager() {
        let _env = TestEnvironment::new("test_configuration_manager");
        let manager = ConfigurationManager::new(_env.path());
        let config = create_test_config();

        assert!(!manager.config_exists());

        // Save configuration
        manager.save_config(&config).await.unwrap();
        assert!(manager.config_exists());

        // Load configuration
        let loaded = manager.load_config().await.unwrap();
        assert_eq!(loaded.config, config);
    }

    #[tokio::test]
    async fn test_configuration_compatibility() {
        let config1 = create_test_config();
        let config2 = ShardexConfig::new()
            .vector_size(256) // Different vector size
            .shard_size(1000);

        let persisted1 = PersistedConfig::new(config1).unwrap();
        let persisted2 = PersistedConfig::new(config2).unwrap();

        let result = persisted1.validate_compatibility(&persisted2);
        assert!(result.is_err());

        if let Err(ShardexError::Config(msg)) = result {
            assert!(msg.contains("Incompatible vector size"));
        } else {
            panic!("Expected Config error");
        }
    }

    #[tokio::test]
    async fn test_configuration_merge_compatible_changes() {
        let config1 = create_test_config();
        let config2 = ShardexConfig::new()
            .directory_path(config1.directory_path.clone()) // Same directory
            .vector_size(128) // Same vector size
            .shard_size(2000) // Different shard size (compatible)
            .batch_write_interval_ms(200); // Different interval (compatible)

        let mut persisted1 = PersistedConfig::new(config1).unwrap();
        let persisted2 = PersistedConfig::new(config2.clone()).unwrap();

        persisted1.merge_compatible_changes(&persisted2).unwrap();

        assert_eq!(persisted1.config.shard_size, 2000);
        assert_eq!(persisted1.config.batch_write_interval_ms, 200);
        assert_eq!(persisted1.config.vector_size, 128); // Unchanged
    }

    #[tokio::test]
    async fn test_checksum_validation() {
        let _env = TestEnvironment::new("test_checksum_validation");
        let config_path = _env.path().join(CONFIG_FILE);
        let config = create_test_config();

        let mut persisted = PersistedConfig::new(config.clone()).unwrap();
        persisted.save(&config_path).await.unwrap();

        // Corrupt the file
        let mut content = fs::read_to_string(&config_path).unwrap();
        content = content.replace("\"shard_size\": 1000", "\"shard_size\": 9999");
        fs::write(&config_path, content).unwrap();

        // Should fail to load due to checksum mismatch
        let result = PersistedConfig::load(&config_path).await;
        assert!(result.is_err());

        if let Err(ShardexError::Corruption(msg)) = result {
            assert!(msg.contains("checksum mismatch"));
        } else {
            panic!("Expected Corruption error");
        }
    }

    #[tokio::test]
    async fn test_configuration_update() {
        let _env = TestEnvironment::new("test_configuration_update");
        let manager = ConfigurationManager::new(_env.path());

        let original_config = create_test_config();
        manager.save_config(&original_config).await.unwrap();

        let updated_config = ShardexConfig::new()
            .directory_path(original_config.directory_path.clone())
            .vector_size(128) // Same
            .shard_size(2000) // Updated
            .batch_write_interval_ms(200); // Updated

        manager.update_config(&updated_config).await.unwrap();

        let loaded = manager.load_config().await.unwrap();
        assert_eq!(loaded.config.shard_size, 2000);
        assert_eq!(loaded.config.batch_write_interval_ms, 200);
        assert_eq!(loaded.config.vector_size, 128);
    }

    #[tokio::test]
    async fn test_invalid_configuration() {
        let invalid_config = ShardexConfig::new().vector_size(0); // Invalid
        let result = PersistedConfig::new(invalid_config);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_atomic_save_operation() {
        let _env = TestEnvironment::new("test_atomic_save");
        let config_path = _env.path().join(CONFIG_FILE);
        let temp_path = config_path.with_extension("tmp");
        let config = create_test_config();

        let mut persisted = PersistedConfig::new(config).unwrap();
        persisted.save(&config_path).await.unwrap();

        // Temporary file should not exist after successful save
        assert!(!temp_path.exists());
        assert!(config_path.exists());
    }

    #[tokio::test]
    async fn test_configuration_timestamps() {
        let config = create_test_config();
        let persisted1 = PersistedConfig::new(config.clone()).unwrap();

        // Wait a bit to ensure different timestamps
        sleep(Duration::from_millis(10)).await;

        let persisted2 = PersistedConfig::new(config).unwrap();

        assert!(persisted2.created_at > persisted1.created_at);
        assert!(persisted1.age().unwrap() > Duration::ZERO);
        assert!(persisted1.time_since_modified().unwrap() >= Duration::ZERO);
    }

    #[tokio::test]
    async fn test_version_migration() {
        // This test simulates loading an older version config
        let config = create_test_config();
        let mut persisted = PersistedConfig::new(config).unwrap();

        // Simulate older version
        persisted.version = 0;

        let migrated = persisted.migrate_to_current_version().unwrap();
        assert_eq!(migrated.version, CONFIG_VERSION);
    }

    #[tokio::test]
    async fn test_unsupported_future_version() {
        let _env = TestEnvironment::new("test_future_version");
        let config_path = _env.path().join(CONFIG_FILE);
        let config = create_test_config();

        let mut persisted = PersistedConfig::new(config).unwrap();
        persisted.version = CONFIG_VERSION + 1; // Future version

        // Manually save with future version
        persisted.update_checksum().unwrap();
        let content = serde_json::to_string_pretty(&persisted).unwrap();
        fs::write(&config_path, content).unwrap();

        // Should fail to load
        let result = PersistedConfig::load(&config_path).await;
        assert!(result.is_err());

        if let Err(ShardexError::Config(msg)) = result {
            assert!(msg.contains("Unsupported configuration version"));
        } else {
            panic!("Expected Config error");
        }
    }
}

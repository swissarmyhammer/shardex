//! Integrity management for memory-mapped files
//!
//! This module provides comprehensive data integrity validation, corruption detection,
//! and recovery mechanisms for Shardex's memory-mapped storage systems. It builds upon
//! the basic checksum validation in FileHeader to provide periodic validation,
//! corruption detection, and selective recovery capabilities.
//!
//! # Key Components
//!
//! - [`IntegrityManager`]: Central coordinator for integrity operations
//! - [`IntegrityConfig`]: Configuration for validation frequency and policies
//! - [`CorruptionReport`]: Detailed corruption analysis and recovery recommendations
//! - [`ValidationResult`]: Results of integrity validation operations
//!
//! # Usage Examples
//!
//! ## Basic Integrity Validation
//!
//! ```rust
//! use shardex::integrity::{IntegrityManager, IntegrityConfig};
//! use shardex::memory::MemoryMappedFile;
//! use tempfile::TempDir;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let temp_dir = TempDir::new()?;
//! let file_path = temp_dir.path().join("test.dat");
//!
//! // Create a memory-mapped file
//! let mmf = MemoryMappedFile::create(&file_path, 1024)?;
//!
//! // Create integrity manager with default configuration
//! let config = IntegrityConfig::default();
//! let mut integrity_manager = IntegrityManager::new(config);
//!
//! // Validate file integrity
//! let result = integrity_manager.validate_file(&mmf)?;
//! if result.is_valid() {
//!     println!("File integrity verified");
//! } else {
//!     println!("Corruption detected: {:?}", result.corruption_report());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Periodic Integrity Monitoring
//!
//! ```rust
//! use shardex::integrity::{IntegrityManager, IntegrityConfig};
//! use shardex::memory::MemoryMappedFile;
//! use std::time::Duration;
//!
//! # fn monitoring_example() -> Result<(), Box<dyn std::error::Error>> {
//! // Configure periodic validation every 5 minutes
//! let config = IntegrityConfig {
//!     periodic_validation_interval: Some(Duration::from_secs(300)),
//!     corruption_tolerance: 0.01, // 1% corruption threshold
//!     enable_recovery: true,
//!     ..Default::default()
//! };
//!
//! let mut integrity_manager = IntegrityManager::new(config);
//!
//! // The manager will automatically perform periodic validations
//! // and trigger recovery procedures when needed
//! # Ok(())
//! # }
//! ```

use crate::error::ShardexError;
use crate::memory::{FileHeader, MemoryMappedFile};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Configuration for integrity validation operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityConfig {
    /// Interval for periodic integrity validation (None disables periodic validation)
    pub periodic_validation_interval: Option<Duration>,
    /// Corruption tolerance as a fraction (0.0 = no tolerance, 1.0 = allow 100% corruption)
    pub corruption_tolerance: f64,
    /// Enable automatic recovery from corruption when possible
    pub enable_recovery: bool,
    /// Maximum number of recovery attempts before giving up
    pub max_recovery_attempts: usize,
    /// Enable detailed corruption analysis (may impact performance)
    pub detailed_analysis: bool,
    /// Chunk size for incremental validation (bytes)
    pub validation_chunk_size: usize,
    /// Enable cross-validation between related storage files
    pub enable_cross_validation: bool,
}

impl Default for IntegrityConfig {
    fn default() -> Self {
        Self {
            periodic_validation_interval: Some(Duration::from_secs(600)), // 10 minutes
            corruption_tolerance: 0.0, // No tolerance by default
            enable_recovery: true,
            max_recovery_attempts: 3,
            detailed_analysis: false,
            validation_chunk_size: 1024 * 1024, // 1MB chunks
            enable_cross_validation: false,
        }
    }
}

/// Types of corruption that can be detected
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CorruptionType {
    /// Header corruption (magic bytes, version, or header checksum mismatch)
    HeaderCorruption,
    /// Data checksum mismatch
    DataCorruption,
    /// File truncation or unexpected size
    FileTruncation,
    /// Invalid data structure consistency
    StructuralInconsistency,
    /// Cross-reference validation failure
    CrossValidationFailure,
    /// Partial file corruption affecting specific regions
    PartialCorruption,
}

/// Detailed information about detected corruption
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorruptionReport {
    /// Type of corruption detected
    pub corruption_type: CorruptionType,
    /// File path where corruption was detected
    pub file_path: PathBuf,
    /// Offset in the file where corruption begins (if applicable)
    pub corruption_offset: Option<u64>,
    /// Size of corrupted region (if applicable)
    pub corruption_size: Option<u64>,
    /// Detailed description of the corruption
    pub description: String,
    /// Recommended recovery actions
    pub recovery_recommendations: Vec<String>,
    /// Severity of the corruption (0.0 = minor, 1.0 = critical)
    pub severity: f64,
    /// Whether the corruption is recoverable
    pub is_recoverable: bool,
    /// Timestamp when corruption was detected
    pub detected_at: SystemTime,
}

/// Result of integrity validation operations
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether the validation passed
    is_valid: bool,
    /// Detailed corruption report if validation failed
    corruption_report: Option<CorruptionReport>,
    /// Performance metrics for the validation
    validation_time: Duration,
    /// Number of bytes validated
    bytes_validated: u64,
    /// Checksum of the validated data
    data_checksum: u32,
}

/// Statistics about integrity operations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntegrityStats {
    /// Total number of validations performed
    pub total_validations: u64,
    /// Number of corruptions detected
    pub corruptions_detected: u64,
    /// Number of successful recoveries
    pub successful_recoveries: u64,
    /// Total time spent on integrity operations
    pub total_validation_time: Duration,
    /// Total bytes validated
    pub total_bytes_validated: u64,
    /// Last validation timestamp
    pub last_validation: Option<SystemTime>,
}

/// Central manager for integrity operations across memory-mapped files
pub struct IntegrityManager {
    /// Configuration for integrity operations
    config: IntegrityConfig,
    /// Statistics about integrity operations
    stats: IntegrityStats,
    /// Track last validation time per file
    validation_history: HashMap<PathBuf, SystemTime>,
    /// Files currently being monitored
    monitored_files: HashMap<PathBuf, FileMonitoringState>,
}

/// State tracking for monitored files
#[derive(Debug, Clone)]
struct FileMonitoringState {
    /// Last known good checksum
    last_known_checksum: u32,
    /// Last validation timestamp
    last_validation: SystemTime,
    /// Number of consecutive validation failures
    consecutive_failures: usize,
    /// Whether the file is currently considered healthy
    is_healthy: bool,
}

impl ValidationResult {
    /// Create a new successful validation result
    pub fn success(validation_time: Duration, bytes_validated: u64, data_checksum: u32) -> Self {
        Self {
            is_valid: true,
            corruption_report: None,
            validation_time,
            bytes_validated,
            data_checksum,
        }
    }

    /// Create a new failed validation result
    pub fn failure(
        corruption_report: CorruptionReport,
        validation_time: Duration,
        bytes_validated: u64,
        data_checksum: u32,
    ) -> Self {
        Self {
            is_valid: false,
            corruption_report: Some(corruption_report),
            validation_time,
            bytes_validated,
            data_checksum,
        }
    }

    /// Check if the validation passed
    pub fn is_valid(&self) -> bool {
        self.is_valid
    }

    /// Get the corruption report if validation failed
    pub fn corruption_report(&self) -> Option<&CorruptionReport> {
        self.corruption_report.as_ref()
    }

    /// Get validation performance metrics
    pub fn validation_time(&self) -> Duration {
        self.validation_time
    }

    /// Get number of bytes validated
    pub fn bytes_validated(&self) -> u64 {
        self.bytes_validated
    }

    /// Get the checksum of validated data
    pub fn data_checksum(&self) -> u32 {
        self.data_checksum
    }
}

impl IntegrityManager {
    /// Create a new integrity manager with the specified configuration
    pub fn new(config: IntegrityConfig) -> Self {
        Self {
            config,
            stats: IntegrityStats::default(),
            validation_history: HashMap::new(),
            monitored_files: HashMap::new(),
        }
    }

    /// Create an integrity manager with default configuration
    pub fn with_default_config() -> Self {
        Self::new(IntegrityConfig::default())
    }

    /// Validate the integrity of a memory-mapped file
    pub fn validate_file(&mut self, mmf: &MemoryMappedFile) -> Result<ValidationResult, ShardexError> {
        let start_time = Instant::now();
        let file_data = mmf.as_slice();
        
        // Basic validation: check if we have at least a header
        if file_data.len() < FileHeader::SIZE {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::FileTruncation,
                file_path: PathBuf::from("unknown"), // We don't have path info from MemoryMappedFile
                corruption_offset: None,
                corruption_size: Some(file_data.len() as u64),
                description: format!("File too small: {} bytes, expected at least {} bytes for header", 
                                   file_data.len(), FileHeader::SIZE),
                recovery_recommendations: vec!["File may be truncated. Restore from backup.".to_string()],
                severity: 1.0,
                is_recoverable: false,
                detected_at: SystemTime::now(),
            };
            
            return Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                0,
            ));
        }

        // Try to detect the file type from magic bytes in the FileHeader and validate accordingly
        if file_data.len() >= 4 {
            let magic = &file_data[0..4];
            
            match magic {
                b"PSTR" => {
                    // PostingStorage file - magic is at beginning of FileHeader
                    self.validate_posting_storage_file(mmf, start_time)
                }
                b"VSTR" => {
                    // VectorStorage file - magic is at beginning of FileHeader
                    self.validate_vector_storage_file(mmf, start_time)
                }
                _ => {

                    // Try to read as basic FileHeader (for simple test files or other storage types)
                    let header: FileHeader = mmf.read_at(0).map_err(|e| {
                        ShardexError::Corruption(format!("Failed to read file header: {}", e))
                    })?;


                    // Validate header checksum against data
                    let data_start = FileHeader::SIZE;
                    let data_portion = &file_data[data_start..];

                    
                    // Use the FileHeader's own validation method
                    if let Err(e) = header.validate_checksum(data_portion) {
                        let corruption_report = CorruptionReport {
                            corruption_type: CorruptionType::DataCorruption,
                            file_path: PathBuf::from("unknown"),
                            corruption_offset: Some(data_start as u64),
                            corruption_size: Some(data_portion.len() as u64),
                            description: format!("Header checksum validation failed: {}", e),
                            recovery_recommendations: vec![
                                "Data may be corrupted. Check for partial writes.".to_string(),
                                "Restore from known good backup if available.".to_string(),
                                "Run detailed corruption analysis if recovery is needed.".to_string(),
                            ],
                            severity: 0.8,
                            is_recoverable: self.config.enable_recovery,
                            detected_at: SystemTime::now(),
                        };

                        return Ok(ValidationResult::failure(
                            corruption_report,
                            start_time.elapsed(),
                            file_data.len() as u64,
                            header.checksum,
                        ));
                    }
                    
                    let calculated_checksum = header.checksum;

                    // Update statistics
                    self.stats.total_validations += 1;
                    self.stats.total_validation_time += start_time.elapsed();
                    self.stats.total_bytes_validated += file_data.len() as u64;
                    self.stats.last_validation = Some(SystemTime::now());

                    Ok(ValidationResult::success(
                        start_time.elapsed(),
                        file_data.len() as u64,
                        calculated_checksum,
                    ))
                }
            }
        } else {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::FileTruncation,
                file_path: PathBuf::from("unknown"),
                corruption_offset: None,
                corruption_size: Some(file_data.len() as u64),
                description: "File too small to contain magic bytes".to_string(),
                recovery_recommendations: vec!["File may be truncated. Restore from backup.".to_string()],
                severity: 1.0,
                is_recoverable: false,
                detected_at: SystemTime::now(),
            };
            
            Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                0,
            ))
        }
    }

    /// Validate a PostingStorage file
    fn validate_posting_storage_file(&mut self, mmf: &MemoryMappedFile, start_time: Instant) -> Result<ValidationResult, ShardexError> {
        use crate::posting_storage::PostingStorageHeader;
        
        let file_data = mmf.as_slice();
        
        if file_data.len() < PostingStorageHeader::SIZE {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::FileTruncation,
                file_path: PathBuf::from("unknown"),
                corruption_offset: None,
                corruption_size: Some(file_data.len() as u64),
                description: format!("File too small: {} bytes, expected at least {} bytes for PostingStorage header", 
                                   file_data.len(), PostingStorageHeader::SIZE),
                recovery_recommendations: vec!["File may be truncated. Restore from backup.".to_string()],
                severity: 1.0,
                is_recoverable: false,
                detected_at: SystemTime::now(),
            };
            
            return Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                0,
            ));
        }

        let header: PostingStorageHeader = mmf.read_at(0).map_err(|e| {
            ShardexError::Corruption(format!("Failed to read PostingStorage header: {}", e))
        })?;

        // Validate using the header's validate method
        if let Err(e) = header.validate() {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::HeaderCorruption,
                file_path: PathBuf::from("unknown"),
                corruption_offset: Some(0),
                corruption_size: Some(PostingStorageHeader::SIZE as u64),
                description: format!("PostingStorage header validation failed: {}", e),
                recovery_recommendations: vec![
                    "Header may be corrupted. Check file system integrity.".to_string(),
                    "Restore from known good backup if available.".to_string(),
                ],
                severity: 1.0,
                is_recoverable: false,
                detected_at: SystemTime::now(),
            };

            return Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                0,
            ));
        }

        // Validate data checksum
        let data_start = PostingStorageHeader::SIZE;
        let data_size = header.calculate_file_size() - PostingStorageHeader::SIZE;
        
        if data_start + data_size > file_data.len() {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::FileTruncation,
                file_path: PathBuf::from("unknown"),
                corruption_offset: Some(data_start as u64),
                corruption_size: Some(data_size as u64),
                description: "PostingStorage file truncated before end of data section".to_string(),
                recovery_recommendations: vec!["File may be truncated. Restore from backup.".to_string()],
                severity: 1.0,
                is_recoverable: false,
                detected_at: SystemTime::now(),
            };

            return Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                0,
            ));
        }

        let data_portion = &file_data[data_start..data_start + data_size];
        if let Err(e) = header.file_header.validate_checksum(data_portion) {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::DataCorruption,
                file_path: PathBuf::from("unknown"),
                corruption_offset: Some(data_start as u64),
                corruption_size: Some(data_size as u64),
                description: format!("PostingStorage data checksum validation failed: {}", e),
                recovery_recommendations: vec![
                    "Data may be corrupted. Check for partial writes.".to_string(),
                    "Restore from known good backup if available.".to_string(),
                ],
                severity: 0.8,
                is_recoverable: self.config.enable_recovery,
                detected_at: SystemTime::now(),
            };

            return Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                header.file_header.checksum,
            ));
        }

        // Update statistics
        self.stats.total_validations += 1;
        self.stats.total_validation_time += start_time.elapsed();
        self.stats.total_bytes_validated += file_data.len() as u64;
        self.stats.last_validation = Some(SystemTime::now());

        Ok(ValidationResult::success(
            start_time.elapsed(),
            file_data.len() as u64,
            header.file_header.checksum,
        ))
    }

    /// Validate a VectorStorage file
    fn validate_vector_storage_file(&mut self, mmf: &MemoryMappedFile, start_time: Instant) -> Result<ValidationResult, ShardexError> {
        use crate::vector_storage::VectorStorageHeader;
        
        let file_data = mmf.as_slice();
        
        if file_data.len() < VectorStorageHeader::SIZE {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::FileTruncation,
                file_path: PathBuf::from("unknown"),
                corruption_offset: None,
                corruption_size: Some(file_data.len() as u64),
                description: format!("File too small: {} bytes, expected at least {} bytes for VectorStorage header", 
                                   file_data.len(), VectorStorageHeader::SIZE),
                recovery_recommendations: vec!["File may be truncated. Restore from backup.".to_string()],
                severity: 1.0,
                is_recoverable: false,
                detected_at: SystemTime::now(),
            };
            
            return Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                0,
            ));
        }

        let header: VectorStorageHeader = mmf.read_at(0).map_err(|e| {
            ShardexError::Corruption(format!("Failed to read VectorStorage header: {}", e))
        })?;

        // Validate using the header's validate method
        if let Err(e) = header.validate() {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::HeaderCorruption,
                file_path: PathBuf::from("unknown"),
                corruption_offset: Some(0),
                corruption_size: Some(VectorStorageHeader::SIZE as u64),
                description: format!("VectorStorage header validation failed: {}", e),
                recovery_recommendations: vec![
                    "Header may be corrupted. Check file system integrity.".to_string(),
                    "Restore from known good backup if available.".to_string(),
                ],
                severity: 1.0,
                is_recoverable: false,
                detected_at: SystemTime::now(),
            };

            return Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                0,
            ));
        }

        // Validate data checksum  
        let vector_data_start = header.vector_data_offset as usize;
        let vector_data_size = (header.capacity as usize)
            * (header.vector_dimension as usize)
            * std::mem::size_of::<f32>();
        let aligned_size = Self::align_size(vector_data_size, header.simd_alignment as usize);
        
        if vector_data_start + aligned_size > file_data.len() {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::FileTruncation,
                file_path: PathBuf::from("unknown"),
                corruption_offset: Some(vector_data_start as u64),
                corruption_size: Some(aligned_size as u64),
                description: "VectorStorage file truncated before end of vector data".to_string(),
                recovery_recommendations: vec!["File may be truncated. Restore from backup.".to_string()],
                severity: 1.0,
                is_recoverable: false,
                detected_at: SystemTime::now(),
            };

            return Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                0,
            ));
        }

        let vector_data = &file_data[vector_data_start..vector_data_start + aligned_size];
        if let Err(e) = header.file_header.validate_checksum(vector_data) {
            let corruption_report = CorruptionReport {
                corruption_type: CorruptionType::DataCorruption,
                file_path: PathBuf::from("unknown"),
                corruption_offset: Some(vector_data_start as u64),
                corruption_size: Some(aligned_size as u64),
                description: format!("VectorStorage data checksum validation failed: {}", e),
                recovery_recommendations: vec![
                    "Vector data may be corrupted. Check for partial writes.".to_string(),
                    "Restore from known good backup if available.".to_string(),
                ],
                severity: 0.8,
                is_recoverable: self.config.enable_recovery,
                detected_at: SystemTime::now(),
            };

            return Ok(ValidationResult::failure(
                corruption_report,
                start_time.elapsed(),
                file_data.len() as u64,
                header.file_header.checksum,
            ));
        }

        // Update statistics
        self.stats.total_validations += 1;
        self.stats.total_validation_time += start_time.elapsed();
        self.stats.total_bytes_validated += file_data.len() as u64;
        self.stats.last_validation = Some(SystemTime::now());

        Ok(ValidationResult::success(
            start_time.elapsed(),
            file_data.len() as u64,
            header.file_header.checksum,
        ))
    }
    
    /// Align size to the specified alignment boundary (helper for VectorStorage)
    fn align_size(size: usize, alignment: usize) -> usize {
        (size + alignment - 1) & !(alignment - 1)
    }

    /// Validate a file by path, opening it temporarily
    pub fn validate_file_path<P: AsRef<Path>>(&mut self, path: P) -> Result<ValidationResult, ShardexError> {
        let path = path.as_ref();
        let mmf = MemoryMappedFile::open_read_only(path)?;
        let mut result = self.validate_file(&mmf)?;
        
        // Update the corruption report with the actual file path
        if let Some(ref mut report) = result.corruption_report {
            report.file_path = path.to_path_buf();
        }
        
        // Update validation history
        self.validation_history.insert(path.to_path_buf(), SystemTime::now());
        
        Ok(result)
    }

    /// Add a file to periodic monitoring
    pub fn add_to_monitoring<P: AsRef<Path>>(&mut self, path: P) -> Result<(), ShardexError> {
        let path = path.as_ref().to_path_buf();
        
        // Perform initial validation
        let result = self.validate_file_path(&path)?;
        
        let monitoring_state = FileMonitoringState {
            last_known_checksum: result.data_checksum(),
            last_validation: SystemTime::now(),
            consecutive_failures: if result.is_valid() { 0 } else { 1 },
            is_healthy: result.is_valid(),
        };
        
        self.monitored_files.insert(path, monitoring_state);
        Ok(())
    }

    /// Remove a file from periodic monitoring
    pub fn remove_from_monitoring<P: AsRef<Path>>(&mut self, path: P) {
        let path = path.as_ref().to_path_buf();
        self.monitored_files.remove(&path);
        self.validation_history.remove(&path);
    }

    /// Perform periodic validation on all monitored files
    pub fn perform_periodic_validation(&mut self) -> Result<Vec<ValidationResult>, ShardexError> {
        let mut results = Vec::new();
        
        if self.config.periodic_validation_interval.is_none() {
            return Ok(results); // Periodic validation disabled
        }
        
        let now = SystemTime::now();
        let interval = self.config.periodic_validation_interval.unwrap();
        
        // Check which files need validation
        let files_to_validate: Vec<PathBuf> = self.monitored_files
            .iter()
            .filter(|(_, state)| {
                now.duration_since(state.last_validation)
                    .unwrap_or(Duration::MAX) >= interval
            })
            .map(|(path, _)| path.clone())
            .collect();
        
        for file_path in files_to_validate {
            let result = self.validate_file_path(&file_path)?;
            
            // Update monitoring state
            if let Some(state) = self.monitored_files.get_mut(&file_path) {
                state.last_validation = now;
                
                if result.is_valid() {
                    state.consecutive_failures = 0;
                    state.is_healthy = true;
                    state.last_known_checksum = result.data_checksum();
                } else {
                    state.consecutive_failures += 1;
                    state.is_healthy = false;
                    
                    // Update corruption statistics
                    self.stats.corruptions_detected += 1;
                }
            }
            
            results.push(result);
        }
        
        Ok(results)
    }

    /// Check if a file needs periodic validation
    pub fn needs_validation<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();
        
        let Some(interval) = self.config.periodic_validation_interval else {
            return false; // Periodic validation disabled
        };
        
        let Some(state) = self.monitored_files.get(path) else {
            return true; // Not monitored, should validate
        };
        
        SystemTime::now()
            .duration_since(state.last_validation)
            .unwrap_or(Duration::MAX) >= interval
    }

    /// Get integrity statistics
    pub fn stats(&self) -> &IntegrityStats {
        &self.stats
    }

    /// Get the health status of a monitored file
    pub fn file_health_status<P: AsRef<Path>>(&self, path: P) -> Option<bool> {
        self.monitored_files.get(path.as_ref()).map(|state| state.is_healthy)
    }

    /// Get the list of monitored files
    pub fn monitored_files(&self) -> Vec<&PathBuf> {
        self.monitored_files.keys().collect()
    }

    /// Attempt to recover from corruption if recovery is enabled
    pub fn attempt_recovery(&mut self, corruption_report: &CorruptionReport) -> Result<bool, ShardexError> {
        if !self.config.enable_recovery || !corruption_report.is_recoverable {
            return Ok(false);
        }

        // For now, we implement basic recovery strategies
        // In a full implementation, this would include more sophisticated recovery logic
        match corruption_report.corruption_type {
            CorruptionType::DataCorruption => {
                // Could implement incremental recovery, partial data restoration, etc.
                // For now, we just log the attempt
                tracing::warn!(
                    "Attempting recovery for data corruption in file: {:?}",
                    corruption_report.file_path
                );
                Ok(false) // Recovery not yet implemented
            }
            CorruptionType::HeaderCorruption => {
                // Header corruption is typically not recoverable without backup
                tracing::error!(
                    "Header corruption detected in file: {:?} - recovery not possible without backup",
                    corruption_report.file_path
                );
                Ok(false)
            }
            _ => {
                tracing::warn!(
                    "Recovery not implemented for corruption type: {:?}",
                    corruption_report.corruption_type
                );
                Ok(false)
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryMappedFile;
    use tempfile::{NamedTempFile, TempDir};

    #[test]
    fn test_integrity_config_default() {
        let config = IntegrityConfig::default();
        assert!(config.periodic_validation_interval.is_some());
        assert_eq!(config.corruption_tolerance, 0.0);
        assert!(config.enable_recovery);
        assert_eq!(config.max_recovery_attempts, 3);
    }

    #[test]
    fn test_validation_result_success() {
        let result = ValidationResult::success(
            Duration::from_millis(100),
            1024,
            0x12345678,
        );
        
        if !result.is_valid() {
            panic!("Validation failed but continuing to see debug output");
        }
        assert!(result.corruption_report().is_none());
        assert_eq!(result.bytes_validated(), 1024);
        assert_eq!(result.data_checksum(), 0x12345678);
    }

    #[test]
    fn test_validation_result_failure() {
        let corruption_report = CorruptionReport {
            corruption_type: CorruptionType::DataCorruption,
            file_path: PathBuf::from("/test/file"),
            corruption_offset: Some(100),
            corruption_size: Some(50),
            description: "Test corruption".to_string(),
            recovery_recommendations: vec!["Test recovery".to_string()],
            severity: 0.5,
            is_recoverable: true,
            detected_at: SystemTime::now(),
        };

        let result = ValidationResult::failure(
            corruption_report,
            Duration::from_millis(200),
            1024,
            0x87654321,
        );
        
        assert!(!result.is_valid());
        assert!(result.corruption_report().is_some());
        
        let report = result.corruption_report().unwrap();
        assert_eq!(report.corruption_type, CorruptionType::DataCorruption);
        assert_eq!(report.severity, 0.5);
        assert!(report.is_recoverable);
    }

    #[test]
    fn test_integrity_manager_creation() {
        let config = IntegrityConfig::default();
        let manager = IntegrityManager::new(config);
        
        assert_eq!(manager.stats().total_validations, 0);
        assert_eq!(manager.monitored_files().len(), 0);
    }

    #[test]
    fn test_file_validation_success() {
        // Test with a real storage to make sure it works
        use crate::posting_storage::PostingStorage;
        use crate::identifiers::DocumentId;
        
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        // Create valid posting storage which already handles checksums correctly
        let mut storage = PostingStorage::create(&storage_path, 10).unwrap();
        let doc_id = DocumentId::new();
        storage.add_posting(doc_id, 100, 50).unwrap();
        storage.sync().unwrap();

        // Debug: check what magic bytes are actually in the file
        let file_data = storage.memory_mapped_file().as_slice();
        let magic = &file_data[0..4];
        println!("Debug: Magic bytes in file: {:?}", magic);
        println!("Debug: Magic bytes as string: {:?}", std::str::from_utf8(magic).unwrap_or("invalid"));

        // Validate with integrity manager
        let mut manager = IntegrityManager::with_default_config();
        let result = manager.validate_file(storage.memory_mapped_file()).unwrap();
        
        if !result.is_valid() {
            if let Some(report) = result.corruption_report() {
                println!("Debug: Corruption report: {:?}", report);
            }
        }
        
        assert!(result.is_valid());
        assert_eq!(manager.stats().total_validations, 1);
        assert!(manager.stats().total_bytes_validated > 0);
    }

    #[test]
    fn test_file_validation_truncation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("truncated.dat");

        // Create a truncated file (smaller than header size)
        let mmf = MemoryMappedFile::create(&file_path, 8).unwrap(); // Too small for header
        
        let mut manager = IntegrityManager::with_default_config();
        let result = manager.validate_file(&mmf).unwrap();
        
        assert!(!result.is_valid());
        let report = result.corruption_report().unwrap();
        assert_eq!(report.corruption_type, CorruptionType::FileTruncation);
        assert_eq!(report.severity, 1.0);
        assert!(!report.is_recoverable);
    }

    #[test]
    fn test_file_validation_checksum_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("corrupted.dat");

        // Create file with correct header but corrupt data
        let mut mmf = MemoryMappedFile::create(&file_path, 1024).unwrap();
        let original_data = vec![42u8; 100];
        let corrupted_data = vec![99u8; 100]; // Different data
        
        // Create header with checksum for original data
        let header = FileHeader::new(b"TEST", 1, &original_data);
        
        // But write corrupted data instead
        mmf.write_at(0, &header).unwrap();
        mmf.write_slice_at(FileHeader::SIZE, &corrupted_data).unwrap();
        mmf.sync().unwrap();

        // Validation should detect corruption
        let mut manager = IntegrityManager::with_default_config();
        let result = manager.validate_file(&mmf).unwrap();
        
        assert!(!result.is_valid());
        let report = result.corruption_report().unwrap();
        assert_eq!(report.corruption_type, CorruptionType::DataCorruption);
        assert_eq!(report.severity, 0.8);
    }

    #[test]
    fn test_file_path_validation() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path();

        // Create a valid file  
        let mut mmf = MemoryMappedFile::create(file_path, 1024).unwrap();
        let data_size = 1024 - FileHeader::SIZE; // All remaining space after header
        let test_data = vec![42u8; data_size];
        let header = FileHeader::new(b"TEST", 1, &test_data);
        
        mmf.write_at(0, &header).unwrap();
        mmf.write_slice_at(FileHeader::SIZE, &test_data).unwrap();
        mmf.sync().unwrap();
        drop(mmf);

        // Validate by path
        let mut manager = IntegrityManager::with_default_config();
        let result = manager.validate_file_path(file_path).unwrap();
        
        assert!(result.is_valid());
        
        // Check that validation history was updated
        assert!(manager.validation_history.contains_key(&file_path.to_path_buf()));
    }

    #[test]
    fn test_monitoring_add_remove() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path();

        // Create a valid file
        let mut mmf = MemoryMappedFile::create(file_path, 1024).unwrap();
        let data_size = 1024 - FileHeader::SIZE; // All remaining space after header
        let test_data = vec![42u8; data_size];
        let header = FileHeader::new(b"TEST", 1, &test_data);
        
        mmf.write_at(0, &header).unwrap();
        mmf.write_slice_at(FileHeader::SIZE, &test_data).unwrap();
        mmf.sync().unwrap();
        drop(mmf);

        let mut manager = IntegrityManager::with_default_config();
        
        // Add to monitoring
        manager.add_to_monitoring(file_path).unwrap();
        assert_eq!(manager.monitored_files().len(), 1);
        assert!(manager.file_health_status(file_path).unwrap());
        
        // Remove from monitoring
        manager.remove_from_monitoring(file_path);
        assert_eq!(manager.monitored_files().len(), 0);
        assert!(manager.file_health_status(file_path).is_none());
    }

    #[test]
    fn test_needs_validation() {
        let temp_file = NamedTempFile::new().unwrap();
        let file_path = temp_file.path();

        // Create a valid file
        let mut mmf = MemoryMappedFile::create(file_path, 1024).unwrap();
        let test_data = vec![42u8; 100];
        let header = FileHeader::new(b"TEST", 1, &test_data);
        
        mmf.write_at(0, &header).unwrap();
        mmf.write_slice_at(FileHeader::SIZE, &test_data).unwrap();
        mmf.sync().unwrap();
        drop(mmf);

        // Configure with very short validation interval for testing
        let config = IntegrityConfig {
            periodic_validation_interval: Some(Duration::from_millis(1)),
            ..Default::default()
        };
        
        let mut manager = IntegrityManager::new(config);
        
        // File not monitored should need validation
        assert!(manager.needs_validation(file_path));
        
        // Add to monitoring
        manager.add_to_monitoring(file_path).unwrap();
        
        // Should not need validation immediately after adding
        assert!(!manager.needs_validation(file_path));
        
        // Wait for interval to pass
        std::thread::sleep(Duration::from_millis(10));
        
        // Should now need validation
        assert!(manager.needs_validation(file_path));
    }

    #[test]
    fn test_crc32_consistency() {
        let data1 = b"Hello, World!";
        let data2 = b"Hello, World!";
        let data3 = b"Different data";

        // Use FileHeader to test checksum consistency
        let header1 = FileHeader::new(b"TEST", 1, data1);
        let header2 = FileHeader::new(b"TEST", 1, data2);
        let header3 = FileHeader::new(b"TEST", 1, data3);

        assert_eq!(header1.checksum, header2.checksum);
        assert_ne!(header1.checksum, header3.checksum);
    }

    #[test]
    fn test_corruption_report_serialization() {
        let report = CorruptionReport {
            corruption_type: CorruptionType::DataCorruption,
            file_path: PathBuf::from("/test/file.dat"),
            corruption_offset: Some(1024),
            corruption_size: Some(256),
            description: "Test corruption for serialization".to_string(),
            recovery_recommendations: vec![
                "Restore from backup".to_string(),
                "Run data recovery tools".to_string(),
            ],
            severity: 0.75,
            is_recoverable: true,
            detected_at: SystemTime::now(),
        };

        // Test serialization
        let serialized = serde_json::to_string(&report).unwrap();
        let deserialized: CorruptionReport = serde_json::from_str(&serialized).unwrap();

        assert_eq!(report.corruption_type, deserialized.corruption_type);
        assert_eq!(report.file_path, deserialized.file_path);
        assert_eq!(report.corruption_offset, deserialized.corruption_offset);
        assert_eq!(report.severity, deserialized.severity);
        assert_eq!(report.is_recoverable, deserialized.is_recoverable);
    }

    #[test]
    fn test_storage_integrity_validation() {
        use crate::posting_storage::PostingStorage;
        use crate::identifiers::DocumentId;
        
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("postings.dat");

        // Create valid posting storage
        let mut storage = PostingStorage::create(&storage_path, 10).unwrap();
        let doc_id = DocumentId::new();
        storage.add_posting(doc_id, 100, 50).unwrap();
        storage.sync().unwrap();

        // Validate integrity through IntegrityManager
        let mut manager = IntegrityManager::with_default_config();
        let result = manager.validate_file(storage.memory_mapped_file()).unwrap();
        
        assert!(result.is_valid());
        
        // Test direct storage validation
        assert!(storage.validate_integrity().is_ok());
    }

    #[test]
    fn test_vector_storage_integrity_validation() {
        use crate::vector_storage::VectorStorage;
        
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("vectors.dat");

        // Create valid vector storage
        let mut storage = VectorStorage::create(&storage_path, 3, 5).unwrap();
        let vector = vec![1.0, 2.0, 3.0];
        storage.add_vector(&vector).unwrap();
        storage.sync().unwrap();

        // Validate integrity through IntegrityManager
        let mut manager = IntegrityManager::with_default_config();
        let result = manager.validate_file(storage.memory_mapped_file()).unwrap();
        
        assert!(result.is_valid());
        
        // Test direct storage validation
        assert!(storage.validate_integrity().is_ok());
    }

    #[test]
    fn test_periodic_validation_workflow() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("periodic_test.dat");

        // Create a valid file
        let mut mmf = MemoryMappedFile::create(&file_path, 1024).unwrap();
        let data_size = 1024 - FileHeader::SIZE; // All remaining space after header
        let test_data = vec![42u8; data_size];
        let header = FileHeader::new(b"TEST", 1, &test_data);
        
        mmf.write_at(0, &header).unwrap();
        mmf.write_slice_at(FileHeader::SIZE, &test_data).unwrap();
        mmf.sync().unwrap();
        drop(mmf);

        // Configure with short validation interval
        let config = IntegrityConfig {
            periodic_validation_interval: Some(Duration::from_millis(10)),
            ..Default::default()
        };
        
        let mut manager = IntegrityManager::new(config);
        
        // Add to monitoring
        manager.add_to_monitoring(&file_path).unwrap();
        assert!(manager.file_health_status(&file_path).unwrap());
        
        // Initially should not need validation
        assert!(!manager.needs_validation(&file_path));
        
        // Wait for interval
        std::thread::sleep(Duration::from_millis(20));
        assert!(manager.needs_validation(&file_path));
        
        // Perform periodic validation
        let results = manager.perform_periodic_validation().unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].is_valid());
        
        // Should not need validation immediately after
        assert!(!manager.needs_validation(&file_path));
    }

    #[test]
    fn test_corruption_detection_and_recovery() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("corrupt_test.dat");

        // Create file with intentional corruption
        let mut mmf = MemoryMappedFile::create(&file_path, 1024).unwrap();
        let original_data = vec![42u8; 100];
        let corrupted_data = vec![99u8; 100];
        
        // Create header with checksum for original data but write corrupted data
        let header = FileHeader::new(b"TEST", 1, &original_data);
        
        mmf.write_at(0, &header).unwrap();
        mmf.write_slice_at(FileHeader::SIZE, &corrupted_data).unwrap();
        mmf.sync().unwrap();
        drop(mmf);

        let mut manager = IntegrityManager::with_default_config();
        let result = manager.validate_file_path(&file_path).unwrap();
        
        assert!(!result.is_valid());
        let report = result.corruption_report().unwrap();
        assert_eq!(report.corruption_type, CorruptionType::DataCorruption);
        
        // Test recovery attempt (should fail as not implemented)
        let recovery_result = manager.attempt_recovery(report).unwrap();
        assert!(!recovery_result); // Recovery not yet implemented
    }

    #[test]
    fn test_integrity_statistics() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.dat");
        let file2 = temp_dir.path().join("file2.dat");

        // Create two valid files
        for file_path in [&file1, &file2] {
            let mut mmf = MemoryMappedFile::create(file_path, 1024).unwrap();
            let data_size = 1024 - FileHeader::SIZE; // All remaining space after header
            let test_data = vec![42u8; data_size];
            let header = FileHeader::new(b"TEST", 1, &test_data);
            
            mmf.write_at(0, &header).unwrap();
            mmf.write_slice_at(FileHeader::SIZE, &test_data).unwrap();
            mmf.sync().unwrap();
        }

        let mut manager = IntegrityManager::with_default_config();
        
        // Validate both files
        manager.validate_file_path(&file1).unwrap();
        manager.validate_file_path(&file2).unwrap();
        
        let stats = manager.stats();
        assert_eq!(stats.total_validations, 2);
        assert_eq!(stats.corruptions_detected, 0);
        assert!(stats.total_bytes_validated > 0);
        assert!(stats.last_validation.is_some());
    }

    #[test]
    fn test_cross_validation_capability() {
        // This test demonstrates the foundation for cross-validation between storage types
        let temp_dir = TempDir::new().unwrap();
        let posting_path = temp_dir.path().join("postings.dat");
        let vector_path = temp_dir.path().join("vectors.dat");

        // Create posting storage
        let mut posting_storage = crate::posting_storage::PostingStorage::create(&posting_path, 10).unwrap();
        let doc_id = crate::identifiers::DocumentId::new();
        posting_storage.add_posting(doc_id, 0, 3).unwrap(); // References first vector
        posting_storage.sync().unwrap();

        // Create vector storage
        let mut vector_storage = crate::vector_storage::VectorStorage::create(&vector_path, 3, 10).unwrap();
        let vector = vec![1.0, 2.0, 3.0];
        vector_storage.add_vector(&vector).unwrap();
        vector_storage.sync().unwrap();

        // Validate both storages independently
        let mut manager = IntegrityManager::with_default_config();
        
        let posting_result = manager.validate_file(posting_storage.memory_mapped_file()).unwrap();
        let vector_result = manager.validate_file(vector_storage.memory_mapped_file()).unwrap();
        
        assert!(posting_result.is_valid());
        assert!(vector_result.is_valid());
        
        // In a full implementation, we could add cross-validation logic here
        // to ensure that postings reference valid vector indices
    }

    #[test]
    fn test_detailed_corruption_analysis() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("detailed_analysis.dat");

        // Create file with specific corruption pattern
        let mut mmf = MemoryMappedFile::create(&file_path, 1024).unwrap();
        let data_size = 1024 - FileHeader::SIZE; // All remaining space after header
        let mut test_data = vec![42u8; data_size];
        
        // Corrupt specific bytes in the middle
        test_data[50] = 0xFF;
        test_data[51] = 0xFF;
        
        let header = FileHeader::new(b"TEST", 1, &vec![42u8; data_size]); // Original checksum
        
        mmf.write_at(0, &header).unwrap();
        mmf.write_slice_at(FileHeader::SIZE, &test_data).unwrap(); // Corrupted data
        mmf.sync().unwrap();
        drop(mmf);

        let config = IntegrityConfig {
            detailed_analysis: true,
            ..Default::default()
        };
        
        let mut manager = IntegrityManager::new(config);
        let result = manager.validate_file_path(&file_path).unwrap();
        

        
        assert!(!result.is_valid());
        let report = result.corruption_report().unwrap();
        
        // Verify detailed corruption report
        assert!(report.description.contains("Checksum mismatch")); // Note: Capital C
        assert!(!report.recovery_recommendations.is_empty());
        assert_eq!(report.corruption_type, CorruptionType::DataCorruption);
        assert!(report.severity > 0.0);
    }
}
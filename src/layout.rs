//! Directory structure and file layout management for Shardex indexes
//!
//! This module provides comprehensive file system management for Shardex indexes,
//! including directory structure creation, file naming conventions, metadata handling,
//! file discovery, and cleanup procedures.
//!
//! # Architecture
//!
//! The layout system consists of several key components:
//!
//! - [`DirectoryLayout`]: Manages the complete directory structure and file paths
//! - [`IndexMetadata`]: Handles index configuration and state persistence
//! - [`FileDiscovery`]: Provides utilities for finding and validating files
//! - [`CleanupManager`]: Handles atomic operations and cleanup procedures
//!
//! # Directory Structure
//!
//! ```text
//! shardex_index/
//! ├── shardex.meta          # Index metadata and configuration
//! ├── centroids/            # Shardex segments (centroids + metadata + bloom filters)
//! │   ├── segment_000001.shx
//! │   ├── segment_000002.shx
//! │   └── ...
//! ├── shards/              # Individual shard data
//! │   ├── {shard_ulid}.vectors
//! │   ├── {shard_ulid}.postings
//! │   └── ...
//! └── wal/                 # Write-ahead log segments
//!     ├── wal_000001.log
//!     ├── wal_000002.log
//!     └── ...
//! ```

use crate::config::ShardexConfig;
use crate::error::ShardexError;
use crate::identifiers::ShardId;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Version of the layout format for compatibility checking
pub const LAYOUT_VERSION: u32 = 1;

/// Metadata file name
pub const METADATA_FILE: &str = "shardex.meta";

/// Directory names
pub const CENTROIDS_DIR: &str = "centroids";
pub const SHARDS_DIR: &str = "shards";
pub const WAL_DIR: &str = "wal";

/// File extensions
pub const VECTORS_EXT: &str = "vectors";
pub const POSTINGS_EXT: &str = "postings";
pub const SEGMENT_EXT: &str = "shx";
pub const WAL_EXT: &str = "log";

/// Manages the complete directory structure and file layout for a Shardex index
///
/// DirectoryLayout provides a centralized interface for managing all file system
/// operations within a Shardex index, including path resolution, directory creation,
/// and file validation.
#[derive(Debug, Clone)]
pub struct DirectoryLayout {
    /// Root directory path for the index
    root_path: PathBuf,
    /// Path to the metadata file
    metadata_path: PathBuf,
    /// Path to the centroids directory
    centroids_dir: PathBuf,
    /// Path to the shards directory
    shards_dir: PathBuf,
    /// Path to the WAL directory
    wal_dir: PathBuf,
}

/// Index metadata containing configuration and state information
///
/// IndexMetadata is persisted to disk and contains all the information needed
/// to open and validate an existing index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexMetadata {
    /// Layout format version for compatibility
    pub layout_version: u32,
    /// Shardex configuration
    pub config: ShardexConfig,
    /// Timestamp when the index was created
    pub created_at: u64,
    /// Timestamp when the index was last modified
    pub modified_at: u64,
    /// Current number of shards in the index
    pub shard_count: usize,
    /// Current number of centroid segments
    pub centroid_segment_count: usize,
    /// Current number of WAL segments
    pub wal_segment_count: usize,
    /// Index state flags
    pub flags: IndexFlags,
}

/// Flags indicating various index states and capabilities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexFlags {
    /// Whether the index is currently being written to
    pub active: bool,
    /// Whether the index needs recovery from WAL
    pub needs_recovery: bool,
    /// Whether the index has been cleanly shut down
    pub clean_shutdown: bool,
}

/// File discovery utilities for finding and validating index files
#[derive(Debug)]
pub struct FileDiscovery {
    layout: DirectoryLayout,
}

/// Information about discovered files in the index
#[derive(Debug, Clone)]
pub struct DiscoveredFiles {
    /// Found shard files grouped by shard ID
    pub shards: Vec<ShardFiles>,
    /// Found centroid segment files
    pub centroid_segments: Vec<SegmentFile>,
    /// Found WAL segment files
    pub wal_segments: Vec<SegmentFile>,
    /// Orphaned files that don't match expected patterns
    pub orphaned_files: Vec<PathBuf>,
}

/// Files belonging to a single shard
#[derive(Debug, Clone)]
pub struct ShardFiles {
    /// Shard identifier
    pub shard_id: ShardId,
    /// Path to vectors file (if exists)
    pub vectors_file: Option<PathBuf>,
    /// Path to postings file (if exists)
    pub postings_file: Option<PathBuf>,
}

/// Information about a segment file
#[derive(Debug, Clone)]
pub struct SegmentFile {
    /// Segment number
    pub segment_number: u32,
    /// Full path to the segment file
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
}

/// Cleanup manager for handling atomic operations and file cleanup
#[derive(Debug)]
pub struct CleanupManager {
    layout: DirectoryLayout,
    /// Temporary files that need cleanup
    temp_files: HashSet<PathBuf>,
}

impl DirectoryLayout {
    /// Create a new directory layout for the given root path
    pub fn new<P: AsRef<Path>>(root_path: P) -> Self {
        let root_path = root_path.as_ref().to_path_buf();
        let metadata_path = root_path.join(METADATA_FILE);
        let centroids_dir = root_path.join(CENTROIDS_DIR);
        let shards_dir = root_path.join(SHARDS_DIR);
        let wal_dir = root_path.join(WAL_DIR);

        Self {
            root_path,
            metadata_path,
            centroids_dir,
            shards_dir,
            wal_dir,
        }
    }

    /// Get the root directory path
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Get the metadata file path
    pub fn metadata_path(&self) -> &Path {
        &self.metadata_path
    }

    /// Get the centroids directory path
    pub fn centroids_dir(&self) -> &Path {
        &self.centroids_dir
    }

    /// Get the shards directory path
    pub fn shards_dir(&self) -> &Path {
        &self.shards_dir
    }

    /// Get the WAL directory path
    pub fn wal_dir(&self) -> &Path {
        &self.wal_dir
    }

    /// Get the path for a shard's vectors file
    pub fn shard_vectors_path(&self, shard_id: &ShardId) -> PathBuf {
        self.shards_dir
            .join(format!("{}.{}", shard_id, VECTORS_EXT))
    }

    /// Get the path for a shard's postings file
    pub fn shard_postings_path(&self, shard_id: &ShardId) -> PathBuf {
        self.shards_dir
            .join(format!("{}.{}", shard_id, POSTINGS_EXT))
    }

    /// Get the path for a centroid segment file
    pub fn centroid_segment_path(&self, segment_number: u32) -> PathBuf {
        self.centroids_dir
            .join(format!("segment_{:06}.{}", segment_number, SEGMENT_EXT))
    }

    /// Get the path for a WAL segment file
    pub fn wal_segment_path(&self, segment_number: u32) -> PathBuf {
        self.wal_dir
            .join(format!("wal_{:06}.{}", segment_number, WAL_EXT))
    }

    /// Create all necessary directories for the index
    pub fn create_directories(&self) -> Result<()> {
        // Create root directory
        fs::create_dir_all(&self.root_path).map_err(|e| {
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to create root directory {}: {}",
                    self.root_path.display(),
                    e
                ),
            ))
        })?;

        // Create subdirectories
        for dir in [&self.centroids_dir, &self.shards_dir, &self.wal_dir] {
            fs::create_dir_all(dir).map_err(|e| {
                ShardexError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to create directory {}: {}", dir.display(), e),
                ))
            })?;
        }

        Ok(())
    }

    /// Check if the index directory exists and is valid
    pub fn exists(&self) -> bool {
        self.root_path.is_dir()
            && self.metadata_path.is_file()
            && self.centroids_dir.is_dir()
            && self.shards_dir.is_dir()
            && self.wal_dir.is_dir()
    }

    /// Validate the directory structure
    pub fn validate(&self) -> Result<()> {
        // Check root directory exists
        if !self.root_path.is_dir() {
            return Err(ShardexError::Corruption(format!(
                "Index root directory does not exist: {}",
                self.root_path.display()
            )));
        }

        // Check metadata file exists
        if !self.metadata_path.is_file() {
            return Err(ShardexError::Corruption(format!(
                "Index metadata file does not exist: {}",
                self.metadata_path.display()
            )));
        }

        // Check subdirectories exist
        for (name, dir) in [
            (CENTROIDS_DIR, &self.centroids_dir),
            (SHARDS_DIR, &self.shards_dir),
            (WAL_DIR, &self.wal_dir),
        ] {
            if !dir.is_dir() {
                return Err(ShardexError::Corruption(format!(
                    "Index {} directory does not exist: {}",
                    name,
                    dir.display()
                )));
            }
        }

        Ok(())
    }
}

impl IndexMetadata {
    /// Create new index metadata with the given configuration
    pub fn new(config: ShardexConfig) -> Result<Self> {
        config.validate()?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ShardexError::Config(format!("System time error: {}", e)))?
            .as_secs();

        Ok(Self {
            layout_version: LAYOUT_VERSION,
            config,
            created_at: now,
            modified_at: now,
            shard_count: 0,
            centroid_segment_count: 0,
            wal_segment_count: 0,
            flags: IndexFlags {
                active: false,
                needs_recovery: false,
                clean_shutdown: true,
            },
        })
    }

    /// Load metadata from a file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|e| {
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read metadata file {}: {}", path.display(), e),
            ))
        })?;

        let metadata: Self = toml::from_str(&contents).map_err(|e| {
            ShardexError::Corruption(format!(
                "Failed to parse metadata file {}: {}",
                path.display(),
                e
            ))
        })?;

        // Validate layout version compatibility
        if metadata.layout_version != LAYOUT_VERSION {
            return Err(ShardexError::Config(format!(
                "Unsupported layout version: expected {}, found {}",
                LAYOUT_VERSION, metadata.layout_version
            )));
        }

        // Validate the configuration
        metadata.config.validate()?;

        Ok(metadata)
    }

    /// Save metadata to a file
    pub fn save<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();

        // Update modified timestamp
        self.modified_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| ShardexError::Config(format!("System time error: {}", e)))?
            .as_secs();

        let contents = toml::to_string_pretty(self).map_err(|e| {
            ShardexError::Corruption(format!("Failed to serialize metadata: {}", e))
        })?;

        // Write atomically using a temporary file
        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, contents).map_err(|e| {
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to write metadata to {}: {}", temp_path.display(), e),
            ))
        })?;

        fs::rename(&temp_path, path).map_err(|e| {
            // Clean up temporary file on error
            let _ = fs::remove_file(&temp_path);
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to move metadata from {} to {}: {}",
                    temp_path.display(),
                    path.display(),
                    e
                ),
            ))
        })?;

        Ok(())
    }

    /// Check if the metadata is compatible with the given configuration
    pub fn is_compatible_with(&self, config: &ShardexConfig) -> bool {
        // Check critical configuration parameters that affect file format
        self.config.vector_size == config.vector_size
            && self.config.directory_path == config.directory_path
    }

    /// Update shard count
    pub fn set_shard_count(&mut self, count: usize) {
        self.shard_count = count;
    }

    /// Update centroid segment count
    pub fn set_centroid_segment_count(&mut self, count: usize) {
        self.centroid_segment_count = count;
    }

    /// Update WAL segment count
    pub fn set_wal_segment_count(&mut self, count: usize) {
        self.wal_segment_count = count;
    }

    /// Mark the index as active
    pub fn mark_active(&mut self) {
        self.flags.active = true;
        self.flags.clean_shutdown = false;
    }

    /// Mark the index as inactive and cleanly shut down
    pub fn mark_inactive(&mut self) {
        self.flags.active = false;
        self.flags.clean_shutdown = true;
        self.flags.needs_recovery = false;
    }

    /// Mark the index as needing recovery
    pub fn mark_needs_recovery(&mut self) {
        self.flags.needs_recovery = true;
        self.flags.clean_shutdown = false;
    }
}

impl Default for IndexFlags {
    fn default() -> Self {
        Self {
            active: false,
            needs_recovery: false,
            clean_shutdown: true,
        }
    }
}

impl FileDiscovery {
    /// Create a new file discovery instance
    pub fn new(layout: DirectoryLayout) -> Self {
        Self { layout }
    }

    /// Discover all files in the index
    pub fn discover_all(&self) -> Result<DiscoveredFiles> {
        let shards = self.discover_shards()?;
        let centroid_segments = self.discover_centroid_segments()?;
        let wal_segments = self.discover_wal_segments()?;
        let orphaned_files =
            self.find_orphaned_files(&shards, &centroid_segments, &wal_segments)?;

        Ok(DiscoveredFiles {
            shards,
            centroid_segments,
            wal_segments,
            orphaned_files,
        })
    }

    /// Discover shard files
    pub fn discover_shards(&self) -> Result<Vec<ShardFiles>> {
        let mut shard_map: std::collections::HashMap<ShardId, ShardFiles> =
            std::collections::HashMap::new();

        if !self.layout.shards_dir().exists() {
            return Ok(Vec::new());
        }

        let entries = fs::read_dir(self.layout.shards_dir()).map_err(|e| {
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to read shards directory {}: {}",
                    self.layout.shards_dir().display(),
                    e
                ),
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                ShardexError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to read directory entry: {}", e),
                ))
            })?;

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };

            // Parse shard files: {shard_id}.{extension}
            let parts: Vec<&str> = file_name.split('.').collect();
            if parts.len() != 2 {
                continue;
            }

            let shard_id_str = parts[0];
            let extension = parts[1];

            let shard_id = match ShardId::parse_str(shard_id_str) {
                Ok(id) => id,
                Err(_) => continue, // Not a valid shard ID
            };

            let shard_files = shard_map.entry(shard_id).or_insert_with(|| ShardFiles {
                shard_id,
                vectors_file: None,
                postings_file: None,
            });

            match extension {
                VECTORS_EXT => shard_files.vectors_file = Some(path),
                POSTINGS_EXT => shard_files.postings_file = Some(path),
                _ => {} // Unknown extension, ignore
            }
        }

        Ok(shard_map.into_values().collect())
    }

    /// Discover centroid segment files
    pub fn discover_centroid_segments(&self) -> Result<Vec<SegmentFile>> {
        self.discover_segments(self.layout.centroids_dir(), "segment_", SEGMENT_EXT)
    }

    /// Discover WAL segment files
    pub fn discover_wal_segments(&self) -> Result<Vec<SegmentFile>> {
        self.discover_segments(self.layout.wal_dir(), "wal_", WAL_EXT)
    }

    fn discover_segments(
        &self,
        dir: &Path,
        prefix: &str,
        extension: &str,
    ) -> Result<Vec<SegmentFile>> {
        let mut segments = Vec::new();

        if !dir.exists() {
            return Ok(segments);
        }

        let entries = fs::read_dir(dir).map_err(|e| {
            ShardexError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read directory {}: {}", dir.display(), e),
            ))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                ShardexError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to read directory entry: {}", e),
                ))
            })?;

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };

            // Parse segment files: {prefix}{number:06}.{extension}
            if !file_name.starts_with(prefix) || !file_name.ends_with(&format!(".{}", extension)) {
                continue;
            }

            let number_part = &file_name[prefix.len()..file_name.len() - extension.len() - 1];
            let segment_number = match number_part.parse::<u32>() {
                Ok(num) => num,
                Err(_) => continue, // Not a valid segment number
            };

            let metadata = entry.metadata().map_err(|e| {
                ShardexError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to get metadata for {}: {}", path.display(), e),
                ))
            })?;

            segments.push(SegmentFile {
                segment_number,
                path,
                size: metadata.len(),
            });
        }

        // Sort segments by number
        segments.sort_by_key(|s| s.segment_number);

        Ok(segments)
    }

    fn find_orphaned_files(
        &self,
        shards: &[ShardFiles],
        centroid_segments: &[SegmentFile],
        wal_segments: &[SegmentFile],
    ) -> Result<Vec<PathBuf>> {
        let mut orphaned = Vec::new();
        let mut known_files = HashSet::new();

        // Collect all known files
        for shard in shards {
            if let Some(ref path) = shard.vectors_file {
                known_files.insert(path.clone());
            }
            if let Some(ref path) = shard.postings_file {
                known_files.insert(path.clone());
            }
        }

        for segment in centroid_segments {
            known_files.insert(segment.path.clone());
        }

        for segment in wal_segments {
            known_files.insert(segment.path.clone());
        }

        // Check each directory for unknown files
        for dir in [
            self.layout.shards_dir(),
            self.layout.centroids_dir(),
            self.layout.wal_dir(),
        ] {
            if !dir.exists() {
                continue;
            }

            let entries = fs::read_dir(dir).map_err(|e| {
                ShardexError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to read directory {}: {}", dir.display(), e),
                ))
            })?;

            for entry in entries {
                let entry = entry.map_err(|e| {
                    ShardexError::Io(std::io::Error::new(
                        e.kind(),
                        format!("Failed to read directory entry: {}", e),
                    ))
                })?;

                let path = entry.path();
                if path.is_file() && !known_files.contains(&path) {
                    orphaned.push(path);
                }
            }
        }

        Ok(orphaned)
    }
}

impl CleanupManager {
    /// Create a new cleanup manager
    pub fn new(layout: DirectoryLayout) -> Self {
        Self {
            layout,
            temp_files: HashSet::new(),
        }
    }

    /// Register a temporary file for cleanup
    pub fn register_temp_file(&mut self, path: PathBuf) {
        self.temp_files.insert(path);
    }

    /// Clean up all temporary files
    pub fn cleanup_temp_files(&mut self) -> Result<()> {
        let mut errors = Vec::new();

        for path in &self.temp_files {
            if path.exists() {
                if let Err(e) = fs::remove_file(path) {
                    errors.push(format!("Failed to remove {}: {}", path.display(), e));
                }
            }
        }

        self.temp_files.clear();

        if !errors.is_empty() {
            return Err(ShardexError::Io(std::io::Error::other(
                format!("Cleanup errors: {}", errors.join(", ")),
            )));
        }

        Ok(())
    }

    /// Remove orphaned files
    pub fn cleanup_orphaned_files(&mut self, orphaned_files: &[PathBuf]) -> Result<()> {
        let mut errors = Vec::new();

        for path in orphaned_files {
            if path.exists() {
                if let Err(e) = fs::remove_file(path) {
                    errors.push(format!(
                        "Failed to remove orphaned file {}: {}",
                        path.display(),
                        e
                    ));
                }
            }
        }

        if !errors.is_empty() {
            return Err(ShardexError::Io(std::io::Error::other(
                format!("Orphaned file cleanup errors: {}", errors.join(", ")),
            )));
        }

        Ok(())
    }
}

impl Drop for CleanupManager {
    fn drop(&mut self) {
        // Best effort cleanup on drop
        let _ = self.cleanup_temp_files();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifiers::ShardId;
    use tempfile::TempDir;

    fn create_test_config() -> ShardexConfig {
        ShardexConfig::new().vector_size(128).shard_size(1000)
    }

    #[test]
    fn test_directory_layout_creation() {
        let temp_dir = TempDir::new().unwrap();
        let layout = DirectoryLayout::new(temp_dir.path());

        assert_eq!(layout.root_path(), temp_dir.path());
        assert_eq!(layout.metadata_path(), temp_dir.path().join(METADATA_FILE));
        assert_eq!(layout.centroids_dir(), temp_dir.path().join(CENTROIDS_DIR));
        assert_eq!(layout.shards_dir(), temp_dir.path().join(SHARDS_DIR));
        assert_eq!(layout.wal_dir(), temp_dir.path().join(WAL_DIR));
    }

    #[test]
    fn test_directory_creation() {
        let temp_dir = TempDir::new().unwrap();
        let layout = DirectoryLayout::new(temp_dir.path());

        assert!(!layout.exists());
        layout.create_directories().unwrap();

        assert!(layout.centroids_dir().is_dir());
        assert!(layout.shards_dir().is_dir());
        assert!(layout.wal_dir().is_dir());
    }

    #[test]
    fn test_file_path_generation() {
        let temp_dir = TempDir::new().unwrap();
        let layout = DirectoryLayout::new(temp_dir.path());

        let shard_id = ShardId::new();
        let vectors_path = layout.shard_vectors_path(&shard_id);
        let postings_path = layout.shard_postings_path(&shard_id);

        assert!(vectors_path
            .to_string_lossy()
            .contains(&shard_id.to_string()));
        assert!(vectors_path.to_string_lossy().ends_with(".vectors"));
        assert!(postings_path.to_string_lossy().ends_with(".postings"));

        let centroid_path = layout.centroid_segment_path(1);
        assert!(centroid_path.to_string_lossy().contains("segment_000001"));
        assert!(centroid_path.to_string_lossy().ends_with(".shx"));

        let wal_path = layout.wal_segment_path(5);
        assert!(wal_path.to_string_lossy().contains("wal_000005"));
        assert!(wal_path.to_string_lossy().ends_with(".log"));
    }

    #[test]
    fn test_index_metadata_creation() {
        let config = create_test_config();
        let metadata = IndexMetadata::new(config.clone()).unwrap();

        assert_eq!(metadata.layout_version, LAYOUT_VERSION);
        assert_eq!(metadata.config, config);
        assert_eq!(metadata.shard_count, 0);
        assert_eq!(metadata.centroid_segment_count, 0);
        assert_eq!(metadata.wal_segment_count, 0);
        assert!(!metadata.flags.active);
        assert!(!metadata.flags.needs_recovery);
        assert!(metadata.flags.clean_shutdown);
    }

    #[test]
    fn test_metadata_serialization() {
        let temp_dir = TempDir::new().unwrap();
        let metadata_path = temp_dir.path().join("test.meta");
        let config = create_test_config();

        let mut metadata = IndexMetadata::new(config).unwrap();
        metadata.shard_count = 10;
        metadata.mark_active();

        // Save metadata
        metadata.save(&metadata_path).unwrap();
        assert!(metadata_path.exists());

        // Load metadata
        let loaded_metadata = IndexMetadata::load(&metadata_path).unwrap();
        assert_eq!(loaded_metadata.layout_version, metadata.layout_version);
        assert_eq!(loaded_metadata.shard_count, metadata.shard_count);
        assert_eq!(loaded_metadata.flags.active, metadata.flags.active);
    }

    #[test]
    fn test_metadata_compatibility() {
        let config1 = create_test_config();
        let config2 = ShardexConfig::new().vector_size(256).shard_size(1000);

        let metadata = IndexMetadata::new(config1.clone()).unwrap();

        assert!(metadata.is_compatible_with(&config1));
        assert!(!metadata.is_compatible_with(&config2)); // Different vector size
    }

    #[test]
    fn test_file_discovery_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let layout = DirectoryLayout::new(temp_dir.path());
        layout.create_directories().unwrap();

        let discovery = FileDiscovery::new(layout);
        let discovered = discovery.discover_all().unwrap();

        assert!(discovered.shards.is_empty());
        assert!(discovered.centroid_segments.is_empty());
        assert!(discovered.wal_segments.is_empty());
        assert!(discovered.orphaned_files.is_empty());
    }

    #[test]
    fn test_file_discovery_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let layout = DirectoryLayout::new(temp_dir.path());
        layout.create_directories().unwrap();

        // Create some test files
        let shard_id = ShardId::new();
        let vectors_path = layout.shard_vectors_path(&shard_id);
        let postings_path = layout.shard_postings_path(&shard_id);
        let centroid_path = layout.centroid_segment_path(1);
        let wal_path = layout.wal_segment_path(1);

        fs::write(&vectors_path, b"vectors data").unwrap();
        fs::write(&postings_path, b"postings data").unwrap();
        fs::write(&centroid_path, b"centroid data").unwrap();
        fs::write(&wal_path, b"wal data").unwrap();

        let discovery = FileDiscovery::new(layout);
        let discovered = discovery.discover_all().unwrap();

        assert_eq!(discovered.shards.len(), 1);
        assert_eq!(discovered.shards[0].shard_id, shard_id);
        assert!(discovered.shards[0].vectors_file.is_some());
        assert!(discovered.shards[0].postings_file.is_some());

        assert_eq!(discovered.centroid_segments.len(), 1);
        assert_eq!(discovered.centroid_segments[0].segment_number, 1);

        assert_eq!(discovered.wal_segments.len(), 1);
        assert_eq!(discovered.wal_segments[0].segment_number, 1);

        assert!(discovered.orphaned_files.is_empty());
    }

    #[test]
    fn test_orphaned_file_detection() {
        let temp_dir = TempDir::new().unwrap();
        let layout = DirectoryLayout::new(temp_dir.path());
        layout.create_directories().unwrap();

        // Create a valid shard file
        let shard_id = ShardId::new();
        let vectors_path = layout.shard_vectors_path(&shard_id);
        fs::write(&vectors_path, b"vectors data").unwrap();

        // Create an orphaned file
        let orphaned_path = layout.shards_dir().join("orphaned_file.txt");
        fs::write(&orphaned_path, b"orphaned data").unwrap();

        let discovery = FileDiscovery::new(layout);
        let discovered = discovery.discover_all().unwrap();

        assert_eq!(discovered.shards.len(), 1);
        assert_eq!(discovered.orphaned_files.len(), 1);
        assert_eq!(discovered.orphaned_files[0], orphaned_path);
    }

    #[test]
    fn test_cleanup_manager() {
        let temp_dir = TempDir::new().unwrap();
        let layout = DirectoryLayout::new(temp_dir.path());

        let mut cleanup_manager = CleanupManager::new(layout);

        // Create a temporary file
        let temp_file = temp_dir.path().join("temp_file.tmp");
        fs::write(&temp_file, b"temporary data").unwrap();
        assert!(temp_file.exists());

        // Register and cleanup
        cleanup_manager.register_temp_file(temp_file.clone());
        cleanup_manager.cleanup_temp_files().unwrap();
        assert!(!temp_file.exists());
    }

    #[test]
    fn test_atomic_metadata_save() {
        let temp_dir = TempDir::new().unwrap();
        let metadata_path = temp_dir.path().join("test.meta");
        let config = create_test_config();

        let mut metadata = IndexMetadata::new(config).unwrap();

        // Save should be atomic - no temporary file left behind
        metadata.save(&metadata_path).unwrap();
        assert!(metadata_path.exists());
        assert!(!metadata_path.with_extension("tmp").exists());
    }

    #[test]
    fn test_layout_validation() {
        let temp_dir = TempDir::new().unwrap();
        let layout = DirectoryLayout::new(temp_dir.path());

        // Should fail validation before creation
        assert!(layout.validate().is_err());
        assert!(!layout.exists());

        // Create directories and metadata
        layout.create_directories().unwrap();
        let mut metadata = IndexMetadata::new(create_test_config()).unwrap();
        metadata.save(layout.metadata_path()).unwrap();

        // Should pass validation after creation
        assert!(layout.validate().is_ok());
        assert!(layout.exists());
    }
}

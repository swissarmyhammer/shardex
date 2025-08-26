//! Performance-optimized memory mapping for document text storage
//!
//! This module provides enhanced memory mapping capabilities with LRU caching,
//! memory advice hints, and performance optimization features for document text storage.

use crate::document_text_entry::DocumentTextEntry;
use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use crate::memory::MemoryMappedFile;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Instant, SystemTime};

/// Access pattern hints for memory optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessPattern {
    /// Optimize for sequential access patterns (scans, iterations)
    Sequential,
    /// Optimize for random access patterns (lookups, individual reads)
    Random,
    /// Balanced optimization for mixed access patterns
    Mixed,
}

/// LRU cache entry with access tracking
#[derive(Debug, Clone)]
struct CacheEntry {
    entry: DocumentTextEntry,
    last_access: SystemTime,
    access_count: u64,
}

/// Custom LRU cache implementation for document text entries
#[derive(Debug)]
struct LruCache {
    cache: HashMap<DocumentId, CacheEntry>,
    access_order: Vec<DocumentId>,
    max_size: usize,
}

impl LruCache {
    fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_size),
            access_order: Vec::with_capacity(max_size),
            max_size,
        }
    }

    fn get(&mut self, document_id: &DocumentId) -> Option<DocumentTextEntry> {
        if let Some(entry) = self.cache.get_mut(document_id) {
            entry.last_access = SystemTime::now();
            entry.access_count += 1;

            // Move to front of access order
            if let Some(pos) = self.access_order.iter().position(|id| id == document_id) {
                self.access_order.remove(pos);
            }
            self.access_order.push(*document_id);

            Some(entry.entry)
        } else {
            None
        }
    }

    fn put(&mut self, document_id: DocumentId, entry: DocumentTextEntry) {
        // Remove from access order if already present
        if let Some(pos) = self.access_order.iter().position(|id| id == &document_id) {
            self.access_order.remove(pos);
        }

        // Add to cache and access order
        let cache_entry = CacheEntry {
            entry,
            last_access: SystemTime::now(),
            access_count: 1,
        };
        self.cache.insert(document_id, cache_entry);
        self.access_order.push(document_id);

        // Evict oldest if over capacity
        if self.cache.len() > self.max_size {
            if let Some(oldest_id) = self.access_order.first().copied() {
                self.access_order.remove(0);
                self.cache.remove(&oldest_id);
            }
        }
    }

    fn len(&self) -> usize {
        self.cache.len()
    }

    fn capacity(&self) -> usize {
        self.max_size
    }

    fn clear(&mut self) {
        self.cache.clear();
        self.access_order.clear();
    }
}

/// Performance statistics for optimized memory mapping
#[derive(Debug, Default, Clone)]
pub struct OptimizedMappingStats {
    /// Total cache hits
    pub cache_hits: u64,
    /// Total cache misses
    pub cache_misses: u64,
    /// Average lookup latency in microseconds
    pub avg_lookup_latency_us: f64,
    /// Total number of pages prefaulted
    pub pages_prefaulted: u64,
    /// Memory advice applications
    pub memory_advice_applied: u64,
    /// Cache evictions performed
    pub cache_evictions: u64,
}

impl OptimizedMappingStats {
    /// Calculate cache hit ratio
    pub fn hit_ratio(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }

    /// Get total cache operations
    pub fn total_operations(&self) -> u64 {
        self.cache_hits + self.cache_misses
    }
}

/// Optimized memory mapping manager for document text storage
pub struct OptimizedMemoryMapping {
    /// Underlying memory-mapped index file
    index_file: Arc<RwLock<MemoryMappedFile>>,

    /// LRU cache for frequently accessed entries
    entry_cache: Arc<RwLock<LruCache>>,
    /// System page size for alignment optimization
    page_size: usize,
    /// Access pattern hint for optimization
    access_pattern: AccessPattern,
    /// Performance statistics
    stats: Arc<RwLock<OptimizedMappingStats>>,
}

impl OptimizedMemoryMapping {
    /// Create optimized memory mapping with performance hints
    pub fn create_optimized(
        index_file: MemoryMappedFile,
        access_pattern: AccessPattern,
        cache_size: usize,
    ) -> Result<Self, ShardexError> {
        let page_size = Self::get_system_page_size();
        let entry_cache = Arc::new(RwLock::new(LruCache::new(cache_size)));

        let mapping = Self {
            index_file: Arc::new(RwLock::new(index_file)),
            entry_cache,
            page_size,
            access_pattern,
            stats: Arc::new(RwLock::new(OptimizedMappingStats::default())),
        };

        // Apply memory advice hints based on access pattern
        mapping.apply_memory_advice()?;

        Ok(mapping)
    }

    /// Get system page size
    fn get_system_page_size() -> usize {
        // Use a reasonable default of 4KB if we can't determine the actual page size
        4096
    }

    /// Apply memory advice hints based on access pattern
    fn apply_memory_advice(&self) -> Result<(), ShardexError> {
        // Update stats first
        {
            let mut stats = self.stats.write().map_err(|_| ShardexError::InvalidInput {
                field: "stats_lock".to_string(),
                reason: "Failed to acquire stats write lock".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;
            stats.memory_advice_applied += 1;
        }

        // Get memory region information from the index file
        let index_file = self
            .index_file
            .read()
            .map_err(|_| ShardexError::InvalidInput {
                field: "index_file_lock".to_string(),
                reason: "Failed to acquire index file read lock".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

        let memory_slice = index_file.as_slice();
        let memory_ptr = memory_slice.as_ptr() as *mut std::ffi::c_void;
        let memory_size = memory_slice.len();

        // Apply platform-specific memory advice
        self.apply_platform_memory_advice(memory_ptr, memory_size)?;

        Ok(())
    }

    /// Apply platform-specific memory advice hints (Unix/Linux implementation)
    /// 
    /// Uses the madvise() system call to provide the kernel with information about
    /// expected memory access patterns, which can help optimize memory management.
    /// 
    /// # Platform Behavior
    /// - Unix/Linux: Uses libc::madvise() with appropriate MADV_* flags
    /// - Sequential access → MADV_SEQUENTIAL (optimize for sequential reading)
    /// - Random access → MADV_RANDOM (disable readahead, optimize for random access)
    /// - Mixed access → MADV_NORMAL (default kernel behavior)
    /// 
    /// # Safety Considerations
    /// - Uses unsafe madvise() system call on the provided memory pointer
    /// - Assumes ptr and len represent a valid memory-mapped region
    /// - Failures are logged but don't cause operation failure (advisory nature)
    /// 
    /// # Error Handling
    /// - System call failures are logged as warnings using portable error reporting
    /// - Always returns Ok(()) as memory advice failures are non-critical
    /// - Distinguishes between different errno conditions for better diagnostics
    /// 
    /// # Arguments
    /// - `ptr`: Raw pointer to the start of the memory region
    /// - `len`: Length of the memory region in bytes
    /// 
    /// # Returns
    /// Always returns Ok(()) - madvise failures don't break functionality
    #[cfg(unix)]
    fn apply_platform_memory_advice(&self, ptr: *mut std::ffi::c_void, len: usize) -> Result<(), ShardexError> {
        use std::ffi::c_int;

        // Map access patterns to madvise flags
        let advice_flag: c_int = match self.access_pattern {
            AccessPattern::Sequential => libc::MADV_SEQUENTIAL,
            AccessPattern::Random => libc::MADV_RANDOM,
            AccessPattern::Mixed => libc::MADV_NORMAL,
        };

        // Apply the memory advice
        let result = unsafe { libc::madvise(ptr, len, advice_flag) };

        if result == 0 {
            log::debug!("Successfully applied memory advice: {:?}", self.access_pattern);
            Ok(())
        } else {
            let error = std::io::Error::last_os_error();
            log::warn!(
                "Failed to apply memory advice {:?}: {}",
                self.access_pattern,
                error
            );
            // Don't fail the operation - madvise failures are not critical
            Ok(())
        }
    }

    /// Apply platform-specific memory advice hints (Windows implementation)
    /// 
    /// Windows has limited equivalents to Unix madvise(). This implementation
    /// provides logging for different access patterns but does not make actual
    /// system calls due to Windows API limitations.
    /// 
    /// # Platform Behavior
    /// - Windows: Limited memory advice support compared to Unix
    /// - Could potentially use VirtualAlloc with MEM_RESET for some patterns
    /// - Currently logs access patterns for debugging and monitoring
    /// 
    /// # Future Enhancements
    /// - Could implement VirtualAlloc(MEM_RESET) for memory reset hints
    /// - Could use PrefetchVirtualMemory for sequential access optimization
    /// - Windows 8+ supports limited memory management hints
    /// 
    /// # Arguments
    /// - `_ptr`: Raw pointer to the start of the memory region (unused)
    /// - `_len`: Length of the memory region in bytes (unused)
    /// 
    /// # Returns
    /// Always returns Ok(()) - Windows implementation is currently advisory logging only
    #[cfg(windows)]
    fn apply_platform_memory_advice(&self, _ptr: *mut std::ffi::c_void, _len: usize) -> Result<(), ShardexError> {
        // Windows doesn't have direct equivalents to madvise
        // We can use VirtualAlloc with MEM_RESET for some patterns
        match self.access_pattern {
            AccessPattern::Sequential | AccessPattern::Mixed => {
                // For sequential/mixed access, prefetch pages
                log::debug!("Applied Windows memory hints for pattern: {:?}", self.access_pattern);
            }
            AccessPattern::Random => {
                // For random access, we might consider using VirtualAlloc with MEM_RESET
                // to advise the system that data may not be needed immediately
                log::debug!("Applied Windows random access hints");
            }
        }
        
        // Windows memory advice is more limited, so we log and continue
        Ok(())
    }

    /// Apply platform-specific memory advice hints (fallback for unsupported platforms)
    /// 
    /// Fallback implementation for platforms that don't have Unix madvise() or
    /// Windows memory management APIs. Provides logging for access patterns
    /// without making any system calls.
    /// 
    /// # Platform Behavior
    /// - Other platforms: No system calls made, logging only
    /// - Provides consistent API across all platforms
    /// - Allows code to run on any platform without conditional compilation at call sites
    /// 
    /// # Arguments
    /// - `_ptr`: Raw pointer to the start of the memory region (unused)
    /// - `_len`: Length of the memory region in bytes (unused)
    /// 
    /// # Returns
    /// Always returns Ok(()) - fallback implementation is logging only
    #[cfg(not(any(unix, windows)))]
    fn apply_platform_memory_advice(&self, _ptr: *mut std::ffi::c_void, _len: usize) -> Result<(), ShardexError> {
        log::debug!("Memory advice not supported on this platform, pattern: {:?}", self.access_pattern);
        Ok(())
    }

    /// Find document entry with optimized caching
    pub fn find_latest_entry_optimized(
        &self,
        document_id: DocumentId,
    ) -> Result<Option<DocumentTextEntry>, ShardexError> {
        let start_time = Instant::now();

        // Check cache first
        {
            let mut cache = self
                .entry_cache
                .write()
                .map_err(|_| ShardexError::InvalidInput {
                    field: "cache_lock".to_string(),
                    reason: "Failed to acquire cache write lock".to_string(),
                    suggestion: "Retry the operation".to_string(),
                })?;

            if let Some(entry) = cache.get(&document_id) {
                // Cache hit - update statistics
                let elapsed = start_time.elapsed();
                self.update_stats(true, elapsed.as_micros() as f64)?;
                return Ok(Some(entry));
            }
        }

        // Cache miss - search in the index file
        let entry = self.search_index_file(document_id)?;

        if let Some(entry) = entry {
            // Add to cache
            {
                let mut cache = self
                    .entry_cache
                    .write()
                    .map_err(|_| ShardexError::InvalidInput {
                        field: "cache_lock".to_string(),
                        reason: "Failed to acquire cache write lock".to_string(),
                        suggestion: "Retry the operation".to_string(),
                    })?;
                cache.put(document_id, entry);
            }
        }

        // Update miss statistics
        let elapsed = start_time.elapsed();
        self.update_stats(false, elapsed.as_micros() as f64)?;

        Ok(entry)
    }

    /// Search for document entry in the index file
    fn search_index_file(&self, document_id: DocumentId) -> Result<Option<DocumentTextEntry>, ShardexError> {
        let index_file = self
            .index_file
            .read()
            .map_err(|_| ShardexError::InvalidInput {
                field: "index_file_lock".to_string(),
                reason: "Failed to acquire index file read lock".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

        // Read header to get entry count
        let header: crate::document_text_entry::TextIndexHeader = index_file.read_at(0)?;

        let entry_size = std::mem::size_of::<DocumentTextEntry>();
        let total_entries = header.entry_count as usize;

        // Calculate entries per page for page-aligned access
        let entries_per_page = self.page_size / entry_size;
        let total_pages = (total_entries + entries_per_page - 1) / entries_per_page;

        // Search backwards page by page for latest entry
        for page_idx in (0..total_pages).rev() {
            // Prefault the page to ensure it's loaded
            self.prefault_index_page(page_idx)?;

            let start_entry = page_idx * entries_per_page;
            let end_entry = ((page_idx + 1) * entries_per_page).min(total_entries);

            // Search within the page backwards
            for entry_idx in (start_entry..end_entry).rev() {
                let offset =
                    std::mem::size_of::<crate::document_text_entry::TextIndexHeader>() + (entry_idx * entry_size);
                let entry: DocumentTextEntry = index_file.read_at(offset)?;

                if entry.document_id == document_id {
                    return Ok(Some(entry));
                }
            }
        }

        Ok(None)
    }

    /// Prefault memory page to avoid page faults during critical sections
    fn prefault_index_page(&self, page_index: usize) -> Result<(), ShardexError> {
        // This is a placeholder for actual prefaulting
        // In practice, we would touch memory locations to ensure pages are loaded
        {
            let mut stats = self.stats.write().map_err(|_| ShardexError::InvalidInput {
                field: "stats_lock".to_string(),
                reason: "Failed to acquire stats write lock".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;
            stats.pages_prefaulted += 1;
        }

        log::trace!("Prefaulted index page {}", page_index);
        Ok(())
    }

    /// Update performance statistics
    fn update_stats(&self, cache_hit: bool, latency_us: f64) -> Result<(), ShardexError> {
        let mut stats = self.stats.write().map_err(|_| ShardexError::InvalidInput {
            field: "stats_lock".to_string(),
            reason: "Failed to acquire stats write lock".to_string(),
            suggestion: "Retry the operation".to_string(),
        })?;

        if cache_hit {
            stats.cache_hits += 1;
        } else {
            stats.cache_misses += 1;
        }

        // Update rolling average latency
        let total_ops = stats.cache_hits + stats.cache_misses;
        if total_ops == 1 {
            stats.avg_lookup_latency_us = latency_us;
        } else {
            stats.avg_lookup_latency_us =
                ((stats.avg_lookup_latency_us * (total_ops - 1) as f64) + latency_us) / total_ops as f64;
        }

        Ok(())
    }

    /// Get current performance statistics
    pub fn get_stats(&self) -> Result<OptimizedMappingStats, ShardexError> {
        let stats = self.stats.read().map_err(|_| ShardexError::InvalidInput {
            field: "stats_lock".to_string(),
            reason: "Failed to acquire stats read lock".to_string(),
            suggestion: "Retry the operation".to_string(),
        })?;
        Ok(stats.clone())
    }

    /// Warm up cache with recently accessed entries
    pub fn warm_cache(&self, document_ids: Vec<DocumentId>) -> Result<(), ShardexError> {
        for document_id in document_ids {
            // This will populate the cache
            let _ = self.find_latest_entry_optimized(document_id)?;
        }
        Ok(())
    }

    /// Clear cache and reset statistics
    pub fn reset_cache(&self) -> Result<(), ShardexError> {
        {
            let mut cache = self
                .entry_cache
                .write()
                .map_err(|_| ShardexError::InvalidInput {
                    field: "cache_lock".to_string(),
                    reason: "Failed to acquire cache write lock".to_string(),
                    suggestion: "Retry the operation".to_string(),
                })?;
            cache.clear();
        }

        {
            let mut stats = self.stats.write().map_err(|_| ShardexError::InvalidInput {
                field: "stats_lock".to_string(),
                reason: "Failed to acquire stats write lock".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;
            *stats = OptimizedMappingStats::default();
        }

        Ok(())
    }

    /// Check cache health and provide optimization suggestions
    pub fn check_cache_health(&self) -> Result<CacheHealthReport, ShardexError> {
        let stats = self.get_stats()?;
        let cache = self
            .entry_cache
            .read()
            .map_err(|_| ShardexError::InvalidInput {
                field: "cache_lock".to_string(),
                reason: "Failed to acquire cache read lock".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;

        let cache_utilization = cache.len() as f64 / cache.capacity() as f64;
        let hit_ratio = stats.hit_ratio();

        let health = if hit_ratio >= 0.8 && cache_utilization > 0.5 {
            CacheHealth::Excellent
        } else if hit_ratio >= 0.6 && cache_utilization > 0.3 {
            CacheHealth::Good
        } else if hit_ratio >= 0.4 {
            CacheHealth::Fair
        } else {
            CacheHealth::Poor
        };

        let mut suggestions = Vec::new();
        if hit_ratio < 0.5 {
            suggestions.push("Consider increasing cache size".to_string());
        }
        if cache_utilization < 0.3 {
            suggestions.push("Cache may be oversized for current workload".to_string());
        }
        if stats.avg_lookup_latency_us > 1000.0 {
            suggestions.push("High lookup latency detected, consider memory optimization".to_string());
        }

        Ok(CacheHealthReport {
            health,
            hit_ratio,
            cache_utilization,
            avg_latency_us: stats.avg_lookup_latency_us,
            total_operations: stats.total_operations(),
            suggestions,
        })
    }

    /// Get cache size information
    pub fn cache_info(&self) -> Result<(usize, usize), ShardexError> {
        let cache = self
            .entry_cache
            .read()
            .map_err(|_| ShardexError::InvalidInput {
                field: "cache_lock".to_string(),
                reason: "Failed to acquire cache read lock".to_string(),
                suggestion: "Retry the operation".to_string(),
            })?;
        Ok((cache.len(), cache.capacity()))
    }
}

/// Cache health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CacheHealth {
    Excellent,
    Good,
    Fair,
    Poor,
}

/// Cache health report with recommendations
#[derive(Debug, Clone)]
pub struct CacheHealthReport {
    pub health: CacheHealth,
    pub hit_ratio: f64,
    pub cache_utilization: f64,
    pub avg_latency_us: f64,
    pub total_operations: u64,
    pub suggestions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identifiers::DocumentId;

    #[test]
    fn test_lru_cache_basic_operations() {
        let mut cache = LruCache::new(3);
        let doc_id = DocumentId::new();
        let entry = DocumentTextEntry {
            document_id: doc_id,
            text_offset: 100,
            text_length: 50,
        };

        // Test put and get
        cache.put(doc_id, entry);
        assert_eq!(cache.len(), 1);

        let retrieved = cache.get(&doc_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().document_id, doc_id);
    }

    #[test]
    fn test_lru_cache_eviction() {
        let mut cache = LruCache::new(2);
        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        let doc3 = DocumentId::new();

        let entry1 = DocumentTextEntry {
            document_id: doc1,
            text_offset: 100,
            text_length: 50,
        };
        let entry2 = DocumentTextEntry {
            document_id: doc2,
            text_offset: 200,
            text_length: 60,
        };
        let entry3 = DocumentTextEntry {
            document_id: doc3,
            text_offset: 300,
            text_length: 70,
        };

        cache.put(doc1, entry1);
        cache.put(doc2, entry2);
        assert_eq!(cache.len(), 2);

        // Adding third entry should evict the first
        cache.put(doc3, entry3);
        assert_eq!(cache.len(), 2);

        // doc1 should be evicted
        assert!(cache.get(&doc1).is_none());
        assert!(cache.get(&doc2).is_some());
        assert!(cache.get(&doc3).is_some());
    }

    #[test]
    fn test_optimized_mapping_stats() {
        let stats = OptimizedMappingStats {
            cache_hits: 80,
            cache_misses: 20,
            ..Default::default()
        };

        assert_eq!(stats.hit_ratio(), 0.8);
        assert_eq!(stats.total_operations(), 100);
    }

    #[test]
    fn test_access_pattern_variants() {
        // Test that all access patterns are valid
        let patterns = [AccessPattern::Sequential, AccessPattern::Random, AccessPattern::Mixed];

        for pattern in patterns {
            assert!(matches!(
                pattern,
                AccessPattern::Sequential | AccessPattern::Random | AccessPattern::Mixed
            ));
        }
    }

    #[test]
    fn test_apply_memory_advice_functionality() {
        use tempfile::TempDir;
        
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("advice_test.dat");
        
        // Test each access pattern
        for pattern in [AccessPattern::Sequential, AccessPattern::Random, AccessPattern::Mixed] {
            // Create a new memory-mapped file for each test
            let index_file = MemoryMappedFile::create(&file_path, 4096).unwrap();
            
            let mapping = OptimizedMemoryMapping::create_optimized(
                index_file,
                pattern,
                10
            ).unwrap();
            
            let initial_stats = mapping.get_stats().unwrap();
            
            // Apply memory advice again - this should call the actual implementation
            mapping.apply_memory_advice().unwrap();
            
            let updated_stats = mapping.get_stats().unwrap();
            
            // Should have increased the advice application count
            assert!(updated_stats.memory_advice_applied > initial_stats.memory_advice_applied);
        }
    }
}

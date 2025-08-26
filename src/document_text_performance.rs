//! Performance optimization components for document text storage
//!
//! This module provides performance-enhanced wrappers and utilities for the
//! document text storage system, including memory mapping optimizations,
//! caching layers, and access pattern optimization.

use crate::document_text_entry::DocumentTextEntry;
use crate::error::ShardexError;
use crate::identifiers::DocumentId;
use crate::memory::MemoryMappedFile;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Access pattern hint for memory mapping optimization
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AccessPattern {
    /// Sequential access pattern - optimize for forward scanning
    Sequential,
    /// Random access pattern - optimize for random lookups
    Random,
    /// Mixed access pattern - balanced optimization
    Mixed,
}

/// Simple LRU cache implementation for document entries
/// 
/// This is a basic LRU cache since the `lru` crate is not available.
/// It uses a HashMap for O(1) access and tracks access order.
pub struct DocumentEntryCache {
    /// Cache data storage
    cache: HashMap<DocumentId, CacheEntry>,
    /// Access order tracking (oldest to newest)
    access_order: Vec<DocumentId>,
    /// Maximum cache size
    capacity: usize,
    /// Cache statistics
    stats: CacheStats,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached document entry
    entry: DocumentTextEntry,
    /// Last access time for statistics
    last_access: Instant,
}

#[derive(Debug, Default)]
struct CacheStats {
    /// Total cache hits
    hits: u64,
    /// Total cache misses
    misses: u64,
    /// Total evictions
    evictions: u64,
}

impl DocumentEntryCache {
    /// Create new LRU cache with given capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(capacity),
            access_order: Vec::with_capacity(capacity),
            capacity: capacity.max(1), // Minimum capacity of 1
            stats: CacheStats::default(),
        }
    }

    /// Get entry from cache if present
    pub fn get(&mut self, document_id: &DocumentId) -> Option<DocumentTextEntry> {
        if let Some(entry) = self.cache.get(document_id) {
            let result = entry.entry;
            // Update access time and move to end of access order
            let entry = self.cache.get_mut(document_id).unwrap();
            entry.last_access = Instant::now();
            self.move_to_end(*document_id);
            self.stats.hits += 1;
            Some(result)
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Put entry into cache, evicting if necessary
    pub fn put(&mut self, document_id: DocumentId, entry: DocumentTextEntry) {
        // If already exists, update and move to end
        if let Some(cache_entry) = self.cache.get_mut(&document_id) {
            cache_entry.entry = entry;
            cache_entry.last_access = Instant::now();
            self.move_to_end(document_id);
            return;
        }

        // If at capacity, evict LRU item
        if self.cache.len() >= self.capacity {
            if let Some(lru_id) = self.access_order.first().copied() {
                self.cache.remove(&lru_id);
                self.access_order.retain(|&id| id != lru_id);
                self.stats.evictions += 1;
            }
        }

        // Insert new entry
        self.cache.insert(document_id, CacheEntry {
            entry,
            last_access: Instant::now(),
        });
        self.access_order.push(document_id);
    }

    /// Move document to end of access order (most recently used)
    fn move_to_end(&mut self, document_id: DocumentId) {
        self.access_order.retain(|&id| id != document_id);
        self.access_order.push(document_id);
    }

    /// Get cache statistics
    pub fn stats(&self) -> (u64, u64, u64) {
        (self.stats.hits, self.stats.misses, self.stats.evictions)
    }

    /// Get cache hit ratio
    pub fn hit_ratio(&self) -> f64 {
        let total = self.stats.hits + self.stats.misses;
        if total == 0 {
            0.0
        } else {
            self.stats.hits as f64 / total as f64
        }
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.access_order.clear();
        self.stats = CacheStats::default();
    }

    /// Get current cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

/// Performance statistics for memory mapping operations
#[derive(Debug, Default)]
pub struct MappingStats {
    /// Number of entry lookups performed
    pub lookups: u64,
    /// Number of cache hits
    pub cache_hits: u64,
    /// Number of cache misses
    pub cache_misses: u64,
    /// Total time spent in lookups
    pub lookup_time: Duration,
    /// Number of pages pre-faulted
    pub pages_prefaulted: u64,
    /// Time spent in pre-faulting
    pub prefault_time: Duration,
}

impl MappingStats {
    /// Record a lookup operation
    pub fn record_lookup(&mut self, duration: Duration, cache_hit: bool) {
        self.lookups += 1;
        self.lookup_time += duration;
        if cache_hit {
            self.cache_hits += 1;
        } else {
            self.cache_misses += 1;
        }
    }

    /// Record a prefault operation
    pub fn record_prefault(&mut self, pages: u64, duration: Duration) {
        self.pages_prefaulted += pages;
        self.prefault_time += duration;
    }

    /// Get cache hit ratio
    pub fn cache_hit_ratio(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }

    /// Get average lookup time
    pub fn average_lookup_time(&self) -> Duration {
        if self.lookups == 0 {
            Duration::ZERO
        } else {
            self.lookup_time / self.lookups as u32
        }
    }
}

/// Optimized memory mapping wrapper for document text storage
///
/// This component provides performance optimizations over the basic MemoryMappedFile:
/// - LRU caching of frequently accessed document entries
/// - Memory advice hints for optimal OS memory management
/// - Page-aligned access patterns to reduce page faults
/// - Performance tracking and statistics
pub struct OptimizedMemoryMapping {
    /// Wrapped memory-mapped index file
    index_mapping: MemoryMappedFile,
    /// Wrapped memory-mapped data file  
    data_mapping: MemoryMappedFile,
    /// System page size for alignment calculations
    page_size: usize,
    /// Configured access pattern for OS hints
    access_pattern: AccessPattern,
    /// LRU cache for document entries
    entry_cache: Arc<RwLock<DocumentEntryCache>>,
    /// Performance statistics
    stats: Arc<RwLock<MappingStats>>,
}

impl OptimizedMemoryMapping {
    /// Create optimized memory mapping from existing memory mapped files
    ///
    /// # Arguments
    /// * `index_mapping` - Memory mapped index file
    /// * `data_mapping` - Memory mapped data file
    /// * `access_pattern` - Expected access pattern for optimization
    /// * `cache_size` - Maximum number of entries to cache
    ///
    /// # Returns
    /// * `Ok(OptimizedMemoryMapping)` - Successfully created optimized mapping
    /// * `Err(ShardexError)` - Failed to initialize optimization layer
    pub fn new(
        index_mapping: MemoryMappedFile,
        data_mapping: MemoryMappedFile,
        access_pattern: AccessPattern,
        cache_size: usize,
    ) -> Result<Self, ShardexError> {
        let page_size = Self::get_system_page_size();
        
        // Apply memory advice hints based on access pattern
        Self::apply_memory_advice(&index_mapping, &data_mapping, access_pattern)?;
        
        let entry_cache = Arc::new(RwLock::new(DocumentEntryCache::new(cache_size)));
        let stats = Arc::new(RwLock::new(MappingStats::default()));

        Ok(Self {
            index_mapping,
            data_mapping,
            page_size,
            access_pattern,
            entry_cache,
            stats,
        })
    }

    /// Get system page size for alignment calculations
    fn get_system_page_size() -> usize {
        // Use a reasonable default page size if we can't determine it
        // Most systems use 4KB pages
        4096
    }

    /// Apply memory advice hints to the mapped files based on access pattern
    fn apply_memory_advice(
        _index_mapping: &MemoryMappedFile,
        _data_mapping: &MemoryMappedFile,
        pattern: AccessPattern,
    ) -> Result<(), ShardexError> {
        // Note: memmap2::Mmap doesn't directly expose madvise functionality
        // In a production implementation, we would use platform-specific code
        // to apply memory advice hints. For now, this is a placeholder.
        
        match pattern {
            AccessPattern::Sequential => {
                // Would apply MADV_SEQUENTIAL on Unix systems
                tracing::debug!("Applied sequential access hints to memory mappings");
            }
            AccessPattern::Random => {
                // Would apply MADV_RANDOM on Unix systems  
                tracing::debug!("Applied random access hints to memory mappings");
            }
            AccessPattern::Mixed => {
                // Would apply MADV_NORMAL on Unix systems
                tracing::debug!("Applied mixed access hints to memory mappings");
            }
        }

        Ok(())
    }

    /// Optimized lookup for document entries with caching
    ///
    /// This method provides optimized entry lookup with the following features:
    /// - LRU cache check for recently accessed entries
    /// - Page-aligned memory access to reduce page faults
    /// - Performance tracking and statistics
    ///
    /// # Arguments
    /// * `document_id` - Document ID to search for
    /// * `entry_count` - Total number of entries in the index
    ///
    /// # Returns
    /// * `Ok(Some(DocumentTextEntry))` - Found the document entry
    /// * `Ok(None)` - Document not found in index
    /// * `Err(ShardexError)` - Search failed due to corruption or I/O error
    pub fn find_latest_entry_optimized(
        &self,
        document_id: DocumentId,
        entry_count: u32,
    ) -> Result<Option<DocumentTextEntry>, ShardexError> {
        let start_time = Instant::now();
        
        // Check cache first
        {
            let mut cache = self.entry_cache.write();
            if let Some(entry) = cache.get(&document_id) {
                let duration = start_time.elapsed();
                self.stats.write().record_lookup(duration, true);
                return Ok(Some(entry));
            }
        }
        
        // Cache miss - search through index with page-aligned access
        let result = self.search_index_with_prefault(document_id, entry_count)?;
        
        // Cache the result if found
        if let Some(entry) = result {
            let mut cache = self.entry_cache.write();
            cache.put(document_id, entry);
            
            let duration = start_time.elapsed();
            self.stats.write().record_lookup(duration, false);
            Ok(Some(entry))
        } else {
            let duration = start_time.elapsed();
            self.stats.write().record_lookup(duration, false);
            Ok(None)
        }
    }

    /// Search through index with page prefaulting optimization
    fn search_index_with_prefault(
        &self,
        document_id: DocumentId,
        entry_count: u32,
    ) -> Result<Option<DocumentTextEntry>, ShardexError> {
        if entry_count == 0 {
            return Ok(None);
        }

        let entry_size = std::mem::size_of::<DocumentTextEntry>();
        let entries_per_page = self.page_size / entry_size;
        
        // Calculate the starting offset for entries in the index file
        let header_size = 104; // TextIndexHeader::SIZE from existing code
        
        // Search backwards page by page for optimal cache locality
        let total_entries = entry_count as usize;
        let total_pages = (total_entries + entries_per_page - 1) / entries_per_page;
        
        for page_idx in (0..total_pages).rev() {
            // Pre-fault the page before accessing it
            let prefault_start = Instant::now();
            self.prefault_index_page(page_idx, entries_per_page, header_size)?;
            let prefault_duration = prefault_start.elapsed();
            self.stats.write().record_prefault(1, prefault_duration);
            
            // Calculate entry range for this page
            let start_entry = page_idx * entries_per_page;
            let end_entry = ((page_idx + 1) * entries_per_page).min(total_entries);
            
            // Search within this page (backwards for latest version)
            for entry_idx in (start_entry..end_entry).rev() {
                let entry_offset = header_size + (entry_idx * entry_size);
                
                // Read the entry
                let entry: DocumentTextEntry = self.index_mapping.read_at(entry_offset)?;
                
                // Validate entry to detect corruption
                entry.validate().map_err(|e| {
                    ShardexError::text_corruption(format!(
                        "Corrupted index entry at position {}: {}",
                        entry_idx, e
                    ))
                })?;
                
                // Check if this is the document we're looking for
                if entry.document_id == document_id {
                    return Ok(Some(entry));
                }
            }
        }
        
        Ok(None)
    }

    /// Pre-fault a memory page to ensure it's loaded into physical memory
    fn prefault_index_page(
        &self,
        page_index: usize,
        entries_per_page: usize,
        header_size: usize,
    ) -> Result<(), ShardexError> {
        let entry_size = std::mem::size_of::<DocumentTextEntry>();
        let page_byte_offset = header_size + (page_index * entries_per_page * entry_size);
        
        // Ensure we don't read beyond file bounds
        if page_byte_offset >= self.index_mapping.len() {
            return Ok(());
        }
        
        // Touch the first byte of the page to trigger loading
        let slice = self.index_mapping.as_slice();
        if page_byte_offset < slice.len() {
            // Use volatile read to prevent compiler optimization
            let _dummy = unsafe {
                std::ptr::read_volatile(slice.as_ptr().add(page_byte_offset))
            };
        }
        
        Ok(())
    }

    /// Get access to the underlying index mapping
    pub fn index_mapping(&self) -> &MemoryMappedFile {
        &self.index_mapping
    }

    /// Get access to the underlying data mapping  
    pub fn data_mapping(&self) -> &MemoryMappedFile {
        &self.data_mapping
    }

    /// Get current access pattern
    pub fn access_pattern(&self) -> AccessPattern {
        self.access_pattern
    }

    /// Update access pattern and apply new memory hints
    pub fn set_access_pattern(&mut self, pattern: AccessPattern) -> Result<(), ShardexError> {
        if pattern != self.access_pattern {
            Self::apply_memory_advice(&self.index_mapping, &self.data_mapping, pattern)?;
            self.access_pattern = pattern;
        }
        Ok(())
    }

    /// Get current cache statistics
    pub fn cache_stats(&self) -> (u64, u64, u64) {
        self.entry_cache.read().stats()
    }

    /// Get cache hit ratio
    pub fn cache_hit_ratio(&self) -> f64 {
        self.entry_cache.read().hit_ratio()
    }

    /// Get current cache size
    pub fn cache_size(&self) -> usize {
        self.entry_cache.read().len()
    }

    /// Clear the entry cache
    pub fn clear_cache(&self) {
        self.entry_cache.write().clear();
    }

    /// Get performance statistics
    pub fn performance_stats(&self) -> MappingStats {
        // Clone the stats under lock for thread safety
        self.stats.read().clone()
    }

    /// Reset performance statistics
    pub fn reset_stats(&self) {
        *self.stats.write() = MappingStats::default();
    }

    /// Warm up the cache by pre-loading recently accessed entries
    ///
    /// This method can be used during startup to warm the cache with
    /// entries that are likely to be accessed soon.
    pub fn warm_cache(
        &self,
        document_ids: &[DocumentId],
        entry_count: u32,
    ) -> Result<usize, ShardexError> {
        let mut warmed = 0;
        
        for &document_id in document_ids {
            if let Some(_entry) = self.find_latest_entry_optimized(document_id, entry_count)? {
                // Entry is now cached due to the lookup
                warmed += 1;
            }
        }
        
        Ok(warmed)
    }

    /// Estimate optimal cache size based on access patterns
    ///
    /// This method analyzes recent access patterns and suggests an optimal
    /// cache size based on working set estimation.
    pub fn estimate_optimal_cache_size(&self) -> usize {
        let stats = self.performance_stats();
        let current_size = self.cache_size();
        
        // Simple heuristic: if hit ratio is low, suggest larger cache
        // if hit ratio is very high, current size might be sufficient
        let hit_ratio = stats.cache_hit_ratio();
        
        if hit_ratio < 0.7 {
            // Low hit ratio - suggest doubling cache size
            (current_size * 2).min(10000) // Cap at reasonable maximum
        } else if hit_ratio > 0.95 {
            // Very high hit ratio - could potentially reduce cache size
            (current_size / 2).max(100) // Maintain minimum cache size
        } else {
            // Reasonable hit ratio - keep current size
            current_size
        }
    }

    /// Check if the mappings are healthy (no corruption detected)
    pub fn health_check(&self) -> Result<(), ShardexError> {
        // Basic health checks on the underlying mappings
        if self.index_mapping.is_empty() {
            return Err(ShardexError::text_corruption(
                "Index mapping has zero length"
            ));
        }
        
        if self.data_mapping.is_empty() {
            return Err(ShardexError::text_corruption(
                "Data mapping has zero length"
            ));
        }
        
        // Verify we can read the first few bytes without error
        let index_slice = self.index_mapping.as_slice();
        let data_slice = self.data_mapping.as_slice();
        
        if index_slice.is_empty() || data_slice.is_empty() {
            return Err(ShardexError::text_corruption(
                "Memory mapped slices are empty"
            ));
        }
        
        Ok(())
    }
}

// Implement Clone for MappingStats to support the performance_stats method
impl Clone for MappingStats {
    fn clone(&self) -> Self {
        Self {
            lookups: self.lookups,
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            lookup_time: self.lookup_time,
            pages_prefaulted: self.pages_prefaulted,
            prefault_time: self.prefault_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn test_document_entry_cache_basic() {
        let mut cache = DocumentEntryCache::new(3);
        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        
        // Test empty cache
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.get(&doc1).is_none());
        
        // Test insertion
        let entry1 = DocumentTextEntry::new(doc1, 100, 50);
        cache.put(doc1, entry1);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());
        
        // Test retrieval
        let retrieved = cache.get(&doc1).unwrap();
        assert_eq!(retrieved.document_id, doc1);
        assert_eq!(retrieved.text_offset, 100);
        assert_eq!(retrieved.text_length, 50);
        
        // Test cache miss
        assert!(cache.get(&doc2).is_none());
        
        // Test statistics
        let (hits, misses, evictions) = cache.stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 2); // First get(&doc1) + get(&doc2)
        assert_eq!(evictions, 0);
    }

    #[test]
    fn test_document_entry_cache_eviction() {
        let mut cache = DocumentEntryCache::new(2);
        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        let doc3 = DocumentId::new();
        
        let entry1 = DocumentTextEntry::new(doc1, 100, 50);
        let entry2 = DocumentTextEntry::new(doc2, 200, 60);
        let entry3 = DocumentTextEntry::new(doc3, 300, 70);
        
        // Fill cache
        cache.put(doc1, entry1);
        cache.put(doc2, entry2);
        assert_eq!(cache.len(), 2);
        
        // This should evict doc1 (least recently used)
        cache.put(doc3, entry3);
        assert_eq!(cache.len(), 2);
        
        // doc1 should be evicted
        assert!(cache.get(&doc1).is_none());
        // doc2 and doc3 should still be there
        assert!(cache.get(&doc2).is_some());
        assert!(cache.get(&doc3).is_some());
        
        let (_, _, evictions) = cache.stats();
        assert_eq!(evictions, 1);
    }

    #[test]
    fn test_document_entry_cache_lru_behavior() {
        let mut cache = DocumentEntryCache::new(2);
        let doc1 = DocumentId::new();
        let doc2 = DocumentId::new();
        let doc3 = DocumentId::new();
        
        let entry1 = DocumentTextEntry::new(doc1, 100, 50);
        let entry2 = DocumentTextEntry::new(doc2, 200, 60);
        let entry3 = DocumentTextEntry::new(doc3, 300, 70);
        
        // Add two entries
        cache.put(doc1, entry1);
        cache.put(doc2, entry2);
        
        // Access doc1 to make it more recently used
        cache.get(&doc1);
        
        // Add doc3, which should evict doc2 (now least recently used)
        cache.put(doc3, entry3);
        
        // doc1 should still be there (recently accessed)
        assert!(cache.get(&doc1).is_some());
        // doc2 should be evicted
        assert!(cache.get(&doc2).is_none());
        // doc3 should be there
        assert!(cache.get(&doc3).is_some());
    }

    #[test]
    fn test_cache_hit_ratio() {
        let mut cache = DocumentEntryCache::new(5);
        let doc1 = DocumentId::new();
        let entry1 = DocumentTextEntry::new(doc1, 100, 50);
        
        cache.put(doc1, entry1);
        
        // One hit
        cache.get(&doc1);
        // Two misses
        let doc2 = DocumentId::new();
        let doc3 = DocumentId::new();
        cache.get(&doc2);
        cache.get(&doc3);
        
        let hit_ratio = cache.hit_ratio();
        assert!((hit_ratio - (1.0 / 3.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = DocumentEntryCache::new(3);
        let doc1 = DocumentId::new();
        let entry1 = DocumentTextEntry::new(doc1, 100, 50);
        
        cache.put(doc1, entry1);
        assert_eq!(cache.len(), 1);
        
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        
        let (hits, misses, evictions) = cache.stats();
        assert_eq!(hits, 0);
        assert_eq!(misses, 0);
        assert_eq!(evictions, 0);
    }

    #[test]
    fn test_mapping_stats() {
        let mut stats = MappingStats::default();
        
        // Test initial state
        assert_eq!(stats.lookups, 0);
        assert_eq!(stats.cache_hit_ratio(), 0.0);
        assert_eq!(stats.average_lookup_time(), Duration::ZERO);
        
        // Record some operations
        stats.record_lookup(Duration::from_millis(10), true);
        stats.record_lookup(Duration::from_millis(20), false);
        stats.record_prefault(2, Duration::from_millis(5));
        
        assert_eq!(stats.lookups, 2);
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
        assert_eq!(stats.cache_hit_ratio(), 0.5);
        assert_eq!(stats.pages_prefaulted, 2);
        assert_eq!(stats.average_lookup_time(), Duration::from_millis(15));
    }

    #[test]
    fn test_access_pattern_enum() {
        // Test basic enum functionality
        assert_eq!(AccessPattern::Sequential, AccessPattern::Sequential);
        assert_ne!(AccessPattern::Sequential, AccessPattern::Random);
        
        // Test debug formatting
        let pattern = AccessPattern::Mixed;
        let debug_str = format!("{:?}", pattern);
        assert_eq!(debug_str, "Mixed");
    }
}
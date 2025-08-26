//! Memory pool for text operations to reduce allocation overhead
//!
//! This module provides memory pools for reusable string buffers and byte vectors
//! to reduce allocation overhead during frequent text storage operations.

use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Statistics for memory pool operations
#[derive(Debug, Default)]
pub struct PoolStatistics {
    /// Number of successful gets from pool (reused buffers)
    pub pool_hits: AtomicU64,
    /// Number of gets that required new allocation (pool empty or inadequate)
    pub pool_misses: AtomicU64,
    /// Number of buffers returned to pool
    pub returns: AtomicU64,
    /// Number of buffers discarded (too large for pool)
    pub discards: AtomicU64,
    /// Number of buffers preallocated during prewarming
    pub prewarmed: AtomicU64,
    /// Total bytes allocated across all operations
    pub bytes_allocated: AtomicU64,
    /// Total bytes reused from pool
    pub bytes_reused: AtomicU64,
    /// Peak number of buffers in pool simultaneously
    pub peak_pool_size: AtomicU64,
}

impl PoolStatistics {
    /// Record a pool hit (buffer reused)
    pub fn record_pool_hit(&self, bytes_reused: usize) {
        self.pool_hits.fetch_add(1, Ordering::Relaxed);
        self.bytes_reused.fetch_add(bytes_reused as u64, Ordering::Relaxed);
    }

    /// Record a pool miss (new allocation required)
    pub fn record_pool_miss(&self, bytes_allocated: usize) {
        self.pool_misses.fetch_add(1, Ordering::Relaxed);
        self.bytes_allocated.fetch_add(bytes_allocated as u64, Ordering::Relaxed);
    }

    /// Record buffer return to pool
    pub fn record_return(&self) {
        self.returns.fetch_add(1, Ordering::Relaxed);
    }

    /// Record buffer discard (too large for pool)
    pub fn record_discard(&self) {
        self.discards.fetch_add(1, Ordering::Relaxed);
    }

    /// Record prewarming operation
    pub fn record_prewarm(&self, count: usize) {
        self.prewarmed.fetch_add(count as u64, Ordering::Relaxed);
    }

    /// Update peak pool size
    pub fn update_peak_pool_size(&self, current_size: usize) {
        let current = current_size as u64;
        let mut peak = self.peak_pool_size.load(Ordering::Relaxed);
        
        while current > peak {
            match self.peak_pool_size.compare_exchange_weak(
                peak,
                current,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(new_peak) => peak = new_peak,
            }
        }
    }

    /// Get pool hit ratio
    pub fn hit_ratio(&self) -> f64 {
        let hits = self.pool_hits.load(Ordering::Relaxed);
        let misses = self.pool_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Get total requests served
    pub fn total_requests(&self) -> u64 {
        self.pool_hits.load(Ordering::Relaxed) + self.pool_misses.load(Ordering::Relaxed)
    }

    /// Get efficiency ratio (bytes reused / total bytes allocated)
    pub fn efficiency_ratio(&self) -> f64 {
        let reused = self.bytes_reused.load(Ordering::Relaxed);
        let allocated = self.bytes_allocated.load(Ordering::Relaxed);
        
        if allocated == 0 {
            0.0
        } else {
            reused as f64 / allocated as f64
        }
    }
}

/// Configuration for memory pool behavior
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum number of buffers to keep in pool
    pub max_pool_size: usize,
    /// Maximum size of buffer to keep in pool (larger buffers are discarded)
    pub max_buffer_size: usize,
    /// Minimum capacity for new buffer allocations
    pub min_capacity: usize,
    /// Growth factor for resizing inadequate buffers
    pub growth_factor: f64,
    /// Maximum age for buffers in pool before cleanup
    pub max_buffer_age: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_pool_size: 1000,
            max_buffer_size: 1024 * 1024, // 1MB
            min_capacity: 256,
            growth_factor: 1.5,
            max_buffer_age: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Entry in the string pool with metadata
#[derive(Debug)]
struct PooledStringEntry {
    /// The reusable string buffer
    buffer: String,
    /// Time when this buffer was last used
    last_used: Instant,
}

impl PooledStringEntry {
    /// Create new pool entry
    fn new(buffer: String) -> Self {
        Self {
            buffer,
            last_used: Instant::now(),
        }
    }

    /// Update last used time
    fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// Check if buffer is older than maximum age
    fn is_expired(&self, max_age: Duration) -> bool {
        self.last_used.elapsed() > max_age
    }
}

/// Entry in the byte vector pool with metadata
#[derive(Debug)]
struct PooledByteEntry {
    /// The reusable byte vector buffer
    buffer: Vec<u8>,
    /// Time when this buffer was last used
    last_used: Instant,
}

impl PooledByteEntry {
    /// Create new pool entry
    fn new(buffer: Vec<u8>) -> Self {
        Self {
            buffer,
            last_used: Instant::now(),
        }
    }

    /// Update last used time
    fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// Check if buffer is older than maximum age
    fn is_expired(&self, max_age: Duration) -> bool {
        self.last_used.elapsed() > max_age
    }
}

/// Memory pool for text operations to reduce allocation overhead
///
/// This component manages pools of reusable String and Vec<u8> buffers
/// to reduce allocation overhead during frequent text operations:
/// - String pool for text processing and formatting
/// - Byte vector pool for binary data and UTF-8 processing
/// - Automatic size management and cleanup
/// - Performance statistics and monitoring
pub struct TextMemoryPool {
    /// Pool of reusable string buffers
    string_pool: Arc<Mutex<VecDeque<PooledStringEntry>>>,
    /// Pool of reusable byte vector buffers
    byte_pool: Arc<Mutex<VecDeque<PooledByteEntry>>>,
    /// Pool configuration
    config: PoolConfig,
    /// Pool statistics
    stats: Arc<PoolStatistics>,
}

impl TextMemoryPool {
    /// Create new memory pool with default configuration
    pub fn new() -> Self {
        Self::with_config(PoolConfig::default())
    }

    /// Create new memory pool with custom configuration
    pub fn with_config(config: PoolConfig) -> Self {
        Self {
            string_pool: Arc::new(Mutex::new(VecDeque::new())),
            byte_pool: Arc::new(Mutex::new(VecDeque::new())),
            config,
            stats: Arc::new(PoolStatistics::default()),
        }
    }

    /// Get string buffer from pool or create new one
    ///
    /// This method provides a reusable string buffer with at least the
    /// requested minimum capacity. If a suitable buffer is available in
    /// the pool, it will be reused; otherwise, a new buffer is allocated.
    ///
    /// # Arguments
    /// * `min_capacity` - Minimum required capacity for the buffer
    ///
    /// # Returns
    /// * `PooledString` - RAII wrapper that automatically returns buffer to pool
    pub fn get_string_buffer(&self, min_capacity: usize) -> PooledString {
        let effective_min = min_capacity.max(self.config.min_capacity);
        
        // Try to get suitable buffer from pool
        let mut pool = self.string_pool.lock();
        
        // Look for a buffer with sufficient capacity
        for i in 0..pool.len() {
            if pool[i].buffer.capacity() >= effective_min {
                let mut entry = pool.remove(i).unwrap();
                entry.touch();
                
                // Clear the buffer and prepare for use
                entry.buffer.clear();
                
                self.stats.record_pool_hit(entry.buffer.capacity());
                
                return PooledString::new(
                    entry.buffer,
                    Arc::clone(&self.string_pool),
                    Arc::clone(&self.stats),
                    &self.config,
                );
            }
        }
        
        // No suitable buffer found - create new one
        let capacity = if effective_min > 0 {
            // Grow by growth factor to reduce future allocations
            (effective_min as f64 * self.config.growth_factor) as usize
        } else {
            self.config.min_capacity
        };
        
        let buffer = String::with_capacity(capacity);
        self.stats.record_pool_miss(capacity);
        
        PooledString::new(
            buffer,
            Arc::clone(&self.string_pool),
            Arc::clone(&self.stats),
            &self.config,
        )
    }

    /// Get byte vector buffer from pool or create new one
    ///
    /// This method provides a reusable byte vector buffer with at least the
    /// requested minimum capacity. If a suitable buffer is available in
    /// the pool, it will be reused; otherwise, a new buffer is allocated.
    ///
    /// # Arguments
    /// * `min_capacity` - Minimum required capacity for the buffer
    ///
    /// # Returns
    /// * `PooledBytes` - RAII wrapper that automatically returns buffer to pool
    pub fn get_byte_buffer(&self, min_capacity: usize) -> PooledBytes {
        let effective_min = min_capacity.max(self.config.min_capacity);
        
        // Try to get suitable buffer from pool
        let mut pool = self.byte_pool.lock();
        
        // Look for a buffer with sufficient capacity
        for i in 0..pool.len() {
            if pool[i].buffer.capacity() >= effective_min {
                let mut entry = pool.remove(i).unwrap();
                entry.touch();
                
                // Clear the buffer and prepare for use
                entry.buffer.clear();
                
                self.stats.record_pool_hit(entry.buffer.capacity());
                
                return PooledBytes::new(
                    entry.buffer,
                    Arc::clone(&self.byte_pool),
                    Arc::clone(&self.stats),
                    &self.config,
                );
            }
        }
        
        // No suitable buffer found - create new one
        let capacity = if effective_min > 0 {
            // Grow by growth factor to reduce future allocations
            (effective_min as f64 * self.config.growth_factor) as usize
        } else {
            self.config.min_capacity
        };
        
        let buffer = Vec::with_capacity(capacity);
        self.stats.record_pool_miss(capacity);
        
        PooledBytes::new(
            buffer,
            Arc::clone(&self.byte_pool),
            Arc::clone(&self.stats),
            &self.config,
        )
    }

    /// Pre-warm the string pool with buffers of specified capacity
    ///
    /// This method pre-allocates buffers to warm up the pool, which can
    /// improve performance for applications with predictable allocation patterns.
    ///
    /// # Arguments
    /// * `count` - Number of buffers to pre-allocate
    /// * `capacity` - Capacity for each pre-allocated buffer
    pub fn prewarm_string_pool(&self, count: usize, capacity: usize) {
        let mut pool = self.string_pool.lock();
        
        for _ in 0..count {
            if pool.len() >= self.config.max_pool_size {
                break;
            }
            
            let buffer = String::with_capacity(capacity);
            pool.push_back(PooledStringEntry::new(buffer));
        }
        
        self.stats.update_peak_pool_size(pool.len());
        self.stats.record_prewarm(count);
    }

    /// Pre-warm the byte pool with buffers of specified capacity
    ///
    /// # Arguments
    /// * `count` - Number of buffers to pre-allocate
    /// * `capacity` - Capacity for each pre-allocated buffer
    pub fn prewarm_byte_pool(&self, count: usize, capacity: usize) {
        let mut pool = self.byte_pool.lock();
        
        for _ in 0..count {
            if pool.len() >= self.config.max_pool_size {
                break;
            }
            
            let buffer = Vec::with_capacity(capacity);
            pool.push_back(PooledByteEntry::new(buffer));
        }
        
        self.stats.update_peak_pool_size(pool.len());
        self.stats.record_prewarm(count);
    }

    /// Clean up expired buffers from pools
    ///
    /// This method removes buffers that have been idle longer than the
    /// configured maximum age to free up memory.
    pub fn cleanup_expired(&self) {
        let max_age = self.config.max_buffer_age;
        
        // Clean string pool
        {
            let mut pool = self.string_pool.lock();
            pool.retain(|entry| !entry.is_expired(max_age));
        }
        
        // Clean byte pool
        {
            let mut pool = self.byte_pool.lock();
            pool.retain(|entry| !entry.is_expired(max_age));
        }
    }

    /// Get current pool statistics
    pub fn get_statistics(&self) -> Arc<PoolStatistics> {
        Arc::clone(&self.stats)
    }

    /// Get pool configuration
    pub fn get_config(&self) -> &PoolConfig {
        &self.config
    }

    /// Get current pool sizes (string_pool_size, byte_pool_size)
    pub fn get_pool_sizes(&self) -> (usize, usize) {
        let string_size = self.string_pool.lock().len();
        let byte_size = self.byte_pool.lock().len();
        (string_size, byte_size)
    }

    /// Clear all pools and reset statistics
    pub fn clear(&self) {
        self.string_pool.lock().clear();
        self.byte_pool.lock().clear();
        // Note: We don't reset statistics as they provide historical insight
    }

    /// Estimate memory usage of pools in bytes
    pub fn estimate_memory_usage(&self) -> usize {
        let string_pool = self.string_pool.lock();
        let byte_pool = self.byte_pool.lock();
        
        let string_memory: usize = string_pool
            .iter()
            .map(|entry| entry.buffer.capacity())
            .sum();
            
        let byte_memory: usize = byte_pool
            .iter()
            .map(|entry| entry.buffer.capacity())
            .sum();
            
        string_memory + byte_memory
    }
}

impl Default for TextMemoryPool {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII wrapper for pooled string buffers
///
/// This wrapper automatically returns the buffer to the pool when dropped,
/// ensuring proper resource management without manual intervention.
pub struct PooledString {
    /// The string buffer (None after being taken)
    buffer: Option<String>,
    /// Reference to the pool for return on drop
    pool: Arc<Mutex<VecDeque<PooledStringEntry>>>,
    /// Statistics reference
    stats: Arc<PoolStatistics>,
    /// Pool configuration
    config: PoolConfig,
}

impl PooledString {
    /// Create new pooled string wrapper
    fn new(
        buffer: String,
        pool: Arc<Mutex<VecDeque<PooledStringEntry>>>,
        stats: Arc<PoolStatistics>,
        config: &PoolConfig,
    ) -> Self {
        Self {
            buffer: Some(buffer),
            pool,
            stats,
            config: config.clone(),
        }
    }

    /// Get mutable reference to the string buffer
    pub fn buffer_mut(&mut self) -> &mut String {
        self.buffer.as_mut().expect("Buffer was already taken")
    }

    /// Get immutable reference to the string buffer
    pub fn buffer(&self) -> &String {
        self.buffer.as_ref().expect("Buffer was already taken")
    }

    /// Take ownership of the buffer, consuming the wrapper
    pub fn into_string(mut self) -> String {
        self.buffer.take().expect("Buffer was already taken")
    }

    /// Get current capacity of the buffer
    pub fn capacity(&self) -> usize {
        self.buffer.as_ref().map(|b| b.capacity()).unwrap_or(0)
    }

    /// Get current length of the buffer
    pub fn len(&self) -> usize {
        self.buffer.as_ref().map(|b| b.len()).unwrap_or(0)
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.as_ref().map(|b| b.is_empty()).unwrap_or(true)
    }

    /// Reserve additional capacity
    pub fn reserve(&mut self, additional: usize) {
        if let Some(buffer) = &mut self.buffer {
            buffer.reserve(additional);
        }
    }

    /// Shrink capacity to fit content
    pub fn shrink_to_fit(&mut self) {
        if let Some(buffer) = &mut self.buffer {
            buffer.shrink_to_fit();
        }
    }
}

impl std::ops::Deref for PooledString {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        self.buffer()
    }
}

impl std::ops::DerefMut for PooledString {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer_mut()
    }
}

impl Drop for PooledString {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            // Only return to pool if buffer is within size limits
            if buffer.capacity() <= self.config.max_buffer_size {
                let mut pool = self.pool.lock();
                
                // Only add if pool has space
                if pool.len() < self.config.max_pool_size {
                    pool.push_back(PooledStringEntry::new(buffer));
                    self.stats.record_return();
                    self.stats.update_peak_pool_size(pool.len());
                } else {
                    self.stats.record_discard();
                }
            } else {
                self.stats.record_discard();
            }
        }
    }
}

/// RAII wrapper for pooled byte vector buffers
///
/// This wrapper automatically returns the buffer to the pool when dropped,
/// ensuring proper resource management without manual intervention.
pub struct PooledBytes {
    /// The byte vector buffer (None after being taken)
    buffer: Option<Vec<u8>>,
    /// Reference to the pool for return on drop
    pool: Arc<Mutex<VecDeque<PooledByteEntry>>>,
    /// Statistics reference
    stats: Arc<PoolStatistics>,
    /// Pool configuration
    config: PoolConfig,
}

impl PooledBytes {
    /// Create new pooled bytes wrapper
    fn new(
        buffer: Vec<u8>,
        pool: Arc<Mutex<VecDeque<PooledByteEntry>>>,
        stats: Arc<PoolStatistics>,
        config: &PoolConfig,
    ) -> Self {
        Self {
            buffer: Some(buffer),
            pool,
            stats,
            config: config.clone(),
        }
    }

    /// Get mutable reference to the byte vector buffer
    pub fn buffer_mut(&mut self) -> &mut Vec<u8> {
        self.buffer.as_mut().expect("Buffer was already taken")
    }

    /// Get immutable reference to the byte vector buffer
    pub fn buffer(&self) -> &Vec<u8> {
        self.buffer.as_ref().expect("Buffer was already taken")
    }

    /// Take ownership of the buffer, consuming the wrapper
    pub fn into_vec(mut self) -> Vec<u8> {
        self.buffer.take().expect("Buffer was already taken")
    }

    /// Get current capacity of the buffer
    pub fn capacity(&self) -> usize {
        self.buffer.as_ref().map(|b| b.capacity()).unwrap_or(0)
    }

    /// Get current length of the buffer
    pub fn len(&self) -> usize {
        self.buffer.as_ref().map(|b| b.len()).unwrap_or(0)
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.as_ref().map(|b| b.is_empty()).unwrap_or(true)
    }

    /// Reserve additional capacity
    pub fn reserve(&mut self, additional: usize) {
        if let Some(buffer) = &mut self.buffer {
            buffer.reserve(additional);
        }
    }

    /// Shrink capacity to fit content
    pub fn shrink_to_fit(&mut self) {
        if let Some(buffer) = &mut self.buffer {
            buffer.shrink_to_fit();
        }
    }
}

impl std::ops::Deref for PooledBytes {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        self.buffer()
    }
}

impl std::ops::DerefMut for PooledBytes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.buffer_mut()
    }
}

impl Drop for PooledBytes {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            // Only return to pool if buffer is within size limits
            if buffer.capacity() <= self.config.max_buffer_size {
                let mut pool = self.pool.lock();
                
                // Only add if pool has space
                if pool.len() < self.config.max_pool_size {
                    pool.push_back(PooledByteEntry::new(buffer));
                    self.stats.record_return();
                    self.stats.update_peak_pool_size(pool.len());
                } else {
                    self.stats.record_discard();
                }
            } else {
                self.stats.record_discard();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_string_pool_basic_operations() {
        let pool = TextMemoryPool::new();
        
        // Get a string buffer
        let mut buffer = pool.get_string_buffer(100);
        assert!(buffer.capacity() >= 100);
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        
        // Use the buffer
        buffer.push_str("Hello, world!");
        assert_eq!(buffer.len(), 13);
        assert!(!buffer.is_empty());
        assert_eq!(buffer.as_str(), "Hello, world!");
        
        // Buffer should be returned to pool on drop
        drop(buffer);
        
        // Get another buffer - should reuse the previous one
        let buffer2 = pool.get_string_buffer(50);
        assert!(buffer2.is_empty()); // Should be cleared
        
        // Check statistics
        let stats = pool.get_statistics();
        let (hits, misses) = (
            stats.pool_hits.load(Ordering::Relaxed),
            stats.pool_misses.load(Ordering::Relaxed),
        );
        
        // Should have one miss (first allocation) and one hit (reuse)
        assert_eq!(misses, 1);
        assert_eq!(hits, 1);
        assert_eq!(stats.hit_ratio(), 0.5);
    }

    #[test]
    fn test_byte_pool_basic_operations() {
        let pool = TextMemoryPool::new();
        
        // Get a byte buffer
        let mut buffer = pool.get_byte_buffer(100);
        assert!(buffer.capacity() >= 100);
        assert_eq!(buffer.len(), 0);
        assert!(buffer.is_empty());
        
        // Use the buffer
        buffer.extend_from_slice(b"Hello, bytes!");
        assert_eq!(buffer.len(), 13);
        assert!(!buffer.is_empty());
        assert_eq!(&buffer[..], b"Hello, bytes!");
        
        // Buffer should be returned to pool on drop
        drop(buffer);
        
        // Get another buffer - should reuse the previous one
        let buffer2 = pool.get_byte_buffer(50);
        assert!(buffer2.is_empty()); // Should be cleared
    }

    #[test]
    fn test_pool_size_limits() {
        let config = PoolConfig {
            max_pool_size: 2,
            max_buffer_size: 1000,
            ..Default::default()
        };
        let pool = TextMemoryPool::with_config(config);
        
        // Create buffers and let them return to pool
        let buffer1 = pool.get_string_buffer(100);
        let buffer2 = pool.get_string_buffer(200);
        let buffer3 = pool.get_string_buffer(300);
        
        drop(buffer1);
        drop(buffer2);
        drop(buffer3); // This should be discarded due to pool size limit
        
        let (string_size, _) = pool.get_pool_sizes();
        assert_eq!(string_size, 2); // Pool size limited to 2
        
        let stats = pool.get_statistics();
        assert!(stats.discards.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn test_buffer_size_limits() {
        let config = PoolConfig {
            max_buffer_size: 500,
            ..Default::default()
        };
        let pool = TextMemoryPool::with_config(config);
        
        // Create a large buffer
        let mut large_buffer = pool.get_string_buffer(1000);
        large_buffer.reserve(2000); // Make it even larger
        
        // When dropped, should be discarded due to size limit
        drop(large_buffer);
        
        let stats = pool.get_statistics();
        assert!(stats.discards.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn test_prewarming() {
        let pool = TextMemoryPool::new();
        
        // Prewarm string pool
        pool.prewarm_string_pool(5, 256);
        
        let (string_size, _) = pool.get_pool_sizes();
        assert_eq!(string_size, 5);
        
        // Getting buffers should hit the prewarmed ones
        let _buffer1 = pool.get_string_buffer(100);
        let _buffer2 = pool.get_string_buffer(200);
        
        let stats = pool.get_statistics();
        assert!(stats.pool_hits.load(Ordering::Relaxed) >= 2);
        assert_eq!(stats.prewarmed.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn test_growth_factor() {
        let config = PoolConfig {
            growth_factor: 2.0,
            min_capacity: 100,
            ..Default::default()
        };
        let pool = TextMemoryPool::with_config(config);
        
        // Request buffer with specific capacity
        let buffer = pool.get_string_buffer(150);
        
        // Should be grown by growth factor
        assert!(buffer.capacity() >= 300); // 150 * 2.0
    }

    #[test]
    fn test_statistics() {
        let pool = TextMemoryPool::new();
        
        // Perform various operations to generate statistics
        let buffer1 = pool.get_string_buffer(100); // Miss
        drop(buffer1);
        
        let buffer2 = pool.get_string_buffer(50);  // Hit
        drop(buffer2);
        
        let mut large_buffer = pool.get_string_buffer(1000);
        large_buffer.reserve(2_000_000); // Make it too large
        drop(large_buffer); // Should be discarded
        
        let stats = pool.get_statistics();
        
        assert!(stats.total_requests() > 0);
        assert!(stats.pool_hits.load(Ordering::Relaxed) > 0);
        assert!(stats.pool_misses.load(Ordering::Relaxed) > 0);
        assert!(stats.discards.load(Ordering::Relaxed) > 0);
        assert!(stats.hit_ratio() > 0.0);
        assert!(stats.hit_ratio() < 1.0);
    }

    #[test]
    fn test_memory_usage_estimation() {
        let pool = TextMemoryPool::new();
        
        // Prewarm with known capacities
        pool.prewarm_string_pool(3, 1000);
        pool.prewarm_byte_pool(2, 500);
        
        let memory_usage = pool.estimate_memory_usage();
        let expected = (3 * 1000) + (2 * 500); // 3000 + 1000 = 4000
        assert_eq!(memory_usage, expected);
    }

    #[test]
    fn test_cleanup_expired() {
        let config = PoolConfig {
            max_buffer_age: Duration::from_millis(50),
            ..Default::default()
        };
        let pool = TextMemoryPool::with_config(config);
        
        // Add buffers to pool
        let buffer1 = pool.get_string_buffer(100);
        let buffer2 = pool.get_string_buffer(200);
        drop(buffer1);
        drop(buffer2);
        
        assert_eq!(pool.get_pool_sizes().0, 2);
        
        // Wait for expiration
        thread::sleep(Duration::from_millis(60));
        
        // Cleanup expired buffers
        pool.cleanup_expired();
        
        // Pool should be empty now
        assert_eq!(pool.get_pool_sizes().0, 0);
    }

    #[test]
    fn test_pooled_string_deref() {
        let pool = TextMemoryPool::new();
        let mut buffer = pool.get_string_buffer(100);
        
        // Test deref operations
        buffer.push_str("Hello");
        assert_eq!(buffer.len(), 5);
        assert_eq!(&*buffer, "Hello");
        
        // Test deref_mut operations
        buffer.push_str(", world!");
        assert_eq!(&*buffer, "Hello, world!");
    }

    #[test]
    fn test_pooled_string_into_string() {
        let pool = TextMemoryPool::new();
        let mut buffer = pool.get_string_buffer(100);
        buffer.push_str("Test string");
        
        let owned_string = buffer.into_string();
        assert_eq!(owned_string, "Test string");
    }

    #[test]
    fn test_pooled_bytes_operations() {
        let pool = TextMemoryPool::new();
        let mut buffer = pool.get_byte_buffer(100);
        
        // Test basic operations
        buffer.extend_from_slice(b"Hello");
        assert_eq!(buffer.len(), 5);
        assert_eq!(&buffer[..], b"Hello");
        
        // Test into_vec
        let owned_vec = buffer.into_vec();
        assert_eq!(owned_vec, b"Hello");
    }

    #[test]
    fn test_concurrent_access() {
        let pool = Arc::new(TextMemoryPool::new());
        let mut handles = Vec::new();
        
        // Spawn multiple threads using the pool
        for i in 0..10 {
            let pool_clone = Arc::clone(&pool);
            let handle = thread::spawn(move || {
                let mut buffer = pool_clone.get_string_buffer(100);
                buffer.push_str(&format!("Thread {}", i));
                
                // Use buffer briefly
                thread::sleep(Duration::from_millis(10));
                
                buffer.len()
            });
            handles.push(handle);
        }
        
        // Wait for all threads
        let results: Vec<usize> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        
        // All threads should have used their buffers
        assert_eq!(results.len(), 10);
        for &length in &results {
            assert!(length > 7); // "Thread X" is at least 8 characters
        }
        
        // Check that pool has some buffers returned
        let stats = pool.get_statistics();
        assert!(stats.total_requests() >= 10);
    }

    #[test]
    fn test_efficiency_ratio() {
        let pool = TextMemoryPool::new();
        
        // Create and reuse buffer to generate efficiency data
        let buffer1 = pool.get_string_buffer(1000);
        let capacity1 = buffer1.capacity();
        drop(buffer1);
        
        let buffer2 = pool.get_string_buffer(500);
        let capacity2 = buffer2.capacity();
        drop(buffer2);
        
        let stats = pool.get_statistics();
        let efficiency = stats.efficiency_ratio();
        
        // Should have some efficiency from reuse
        assert!(efficiency > 0.0);
        
        // Total bytes reused should be at least the capacity of the second buffer
        assert!(stats.bytes_reused.load(Ordering::Relaxed) >= capacity2 as u64);
        assert!(stats.bytes_allocated.load(Ordering::Relaxed) >= capacity1 as u64);
    }
}
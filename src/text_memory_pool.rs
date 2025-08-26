//! Memory pool for text operations to reduce allocation overhead
//!
//! This module provides RAII-managed memory pools for String and Vec<u8> buffers,
//! reducing allocation overhead in text processing operations.

use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Configuration for memory pool behavior
#[derive(Debug, Clone)]
pub struct MemoryPoolConfig {
    /// Maximum number of buffers to keep in each pool
    pub max_pool_size: usize,
    /// Maximum buffer capacity to keep in pool (larger buffers are discarded)
    pub max_buffer_capacity: usize,
    /// Time before unused buffers are eligible for cleanup
    pub buffer_ttl: Duration,
    /// Growth factor for buffer capacity when expanding
    pub growth_factor: f64,
}

impl Default for MemoryPoolConfig {
    fn default() -> Self {
        Self {
            max_pool_size: 1000,
            max_buffer_capacity: 1024 * 1024,     // 1MB
            buffer_ttl: Duration::from_secs(300), // 5 minutes
            growth_factor: 1.5,
        }
    }
}

/// Buffer entry with metadata for pool management
#[derive(Debug)]
struct BufferEntry<T> {
    buffer: T,
    last_used: SystemTime,
    usage_count: u64,
}

impl<T> BufferEntry<T> {
    fn new(buffer: T) -> Self {
        Self {
            buffer,
            last_used: SystemTime::now(),
            usage_count: 1,
        }
    }

    fn touch(&mut self) {
        self.last_used = SystemTime::now();
        self.usage_count += 1;
    }

    fn is_expired(&self, ttl: Duration) -> bool {
        self.last_used.elapsed().unwrap_or(Duration::ZERO) > ttl
    }
}

/// Statistics for memory pool performance
#[derive(Debug, Default, Clone)]
pub struct MemoryPoolStats {
    /// Total buffer requests
    pub total_requests: u64,
    /// Requests served from pool (cache hits)
    pub pool_hits: u64,
    /// Requests that required new allocation (cache misses)
    pub pool_misses: u64,
    /// Total buffers currently in string pool
    pub string_pool_size: usize,
    /// Total buffers currently in byte pool
    pub byte_pool_size: usize,
    /// Total capacity in string pool (bytes)
    pub string_pool_capacity: usize,
    /// Total capacity in byte pool (bytes)
    pub byte_pool_capacity: usize,
    /// Number of cleanup operations performed
    pub cleanup_operations: u64,
    /// Number of buffers discarded due to size limits
    pub oversized_discards: u64,
    /// Number of buffers discarded due to TTL
    pub expired_discards: u64,
}

impl MemoryPoolStats {
    /// Calculate hit ratio
    pub fn hit_ratio(&self) -> f64 {
        if self.total_requests == 0 {
            0.0
        } else {
            self.pool_hits as f64 / self.total_requests as f64
        }
    }

    /// Calculate average buffer size in string pool
    pub fn avg_string_buffer_size(&self) -> f64 {
        if self.string_pool_size == 0 {
            0.0
        } else {
            self.string_pool_capacity as f64 / self.string_pool_size as f64
        }
    }

    /// Calculate average buffer size in byte pool
    pub fn avg_byte_buffer_size(&self) -> f64 {
        if self.byte_pool_size == 0 {
            0.0
        } else {
            self.byte_pool_capacity as f64 / self.byte_pool_size as f64
        }
    }

    /// Calculate memory efficiency (hit ratio adjusted for buffer sizes)
    pub fn memory_efficiency(&self) -> f64 {
        let hit_ratio = self.hit_ratio();
        let total_capacity = self.string_pool_capacity + self.byte_pool_capacity;
        let total_buffers = self.string_pool_size + self.byte_pool_size;

        if total_buffers == 0 {
            hit_ratio
        } else {
            let avg_buffer_size = total_capacity as f64 / total_buffers as f64;
            let size_factor = (avg_buffer_size / 1024.0).min(1.0); // Normalize to KB
            hit_ratio * size_factor
        }
    }
}

/// Memory pool for text operations to reduce allocation overhead
pub struct TextMemoryPool {
    /// Pool of reusable string buffers
    string_pool: Arc<Mutex<Vec<BufferEntry<String>>>>,
    /// Pool of reusable byte vectors
    byte_pool: Arc<Mutex<Vec<BufferEntry<Vec<u8>>>>>,
    /// Pool configuration
    config: MemoryPoolConfig,
    /// Pool statistics
    stats: Arc<Mutex<MemoryPoolStats>>,
}

impl TextMemoryPool {
    /// Create new text memory pool with configuration
    pub fn new(config: MemoryPoolConfig) -> Self {
        Self {
            string_pool: Arc::new(Mutex::new(Vec::new())),
            byte_pool: Arc::new(Mutex::new(Vec::new())),
            config,
            stats: Arc::new(Mutex::new(MemoryPoolStats::default())),
        }
    }

    /// Create new text memory pool with default configuration
    pub fn new_default() -> Self {
        Self::new(MemoryPoolConfig::default())
    }

    /// Get string buffer from pool or create new one
    pub fn get_string_buffer(&self, min_capacity: usize) -> PooledString {
        self.record_request();

        let mut pool = self.string_pool.lock();

        // Look for suitable buffer in pool
        for i in (0..pool.len()).rev() {
            if pool[i].buffer.capacity() >= min_capacity {
                let mut entry = pool.swap_remove(i);
                entry.touch();
                entry.buffer.clear();

                // Expand capacity if needed with growth factor
                if entry.buffer.capacity() < min_capacity {
                    let new_capacity = (min_capacity as f64 * self.config.growth_factor) as usize;
                    entry.buffer.reserve(new_capacity - entry.buffer.capacity());
                }

                self.record_hit();
                return PooledString::new(
                    entry.buffer,
                    Arc::clone(&self.string_pool),
                    &self.config,
                    Arc::clone(&self.stats),
                );
            }
        }

        // No suitable buffer found, create new one
        let capacity = (min_capacity as f64 * self.config.growth_factor) as usize;
        let buffer = String::with_capacity(capacity);
        self.record_miss();

        PooledString::new(
            buffer,
            Arc::clone(&self.string_pool),
            &self.config,
            Arc::clone(&self.stats),
        )
    }

    /// Get byte buffer from pool or create new one
    pub fn get_byte_buffer(&self, min_capacity: usize) -> PooledBytes {
        self.record_request();

        let mut pool = self.byte_pool.lock();

        // Look for suitable buffer in pool
        for i in (0..pool.len()).rev() {
            if pool[i].buffer.capacity() >= min_capacity {
                let mut entry = pool.swap_remove(i);
                entry.touch();
                entry.buffer.clear();

                // Expand capacity if needed with growth factor
                if entry.buffer.capacity() < min_capacity {
                    let new_capacity = (min_capacity as f64 * self.config.growth_factor) as usize;
                    entry.buffer.reserve(new_capacity - entry.buffer.capacity());
                }

                self.record_hit();
                return PooledBytes::new(
                    entry.buffer,
                    Arc::clone(&self.byte_pool),
                    &self.config,
                    Arc::clone(&self.stats),
                );
            }
        }

        // No suitable buffer found, create new one
        let capacity = (min_capacity as f64 * self.config.growth_factor) as usize;
        let buffer = Vec::with_capacity(capacity);
        self.record_miss();

        PooledBytes::new(
            buffer,
            Arc::clone(&self.byte_pool),
            &self.config,
            Arc::clone(&self.stats),
        )
    }

    /// Pre-warm pool with buffers of specified capacities
    pub fn prewarm(
        &self,
        string_count: usize,
        string_capacity: usize,
        byte_count: usize,
        byte_capacity: usize,
    ) {
        // Pre-warm string pool
        {
            let mut pool = self.string_pool.lock();
            for _ in 0..string_count {
                if pool.len() >= self.config.max_pool_size {
                    break;
                }
                let buffer = String::with_capacity(string_capacity);
                pool.push(BufferEntry::new(buffer));
            }
        }

        // Pre-warm byte pool
        {
            let mut pool = self.byte_pool.lock();
            for _ in 0..byte_count {
                if pool.len() >= self.config.max_pool_size {
                    break;
                }
                let buffer = Vec::with_capacity(byte_capacity);
                pool.push(BufferEntry::new(buffer));
            }
        }

        log::debug!("Pre-warmed pool with {} string buffers ({} bytes each) and {} byte buffers ({} bytes each)",
                   string_count, string_capacity, byte_count, byte_capacity);
    }

    /// Perform cleanup of expired and oversized buffers
    pub fn cleanup(&self) {
        let mut cleanup_count = 0;

        // Clean up string pool
        {
            let mut pool = self.string_pool.lock();
            let original_len = pool.len();

            pool.retain(|entry| {
                let should_keep = !entry.is_expired(self.config.buffer_ttl)
                    && entry.buffer.capacity() <= self.config.max_buffer_capacity;

                if !should_keep {
                    cleanup_count += 1;
                    if entry.is_expired(self.config.buffer_ttl) {
                        self.record_expired_discard();
                    } else {
                        self.record_oversized_discard();
                    }
                }

                should_keep
            });

            log::trace!(
                "Cleaned up {} of {} string buffers",
                original_len - pool.len(),
                original_len
            );
        }

        // Clean up byte pool
        {
            let mut pool = self.byte_pool.lock();
            let original_len = pool.len();

            pool.retain(|entry| {
                let should_keep = !entry.is_expired(self.config.buffer_ttl)
                    && entry.buffer.capacity() <= self.config.max_buffer_capacity;

                if !should_keep {
                    cleanup_count += 1;
                    if entry.is_expired(self.config.buffer_ttl) {
                        self.record_expired_discard();
                    } else {
                        self.record_oversized_discard();
                    }
                }

                should_keep
            });

            log::trace!(
                "Cleaned up {} of {} byte buffers",
                original_len - pool.len(),
                original_len
            );
        }

        if cleanup_count > 0 {
            self.record_cleanup();
            log::debug!("Memory pool cleanup removed {} buffers", cleanup_count);
        }
    }

    /// Get current pool statistics
    pub fn get_stats(&self) -> MemoryPoolStats {
        let mut stats = self.stats.lock().clone();

        // Update current pool sizes
        {
            let string_pool = self.string_pool.lock();
            stats.string_pool_size = string_pool.len();
            stats.string_pool_capacity = string_pool.iter().map(|e| e.buffer.capacity()).sum();
        }

        {
            let byte_pool = self.byte_pool.lock();
            stats.byte_pool_size = byte_pool.len();
            stats.byte_pool_capacity = byte_pool.iter().map(|e| e.buffer.capacity()).sum();
        }

        stats
    }

    /// Reset pool statistics
    pub fn reset_stats(&self) {
        let mut stats = self.stats.lock();
        *stats = MemoryPoolStats::default();
    }

    /// Clear all pools and reset statistics
    pub fn clear(&self) {
        {
            let mut pool = self.string_pool.lock();
            pool.clear();
        }

        {
            let mut pool = self.byte_pool.lock();
            pool.clear();
        }

        self.reset_stats();
    }

    /// Get pool configuration
    pub fn config(&self) -> &MemoryPoolConfig {
        &self.config
    }

    // Statistics recording methods
    fn record_request(&self) {
        let mut stats = self.stats.lock();
        stats.total_requests += 1;
    }

    fn record_hit(&self) {
        let mut stats = self.stats.lock();
        stats.pool_hits += 1;
    }

    fn record_miss(&self) {
        let mut stats = self.stats.lock();
        stats.pool_misses += 1;
    }

    fn record_cleanup(&self) {
        let mut stats = self.stats.lock();
        stats.cleanup_operations += 1;
    }

    fn record_oversized_discard(&self) {
        let mut stats = self.stats.lock();
        stats.oversized_discards += 1;
    }

    fn record_expired_discard(&self) {
        let mut stats = self.stats.lock();
        stats.expired_discards += 1;
    }
}

/// RAII wrapper for pooled strings
pub struct PooledString {
    buffer: Option<String>,
    pool: Arc<Mutex<Vec<BufferEntry<String>>>>,
    config: MemoryPoolConfig,
    stats: Arc<Mutex<MemoryPoolStats>>,
}

impl PooledString {
    fn new(
        buffer: String,
        pool: Arc<Mutex<Vec<BufferEntry<String>>>>,
        config: &MemoryPoolConfig,
        stats: Arc<Mutex<MemoryPoolStats>>,
    ) -> Self {
        Self {
            buffer: Some(buffer),
            pool,
            config: config.clone(),
            stats,
        }
    }

    /// Get mutable access to the buffer
    pub fn buffer_mut(&mut self) -> &mut String {
        self.buffer
            .as_mut()
            .expect("Buffer should always be present when not dropped")
    }

    /// Get immutable access to the buffer
    pub fn buffer(&self) -> &String {
        self.buffer
            .as_ref()
            .expect("Buffer should always be present when not dropped")
    }

    /// Get buffer length
    pub fn len(&self) -> usize {
        self.buffer().len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer().is_empty()
    }

    /// Get buffer capacity
    pub fn capacity(&self) -> usize {
        self.buffer().capacity()
    }

    /// Take ownership of the buffer (prevents return to pool)
    pub fn into_inner(mut self) -> String {
        self.buffer
            .take()
            .expect("Buffer should always be present when not dropped")
    }
}

impl Drop for PooledString {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            let capacity = buffer.capacity();

            // Only return to pool if it's not too large
            if capacity <= self.config.max_buffer_capacity {
                let mut pool = self.pool.lock();

                // Only add if pool isn't full
                if pool.len() < self.config.max_pool_size {
                    pool.push(BufferEntry::new(buffer));
                    log::trace!("Returned string buffer to pool (capacity: {})", capacity);
                } else {
                    log::trace!("Discarded string buffer - pool full");
                }
            } else {
                // Record oversized discard
                let mut stats = self.stats.lock();
                stats.oversized_discards += 1;
                log::trace!("Discarded oversized string buffer (capacity: {})", capacity);
            }
        }
    }
}

/// RAII wrapper for pooled byte vectors
pub struct PooledBytes {
    buffer: Option<Vec<u8>>,
    pool: Arc<Mutex<Vec<BufferEntry<Vec<u8>>>>>,
    config: MemoryPoolConfig,
    stats: Arc<Mutex<MemoryPoolStats>>,
}

impl PooledBytes {
    fn new(
        buffer: Vec<u8>,
        pool: Arc<Mutex<Vec<BufferEntry<Vec<u8>>>>>,
        config: &MemoryPoolConfig,
        stats: Arc<Mutex<MemoryPoolStats>>,
    ) -> Self {
        Self {
            buffer: Some(buffer),
            pool,
            config: config.clone(),
            stats,
        }
    }

    /// Get mutable access to the buffer
    pub fn buffer_mut(&mut self) -> &mut Vec<u8> {
        self.buffer
            .as_mut()
            .expect("Buffer should always be present when not dropped")
    }

    /// Get immutable access to the buffer
    pub fn buffer(&self) -> &Vec<u8> {
        self.buffer
            .as_ref()
            .expect("Buffer should always be present when not dropped")
    }

    /// Get buffer length
    pub fn len(&self) -> usize {
        self.buffer().len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer().is_empty()
    }

    /// Get buffer capacity
    pub fn capacity(&self) -> usize {
        self.buffer().capacity()
    }

    /// Take ownership of the buffer (prevents return to pool)
    pub fn into_inner(mut self) -> Vec<u8> {
        self.buffer
            .take()
            .expect("Buffer should always be present when not dropped")
    }
}

impl Drop for PooledBytes {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            let capacity = buffer.capacity();

            // Only return to pool if it's not too large
            if capacity <= self.config.max_buffer_capacity {
                let mut pool = self.pool.lock();

                // Only add if pool isn't full
                if pool.len() < self.config.max_pool_size {
                    pool.push(BufferEntry::new(buffer));
                    log::trace!("Returned byte buffer to pool (capacity: {})", capacity);
                } else {
                    log::trace!("Discarded byte buffer - pool full");
                }
            } else {
                // Record oversized discard
                let mut stats = self.stats.lock();
                stats.oversized_discards += 1;
                log::trace!("Discarded oversized byte buffer (capacity: {})", capacity);
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
    fn test_memory_pool_basic_operations() {
        let pool = TextMemoryPool::new_default();

        // Test string buffer
        let mut string_buf = pool.get_string_buffer(100);
        string_buf.buffer_mut().push_str("Hello, world!");
        assert_eq!(string_buf.buffer(), "Hello, world!");
        assert!(string_buf.capacity() >= 100);
        drop(string_buf);

        // Test byte buffer
        let mut byte_buf = pool.get_byte_buffer(50);
        byte_buf.buffer_mut().extend_from_slice(b"Hello, bytes!");
        assert_eq!(byte_buf.buffer(), b"Hello, bytes!");
        assert!(byte_buf.capacity() >= 50);
        drop(byte_buf);

        let stats = pool.get_stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.pool_misses, 2); // First requests are always misses
    }

    #[test]
    fn test_buffer_reuse() {
        let pool = TextMemoryPool::new_default();

        // Create and drop a buffer
        {
            let mut buf = pool.get_string_buffer(100);
            buf.buffer_mut().push_str("Test content");
            assert_eq!(buf.len(), 12);
        }

        // Get another buffer - should reuse the previous one
        {
            let buf = pool.get_string_buffer(50); // Smaller capacity, should still reuse
            assert!(buf.is_empty()); // Should be cleared
            assert!(buf.capacity() >= 100); // Should maintain original capacity
        }

        let stats = pool.get_stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.pool_hits, 1);
        assert_eq!(stats.pool_misses, 1);
        assert_eq!(stats.hit_ratio(), 0.5);
    }

    #[test]
    fn test_prewarming() {
        let pool = TextMemoryPool::new_default();

        // Pre-warm the pool
        pool.prewarm(10, 256, 5, 512);

        let stats = pool.get_stats();
        assert_eq!(stats.string_pool_size, 10);
        assert_eq!(stats.byte_pool_size, 5);
        assert!(stats.string_pool_capacity >= 10 * 256);
        assert!(stats.byte_pool_capacity >= 5 * 512);
    }

    #[test]
    fn test_oversized_buffer_handling() {
        let mut config = MemoryPoolConfig::default();
        config.max_buffer_capacity = 100; // Small limit for testing
        let pool = TextMemoryPool::new(config);

        // Create a large buffer that exceeds the limit
        {
            let mut buf = pool.get_string_buffer(200); // Exceeds max_buffer_capacity
            buf.buffer_mut().push_str("Large buffer content");
        }

        // The buffer should not be returned to the pool
        let stats = pool.get_stats();
        assert_eq!(stats.string_pool_size, 0);
        assert!(stats.oversized_discards > 0);
    }

    #[test]
    fn test_buffer_expiration() {
        let mut config = MemoryPoolConfig::default();
        config.buffer_ttl = Duration::from_millis(10); // Very short TTL for testing
        let pool = TextMemoryPool::new(config);

        // Create a buffer
        {
            let buf = pool.get_string_buffer(100);
            drop(buf);
        }

        // Wait for expiration
        thread::sleep(Duration::from_millis(20));

        // Clean up expired buffers
        pool.cleanup();

        let stats = pool.get_stats();
        assert_eq!(stats.string_pool_size, 0);
    }

    #[test]
    fn test_memory_pool_stats() {
        let pool = TextMemoryPool::new_default();

        // Generate some activity
        for _ in 0..10 {
            let buf = pool.get_string_buffer(100);
            drop(buf);
        }

        // Generate some hits
        for _ in 0..5 {
            let buf = pool.get_string_buffer(50);
            drop(buf);
        }

        let stats = pool.get_stats();
        assert_eq!(stats.total_requests, 15);
        assert!(stats.pool_hits > 0);
        assert!(stats.pool_misses > 0);
        assert!(stats.hit_ratio() > 0.0);
        assert!(stats.hit_ratio() <= 1.0);
    }

    #[test]
    fn test_pool_size_limits() {
        let mut config = MemoryPoolConfig::default();
        config.max_pool_size = 2; // Very small pool for testing
        let pool = TextMemoryPool::new(config);

        // Create more buffers than the pool can hold
        for _ in 0..5 {
            let buf = pool.get_string_buffer(100);
            drop(buf);
        }

        let stats = pool.get_stats();
        assert!(stats.string_pool_size <= 2); // Should not exceed max_pool_size
    }

    #[test]
    fn test_into_inner() {
        let pool = TextMemoryPool::new_default();

        let mut buf = pool.get_string_buffer(100);
        buf.buffer_mut().push_str("Test content");

        let owned_string = buf.into_inner();
        assert_eq!(owned_string, "Test content");

        // The buffer should not be returned to the pool
        let stats = pool.get_stats();
        assert_eq!(stats.string_pool_size, 0);
    }

    #[test]
    fn test_growth_factor() {
        let mut config = MemoryPoolConfig::default();
        config.growth_factor = 2.0;
        let pool = TextMemoryPool::new(config);

        let buf = pool.get_string_buffer(100);
        // Capacity should be at least 100 * 2.0 = 200
        assert!(buf.capacity() >= 200);
    }
}

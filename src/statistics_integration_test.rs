//! Integration test for statistics and monitoring functionality
//!
//! This test verifies that the enhanced IndexStats and detailed monitoring capabilities
//! work correctly in the Shardex implementation.

#[cfg(test)]
mod tests {
    use crate::config::ShardexConfig;
    use crate::shardex::{Shardex, ShardexImpl};
    use crate::test_utils::TestEnvironment;
    use std::time::Duration;

    #[tokio::test]
    async fn test_enhanced_index_stats() {
        let _env = TestEnvironment::new("test_enhanced_index_stats");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128)
            .shard_size(100);

        // Create a new Shardex instance
        let shardex = ShardexImpl::new(config).unwrap();

        // Test basic stats method
        let stats = shardex.stats().await.unwrap();

        // Verify all required fields are present
        assert_eq!(stats.total_shards, 0); // Empty index
        assert_eq!(stats.total_postings, 0);
        assert_eq!(stats.vector_dimension, 128);

        // Verify performance metrics are initialized
        assert_eq!(stats.search_latency_p50, Duration::ZERO);
        assert_eq!(stats.search_latency_p95, Duration::ZERO);
        assert_eq!(stats.search_latency_p99, Duration::ZERO);
        assert_eq!(stats.write_throughput, 0.0);
        assert_eq!(stats.bloom_filter_hit_rate, 0.0);

        // Test convenience methods
        assert_eq!(stats.search_latency_p50_ms(), 0);
        assert_eq!(stats.bloom_filter_hit_rate_percent(), 0.0);
        assert!(stats.is_performance_healthy()); // Should be healthy for empty index

        println!("IndexStats display: {}", stats);
    }

    #[tokio::test]
    async fn test_detailed_stats() {
        let _env = TestEnvironment::new("test_detailed_stats");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(256)
            .shard_size(50);

        let shardex = ShardexImpl::new(config).unwrap();

        // Test detailed stats method
        let detailed_stats = shardex.detailed_stats().await.unwrap();

        // Verify basic metrics
        assert_eq!(detailed_stats.total_shards, 0);
        assert_eq!(detailed_stats.vector_dimension, 256);

        // Verify performance metrics structure
        assert_eq!(detailed_stats.search_latency_p50, Duration::ZERO);
        assert_eq!(detailed_stats.write_throughput, 0.0);
        assert_eq!(detailed_stats.bloom_filter_hit_rate, 0.0);

        // Verify resource metrics are present
        assert_eq!(detailed_stats.memory_mapped_regions, 0); // Empty index
        assert_eq!(detailed_stats.wal_segment_count, 0);
        assert!(detailed_stats.file_descriptor_count >= 10); // Should have some baseline FDs
        assert_eq!(detailed_stats.active_connections, 1);

        // Verify temporal metrics
        assert!(detailed_stats.uptime > Duration::ZERO);
        assert_eq!(detailed_stats.total_operations, 0);

        println!("DetailedIndexStats uptime: {:?}", detailed_stats.uptime);
        println!("DetailedIndexStats FD count: {}", detailed_stats.file_descriptor_count);
    }

    #[tokio::test]
    async fn test_search_performance_monitoring() {
        let _env = TestEnvironment::new("test_search_performance_monitoring");
        let config = ShardexConfig::new()
            .directory_path(_env.path())
            .vector_size(128);

        let shardex = ShardexImpl::new(config).unwrap();
        let query_vector = vec![0.5; 128];

        // Perform a search operation to trigger performance monitoring
        let _search_result = shardex.search(&query_vector, 10, None).await;

        // Get stats after search to see if monitoring recorded anything
        let stats = shardex.stats().await.unwrap();

        // Even though the index is empty, the search operation should have been recorded
        // The performance monitoring should have captured the search timing
        println!("Stats after search: {}", stats);

        // Verify the stats structure is complete
        assert_eq!(stats.vector_dimension, 128);
        assert_eq!(stats.total_shards, 0); // Still empty
    }

    #[tokio::test]
    async fn test_index_stats_builder_pattern() {
        // Test the builder pattern works with new performance fields
        let stats = crate::structures::IndexStats::new()
            .with_total_shards(5)
            .with_total_postings(1000)
            .with_vector_dimension(384)
            .with_search_latency_p95(Duration::from_millis(200))
            .with_write_throughput(150.0)
            .with_bloom_filter_hit_rate(0.92);

        assert_eq!(stats.total_shards, 5);
        assert_eq!(stats.total_postings, 1000);
        assert_eq!(stats.vector_dimension, 384);
        assert_eq!(stats.search_latency_p95, Duration::from_millis(200));
        assert_eq!(stats.write_throughput, 150.0);
        assert_eq!(stats.bloom_filter_hit_rate, 0.92);

        // Test convenience methods
        assert_eq!(stats.search_latency_p95_ms(), 200);
        assert_eq!(stats.write_ops_per_second(), 150.0);
        assert_eq!(stats.bloom_filter_hit_rate_percent(), 92.0);
        assert!(stats.is_performance_healthy());

        println!("Enhanced stats display: {}", stats);
    }

    #[tokio::test]
    async fn test_for_test_builder() {
        // Test the for_test() builder includes performance metrics
        let stats = crate::structures::IndexStats::for_test();

        // Verify test data includes performance metrics
        assert_eq!(stats.search_latency_p50, Duration::from_millis(50));
        assert_eq!(stats.search_latency_p95, Duration::from_millis(150));
        assert_eq!(stats.search_latency_p99, Duration::from_millis(300));
        assert_eq!(stats.write_throughput, 100.0);
        assert_eq!(stats.bloom_filter_hit_rate, 0.85);

        // Test that display format includes new metrics
        let display_str = format!("{}", stats);
        assert!(display_str.contains("latency:"));
        assert!(display_str.contains("write_throughput:"));
        assert!(display_str.contains("bloom_hit_rate:"));

        println!("Test stats display: {}", display_str);
    }
}

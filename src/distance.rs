//! Distance and similarity metrics for vector search
//!
//! This module provides various distance and similarity metrics for K-nearest neighbors search.
//! All metrics are designed to return similarity scores normalized to the range [0.0, 1.0]
//! where 1.0 represents maximum similarity and 0.0 represents minimum similarity.

use crate::error::ShardexError;

/// Supported distance metrics for vector similarity search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DistanceMetric {
    /// Cosine similarity - measures the cosine of the angle between vectors
    /// Best for high-dimensional sparse vectors and text embeddings
    #[default]
    Cosine,
    /// Euclidean distance - measures straight-line distance between points
    /// Best for low-dimensional dense vectors and geometric data
    Euclidean,
    /// Dot product similarity - measures the magnitude of projection
    /// Best for normalized vectors where magnitude matters
    DotProduct,
}

impl DistanceMetric {
    /// Calculate similarity score between two vectors using this metric
    ///
    /// Returns a value between 0.0 and 1.0 where:
    /// - 1.0 means maximum similarity (identical or very similar vectors)
    /// - 0.0 means minimum similarity (opposite or very different vectors)
    /// - 0.5 means neutral/orthogonal vectors (for cosine similarity)
    pub fn similarity(&self, a: &[f32], b: &[f32]) -> Result<f32, ShardexError> {
        if a.len() != b.len() {
            return Err(ShardexError::InvalidDimension {
                expected: a.len(),
                actual: b.len(),
            });
        }

        if a.is_empty() {
            return Ok(0.5); // Neutral similarity for empty vectors
        }

        let score = match self {
            DistanceMetric::Cosine => cosine_similarity(a, b),
            DistanceMetric::Euclidean => euclidean_similarity(a, b),
            DistanceMetric::DotProduct => dot_product_similarity(a, b),
        };

        // Ensure score is in valid range
        let score = score.clamp(0.0, 1.0);

        if score.is_nan() {
            return Ok(0.5); // Default to neutral similarity for NaN
        }

        Ok(score)
    }

    /// Get a human-readable name for this metric
    pub fn name(&self) -> &'static str {
        match self {
            DistanceMetric::Cosine => "cosine",
            DistanceMetric::Euclidean => "euclidean",
            DistanceMetric::DotProduct => "dot_product",
        }
    }

    /// Get a description of this metric
    pub fn description(&self) -> &'static str {
        match self {
            DistanceMetric::Cosine => {
                "Measures angle between vectors, ideal for high-dimensional text embeddings"
            }
            DistanceMetric::Euclidean => {
                "Measures straight-line distance, ideal for geometric and spatial data"
            }
            DistanceMetric::DotProduct => {
                "Measures projection magnitude, ideal for normalized vectors"
            }
        }
    }

    /// Check if this metric works best with normalized vectors
    pub fn prefers_normalized(&self) -> bool {
        matches!(self, DistanceMetric::Cosine | DistanceMetric::DotProduct)
    }
}

/// Calculate cosine similarity between two vectors
///
/// Returns a value between 0.0 and 1.0 by normalizing the standard cosine similarity:
/// - Cosine similarity range: [-1.0, 1.0]
/// - Normalized range: [0.0, 1.0] using formula: (cosine + 1.0) / 2.0
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.5; // Neutral similarity for zero vectors
    }

    let cosine = dot_product / (norm_a * norm_b);
    let cosine = cosine.clamp(-1.0, 1.0); // Handle floating point precision issues

    // Normalize to [0.0, 1.0] range
    (cosine + 1.0) / 2.0
}

/// Calculate Euclidean distance-based similarity between two vectors
///
/// Converts Euclidean distance to similarity using: 1.0 / (1.0 + distance)
/// This ensures similarity is in [0.0, 1.0] range where:
/// - Distance 0.0 -> Similarity 1.0 (identical vectors)
/// - Distance -> infinity -> Similarity -> 0.0 (very different vectors)
fn euclidean_similarity(a: &[f32], b: &[f32]) -> f32 {
    let distance_squared: f32 = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let diff = x - y;
            diff * diff
        })
        .sum();

    let distance = distance_squared.sqrt();

    // Convert distance to similarity: similarity = 1 / (1 + distance)
    1.0 / (1.0 + distance)
}

/// Calculate dot product-based similarity between two vectors
///
/// Normalizes dot product to [0.0, 1.0] range using sigmoid-like transformation:
/// similarity = 1.0 / (1.0 + exp(-dot_product))
///
/// This works best with pre-normalized vectors where dot product represents
/// direct similarity measure.
fn dot_product_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();

    // Use sigmoid transformation to normalize to [0.0, 1.0]
    // For normalized vectors, dot product is already similarity measure
    1.0 / (1.0 + (-dot_product).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_metric_default() {
        assert_eq!(DistanceMetric::default(), DistanceMetric::Cosine);
    }

    #[test]
    fn test_distance_metric_names() {
        assert_eq!(DistanceMetric::Cosine.name(), "cosine");
        assert_eq!(DistanceMetric::Euclidean.name(), "euclidean");
        assert_eq!(DistanceMetric::DotProduct.name(), "dot_product");
    }

    #[test]
    fn test_distance_metric_descriptions() {
        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let desc = metric.description();
            assert!(!desc.is_empty());
            assert!(desc.len() > 10); // Should be meaningful description
        }
    }

    #[test]
    fn test_prefers_normalized() {
        assert!(DistanceMetric::Cosine.prefers_normalized());
        assert!(DistanceMetric::DotProduct.prefers_normalized());
        assert!(!DistanceMetric::Euclidean.prefers_normalized());
    }

    #[test]
    fn test_cosine_similarity_identical_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];

        let similarity = DistanceMetric::Cosine.similarity(&a, &b).unwrap();
        assert!((similarity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];

        let similarity = DistanceMetric::Cosine.similarity(&a, &b).unwrap();
        assert!((similarity - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal_vectors() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];

        let similarity = DistanceMetric::Cosine.similarity(&a, &b).unwrap();
        assert!((similarity - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_zero_vectors() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 2.0, 3.0];

        let similarity = DistanceMetric::Cosine.similarity(&a, &b).unwrap();
        assert!((similarity - 0.5).abs() < 1e-6); // Neutral similarity
    }

    #[test]
    fn test_euclidean_similarity_identical_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];

        let similarity = DistanceMetric::Euclidean.similarity(&a, &b).unwrap();
        assert!((similarity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_similarity_different_vectors() {
        let a = vec![0.0, 0.0];
        let b = vec![3.0, 4.0]; // Distance = 5.0

        let similarity = DistanceMetric::Euclidean.similarity(&a, &b).unwrap();
        let expected = 1.0 / (1.0 + 5.0); // 1/6 ≈ 0.167
        assert!((similarity - expected).abs() < 1e-6);
    }

    #[test]
    fn test_dot_product_similarity_positive_correlation() {
        let a = vec![1.0, 1.0, 1.0];
        let b = vec![2.0, 2.0, 2.0];

        let similarity = DistanceMetric::DotProduct.similarity(&a, &b).unwrap();
        // Dot product = 6.0, sigmoid should give high similarity
        assert!(similarity > 0.9);
    }

    #[test]
    fn test_dot_product_similarity_negative_correlation() {
        let a = vec![1.0, 1.0, 1.0];
        let b = vec![-1.0, -1.0, -1.0];

        let similarity = DistanceMetric::DotProduct.similarity(&a, &b).unwrap();
        // Dot product = -3.0, sigmoid should give low similarity
        assert!(similarity < 0.1);
    }

    #[test]
    fn test_dimension_validation() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0]; // Different dimension

        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let result = metric.similarity(&a, &b);
            assert!(matches!(
                result.unwrap_err(),
                ShardexError::InvalidDimension { .. }
            ));
        }
    }

    #[test]
    fn test_empty_vectors() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];

        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let similarity = metric.similarity(&a, &b).unwrap();
            assert!((similarity - 0.5).abs() < 1e-6); // Should return neutral similarity
        }
    }

    #[test]
    fn test_similarity_range_bounds() {
        let test_vectors = vec![
            (vec![1.0, 0.0, 0.0], vec![1.0, 0.0, 0.0]),  // Identical
            (vec![1.0, 0.0, 0.0], vec![-1.0, 0.0, 0.0]), // Opposite
            (vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]),  // Orthogonal
            (vec![1.0, 2.0, 3.0], vec![4.0, 5.0, 6.0]),  // Different
            (vec![0.0, 0.0, 0.0], vec![1.0, 1.0, 1.0]),  // Zero vector
        ];

        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            for (a, b) in &test_vectors {
                let similarity = metric.similarity(a, b).unwrap();
                assert!(
                    similarity >= 0.0,
                    "Similarity {} should be >= 0.0 for metric {:?}",
                    similarity,
                    metric
                );
                assert!(
                    similarity <= 1.0,
                    "Similarity {} should be <= 1.0 for metric {:?}",
                    similarity,
                    metric
                );
                assert!(
                    !similarity.is_nan(),
                    "Similarity should not be NaN for metric {:?}",
                    metric
                );
            }
        }
    }

    #[test]
    fn test_cosine_similarity_normalization() {
        // Test that our cosine similarity matches expected normalization
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0]; // 180 degrees apart

        // Standard cosine similarity would be -1.0
        // Our normalized version should be 0.0
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity - 0.0).abs() < 1e-6);

        // Test orthogonal vectors (90 degrees)
        let c = vec![1.0, 0.0];
        let d = vec![0.0, 1.0];

        // Standard cosine similarity would be 0.0
        // Our normalized version should be 0.5
        let similarity2 = cosine_similarity(&c, &d);
        assert!((similarity2 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_similarity_behavior() {
        // Test that closer vectors have higher similarity
        let origin = vec![0.0, 0.0];
        let close = vec![1.0, 0.0]; // Distance = 1.0
        let far = vec![10.0, 0.0]; // Distance = 10.0

        let close_similarity = euclidean_similarity(&origin, &close);
        let far_similarity = euclidean_similarity(&origin, &far);

        assert!(close_similarity > far_similarity);
        assert!(close_similarity > 0.4); // 1/(1+1) = 0.5
        assert!(far_similarity < 0.1); // 1/(1+10) ≈ 0.09
    }

    #[test]
    fn test_dot_product_similarity_sigmoid() {
        // Test that the sigmoid transformation works correctly
        let a = vec![1.0, 0.0];
        let b_positive = vec![2.0, 0.0]; // Dot product = 2.0 (positive)
        let b_zero = vec![0.0, 1.0]; // Dot product = 0.0 (orthogonal)
        let b_negative = vec![-1.0, 0.0]; // Dot product = -1.0 (negative)

        let pos_sim = dot_product_similarity(&a, &b_positive);
        let zero_sim = dot_product_similarity(&a, &b_zero);
        let neg_sim = dot_product_similarity(&a, &b_negative);

        assert!(pos_sim > zero_sim);
        assert!(zero_sim > neg_sim);
        assert!(pos_sim > 0.8); // Should be high for positive dot product
        assert!((zero_sim - 0.5).abs() < 0.1); // Should be near 0.5 for zero dot product
        assert!(neg_sim < 0.4); // Should be low for negative dot product
    }
}

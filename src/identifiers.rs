//! Identifier types for Shardex
//!
//! This module provides type-safe wrapper types for ULID-based identifiers used
//! throughout Shardex to prevent mixing different types of identifiers and ensure
//! memory mapping compatibility.

use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;
use ulid::Ulid;

/// Type-safe wrapper for shard identifiers
///
/// ShardId prevents mixing shard identifiers with document identifiers
/// at compile time and supports direct memory mapping via bytemuck traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct ShardId(u128);

/// Type-safe wrapper for document identifiers
///
/// DocumentId prevents mixing document identifiers with shard identifiers
/// at compile time and supports direct memory mapping via bytemuck traits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(transparent)]
pub struct DocumentId(u128);

// SAFETY: ShardId is a transparent wrapper around u128, which is Pod
unsafe impl Pod for ShardId {}
// SAFETY: ShardId can be zero-initialized safely
unsafe impl Zeroable for ShardId {}

// SAFETY: DocumentId is a transparent wrapper around u128, which is Pod
unsafe impl Pod for DocumentId {}
// SAFETY: DocumentId can be zero-initialized safely
unsafe impl Zeroable for DocumentId {}

impl ShardId {
    /// Generate a new ULID-based shard identifier
    pub fn new() -> Self {
        Self(Ulid::new().0)
    }

    /// Create a ShardId from a ULID
    pub fn from_ulid(ulid: Ulid) -> Self {
        Self(ulid.0)
    }

    /// Convert to ULID
    pub fn as_ulid(self) -> Ulid {
        Ulid(self.0)
    }

    /// Parse from string (alias for FromStr implementation)
    pub fn parse_str(s: &str) -> Result<Self, ulid::DecodeError> {
        Self::from_str(s)
    }

    /// Create from raw bytes (little-endian)
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(u128::from_le_bytes(bytes))
    }

    /// Convert to raw bytes (little-endian)
    pub fn to_bytes(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }

    /// Get the raw u128 value (mainly for testing)
    pub fn raw(self) -> u128 {
        self.0
    }
}

impl DocumentId {
    /// Generate a new ULID-based document identifier
    pub fn new() -> Self {
        Self(Ulid::new().0)
    }

    /// Create a DocumentId from a ULID
    pub fn from_ulid(ulid: Ulid) -> Self {
        Self(ulid.0)
    }

    /// Convert to ULID
    pub fn as_ulid(self) -> Ulid {
        Ulid(self.0)
    }

    /// Parse from string (alias for FromStr implementation)
    pub fn parse_str(s: &str) -> Result<Self, ulid::DecodeError> {
        Self::from_str(s)
    }

    /// Create from raw bytes (little-endian)
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(u128::from_le_bytes(bytes))
    }

    /// Convert to raw bytes (little-endian)
    pub fn to_bytes(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }

    /// Get the raw u128 value (mainly for testing)
    pub fn raw(self) -> u128 {
        self.0
    }
}

impl Default for ShardId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for ShardId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_ulid())
    }
}

impl Display for DocumentId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_ulid())
    }
}

impl FromStr for ShardId {
    type Err = ulid::DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_ulid(Ulid::from_str(s)?))
    }
}

impl FromStr for DocumentId {
    type Err = ulid::DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from_ulid(Ulid::from_str(s)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_shard_id_generation() {
        let id1 = ShardId::new();
        let id2 = ShardId::new();

        // IDs should be unique
        assert_ne!(id1, id2);

        // IDs should be ordered (ULID property)
        assert!(id1 < id2 || id2 < id1);
    }

    #[test]
    fn test_document_id_generation() {
        let id1 = DocumentId::new();
        let id2 = DocumentId::new();

        // IDs should be unique
        assert_ne!(id1, id2);

        // IDs should be ordered (ULID property)
        assert!(id1 < id2 || id2 < id1);
    }

    #[test]
    fn test_ulid_conversion() {
        let ulid = Ulid::new();
        let shard_id = ShardId::from_ulid(ulid);
        let document_id = DocumentId::from_ulid(ulid);

        assert_eq!(shard_id.as_ulid(), ulid);
        assert_eq!(document_id.as_ulid(), ulid);
    }

    #[test]
    fn test_bytes_serialization() {
        let shard_id = ShardId::new();
        let document_id = DocumentId::new();

        let shard_bytes = shard_id.to_bytes();
        let document_bytes = document_id.to_bytes();

        let shard_id_restored = ShardId::from_bytes(shard_bytes);
        let document_id_restored = DocumentId::from_bytes(document_bytes);

        assert_eq!(shard_id, shard_id_restored);
        assert_eq!(document_id, document_id_restored);
    }

    #[test]
    fn test_string_serialization() {
        let shard_id = ShardId::new();
        let document_id = DocumentId::new();

        let shard_str = shard_id.to_string();
        let document_str = document_id.to_string();

        let shard_id_restored: ShardId = shard_str.parse().unwrap();
        let document_id_restored: DocumentId = document_str.parse().unwrap();

        assert_eq!(shard_id, shard_id_restored);
        assert_eq!(document_id, document_id_restored);
    }

    #[test]
    fn test_json_serialization() {
        let shard_id = ShardId::new();
        let document_id = DocumentId::new();

        let shard_json = serde_json::to_string(&shard_id).unwrap();
        let document_json = serde_json::to_string(&document_id).unwrap();

        let shard_id_restored: ShardId = serde_json::from_str(&shard_json).unwrap();
        let document_id_restored: DocumentId = serde_json::from_str(&document_json).unwrap();

        assert_eq!(shard_id, shard_id_restored);
        assert_eq!(document_id, document_id_restored);
    }

    #[test]
    fn test_bytemuck_compatibility() {
        let shard_id = ShardId::new();
        let document_id = DocumentId::new();

        // Test Pod trait - should be able to cast to bytes
        let shard_bytes: &[u8] = bytemuck::bytes_of(&shard_id);
        let document_bytes: &[u8] = bytemuck::bytes_of(&document_id);

        assert_eq!(shard_bytes.len(), 16);
        assert_eq!(document_bytes.len(), 16);

        // Test round-trip
        let shard_id_restored: ShardId = bytemuck::pod_read_unaligned(shard_bytes);
        let document_id_restored: DocumentId = bytemuck::pod_read_unaligned(document_bytes);

        assert_eq!(shard_id, shard_id_restored);
        assert_eq!(document_id, document_id_restored);
    }

    #[test]
    fn test_zeroable_trait() {
        let zero_shard: ShardId = bytemuck::Zeroable::zeroed();
        let zero_document: DocumentId = bytemuck::Zeroable::zeroed();

        assert_eq!(zero_shard.raw(), 0);
        assert_eq!(zero_document.raw(), 0);
    }

    #[test]
    fn test_ordering_properties() {
        let mut shard_ids = Vec::new();
        let mut document_ids = Vec::new();

        // Generate IDs with small delays to ensure ordering
        for _ in 0..10 {
            shard_ids.push(ShardId::new());
            document_ids.push(DocumentId::new());
            thread::sleep(Duration::from_millis(1));
        }

        // Check that IDs are generally increasing (ULID timestamp ordering)
        let mut sorted_shard_ids = shard_ids.clone();
        let mut sorted_document_ids = document_ids.clone();
        sorted_shard_ids.sort();
        sorted_document_ids.sort();

        // Due to the millisecond precision of ULID timestamps and the small delay,
        // we expect some ordering but allow for some out-of-order elements
        let shard_matches = shard_ids
            .iter()
            .zip(sorted_shard_ids.iter())
            .filter(|(a, b)| a == b)
            .count();
        let document_matches = document_ids
            .iter()
            .zip(sorted_document_ids.iter())
            .filter(|(a, b)| a == b)
            .count();

        // At least 80% should be in order due to timestamp component
        assert!(
            shard_matches >= 8,
            "Shard IDs should be mostly ordered by timestamp"
        );
        assert!(
            document_matches >= 8,
            "Document IDs should be mostly ordered by timestamp"
        );
    }

    #[test]
    fn test_uniqueness() {
        let mut shard_set = HashSet::new();
        let mut document_set = HashSet::new();

        // Generate many IDs and ensure uniqueness
        for _ in 0..10000 {
            let shard_id = ShardId::new();
            let document_id = DocumentId::new();

            assert!(shard_set.insert(shard_id), "Shard ID should be unique");
            assert!(
                document_set.insert(document_id),
                "Document ID should be unique"
            );
        }

        assert_eq!(shard_set.len(), 10000);
        assert_eq!(document_set.len(), 10000);
    }

    #[test]
    fn test_debug_format() {
        let shard_id = ShardId::new();
        let document_id = DocumentId::new();

        let shard_debug = format!("{:?}", shard_id);
        let document_debug = format!("{:?}", document_id);

        assert!(shard_debug.starts_with("ShardId("));
        assert!(document_debug.starts_with("DocumentId("));
    }

    #[test]
    fn test_clone_and_copy() {
        let shard_id = ShardId::new();
        let document_id = DocumentId::new();

        let shard_copy = shard_id;
        let document_copy = document_id;

        assert_eq!(shard_id, shard_copy);
        assert_eq!(document_id, document_copy);

        let shard_clone = shard_id;
        let document_clone = document_id;

        assert_eq!(shard_id, shard_clone);
        assert_eq!(document_id, document_clone);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::HashMap;

        let shard_id = ShardId::new();
        let document_id = DocumentId::new();

        let mut shard_map = HashMap::new();
        let mut document_map = HashMap::new();

        shard_map.insert(shard_id, "value1");
        document_map.insert(document_id, "value2");

        assert_eq!(shard_map.get(&shard_id), Some(&"value1"));
        assert_eq!(document_map.get(&document_id), Some(&"value2"));
    }

    #[test]
    fn test_default() {
        let shard_id = ShardId::default();
        let document_id = DocumentId::default();

        // Default should generate new IDs, so they should be different
        assert_ne!(shard_id, ShardId::default());
        assert_ne!(document_id, DocumentId::default());
    }

    #[test]
    fn test_raw_access() {
        let shard_id = ShardId::new();
        let document_id = DocumentId::new();

        let shard_raw = shard_id.raw();
        let document_raw = document_id.raw();

        assert_ne!(shard_raw, 0); // Should not be zero
        assert_ne!(document_raw, 0); // Should not be zero
        assert_ne!(shard_raw, document_raw); // Should be different
    }

    #[test]
    fn test_invalid_string_parsing() {
        let invalid_strings = vec![
            "",
            "invalid",
            "0123456789012345678901234567890", // too long
            "01ARYZ3NDEKTSV4RRFFQ69G5FAV",     // invalid length
            "01ARZ3NDEKTSV4RRFFQ69G5FA!",      // invalid character !
            "01ARZ3NDEKTSV4RRFFQ69G5FA$",      // invalid character $
            "01ARZ3NDEKTSV4RRFFQ69G5FA@",      // invalid character @
        ];

        for invalid in &invalid_strings {
            let shard_result = ShardId::from_str(invalid);
            let document_result = DocumentId::from_str(invalid);

            assert!(
                shard_result.is_err(),
                "String '{}' should be invalid but parsed successfully for ShardId: {:?}",
                invalid,
                shard_result
            );
            assert!(
                document_result.is_err(),
                "String '{}' should be invalid but parsed successfully for DocumentId: {:?}",
                invalid,
                document_result
            );
        }
    }

    #[test]
    fn test_memory_layout() {
        use std::mem;

        // Verify size and alignment
        assert_eq!(mem::size_of::<ShardId>(), 16);
        assert_eq!(mem::size_of::<DocumentId>(), 16);
        assert_eq!(mem::align_of::<ShardId>(), 16); // u128 alignment
        assert_eq!(mem::align_of::<DocumentId>(), 16); // u128 alignment

        // Verify transparent representation
        assert_eq!(mem::size_of::<ShardId>(), mem::size_of::<u128>());
        assert_eq!(mem::size_of::<DocumentId>(), mem::size_of::<u128>());
    }
}

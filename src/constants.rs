//! Central constants for Shardex file format magic numbers and common values
//!
//! This module provides a single source of truth for all magic constants used
//! throughout the codebase, eliminating duplication and ensuring consistency.

/// File format magic numbers used for file type identification
pub mod magic {
    /// WAL (Write-Ahead Log) file magic bytes
    pub const WAL: &[u8; 4] = b"WLOG";

    /// Vector storage file magic bytes
    pub const VECTOR_STORAGE: &[u8; 4] = b"VSTR";

    /// Posting storage file magic bytes  
    pub const POSTING_STORAGE: &[u8; 4] = b"PSTR";

    /// Text index file magic bytes
    pub const TEXT_INDEX: &[u8; 4] = b"TIDX";

    /// Text data file magic bytes
    pub const TEXT_DATA: &[u8; 4] = b"TDAT";

    /// Generic test magic bytes used in unit tests  
    #[cfg(test)]
    pub const TEST_GENERIC: &[u8; 4] = b"TEST";

    /// Shared test magic bytes used in memory tests
    #[cfg(test)]
    pub const TEST_SHARD: &[u8; 4] = b"SHRD";

    /// Corruption testing magic bytes
    #[cfg(test)]
    pub const TEST_CORRUPTION: &[u8; 4] = b"XXXX";

    /// Failure testing magic bytes
    #[cfg(test)]
    pub const TEST_FAILURE: &[u8; 4] = b"FAIL";
}

#[cfg(test)]
mod tests {
    use super::magic;

    #[test]
    fn test_magic_constants_are_correct() {
        // Production magic constants
        assert_eq!(magic::WAL, b"WLOG");
        assert_eq!(magic::VECTOR_STORAGE, b"VSTR");
        assert_eq!(magic::POSTING_STORAGE, b"PSTR");
        assert_eq!(magic::TEXT_INDEX, b"TIDX");
        assert_eq!(magic::TEXT_DATA, b"TDAT");

        // Test magic constants
        assert_eq!(magic::TEST_GENERIC, b"TEST");
        assert_eq!(magic::TEST_SHARD, b"SHRD");
        assert_eq!(magic::TEST_CORRUPTION, b"XXXX");
        assert_eq!(magic::TEST_FAILURE, b"FAIL");
    }

    #[test]
    fn test_magic_constants_are_unique() {
        // Verify no production constants conflict
        let production_constants = [
            magic::WAL,
            magic::VECTOR_STORAGE,
            magic::POSTING_STORAGE,
            magic::TEXT_INDEX,
            magic::TEXT_DATA,
        ];

        // Check all production constants are unique
        for (i, &constant1) in production_constants.iter().enumerate() {
            for &constant2 in production_constants.iter().skip(i + 1) {
                assert_ne!(constant1, constant2, "Magic constants must be unique");
            }
        }
    }

    #[test]
    fn test_magic_constants_are_four_bytes() {
        // All magic constants must be exactly 4 bytes
        assert_eq!(magic::WAL.len(), 4);
        assert_eq!(magic::VECTOR_STORAGE.len(), 4);
        assert_eq!(magic::POSTING_STORAGE.len(), 4);
        assert_eq!(magic::TEXT_INDEX.len(), 4);
        assert_eq!(magic::TEXT_DATA.len(), 4);
        assert_eq!(magic::TEST_GENERIC.len(), 4);
        assert_eq!(magic::TEST_SHARD.len(), 4);
        assert_eq!(magic::TEST_CORRUPTION.len(), 4);
        assert_eq!(magic::TEST_FAILURE.len(), 4);
    }
}

//! Test utilities for document text storage testing
//!
//! Provides common utilities and helpers for testing document text storage:
//! - Test data generation with realistic content
//! - Posting generation for integration testing
//! - Performance measurement utilities  
//! - Error simulation and validation helpers
//! - Text boundary and UTF-8 testing utilities

use shardex::document_text_storage::DocumentTextStorage;
use shardex::identifiers::DocumentId;
use shardex::structures::Posting;
use shardex::error::ShardexError;
use tempfile::TempDir;
use std::time::{Duration, Instant};

/// Test environment for document text storage testing
pub struct DocumentTestEnvironment {
    pub temp_dir: TempDir,
    pub storage: DocumentTextStorage,
    pub test_name: String,
}

impl DocumentTestEnvironment {
    /// Create a new test environment with specified limits
    pub fn new(test_name: &str, max_document_size: usize) -> Self {
        let temp_dir = TempDir::new()
            .unwrap_or_else(|e| panic!("Failed to create temp dir for test {}: {}", test_name, e));
        
        let storage = DocumentTextStorage::create(&temp_dir, max_document_size)
            .unwrap_or_else(|e| panic!("Failed to create storage for test {}: {}", test_name, e));
        
        Self {
            temp_dir,
            storage,
            test_name: test_name.to_string(),
        }
    }
    
    /// Create with default 10MB limit
    pub fn new_default(test_name: &str) -> Self {
        Self::new(test_name, 10 * 1024 * 1024)
    }
    
    /// Create with small limit for testing size restrictions
    pub fn new_small(test_name: &str) -> Self {
        Self::new(test_name, 1024) // 1KB limit
    }
    
    /// Get the storage instance
    pub fn storage(&mut self) -> &mut DocumentTextStorage {
        &mut self.storage
    }
    
    /// Get read-only access to storage
    pub fn storage_ref(&self) -> &DocumentTextStorage {
        &self.storage
    }
    
    /// Reopen the storage (simulates restart/crash recovery)
    pub fn reopen(&mut self) {
        self.storage = DocumentTextStorage::open(&self.temp_dir)
            .unwrap_or_else(|e| panic!("Failed to reopen storage for test {}: {}", self.test_name, e));
    }
}

/// Text generation utilities
pub struct TextGenerator {
    rng_state: u32,
    word_list: Vec<&'static str>,
}

impl Default for TextGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl TextGenerator {
    pub fn new() -> Self {
        Self {
            rng_state: 12345,
            word_list: vec![
                "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog",
                "Lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing",
                "elit", "sed", "do", "eiusmod", "tempor", "incididunt", "ut", "labore",
                "et", "dolore", "magna", "aliqua", "Ut", "enim", "ad", "minim", "veniam",
                "quis", "nostrud", "exercitation", "ullamco", "laboris", "nisi",
                "performance", "testing", "document", "storage", "retrieval", "system",
                "database", "memory", "mapping", "efficient", "scalable", "robust",
                "implementation", "algorithm", "optimization", "thread", "safety",
                "concurrency", "parallel", "distributed", "architecture", "design",
            ],
        }
    }
    
    /// Generate deterministic random text with specified word count
    pub fn generate_text(&mut self, word_count: usize) -> String {
        let mut text = String::with_capacity(word_count * 8); // Estimate 8 chars per word
        
        for i in 0..word_count {
            if i > 0 {
                text.push(' ');
            }
            
            // Simple linear congruential generator for deterministic results
            self.rng_state = self.rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
            let word_index = (self.rng_state as usize) % self.word_list.len();
            text.push_str(self.word_list[word_index]);
        }
        
        text
    }
    
    /// Generate text with specific characteristics
    pub fn generate_structured_text(&mut self, paragraphs: usize, sentences_per_paragraph: usize, words_per_sentence: usize) -> String {
        let mut text = String::new();
        
        for p in 0..paragraphs {
            if p > 0 {
                text.push_str("\n\n");
            }
            
            for s in 0..sentences_per_paragraph {
                if s > 0 {
                    text.push(' ');
                }
                
                let sentence = self.generate_text(words_per_sentence);
                text.push_str(&sentence);
                text.push('.');
            }
        }
        
        text
    }
    
    /// Generate text with Unicode content
    pub fn generate_unicode_text(&mut self, language_type: UnicodeTestType) -> String {
        match language_type {
            UnicodeTestType::Chinese => "你好世界！这是一个中文测试文档，用于验证UTF-8编码的正确处理。我们需要确保中文字符能够正确存储和检索。".to_string(),
            UnicodeTestType::Japanese => "こんにちは世界！これは日本語のテスト文書です。UTF-8エンコーディングが正しく動作することを確認します。".to_string(),
            UnicodeTestType::Arabic => "مرحبا بالعالم! هذا مستند اختبار باللغة العربية للتحقق من التعامل الصحيح مع ترميز UTF-8.".to_string(),
            UnicodeTestType::Emoji => "Hello 🌍 World! 🚀 This document contains emojis 🎉✨ to test Unicode handling 🌟💫".to_string(),
            UnicodeTestType::Mixed => {
                let base_text = self.generate_text(20);
                format!("{} 中文 🌍 العربية 🚀 日本語 ✨ Mixed content! 🎉", base_text)
            },
            UnicodeTestType::ControlCharacters => "Text with\ttabs\nand\rcarriage\x01returns\x02and\x03control\x1fcharacters.".to_string(),
        }
    }
    
    /// Generate text that would cause UTF-8 boundary issues
    pub fn generate_boundary_test_text(&mut self) -> String {
        "Héllo Wörld! 🌍🚀 Üñíçødé tëst with multibyte characters é ñ ü".to_string()
    }
    
    /// Generate text with specific size
    pub fn generate_sized_text(&mut self, target_bytes: usize) -> String {
        let base_text = self.generate_text(100);
        let base_len = base_text.len();
        
        if target_bytes <= base_len {
            return base_text[..target_bytes].to_string();
        }
        
        let repetitions = (target_bytes + base_len - 1) / base_len; // Ceiling division
        let mut result = base_text.repeat(repetitions);
        result.truncate(target_bytes);
        result
    }
}

/// Types of Unicode content for testing
pub enum UnicodeTestType {
    Chinese,
    Japanese,
    Arabic,
    Emoji,
    Mixed,
    ControlCharacters,
}

/// Posting generation utilities
pub struct PostingGenerator {
    rng_state: u32,
}

impl Default for PostingGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl PostingGenerator {
    pub fn new() -> Self {
        Self { rng_state: 54321 }
    }
    
    /// Generate realistic postings for a document
    pub fn generate_postings(
        &mut self,
        document_id: DocumentId,
        text: &str,
        vector_dimension: usize,
        posting_count: usize,
    ) -> Result<Vec<Posting>, ShardexError> {
        if posting_count == 0 {
            return Ok(Vec::new());
        }
        
        let mut postings = Vec::new();
        let text_len = text.len();
        
        // Create evenly distributed postings
        for i in 0..posting_count {
            let segment_size = text_len / posting_count;
            let start = (i * segment_size) as u32;
            
            // Calculate length for this posting
            let remaining = text_len - (i * segment_size);
            let max_length = std::cmp::min(segment_size, remaining);
            
            // Use a reasonable length (not too small, not too large)
            let length = std::cmp::max(1, std::cmp::min(max_length, 100)) as u32;
            
            // Adjust start to respect UTF-8 boundaries
            let (adjusted_start, adjusted_length) = self.adjust_for_utf8_boundaries(text, start, length);
            
            if adjusted_length == 0 {
                continue; // Skip invalid postings
            }
            
            // Generate vector for this posting
            let vector = self.generate_vector(vector_dimension, i);
            
            let posting = Posting::new(document_id, adjusted_start, adjusted_length, vector, vector_dimension)?;
            postings.push(posting);
        }
        
        Ok(postings)
    }
    
    /// Generate postings based on word boundaries
    pub fn generate_word_based_postings(
        &mut self,
        document_id: DocumentId,
        text: &str,
        vector_dimension: usize,
    ) -> Result<Vec<Posting>, ShardexError> {
        let words: Vec<(usize, &str)> = text
            .split_whitespace()
            .enumerate()
            .map(|(i, word)| {
                let start_pos = text[..].find(word).unwrap_or(0) + i * " ".len();
                (start_pos, word)
            })
            .collect();
        
        let mut postings = Vec::new();
        
        for (i, (start_pos, word)) in words.iter().enumerate() {
            if word.is_empty() {
                continue;
            }
            
            let vector = self.generate_vector(vector_dimension, i);
            let posting = Posting::new(
                document_id,
                *start_pos as u32,
                word.len() as u32,
                vector,
                vector_dimension,
            )?;
            postings.push(posting);
        }
        
        Ok(postings)
    }
    
    /// Generate overlapping postings for stress testing
    pub fn generate_overlapping_postings(
        &mut self,
        document_id: DocumentId,
        text: &str,
        vector_dimension: usize,
        posting_count: usize,
    ) -> Result<Vec<Posting>, ShardexError> {
        let mut postings = Vec::new();
        let text_len = text.len();
        
        if text_len == 0 || posting_count == 0 {
            return Ok(postings);
        }
        
        for i in 0..posting_count {
            // Create overlapping postings
            let start = (i * text_len / (posting_count + 1)) as u32;
            let length = std::cmp::min(50, text_len - start as usize) as u32;
            
            if length == 0 {
                break;
            }
            
            let (adjusted_start, adjusted_length) = self.adjust_for_utf8_boundaries(text, start, length);
            
            if adjusted_length > 0 {
                let vector = self.generate_vector(vector_dimension, i);
                let posting = Posting::new(document_id, adjusted_start, adjusted_length, vector, vector_dimension)?;
                postings.push(posting);
            }
        }
        
        Ok(postings)
    }
    
    /// Generate vector with deterministic values
    fn generate_vector(&mut self, dimension: usize, seed_modifier: usize) -> Vec<f32> {
        let mut vector = Vec::with_capacity(dimension);
        
        for _i in 0..dimension {
            self.rng_state = self.rng_state.wrapping_mul(1664525).wrapping_add(1013904223);
            let value = (self.rng_state.wrapping_add(seed_modifier as u32) as f32) / (u32::MAX as f32);
            vector.push(value);
        }
        
        vector
    }
    
    /// Adjust posting coordinates to respect UTF-8 character boundaries
    fn adjust_for_utf8_boundaries(&self, text: &str, start: u32, length: u32) -> (u32, u32) {
        let start_usize = start as usize;
        let length_usize = length as usize;
        
        if start_usize >= text.len() {
            return (start, 0);
        }
        
        // Find the nearest valid UTF-8 boundary for start
        let mut adjusted_start = start_usize;
        while adjusted_start > 0 && !text.is_char_boundary(adjusted_start) {
            adjusted_start -= 1;
        }
        
        // Calculate end position
        let end_usize = std::cmp::min(adjusted_start + length_usize, text.len());
        let mut adjusted_end = end_usize;
        while adjusted_end > adjusted_start && !text.is_char_boundary(adjusted_end) {
            adjusted_end -= 1;
        }
        
        let adjusted_length = adjusted_end - adjusted_start;
        
        (adjusted_start as u32, adjusted_length as u32)
    }
}

/// Performance measurement utilities
pub struct PerformanceTracker {
    measurements: Vec<(String, Duration)>,
}

impl Default for PerformanceTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceTracker {
    pub fn new() -> Self {
        Self {
            measurements: Vec::new(),
        }
    }
    
    /// Measure execution time of an operation
    pub fn measure<F, R>(&mut self, operation_name: &str, operation: F) -> R
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = operation();
        let duration = start.elapsed();
        
        self.measurements.push((operation_name.to_string(), duration));
        result
    }
    
    /// Get the last measurement
    pub fn last_measurement(&self) -> Option<&(String, Duration)> {
        self.measurements.last()
    }
    
    /// Get all measurements
    pub fn measurements(&self) -> &[(String, Duration)] {
        &self.measurements
    }
    
    /// Get total time for all measurements
    pub fn total_time(&self) -> Duration {
        self.measurements.iter().map(|(_, duration)| *duration).sum()
    }
    
    /// Assert that the last operation was within expected time
    pub fn assert_last_within(&self, max_duration: Duration) {
        if let Some((name, duration)) = self.last_measurement() {
            assert!(
                *duration <= max_duration,
                "Operation '{}' took {:?}, expected <= {:?}",
                name,
                duration,
                max_duration
            );
        } else {
            assert!(!self.measurements.is_empty(), "No measurements recorded");
        }
    }
    
    /// Print performance summary
    pub fn print_summary(&self) {
        println!("Performance Summary:");
        for (name, duration) in &self.measurements {
            println!("  {}: {:?}", name, duration);
        }
        println!("  Total: {:?}", self.total_time());
    }
}

/// Error testing utilities
pub struct ErrorTestHelper;

impl ErrorTestHelper {
    /// Validate error type and message content
    pub fn assert_error_type<T: std::fmt::Debug>(result: Result<T, ShardexError>, expected_type: ErrorType) {
        assert!(result.is_err(), "Expected error, got success");
        
        let error = result.unwrap_err();
        match (expected_type, &error) {
            (ErrorType::InvalidRange, ShardexError::InvalidRange { .. }) => {},
            (ErrorType::DocumentTooLarge, ShardexError::DocumentTooLarge { .. }) => {},
            (ErrorType::DocumentTextNotFound, ShardexError::DocumentTextNotFound { .. }) => {},
            (ErrorType::InvalidInput, ShardexError::InvalidInput { .. }) => {},
            (ErrorType::TextCorruption, ShardexError::TextCorruption(_)) => {},
            _ => panic!("Expected error type {:?}, got {:?}", expected_type, error),
        }
    }
    
    /// Test that an operation fails with a specific error type
    pub fn expect_error<F, T>(operation: F, expected_type: ErrorType)
    where
        F: FnOnce() -> Result<T, ShardexError>,
        T: std::fmt::Debug,
    {
        Self::assert_error_type(operation(), expected_type);
    }
    
    /// Generate text that exceeds size limits
    pub fn generate_oversized_text(size_limit: usize) -> String {
        "x".repeat(size_limit + 100)
    }
    
    /// Generate text with problematic content
    pub fn generate_problematic_text(problem_type: ProblematicTextType) -> String {
        match problem_type {
            ProblematicTextType::NullBytes => "Hello\x00World\x00Test".to_string(),
            ProblematicTextType::ControlCharacters => "Text\x01with\x02control\x03chars".to_string(),
            ProblematicTextType::VeryLong => "A".repeat(1_000_000),
        }
    }
}

/// Error types for testing
#[derive(Debug, Clone, Copy)]
pub enum ErrorType {
    InvalidRange,
    DocumentTooLarge,
    DocumentTextNotFound,
    InvalidInput,
    TextCorruption,
}

/// Types of problematic text for error testing
#[derive(Debug, Clone, Copy)]
pub enum ProblematicTextType {
    NullBytes,
    ControlCharacters,
    VeryLong,
}

/// Test validation utilities
pub struct ValidationHelper;

impl ValidationHelper {
    /// Validate that storage statistics are consistent
    pub fn validate_storage_consistency(storage: &DocumentTextStorage) {
        // entry_count() and total_text_size() return unsigned types, so they cannot be negative
        assert!(storage.utilization_ratio() >= 0.0, "Utilization ratio cannot be negative");
        assert!(storage.utilization_ratio() <= 1.0, "Utilization ratio cannot exceed 1.0");
        
        if storage.entry_count() == 0 {
            assert!(storage.is_empty(), "Empty storage should report as empty");
            assert_eq!(storage.total_text_size(), 0, "Empty storage should have zero text size");
        } else {
            assert!(!storage.is_empty(), "Non-empty storage should not report as empty");
        }
    }
    
    /// Validate that text extraction matches expectations
    pub fn validate_text_extraction(
        storage: &DocumentTextStorage,
        _doc_id: DocumentId,
        expected_full_text: &str,
        start: u32,
        length: u32,
    ) {
        let extracted = storage.extract_text_substring(_doc_id, start, length).unwrap();
        let expected_start = start as usize;
        let expected_end = expected_start + length as usize;
        
        assert!(expected_end <= expected_full_text.len(), 
            "Test parameters exceed document bounds");
        
        let expected = &expected_full_text[expected_start..expected_end];
        assert_eq!(extracted, expected, 
            "Extracted text doesn't match expected for range {}..{}", 
            expected_start, expected_end);
    }
    
    /// Validate that all postings extract correctly from a document
    pub fn validate_postings_extraction(
        storage: &DocumentTextStorage,
        _doc_id: DocumentId,
        expected_text: &str,
        postings: &[Posting],
    ) {
        for (i, posting) in postings.iter().enumerate() {
            let extracted = storage.extract_text_substring(
                posting.document_id,
                posting.start,
                posting.length,
            ).unwrap_or_else(|e| {
                panic!("Failed to extract posting {}: {:?}", i, e);
            });
            
            let expected_start = posting.start as usize;
            let expected_end = expected_start + posting.length as usize;
            
            assert!(expected_end <= expected_text.len(), 
                "Posting {} has invalid range {}..{} for document of length {}",
                i, expected_start, expected_end, expected_text.len());
            
            let expected = &expected_text[expected_start..expected_end];
            assert_eq!(extracted, expected, 
                "Posting {} extraction mismatch at {}..{}", 
                i, expected_start, expected_end);
        }
    }
}

/// Test data structures for complex scenarios
pub struct TestScenario {
    pub name: String,
    pub documents: Vec<(DocumentId, String)>,
    pub expected_operations: usize,
    pub performance_expectations: Option<Duration>,
}

impl TestScenario {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            documents: Vec::new(),
            expected_operations: 0,
            performance_expectations: None,
        }
    }
    
    /// Add a document to the scenario
    pub fn add_document(mut self, text: String) -> Self {
        let doc_id = DocumentId::new();
        self.documents.push((doc_id, text));
        self
    }
    
    /// Set performance expectations
    pub fn expect_completion_within(mut self, max_duration: Duration) -> Self {
        self.performance_expectations = Some(max_duration);
        self
    }
    
    /// Execute the scenario against storage
    pub fn execute(&self, storage: &mut DocumentTextStorage) -> Duration {
        let start = Instant::now();
        
        for (doc_id, text) in &self.documents {
            storage.store_text_safe(*doc_id, text).unwrap_or_else(|e| {
                panic!("Failed to store document in scenario '{}': {:?}", self.name, e);
            });
            
            // Verify storage
            let retrieved = storage.get_text_safe(*doc_id).unwrap();
            assert_eq!(retrieved, *text);
        }
        
        let duration = start.elapsed();
        
        if let Some(max_duration) = self.performance_expectations {
            assert!(duration <= max_duration, 
                "Scenario '{}' took {:?}, expected <= {:?}",
                self.name, duration, max_duration);
        }
        
        duration
    }
}

// Convenience functions for common test patterns

/// Create a simple test document with predictable content
/// Create a test document of the specified size category
/// 
/// Generates deterministic text content appropriate for testing various scenarios:
/// - `Tiny`: ~60 bytes (10 words) - for boundary testing
/// - `Small`: ~600 bytes (100 words) - for basic functionality
/// - `Medium`: ~6KB (1000 words) - for moderate size testing
/// - `Large`: ~60KB (10,000 words) - for performance testing
/// - `Huge`: ~600KB (100,000 words) - for stress testing
pub fn create_test_document(size_category: DocumentSize) -> String {
    let mut generator = TextGenerator::new();
    match size_category {
        DocumentSize::Tiny => generator.generate_text(10),      // ~60 bytes
        DocumentSize::Small => generator.generate_text(100),    // ~600 bytes
        DocumentSize::Medium => generator.generate_text(1000),  // ~6KB
        DocumentSize::Large => generator.generate_text(10000),  // ~60KB
        DocumentSize::Huge => generator.generate_text(100000),  // ~600KB
    }
}

/// Document size categories for testing
#[derive(Debug, Clone, Copy)]
pub enum DocumentSize {
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
}

/// Create test postings for a document
pub fn create_test_postings(
    document_id: DocumentId,
    text: &str,
    vector_dimension: usize,
    posting_count: usize,
) -> Result<Vec<Posting>, ShardexError> {
    let mut generator = PostingGenerator::new();
    generator.generate_postings(document_id, text, vector_dimension, posting_count)
}

/// Measure operation performance
pub fn measure_operation<F, R>(operation: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = operation();
    let duration = start.elapsed();
    (result, duration)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_text_generator() {
        let mut generator = TextGenerator::new();
        
        let text1 = generator.generate_text(10);
        let text2 = generator.generate_text(10);
        
        // Should be deterministic
        assert_ne!(text1, text2); // Different because of internal state changes
        assert!(!text1.is_empty());
        assert!(!text2.is_empty());
        
        // Test structured text
        let structured = generator.generate_structured_text(2, 3, 5);
        assert!(structured.contains('\n'));
        assert!(structured.contains('.'));
    }
    
    #[test]
    fn test_posting_generator() {
        let mut generator = PostingGenerator::new();
        let doc_id = DocumentId::new();
        let text = "The quick brown fox jumps over the lazy dog";
        
        let postings = generator.generate_postings(doc_id, text, 64, 3).unwrap();
        
        assert_eq!(postings.len(), 3);
        for posting in postings {
            assert_eq!(posting.document_id, doc_id);
            assert!(posting.start < text.len() as u32);
            assert!(posting.length > 0);
            assert_eq!(posting.vector.len(), 64);
        }
    }
    
    #[test]
    fn test_performance_tracker() {
        let mut tracker = PerformanceTracker::new();
        
        let result = tracker.measure("test_operation", || {
            std::thread::sleep(Duration::from_millis(10));
            42
        });
        
        assert_eq!(result, 42);
        assert_eq!(tracker.measurements().len(), 1);
        assert!(tracker.total_time() >= Duration::from_millis(10));
    }
    
    #[test]
    fn test_document_test_environment() {
        let mut env = DocumentTestEnvironment::new_default("test_env");
        let doc_id = DocumentId::new();
        let text = "Test document for environment";
        
        env.storage().store_text_safe(doc_id, text).unwrap();
        let retrieved = env.storage_ref().get_text_safe(doc_id).unwrap();
        assert_eq!(retrieved, text);
        
        // Test reopen functionality
        env.reopen();
        let recovered = env.storage_ref().get_text_safe(doc_id).unwrap();
        assert_eq!(recovered, text);
    }
}
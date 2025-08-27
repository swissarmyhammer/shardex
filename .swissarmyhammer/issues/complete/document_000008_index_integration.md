# Step 8: Index Integration - Connect DocumentTextStorage to ShardexIndex

Refer to /Users/wballard/github/shardex/ideas/document.md

## Objective

Integrate DocumentTextStorage into the ShardexIndex structure, enabling text storage operations to be coordinated with the overall index lifecycle.

## Tasks

### Update `ShardexIndex` in `src/shardex_index.rs`

Add DocumentTextStorage as an optional component:

```rust
use crate::document_text_storage::DocumentTextStorage;

pub struct ShardexIndex {
    // ... existing fields
    
    /// Optional document text storage (None if not enabled)
    document_text_storage: Option<DocumentTextStorage>,
}

impl ShardexIndex {
    /// Create new index with optional text storage
    pub fn create(
        config: ShardexConfig,
        layout: DirectoryLayout,
    ) -> Result<Self, ShardexError> {
        // ... existing creation logic
        
        // Create text storage if max_document_text_size is configured
        let document_text_storage = if config.max_document_text_size > 0 {
            Some(DocumentTextStorage::create(
                &layout.index_directory,
                config.max_document_text_size,
            )?)
        } else {
            None
        };
        
        Ok(Self {
            // ... existing fields
            document_text_storage,
        })
    }
    
    /// Open existing index with optional text storage
    pub fn open(layout: DirectoryLayout) -> Result<Self, ShardexError> {
        // ... existing open logic
        
        // Open text storage if files exist
        let document_text_storage = if layout.has_text_storage() {
            Some(DocumentTextStorage::open(&layout.index_directory)?)
        } else {
            None
        };
        
        Ok(Self {
            // ... existing fields  
            document_text_storage,
        })
    }
}
```

### WAL Operation Application

Integrate text operations with WAL replay:

```rust
impl ShardexIndex {
    /// Apply WAL operation to index
    pub fn apply_wal_operation(&mut self, operation: &WalOperation) -> Result<(), ShardexError> {
        match operation {
            // Text storage operations (index-level)
            WalOperation::StoreDocumentText { document_id, text } => {
                if let Some(ref mut text_storage) = self.document_text_storage {
                    text_storage.store_text_safe(*document_id, text)?;
                } else {
                    return Err(ShardexError::InvalidInput {
                        field: "text_storage".to_string(),
                        reason: "Text storage not enabled for this index".to_string(),
                        suggestion: "Enable text storage in configuration".to_string(),
                    });
                }
            }
            
            WalOperation::DeleteDocumentText { document_id } => {
                // Note: Actual deletion handled by compaction
                // For now, this is a no-op as we use append-only storage
                tracing::debug!("Document text deletion marked for: {}", document_id);
            }
            
            // Shard-level operations (delegate to specific shards)
            WalOperation::AddPosting { document_id, start, length, vector } => {
                let posting = Posting {
                    document_id: *document_id,
                    start: *start,
                    length: *length,
                    vector: vector.clone(),
                };
                
                let shard_id = self.determine_shard_for_posting(&posting)?;
                let shard = self.get_shard_mut(shard_id)?;
                shard.apply_add_posting(&posting)?;
            }
            
            WalOperation::RemoveDocument { document_id } => {
                // Remove from all shards
                for shard in &mut self.shards {
                    shard.apply_remove_document(*document_id)?;
                }
            }
        }
        
        Ok(())
    }
}
```

### Text Storage Access Methods

Add text storage access methods:

```rust
impl ShardexIndex {
    /// Get document text if text storage is enabled
    pub fn get_document_text(&self, document_id: DocumentId) -> Result<String, ShardexError> {
        match &self.document_text_storage {
            Some(storage) => storage.get_text_safe(document_id),
            None => Err(ShardexError::InvalidInput {
                field: "text_storage".to_string(),
                reason: "Text storage not enabled for this index".to_string(),
                suggestion: "Enable text storage in configuration or use an index with text storage".to_string(),
            }),
        }
    }
    
    /// Extract text from posting coordinates
    pub fn extract_text_from_posting(&self, posting: &Posting) -> Result<String, ShardexError> {
        match &self.document_text_storage {
            Some(storage) => {
                storage.extract_text_substring(
                    posting.document_id,
                    posting.start,
                    posting.length,
                )
            }
            None => Err(ShardexError::InvalidInput {
                field: "text_storage".to_string(),
                reason: "Text storage not enabled for this index".to_string(),
                suggestion: "Enable text storage in configuration".to_string(),
            }),
        }
    }
    
    /// Check if text storage is enabled
    pub fn has_text_storage(&self) -> bool {
        self.document_text_storage.is_some()
    }
    
    /// Get text storage statistics
    pub fn text_storage_stats(&self) -> Option<TextStorageStats> {
        self.document_text_storage.as_ref().map(|storage| {
            TextStorageStats {
                index_file_size: storage.index_file_size(),
                data_file_size: storage.data_file_size(),
                document_count: storage.document_count(),
                total_text_size: storage.total_text_size(),
            }
        })
    }
}

/// Statistics for text storage
#[derive(Debug, Clone, PartialEq)]
pub struct TextStorageStats {
    pub index_file_size: u64,
    pub data_file_size: u64,
    pub document_count: u32,
    pub total_text_size: u64,
}
```

### Lifecycle Management

Integrate with index lifecycle:

```rust
impl ShardexIndex {
    /// Flush text storage along with shards
    pub fn flush(&mut self) -> Result<FlushStats, ShardexError> {
        // ... existing shard flush logic
        
        // Flush text storage if enabled
        if let Some(ref mut storage) = self.document_text_storage {
            storage.flush()?;
        }
        
        // ... return combined flush stats
    }
    
    /// Close index including text storage
    pub fn close(&mut self) -> Result<(), ShardexError> {
        // ... existing close logic
        
        // Close text storage
        if let Some(mut storage) = self.document_text_storage.take() {
            storage.close()?;
        }
        
        Ok(())
    }
    
    /// Validate entire index including text storage
    pub fn validate_integrity(&self) -> Result<(), ShardexError> {
        // ... existing shard validation
        
        // Validate text storage if enabled
        if let Some(ref storage) = self.document_text_storage {
            storage.validate_integrity()?;
        }
        
        Ok(())
    }
}
```

## Implementation Requirements

1. **Optional Integration**: Text storage is optional and gracefully disabled
2. **WAL Coordination**: Text operations coordinated with existing WAL system
3. **Lifecycle Management**: Text storage participates in index lifecycle
4. **Error Handling**: Clear errors when text storage unavailable
5. **Statistics**: Text storage metrics integrated with index stats

## Validation Criteria

- [ ] Index can be created with or without text storage
- [ ] WAL operations dispatch correctly to text storage or shards  
- [ ] Text storage methods work when enabled, error when disabled
- [ ] Index lifecycle methods handle text storage properly
- [ ] Statistics include text storage information when available
- [ ] Error messages are clear when text storage not enabled

## Integration Points

- Uses DocumentTextStorage from Step 5 (Storage Implementation)
- Uses WAL operations from Step 3 (WAL Operations)
- Uses file management from Step 6 (File Management)
- Uses safe operations from Step 7 (Text Operations)

## Next Steps

This enables the high-level API methods in Step 9 (Core API Methods) and Step 10 (Atomic Replacement).

## Proposed Solution

Based on my analysis of the existing codebase, I have the following implementation approach:

### Key Findings
- `DocumentTextStorage` exists with methods: `create`, `open`, `store_text_safe`, `get_text_safe`, `extract_text_substring`
- `WalOperation` already includes `StoreDocumentText` and `DeleteDocumentText` variants
- `ShardexConfig` has `max_document_text_size` field
- `DirectoryLayout` has `has_text_storage()` method to detect existing text storage files

### Implementation Steps

1. **ShardexIndex Structure Update**:
   - Add optional `document_text_storage: Option<DocumentTextStorage>` field
   - Import `DocumentTextStorage` and related types

2. **Create Method Enhancement**:
   - Check if `config.max_document_text_size > 0`
   - If yes, call `DocumentTextStorage::create()` with the directory and size
   - Store as `Some(storage)`, otherwise `None`

3. **Open Method Enhancement**:
   - Use `layout.has_text_storage()` to check for existing text storage
   - If exists, call `DocumentTextStorage::open()` 
   - Store as `Some(storage)`, otherwise `None`

4. **WAL Integration**:
   - Add/enhance `apply_wal_operation` method to handle text operations
   - Route `StoreDocumentText` to `text_storage.store_text_safe()`
   - Route `DeleteDocumentText` as no-op (handled by compaction)
   - Keep existing shard operation routing

5. **Text Access Methods**:
   - `get_document_text()`: delegate to `storage.get_text_safe()` or error if disabled
   - `extract_text_from_posting()`: delegate to `storage.extract_text_substring()`
   - `has_text_storage()`: check if `document_text_storage.is_some()`

6. **Lifecycle Integration**:
   - Update `flush()`: call `storage.sync()` if text storage exists
   - Update `close()`: call `storage.drop()` if text storage exists  
   - Update validation: call `storage` validation if exists

7. **Statistics Support**:
   - Create `TextStorageStats` struct with file sizes, document count, total size
   - Add `text_storage_stats()` method returning `Option<TextStorageStats>`
   - Use existing `DocumentTextStorage` methods for data

### Error Handling Strategy
- Clear error messages when text storage not enabled
- Graceful degradation when text storage disabled
- Consistent error types using existing `ShardexError` variants

### Testing Approach
- Test index creation with and without text storage
- Test WAL operation dispatch
- Test lifecycle method integration
- Test error conditions when text storage disabled

This approach maintains backward compatibility while adding comprehensive text storage integration.
## Implementation Complete ✅

**Status**: COMPLETED

### Summary

Successfully integrated DocumentTextStorage into ShardexIndex with comprehensive text storage capabilities:

### ✅ Completed Tasks

1. **✅ ShardexIndex Structure Updated**
   - Added optional `document_text_storage: Option<DocumentTextStorage>` field
   - Added necessary imports (DocumentTextStorage, DirectoryLayout, WalOperation, Posting)

2. **✅ Create Method Enhanced**
   - Detects `max_document_text_size > 0` to enable text storage
   - Creates DocumentTextStorage with configured size limit
   - Maintains backward compatibility

3. **✅ Open Method Enhanced**  
   - Uses `DirectoryLayout::has_text_storage()` to detect existing storage
   - Opens DocumentTextStorage when files exist
   - Gracefully handles missing text storage

4. **✅ WAL Integration Complete**
   - Added `apply_wal_operation()` method handling all WalOperation variants
   - Routes `StoreDocumentText` to `storage.store_text_safe()`
   - Routes `DeleteDocumentText` as no-op (append-only storage)
   - Routes shard operations to appropriate shards with proper borrow handling

5. **✅ Text Access Methods**
   - `get_document_text()`: retrieves full document text with clear error handling
   - `extract_text_from_posting()`: extracts text substrings from posting coordinates
   - `has_text_storage()`: checks if text storage enabled
   - All methods provide clear errors when text storage disabled

6. **✅ Statistics Support**
   - Added `TextStorageStats` struct with file sizes, document count, total size
   - Added `text_storage_stats()` method returning `Option<TextStorageStats>`
   - Uses DocumentTextStorage metadata for accurate statistics

7. **✅ Lifecycle Integration**
   - `flush()`: calls `storage.sync()` for text storage + all cached shards
   - `close()`: flushes data and properly drops text storage
   - `validate_integrity()`: validates text storage when enabled

8. **✅ Helper Methods**
   - `determine_shard_for_posting()`: finds best shard based on centroid distance
   - Handles shard selection with writability and capacity checks

9. **✅ Comprehensive Testing**
   - 12 new integration tests covering all functionality
   - Tests creation with/without text storage
   - Tests opening existing text storage
   - Tests WAL operation handling
   - Tests text retrieval and substring extraction
   - Tests error conditions and lifecycle methods
   - **All 52 ShardexIndex tests passing** ✅

10. **✅ Build & Quality**
    - Clean compilation with no errors
    - All compiler warnings resolved
    - Proper error handling throughout

### Key Features Delivered

- **Optional Integration**: Text storage gracefully disabled when not configured
- **WAL Coordination**: Text operations coordinated with existing WAL system  
- **Lifecycle Management**: Text storage participates in index lifecycle
- **Error Handling**: Clear errors when text storage unavailable
- **Statistics**: Text storage metrics integrated with index stats
- **Backward Compatibility**: Existing code continues to work unchanged

### Code Quality

- Follows existing Shardex patterns and conventions
- Comprehensive documentation with examples
- Full test coverage with edge cases
- Clean error messages and suggestions
- Proper resource management and cleanup

**Ready for Step 9 (Core API Methods) and Step 10 (Atomic Replacement)**
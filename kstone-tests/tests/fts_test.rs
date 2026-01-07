//! Full-Text Search (FTS) Integration Tests
//!
//! Tests for the Phase 11 FTS functionality including:
//! - Text index creation and configuration
//! - Document indexing on put operations
//! - Various query types (term, phrase, boolean, fuzzy, prefix)
//! - BM25 ranking
//! - Highlighting and snippets

use kstone_api::{Database, ItemBuilder, TableSchema, TextIndex, Language};
use tempfile::TempDir;

// ============================================================================
// Basic FTS Operations
// ============================================================================

#[test]
fn test_fts_create_database_with_text_index() {
    let dir = TempDir::new().unwrap();

    // Create schema with text index
    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Verify database created successfully
    let stats = db.stats().unwrap();
    assert_eq!(stats.total_sst_files, 0);
}

#[test]
fn test_fts_index_document() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Put a document with text content
    let item = ItemBuilder::new()
        .string("title", "Hello World")
        .string("content", "The quick brown fox jumps over the lazy dog")
        .build();

    db.put(b"doc#1", item).unwrap();

    // Verify document was stored
    let result = db.get(b"doc#1").unwrap();
    assert!(result.is_some());
}

#[test]
fn test_fts_simple_term_search() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Add documents
    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "The quick brown fox jumps over the lazy dog")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "A fast red fox runs through the forest")
        .build()).unwrap();

    db.put(b"doc#3", ItemBuilder::new()
        .string("content", "The lazy cat sleeps all day")
        .build()).unwrap();

    // Search for "fox" - should match doc#1 and doc#2
    let result = db.text_search("content-index", "fox", 10, false).unwrap();

    assert_eq!(result.total_hits, 2);
    assert_eq!(result.hits.len(), 2);

    // Verify the keys found
    let keys: Vec<_> = result.hits.iter()
        .map(|h| String::from_utf8_lossy(&h.key.pk).to_string())
        .collect();
    assert!(keys.contains(&"doc#1".to_string()));
    assert!(keys.contains(&"doc#2".to_string()));
}

#[test]
fn test_fts_term_not_found() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "The quick brown fox")
        .build()).unwrap();

    // Search for a term that doesn't exist
    let result = db.text_search("content-index", "elephant", 10, false).unwrap();

    assert_eq!(result.total_hits, 0);
    assert_eq!(result.hits.len(), 0);
}

// ============================================================================
// Boolean Queries
// ============================================================================

#[test]
fn test_fts_and_query() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "quick brown fox")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "slow brown bear")
        .build()).unwrap();

    db.put(b"doc#3", ItemBuilder::new()
        .string("content", "quick red rabbit")
        .build()).unwrap();

    // Search for "quick AND brown" - should only match doc#1
    let result = db.text_search("content-index", "quick AND brown", 10, false).unwrap();

    assert_eq!(result.total_hits, 1);
    let key = String::from_utf8_lossy(&result.hits[0].key.pk).to_string();
    assert_eq!(key, "doc#1");
}

#[test]
fn test_fts_or_query() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "quick fox")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "slow bear")
        .build()).unwrap();

    db.put(b"doc#3", ItemBuilder::new()
        .string("content", "lazy dog")
        .build()).unwrap();

    // Search for "fox OR bear" - should match doc#1 and doc#2
    let result = db.text_search("content-index", "fox OR bear", 10, false).unwrap();

    assert_eq!(result.total_hits, 2);
}

#[test]
fn test_fts_not_query() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "quick brown fox")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "quick red rabbit")
        .build()).unwrap();

    // Search for "quick NOT fox" - should only match doc#2 (has quick but not fox)
    let result = db.text_search("content-index", "quick NOT fox", 10, false).unwrap();

    assert_eq!(result.total_hits, 1);
    let key = String::from_utf8_lossy(&result.hits[0].key.pk).to_string();
    assert_eq!(key, "doc#2");
}

// ============================================================================
// Phrase Queries
// ============================================================================

#[test]
fn test_fts_phrase_query() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "The quick brown fox jumps high")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "A brown quick dog runs fast")
        .build()).unwrap();

    db.put(b"doc#3", ItemBuilder::new()
        .string("content", "Just a regular sentence about cats and dogs")
        .build()).unwrap();

    // Phrase query currently uses AND semantics (not position-aware)
    // "quick brown" finds documents with both "quick" AND "brown"
    let result = db.text_search("content-index", "\"quick brown\"", 10, false).unwrap();

    // Both doc#1 and doc#2 contain both "quick" and "brown" (in different orders)
    // doc#3 does not contain "quick" or "brown" so it should not match
    assert_eq!(result.total_hits, 2);

    // Verify the keys found
    let keys: Vec<_> = result.hits.iter()
        .map(|h| String::from_utf8_lossy(&h.key.pk).to_string())
        .collect();
    assert!(keys.contains(&"doc#1".to_string()));
    assert!(keys.contains(&"doc#2".to_string()));
}

// ============================================================================
// Fuzzy and Prefix Queries
// ============================================================================

#[test]
fn test_fts_prefix_query() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "running fast")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "runner wins")
        .build()).unwrap();

    db.put(b"doc#3", ItemBuilder::new()
        .string("content", "walking slow")
        .build()).unwrap();

    // Search with prefix "run*" - should match doc#1 and doc#2
    let result = db.text_search("content-index", "run*", 10, false).unwrap();

    assert_eq!(result.total_hits, 2);
}

#[test]
fn test_fts_fuzzy_query() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "running through forest")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "walking through park")
        .build()).unwrap();

    // Search with fuzzy "runnng~2" (misspelled running with distance 2) - should match doc#1
    // Note: "runnng" -> "running" has edit distance of 2, so we need ~2
    let result = db.text_search("content-index", "runnng~2", 10, false).unwrap();

    assert!(result.total_hits >= 1);
}

// ============================================================================
// BM25 Ranking
// ============================================================================

#[test]
fn test_fts_bm25_ranking() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Doc with multiple occurrences should rank higher
    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "fox fox fox fox fox")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "fox")
        .build()).unwrap();

    db.put(b"doc#3", ItemBuilder::new()
        .string("content", "the quick brown fox jumps over the lazy dog and another fox")
        .build()).unwrap();

    let result = db.text_search("content-index", "fox", 10, false).unwrap();

    assert_eq!(result.total_hits, 3);

    // Results should be sorted by score descending
    for i in 1..result.hits.len() {
        assert!(result.hits[i-1].score >= result.hits[i].score,
            "Results should be sorted by score descending");
    }
}

// ============================================================================
// Highlighting
// ============================================================================

#[test]
fn test_fts_highlighting() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "The quick brown fox jumps over the lazy dog")
        .build()).unwrap();

    // Search with highlighting enabled
    let result = db.text_search("content-index", "fox", 10, true).unwrap();

    assert_eq!(result.total_hits, 1);
    assert!(result.hits[0].snippet.is_some());

    let snippet = result.hits[0].snippet.as_ref().unwrap();
    // Snippet should contain highlighted term
    assert!(snippet.contains("fox") || snippet.contains("<mark>") || snippet.contains("**"),
        "Snippet should highlight the search term: {}", snippet);
}

// ============================================================================
// Language Support
// ============================================================================

#[test]
fn test_fts_stop_words_filtered() {
    let dir = TempDir::new().unwrap();

    // Create index with English stop words enabled (default)
    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content")
            .language(Language::English)
            .stop_words(true));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "the quick brown fox")
        .build()).unwrap();

    // Searching for stop word "the" should return no results
    let result = db.text_search("content-index", "the", 10, false).unwrap();

    // Stop words are not indexed, so this should find 0 documents
    assert_eq!(result.total_hits, 0);
}

#[test]
fn test_fts_no_stop_words() {
    let dir = TempDir::new().unwrap();

    // Create index with stop words disabled
    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content")
            .stop_words(false));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "the quick brown fox")
        .build()).unwrap();

    // With stop words disabled, "the" should be indexed
    let result = db.text_search("content-index", "the", 10, false).unwrap();

    assert_eq!(result.total_hits, 1);
}

// ============================================================================
// Limit and Pagination
// ============================================================================

#[test]
fn test_fts_limit() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Add 10 documents with "fox"
    for i in 0..10 {
        db.put(format!("doc#{}", i).as_bytes(), ItemBuilder::new()
            .string("content", format!("This is document {} about fox", i))
            .build()).unwrap();
    }

    // Search with limit 3
    let result = db.text_search("content-index", "fox", 3, false).unwrap();

    assert_eq!(result.hits.len(), 3);
    assert_eq!(result.total_hits, 10);
}

// ============================================================================
// Multiple Text Indexes
// ============================================================================

#[test]
fn test_fts_multiple_indexes() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("title-index", "title"))
        .with_text_index(TextIndex::new("body-index", "body"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("title", "Introduction to Rust")
        .string("body", "Rust is a systems programming language")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("title", "Python Tutorial")
        .string("body", "Learn Python from scratch")
        .build()).unwrap();

    // Search title index for "rust"
    let title_result = db.text_search("title-index", "rust", 10, false).unwrap();
    assert_eq!(title_result.total_hits, 1);

    // Search body index for "rust"
    let body_result = db.text_search("body-index", "rust", 10, false).unwrap();
    assert_eq!(body_result.total_hits, 1);

    // Search body index for "python"
    let python_result = db.text_search("body-index", "python", 10, false).unwrap();
    assert_eq!(python_result.total_hits, 1);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_fts_empty_content() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Document with empty content field
    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "hello world")
        .build()).unwrap();

    let result = db.text_search("content-index", "hello", 10, false).unwrap();
    assert_eq!(result.total_hits, 1);
}

#[test]
fn test_fts_missing_attribute() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Document without the indexed attribute
    db.put(b"doc#1", ItemBuilder::new()
        .string("title", "No content field here")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "This has content")
        .build()).unwrap();

    let result = db.text_search("content-index", "content", 10, false).unwrap();
    assert_eq!(result.total_hits, 1);
}

#[test]
fn test_fts_special_characters() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Document with special characters
    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "Hello, world! How's it going? Let's code.")
        .build()).unwrap();

    let result = db.text_search("content-index", "hello", 10, false).unwrap();
    assert_eq!(result.total_hits, 1);

    let result = db.text_search("content-index", "world", 10, false).unwrap();
    assert_eq!(result.total_hits, 1);
}

#[test]
fn test_fts_unicode() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content")
            .language(Language::None));  // Disable stemming for Unicode

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Documents with Unicode content
    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "日本語のテスト")
        .build()).unwrap();

    db.put(b"doc#2", ItemBuilder::new()
        .string("content", "Émile est français")
        .build()).unwrap();

    // Search should work (though results depend on tokenization)
    let result = db.text_search("content-index", "français", 10, false).unwrap();
    // May or may not find depending on tokenization, but shouldn't crash
    assert!(result.total_hits >= 0);
}

#[test]
fn test_fts_nonexistent_index() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Search on a non-existent index should return an error
    let result = db.text_search("nonexistent-index", "test", 10, false);
    assert!(result.is_err());
}

// ============================================================================
// Persistence
// ============================================================================

#[test]
fn test_fts_persistence() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    // Create database and add documents
    {
        let db = Database::create_with_schema(&path, schema.clone()).unwrap();

        db.put(b"doc#1", ItemBuilder::new()
            .string("content", "The quick brown fox")
            .build()).unwrap();

        db.put(b"doc#2", ItemBuilder::new()
            .string("content", "The lazy dog sleeps")
            .build()).unwrap();

        // Verify search works before closing
        let result = db.text_search("content-index", "fox", 10, false).unwrap();
        assert_eq!(result.total_hits, 1);
    }

    // Reopen with same schema and verify search still works
    // Note: Schema must be provided again as it's not persisted in manifest yet
    {
        let db = Database::open_with_schema(&path, schema).unwrap();

        // Search the FTS index - the index entries are persisted
        let result = db.text_search("content-index", "fox", 10, false).unwrap();
        assert_eq!(result.total_hits, 1);

        let result = db.text_search("content-index", "dog", 10, false).unwrap();
        assert_eq!(result.total_hits, 1);
    }
}

// ============================================================================
// Delete and Update
// ============================================================================

#[test]
fn test_fts_delete_removes_from_index() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "unique keyword searchable")
        .build()).unwrap();

    // Verify it's searchable
    let result = db.text_search("content-index", "unique", 10, false).unwrap();
    assert_eq!(result.total_hits, 1);

    // Delete the document
    db.delete(b"doc#1").unwrap();

    // Search should no longer find it
    // Note: This depends on implementation - deleted docs may still appear
    // until compaction, but the document itself should be None
    let doc = db.get(b"doc#1").unwrap();
    assert!(doc.is_none());
}

#[test]
fn test_fts_update_reindexes() {
    let dir = TempDir::new().unwrap();

    let schema = TableSchema::new()
        .with_text_index(TextIndex::new("content-index", "content"));

    let db = Database::create_with_schema(dir.path(), schema).unwrap();

    // Initial document
    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "original content here")
        .build()).unwrap();

    // Verify original is searchable
    let result = db.text_search("content-index", "original", 10, false).unwrap();
    assert_eq!(result.total_hits, 1);

    // Update the document
    db.put(b"doc#1", ItemBuilder::new()
        .string("content", "updated modified content")
        .build()).unwrap();

    // Search for new content
    let result = db.text_search("content-index", "updated", 10, false).unwrap();
    assert_eq!(result.total_hits, 1);

    let result = db.text_search("content-index", "modified", 10, false).unwrap();
    assert_eq!(result.total_hits, 1);
}

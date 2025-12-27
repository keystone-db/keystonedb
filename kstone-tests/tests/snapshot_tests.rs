//! Snapshot tests using insta
//!
//! These tests capture the output of complex operations and compare them
//! against saved snapshots. This catches unintended changes in:
//! - Error message formats
//! - Database statistics output
//! - Query result structures
//! - Serialization formats

use insta::{assert_debug_snapshot, assert_snapshot, assert_yaml_snapshot};
use kstone_api::{Database, ItemBuilder, Query, Scan, BatchGetRequest, BatchWriteRequest};
use kstone_core::{Key, Value};
use serde::Serialize;
use std::collections::BTreeMap;
use tempfile::TempDir;

// ===========================================
// Stable structs for deterministic snapshots
// ===========================================

#[derive(Debug, Serialize)]
struct StableStats {
    has_wal: bool,
    total_sst_files: u64,
    has_keys: bool,
}

#[derive(Debug, Serialize)]
struct StableQueryResponse {
    item_count: usize,
    has_last_key: bool,
}

#[derive(Debug, Serialize)]
struct StableScanResponse {
    item_count: usize,
    has_last_key: bool,
}

#[derive(Debug, Serialize)]
struct KeySnapshot {
    pk_len: usize,
    sk_len: Option<usize>,
    stripe: u32,
}

#[derive(Debug, Serialize)]
struct StableBatchGetResponse {
    found_count: usize,
    keys_found: Vec<String>,
}

#[derive(Debug, Serialize)]
struct StableBatchWriteResponse {
    processed_count: usize,
}

// ===========================================
// Error Message Snapshots
// ===========================================

#[test]
fn snapshot_error_not_found() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let result = db.get(b"nonexistent_key").unwrap();
    assert_debug_snapshot!("error_key_not_found", result);
}

#[test]
fn snapshot_error_invalid_path() {
    let result = Database::open("/nonexistent/path/to/database");
    let error_msg = result.err().map(|e| format!("{}", e));
    assert_snapshot!("error_invalid_path", error_msg.unwrap_or_default());
}

// ===========================================
// Database Stats Snapshots
// ===========================================

#[test]
fn snapshot_empty_database_stats() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let stats = db.stats().unwrap();
    let stable_stats = StableStats {
        has_wal: stats.wal_size_bytes.is_some(),
        total_sst_files: stats.total_sst_files,
        has_keys: stats.total_keys.is_some(),
    };
    assert_yaml_snapshot!("empty_database_stats", stable_stats);
}

#[test]
fn snapshot_database_stats_after_writes() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Write some data
    for i in 0..100 {
        let item = ItemBuilder::new()
            .string("name", format!("item_{}", i))
            .number("index", i)
            .build();
        db.put(format!("key_{}", i).as_bytes(), item).unwrap();
    }
    db.flush().unwrap();

    let stats = db.stats().unwrap();
    let stable_stats = StableStats {
        has_wal: stats.wal_size_bytes.is_some(),
        total_sst_files: stats.total_sst_files,
        has_keys: stats.total_keys.is_some(),
    };
    assert_yaml_snapshot!("database_stats_after_writes", stable_stats);
}

// ===========================================
// Item Structure Snapshots
// ===========================================

#[test]
fn snapshot_item_with_all_types() {
    let item = ItemBuilder::new()
        .string("string_field", "hello world")
        .number("number_field", 42)
        .number("negative_number", -123)
        .number("large_number", 9999999999i64)
        .bool("bool_true", true)
        .bool("bool_false", false)
        .build();

    // Sort keys for stable output
    let sorted: BTreeMap<_, _> = item.into_iter().collect();
    assert_yaml_snapshot!("item_with_all_types", sorted);
}

#[test]
fn snapshot_item_with_special_strings() {
    let item = ItemBuilder::new()
        .string("empty", "")
        .string("spaces", "  spaces  ")
        .string("newlines", "line1\nline2\nline3")
        .string("unicode", "Hello 世界 🌍")
        .string("quotes", r#"He said "hello""#)
        .build();

    let sorted: BTreeMap<_, _> = item.into_iter().collect();
    assert_yaml_snapshot!("item_with_special_strings", sorted);
}

// ===========================================
// Query Response Snapshots
// ===========================================

#[test]
fn snapshot_query_response_empty() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let query = Query::new(b"nonexistent_pk");
    let response = db.query(query).unwrap();

    let stable_response = StableQueryResponse {
        item_count: response.items.len(),
        has_last_key: response.last_key.is_some(),
    };
    assert_yaml_snapshot!("query_response_empty", stable_response);
}

#[test]
fn snapshot_query_response_with_items() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Insert items with sort keys
    for i in 0..5 {
        let item = ItemBuilder::new()
            .string("title", format!("Post {}", i))
            .number("likes", i * 10)
            .build();
        db.put_with_sk(b"user#123", format!("post#{:03}", i).as_bytes(), item)
            .unwrap();
    }

    let query = Query::new(b"user#123");
    let response = db.query(query).unwrap();

    let stable_response = StableQueryResponse {
        item_count: response.items.len(),
        has_last_key: response.last_key.is_some(),
    };
    assert_yaml_snapshot!("query_response_with_items", stable_response);
}

#[test]
fn snapshot_query_with_limit() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    for i in 0..10 {
        let item = ItemBuilder::new().number("i", i).build();
        db.put_with_sk(b"pk", format!("sk_{:02}", i).as_bytes(), item)
            .unwrap();
    }

    let query = Query::new(b"pk").limit(3);
    let response = db.query(query).unwrap();

    let stable_response = StableQueryResponse {
        item_count: response.items.len(),
        has_last_key: response.last_key.is_some(),
    };
    assert_yaml_snapshot!("query_with_limit", stable_response);
}

// ===========================================
// Scan Response Snapshots
// ===========================================

#[test]
fn snapshot_scan_response_empty() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let scan = Scan::new();
    let response = db.scan(scan).unwrap();

    let stable_response = StableScanResponse {
        item_count: response.items.len(),
        has_last_key: response.last_key.is_some(),
    };
    assert_yaml_snapshot!("scan_response_empty", stable_response);
}

#[test]
fn snapshot_scan_with_limit() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    for i in 0..20 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(format!("key_{:02}", i).as_bytes(), item).unwrap();
    }

    let scan = Scan::new().limit(5);
    let response = db.scan(scan).unwrap();

    let stable_response = StableScanResponse {
        item_count: response.items.len(),
        has_last_key: response.last_key.is_some(),
    };
    assert_yaml_snapshot!("scan_with_limit", stable_response);
}

// ===========================================
// Key Encoding Snapshots
// ===========================================

#[test]
fn snapshot_key_formats() {
    let keys = vec![
        ("simple", Key::new(b"simple_key".to_vec())),
        ("with_hash", Key::new(b"user#123".to_vec())),
        (
            "composite",
            Key::with_sk(b"pk".to_vec(), b"sk".to_vec()),
        ),
        (
            "composite_complex",
            Key::with_sk(b"user#123".to_vec(), b"post#2024-01-01".to_vec()),
        ),
    ];

    let encoded: Vec<_> = keys
        .into_iter()
        .map(|(name, key)| {
            (
                name,
                KeySnapshot {
                    pk_len: key.pk.len(),
                    sk_len: key.sk.as_ref().map(|s| s.len()),
                    stripe: key.stripe() as u32,
                },
            )
        })
        .collect();

    assert_yaml_snapshot!("key_formats", encoded);
}

// ===========================================
// Value Type Snapshots
// ===========================================

#[test]
fn snapshot_value_types() {
    let values: Vec<(&str, String)> = vec![
        ("string", format!("{:?}", Value::S("hello".to_string()))),
        ("number", format!("{:?}", Value::N("42".to_string()))),
        (
            "number_decimal",
            format!("{:?}", Value::N("3.14159".to_string())),
        ),
        (
            "number_negative",
            format!("{:?}", Value::N("-999".to_string())),
        ),
        ("bool_true", format!("{:?}", Value::Bool(true))),
        ("bool_false", format!("{:?}", Value::Bool(false))),
        ("null", format!("{:?}", Value::Null)),
        (
            "binary",
            format!("{:?}", Value::B(bytes::Bytes::from_static(b"binary data"))),
        ),
        ("list_empty", format!("{:?}", Value::L(vec![]))),
        (
            "list_mixed",
            format!(
                "{:?}",
                Value::L(vec![
                    Value::S("a".to_string()),
                    Value::N("1".to_string()),
                    Value::Bool(true),
                ])
            ),
        ),
        ("map_empty", format!("{:?}", Value::M(Default::default()))),
        ("timestamp", format!("{:?}", Value::Ts(1704067200000))),
        (
            "vector",
            format!("{:?}", Value::VecF32(vec![1.0, 2.0, 3.0])),
        ),
    ];

    assert_yaml_snapshot!("value_types", values);
}

// ===========================================
// Batch Operation Snapshots
// ===========================================

#[test]
fn snapshot_batch_get_response() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Insert some items
    for i in 0..5 {
        let item = ItemBuilder::new().number("id", i).build();
        db.put(format!("item_{}", i).as_bytes(), item).unwrap();
    }

    // Batch get - some exist, some don't
    let request = BatchGetRequest::new()
        .add_key(b"item_0")
        .add_key(b"item_2")
        .add_key(b"item_99") // doesn't exist
        .add_key(b"item_4");

    let response = db.batch_get(request).unwrap();

    let mut keys_found: Vec<String> = response
        .items
        .keys()
        .map(|k| String::from_utf8_lossy(&k.pk).to_string())
        .collect();
    keys_found.sort(); // Sort for deterministic output

    let stable = StableBatchGetResponse {
        found_count: response.items.len(),
        keys_found,
    };
    assert_yaml_snapshot!("batch_get_response", stable);
}

#[test]
fn snapshot_batch_write_response() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let request = BatchWriteRequest::new()
        .put(b"key_1", ItemBuilder::new().string("a", "1").build())
        .put(b"key_2", ItemBuilder::new().string("b", "2").build())
        .put(b"key_3", ItemBuilder::new().string("c", "3").build());

    let response = db.batch_write(request).unwrap();

    let stable = StableBatchWriteResponse {
        processed_count: response.processed_count,
    };
    assert_yaml_snapshot!("batch_write_response", stable);
}

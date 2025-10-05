/// Test that database path is now accessible
use kstone_api::{Database, ItemBuilder};
use tempfile::TempDir;

#[test]
fn test_database_path_is_accessible() {
    // Create a temporary directory
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.keystone");

    // Create a database
    let db = Database::create(&db_path).unwrap();

    // Test that path() returns the correct path
    if let Some(path) = db.path() {
        assert_eq!(path, db_path, "Path should match the database path");
    } else {
        panic!("Database path() returned None");
    }
}

#[test]
fn test_scan_with_keys_works() {
    // Create a temporary directory
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.keystone");

    // Create a database
    let db = Database::create(&db_path).unwrap();

    // Add some test data
    db.put(b"key1", ItemBuilder::new()
        .string("name", "Item 1")
        .build()).unwrap();

    db.put(b"key2", ItemBuilder::new()
        .string("name", "Item 2")
        .build()).unwrap();

    db.put(b"key3", ItemBuilder::new()
        .string("name", "Item 3")
        .build()).unwrap();

    // Scan with keys
    let results = db.scan_with_keys(10).unwrap();

    // Should have 3 items
    assert_eq!(results.len(), 3, "Should have 3 items");

    // Verify we have both keys and items
    for (key, item) in &results {
        assert!(!key.pk.is_empty(), "Key should not be empty");
        assert!(item.contains_key("name"), "Item should have name field");
    }
}

#[test]
fn test_scan_with_keys_limit() {
    // Create a temporary directory
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.keystone");

    // Create a database
    let db = Database::create(&db_path).unwrap();

    // Add more test data
    for i in 0..10 {
        let key = format!("key{}", i);
        db.put(key.as_bytes(), ItemBuilder::new()
            .string("name", format!("Item {}", i))
            .build()).unwrap();
    }

    // Scan with limit of 5
    let results = db.scan_with_keys(5).unwrap();

    // Should respect limit
    assert_eq!(results.len(), 5, "Should return only 5 items with limit");
}
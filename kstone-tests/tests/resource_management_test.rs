use kstone_api::{Database, ItemBuilder, DatabaseConfig};
use kstone_core::Error;
use tempfile::TempDir;

#[test]
fn test_memtable_record_limit() {
    let dir = TempDir::new().unwrap();

    // Create database with small record limit
    let config = DatabaseConfig::new().with_max_memtable_records(10);
    let db = Database::create_with_config(dir.path(), config).unwrap();

    // Insert 9 items - should not flush yet
    for i in 0..9 {
        let item = ItemBuilder::new()
            .string("name", format!("Item {}", i))
            .number("value", i)
            .build();
        db.put(format!("key#{}", i).as_bytes(), item).unwrap();
    }

    // Insert 10th item - should trigger flush
    let item = ItemBuilder::new()
        .string("name", "Item 9")
        .number("value", 9)
        .build();
    db.put(b"key#9", item).unwrap();

    // All items should still be retrievable
    for i in 0..10 {
        let key = format!("key#{}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Item {} should exist", i);
    }
}

#[test]
fn test_memtable_byte_size_limit() {
    let dir = TempDir::new().unwrap();

    // Create database with small byte size limit (10KB)
    let config = DatabaseConfig::new()
        .with_max_memtable_size_bytes(10 * 1024)
        .with_max_memtable_records(1000); // High record count so byte limit triggers first

    let db = Database::create_with_config(dir.path(), config).unwrap();

    // Insert large items until we hit the byte limit
    // Each item has a ~1KB string
    let large_value = "x".repeat(1024);

    for i in 0..20 {
        let item = ItemBuilder::new()
            .string("data", &large_value)
            .number("id", i)
            .build();

        db.put(format!("large#{}", i).as_bytes(), item).unwrap();
    }

    // All items should be retrievable
    for i in 0..20 {
        let key = format!("large#{}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Large item {} should exist", i);
    }
}

#[test]
fn test_config_validation_zero_records() {
    let config = DatabaseConfig::new().with_max_memtable_records(0);
    let validation = config.validate();
    assert!(validation.is_err());
    assert!(validation.unwrap_err().contains("max_memtable_records"));
}

#[test]
fn test_config_validation_zero_buffer_size() {
    let config = DatabaseConfig::new().with_write_buffer_size(0);
    let validation = config.validate();
    assert!(validation.is_err());
    assert!(validation.unwrap_err().contains("write_buffer_size"));
}

#[test]
fn test_config_validation_success() {
    let config = DatabaseConfig::new()
        .with_max_memtable_records(500)
        .with_max_memtable_size_bytes(1024 * 1024)
        .with_write_buffer_size(2048);

    assert!(config.validate().is_ok());
}

#[test]
fn test_default_config_behavior() {
    let dir = TempDir::new().unwrap();

    // Create database with default config
    let db = Database::create(dir.path()).unwrap();

    // Should use default threshold of 1000 records
    // Insert 999 items - should not flush
    for i in 0..999 {
        let item = ItemBuilder::new()
            .number("value", i)
            .build();
        db.put(format!("item#{:04}", i).as_bytes(), item).unwrap();
    }

    // Insert 1000th item - should trigger flush
    let item = ItemBuilder::new().number("value", 999).build();
    db.put(b"item#0999", item).unwrap();

    // All items should be retrievable
    for i in 0..1000 {
        let key = format!("item#{:04}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Item {} should exist", i);
    }
}

#[test]
fn test_create_with_invalid_config() {
    let dir = TempDir::new().unwrap();

    // Try to create with invalid config (zero records)
    let config = DatabaseConfig::new().with_max_memtable_records(0);
    let result = Database::create_with_config(dir.path(), config);

    assert!(result.is_err());
    match result {
        Err(Error::InvalidArgument(msg)) => {
            assert!(msg.contains("max_memtable_records"));
        }
        _ => panic!("Expected InvalidArgument error"),
    }
}

#[test]
fn test_memtable_size_tracking_with_updates() {
    let dir = TempDir::new().unwrap();

    // Small limit to test easily
    let config = DatabaseConfig::new()
        .with_max_memtable_size_bytes(5 * 1024) // 5KB
        .with_max_memtable_records(1000);

    let db = Database::create_with_config(dir.path(), config).unwrap();

    let large_value = "x".repeat(512);

    // Insert and update the same keys multiple times
    for round in 0..3 {
        for i in 0..10 {
            let item = ItemBuilder::new()
                .string("data", &large_value)
                .number("round", round)
                .number("id", i)
                .build();

            db.put(format!("update#{}", i).as_bytes(), item).unwrap();
        }
    }

    // Verify latest values
    for i in 0..10 {
        let key = format!("update#{}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some());

        let item = result.unwrap();
        if let Some(kstone_core::Value::N(round)) = item.get("round") {
            assert_eq!(round, "2", "Should have latest round value");
        } else {
            panic!("Expected round number");
        }
    }
}

#[test]
fn test_mixed_small_and_large_items() {
    let dir = TempDir::new().unwrap();

    let config = DatabaseConfig::new()
        .with_max_memtable_size_bytes(50 * 1024) // 50KB
        .with_max_memtable_records(100);

    let db = Database::create_with_config(dir.path(), config).unwrap();

    // Insert small items
    for i in 0..50 {
        let item = ItemBuilder::new()
            .string("type", "small")
            .number("id", i)
            .build();
        db.put(format!("small#{}", i).as_bytes(), item).unwrap();
    }

    // Insert large items (should trigger size-based flush)
    let large_value = "y".repeat(2048);
    for i in 0..30 {
        let item = ItemBuilder::new()
            .string("type", "large")
            .string("data", &large_value)
            .number("id", i)
            .build();
        db.put(format!("large#{}", i).as_bytes(), item).unwrap();
    }

    // Verify all items
    for i in 0..50 {
        let key = format!("small#{}", i);
        assert!(db.get(key.as_bytes()).unwrap().is_some());
    }

    for i in 0..30 {
        let key = format!("large#{}", i);
        assert!(db.get(key.as_bytes()).unwrap().is_some());
    }
}

#[test]
fn test_config_builder_pattern() {
    let config = DatabaseConfig::new()
        .with_max_memtable_records(500)
        .with_max_memtable_size_bytes(2 * 1024 * 1024)
        .with_max_wal_size_bytes(10 * 1024 * 1024)
        .with_max_total_disk_bytes(100 * 1024 * 1024)
        .with_write_buffer_size(4096);

    assert_eq!(config.max_memtable_records, 500);
    assert_eq!(config.max_memtable_size_bytes, Some(2 * 1024 * 1024));
    assert_eq!(config.max_wal_size_bytes, Some(10 * 1024 * 1024));
    assert_eq!(config.max_total_disk_bytes, Some(100 * 1024 * 1024));
    assert_eq!(config.write_buffer_size, 4096);
}

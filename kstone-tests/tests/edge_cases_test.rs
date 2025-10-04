/// Edge cases and boundary condition tests for KeystoneDB

use kstone_api::{Database, ItemBuilder};
use kstone_core::Key;
use tempfile::TempDir;

#[test]
fn test_empty_database() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Get from empty database
    let result = db.get(b"nonexistent").unwrap();
    assert!(result.is_none());

    // Delete from empty database (should succeed)
    db.delete(b"nonexistent").unwrap();
}

#[test]
fn test_single_record() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let item = ItemBuilder::new().string("data", "solo").build();

    // Single put
    db.put(b"only", item.clone()).unwrap();

    // Single get
    let result = db.get(b"only").unwrap();
    assert_eq!(result, Some(item.clone()));

    // Delete the only record
    db.delete(b"only").unwrap();
    let result = db.get(b"only").unwrap();
    assert!(result.is_none());

    // Database should be empty again
    let result = db.get(b"anything").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_very_large_keys() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // 1KB key
    let large_key = vec![b'x'; 1024];
    let item = ItemBuilder::new().string("data", "large key test").build();

    db.put(&large_key, item.clone()).unwrap();

    let result = db.get(&large_key).unwrap();
    assert_eq!(result, Some(item));
}

#[test]
fn test_very_large_values() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // 1MB value
    let large_string = "x".repeat(1024 * 1024);
    let item = ItemBuilder::new()
        .string("large_data", &large_string)
        .build();

    db.put(b"large_value", item.clone()).unwrap();

    let result = db.get(b"large_value").unwrap();
    assert_eq!(result, Some(item));
}

#[test]
fn test_empty_string_key() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let item = ItemBuilder::new().string("data", "empty key").build();

    // Empty key should work
    db.put(b"", item.clone()).unwrap();

    let result = db.get(b"").unwrap();
    assert_eq!(result, Some(item));
}

#[test]
fn test_empty_item() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Item with no attributes
    let empty_item = ItemBuilder::new().build();

    db.put(b"empty", empty_item.clone()).unwrap();

    let result = db.get(b"empty").unwrap();
    assert_eq!(result, Some(empty_item));
}

#[test]
fn test_special_characters_in_keys() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let special_keys = vec![
        b"user#123".to_vec(),
        b"item:456".to_vec(),
        b"data/789".to_vec(),
        b"key@email.com".to_vec(),
        b"key with spaces".to_vec(),
        b"key\twith\ttabs".to_vec(),
        b"key\nwith\nnewlines".to_vec(),
        vec![0u8, 1, 2, 255], // Binary data
    ];

    for (i, key) in special_keys.iter().enumerate() {
        let item = ItemBuilder::new()
            .number("index", i as i64)
            .build();

        db.put(key, item.clone()).unwrap();

        let result = db.get(key).unwrap();
        assert_eq!(result, Some(item));
    }
}

#[test]
fn test_sequential_keys() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Sequential numeric keys (good for testing ordering)
    for i in 0..1000 {
        let key = format!("{:010}", i); // Zero-padded
        let item = ItemBuilder::new().number("value", i).build();
        db.put(key.as_bytes(), item).unwrap();
    }

    // Verify all
    for i in 0..1000 {
        let key = format!("{:010}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some());
    }
}

#[test]
fn test_reverse_sequential_keys() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Write in reverse order
    for i in (0..1000).rev() {
        let key = format!("key{}", i);
        let item = ItemBuilder::new().number("value", i).build();
        db.put(key.as_bytes(), item).unwrap();
    }

    // Verify all
    for i in 0..1000 {
        let key = format!("key{}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some());
    }
}

#[test]
fn test_random_access_pattern() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Write with pseudo-random keys
    for i in 0..1000 {
        let key = format!("key{}", i * 7919 % 10000); // Prime number for distribution
        let item = ItemBuilder::new().number("value", i).build();
        db.put(key.as_bytes(), item).unwrap();
    }

    // Read back with same pattern
    for i in 0..1000 {
        let key = format!("key{}", i * 7919 % 10000);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some());
    }
}

#[test]
fn test_many_deletes() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Write 1000 items
    for i in 0..1000 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    // Delete all of them
    for i in 0..1000 {
        db.delete(format!("key{}", i).as_bytes()).unwrap();
    }

    // Verify all deleted
    for i in 0..1000 {
        let result = db.get(format!("key{}", i).as_bytes()).unwrap();
        assert!(result.is_none());
    }
}

#[test]
fn test_alternating_put_delete() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let item = ItemBuilder::new().string("data", "test").build();

    // Alternating put and delete on same key
    for _ in 0..100 {
        db.put(b"flip", item.clone()).unwrap();
        let result = db.get(b"flip").unwrap();
        assert!(result.is_some());

        db.delete(b"flip").unwrap();
        let result = db.get(b"flip").unwrap();
        assert!(result.is_none());
    }
}

#[test]
fn test_multiple_flushes() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Trigger multiple flushes
    for round in 0..5 {
        for i in 0..1500 {
            let key = format!("round{}:key{}", round, i);
            let item = ItemBuilder::new()
                .number("round", round)
                .number("index", i)
                .build();
            db.put(key.as_bytes(), item).unwrap();
        }
        db.flush().unwrap();
    }

    // Verify all records across all rounds
    for round in 0..5 {
        for i in 0..1500 {
            let key = format!("round{}:key{}", round, i);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some(), "Missing {}", key);
        }
    }
}

#[test]
fn test_in_memory_edge_cases() {
    let db = Database::create_in_memory().unwrap();

    // Empty database
    let result = db.get(b"none").unwrap();
    assert!(result.is_none());

    // Single record
    let item = ItemBuilder::new().string("data", "memory").build();
    db.put(b"only", item.clone()).unwrap();
    let result = db.get(b"only").unwrap();
    assert_eq!(result, Some(item));

    // Large batch in memory
    for i in 0..5000 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(format!("mem{}", i).as_bytes(), item).unwrap();
    }

    // Verify
    for i in 0..5000 {
        let result = db.get(format!("mem{}", i).as_bytes()).unwrap();
        assert!(result.is_some());
    }
}

#[test]
fn test_sort_key_edge_cases() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let pk = b"user#123";

    // Empty sort key
    let item1 = ItemBuilder::new().string("data", "empty sk").build();
    db.put_with_sk(pk, b"", item1.clone()).unwrap();
    let result = db.get_with_sk(pk, b"").unwrap();
    assert_eq!(result, Some(item1));

    // Very long sort key
    let long_sk = vec![b'y'; 1024];
    let item2 = ItemBuilder::new().string("data", "long sk").build();
    db.put_with_sk(pk, &long_sk, item2.clone()).unwrap();
    let result = db.get_with_sk(pk, &long_sk).unwrap();
    assert_eq!(result, Some(item2));

    // Special characters in sort key
    let special_sk = b"sk#with:special/chars";
    let item3 = ItemBuilder::new().string("data", "special sk").build();
    db.put_with_sk(pk, special_sk, item3.clone()).unwrap();
    let result = db.get_with_sk(pk, special_sk).unwrap();
    assert_eq!(result, Some(item3));
}

#[test]
fn test_persistence_edge_cases() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Create, write empty item, close
    {
        let db = Database::create(&path).unwrap();
        let empty = ItemBuilder::new().build();
        db.put(b"empty", empty).unwrap();
    }

    // Reopen and verify
    {
        let db = Database::open(&path).unwrap();
        let result = db.get(b"empty").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 0);
    }
}

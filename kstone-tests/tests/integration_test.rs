use kstone_api::{Database, ItemBuilder};
use tempfile::TempDir;

#[test]
fn test_end_to_end_basic_operations() {
    let dir = TempDir::new().unwrap();

    // Create database
    let db = Database::create(dir.path()).unwrap();

    // Put items
    let alice = ItemBuilder::new()
        .string("name", "Alice")
        .number("age", 30)
        .bool("active", true)
        .build();

    let bob = ItemBuilder::new()
        .string("name", "Bob")
        .number("age", 25)
        .bool("active", false)
        .build();

    db.put(b"user#1", alice.clone()).unwrap();
    db.put(b"user#2", bob.clone()).unwrap();

    // Get items
    let result1 = db.get(b"user#1").unwrap();
    assert_eq!(result1, Some(alice));

    let result2 = db.get(b"user#2").unwrap();
    assert_eq!(result2, Some(bob));

    // Delete item
    db.delete(b"user#1").unwrap();
    let result = db.get(b"user#1").unwrap();
    assert_eq!(result, None);

    // user#2 should still exist
    let result = db.get(b"user#2").unwrap();
    assert!(result.is_some());
}

#[test]
fn test_persistence() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    let item = ItemBuilder::new()
        .string("data", "persistent")
        .number("value", 42)
        .build();

    // Create, write, and close
    {
        let db = Database::create(&path).unwrap();
        db.put(b"key1", item.clone()).unwrap();
        db.put(b"key2", item.clone()).unwrap();
        db.put(b"key3", item.clone()).unwrap();
    }

    // Reopen and verify
    {
        let db = Database::open(&path).unwrap();
        let result = db.get(b"key1").unwrap();
        assert_eq!(result, Some(item.clone()));

        let result = db.get(b"key2").unwrap();
        assert_eq!(result, Some(item.clone()));

        let result = db.get(b"key3").unwrap();
        assert_eq!(result, Some(item));
    }
}

#[test]
fn test_large_batch() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Write 2000 items (triggers flush)
    for i in 0..2000 {
        let item = ItemBuilder::new()
            .string("id", format!("item{}", i))
            .number("index", i)
            .build();

        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    // Verify all items
    for i in 0..2000 {
        let result = db.get(format!("key{}", i).as_bytes()).unwrap();
        assert!(result.is_some(), "Missing key{}", i);
    }
}

#[test]
fn test_with_sort_keys() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Store posts for a user
    for i in 0..10 {
        let post = ItemBuilder::new()
            .string("title", format!("Post {}", i))
            .string("content", "Lorem ipsum")
            .build();

        db.put_with_sk(b"user#123", format!("post#{}", i).as_bytes(), post)
            .unwrap();
    }

    // Retrieve specific post
    let result = db.get_with_sk(b"user#123", b"post#5").unwrap();
    assert!(result.is_some());
}

#[test]
fn test_overwrite() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let key = b"counter";

    // Write same key multiple times
    for i in 0..100 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(key, item).unwrap();
    }

    // Should have latest value
    let result = db.get(key).unwrap().unwrap();
    let value = result.get("value").unwrap();

    match value {
        kstone_api::KeystoneValue::N(n) => {
            assert_eq!(n, "99");
        }
        _ => panic!("Expected number"),
    }
}

#[test]
fn test_recovery_after_flush() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    {
        let db = Database::create(&path).unwrap();

        // Write enough to trigger flush
        for i in 0..1100 {
            let item = ItemBuilder::new().number("value", i).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }

        db.flush().unwrap();
    }

    // Reopen and verify
    {
        let db = Database::open(&path).unwrap();

        for i in 0..1100 {
            let result = db.get(format!("key{}", i).as_bytes()).unwrap();
            assert!(result.is_some(), "Missing key{} after recovery", i);
        }
    }
}

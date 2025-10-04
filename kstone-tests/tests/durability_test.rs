/// Durability verification tests for KeystoneDB
///
/// These tests verify that data persists correctly and that durability
/// guarantees are honored across crashes, restarts, and flushes.

use kstone_api::{Database, ItemBuilder};
use tempfile::TempDir;

#[test]
fn test_explicit_flush_durability() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write and flush
    {
        let db = Database::create(&path).unwrap();
        for i in 0..500 {
            let item = ItemBuilder::new().number("value", i).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }
        db.flush().unwrap();
        // Clean close
    }

    // Reopen and verify
    {
        let db = Database::open(&path).unwrap();
        for i in 0..500 {
            let result = db.get(format!("key{}", i).as_bytes()).unwrap();
            assert!(result.is_some(), "Lost key{} after flush", i);
        }
    }
}

#[test]
fn test_wal_durability_without_flush() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write without explicit flush (relies on WAL)
    {
        let db = Database::create(&path).unwrap();
        for i in 0..100 {
            let item = ItemBuilder::new()
                .string("data", format!("value{}", i))
                .build();
            db.put(format!("unflushed{}", i).as_bytes(), item).unwrap();
        }
        // No flush call - WAL should protect us
    }

    // Reopen and verify WAL recovery
    {
        let db = Database::open(&path).unwrap();
        for i in 0..100 {
            let result = db.get(format!("unflushed{}", i).as_bytes()).unwrap();
            assert!(result.is_some(), "Lost unflushed{} from WAL", i);
        }
    }
}

#[test]
fn test_write_ordering_preserved() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write same key multiple times with different values
    {
        let db = Database::create(&path).unwrap();

        for version in 1..=10 {
            let item = ItemBuilder::new()
                .number("version", version)
                .build();
            db.put(b"key", item).unwrap();
        }
    }

    // Verify latest version is what we get
    {
        let db = Database::open(&path).unwrap();
        let result = db.get(b"key").unwrap().unwrap();
        match result.get("version").unwrap() {
            kstone_api::KeystoneValue::N(n) => {
                assert_eq!(n, "10", "Should have latest version");
            }
            _ => panic!("Expected number"),
        }
    }
}

#[test]
fn test_multi_session_durability() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Session 1: Write batch A
    {
        let db = Database::create(&path).unwrap();
        for i in 0..100 {
            let item = ItemBuilder::new().number("batch", 1).build();
            db.put(format!("a:key{}", i).as_bytes(), item).unwrap();
        }
    }

    // Session 2: Write batch B
    {
        let db = Database::open(&path).unwrap();
        for i in 0..100 {
            let item = ItemBuilder::new().number("batch", 2).build();
            db.put(format!("b:key{}", i).as_bytes(), item).unwrap();
        }
    }

    // Session 3: Write batch C
    {
        let db = Database::open(&path).unwrap();
        for i in 0..100 {
            let item = ItemBuilder::new().number("batch", 3).build();
            db.put(format!("c:key{}", i).as_bytes(), item).unwrap();
        }
    }

    // Session 4: Verify all batches
    {
        let db = Database::open(&path).unwrap();
        for i in 0..100 {
            let a = db.get(format!("a:key{}", i).as_bytes()).unwrap();
            let b = db.get(format!("b:key{}", i).as_bytes()).unwrap();
            let c = db.get(format!("c:key{}", i).as_bytes()).unwrap();
            assert!(a.is_some(), "Lost a:key{}", i);
            assert!(b.is_some(), "Lost b:key{}", i);
            assert!(c.is_some(), "Lost c:key{}", i);
        }
    }
}

#[test]
fn test_delete_durability() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write and delete
    {
        let db = Database::create(&path).unwrap();

        // Write 100 items
        for i in 0..100 {
            let item = ItemBuilder::new().number("value", i).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }

        // Delete half of them
        for i in (0..100).step_by(2) {
            db.delete(format!("key{}", i).as_bytes()).unwrap();
        }
    }

    // Verify deletes persisted
    {
        let db = Database::open(&path).unwrap();
        for i in 0..100 {
            let result = db.get(format!("key{}", i).as_bytes()).unwrap();
            if i % 2 == 0 {
                assert!(result.is_none(), "key{} should be deleted", i);
            } else {
                assert!(result.is_some(), "key{} should exist", i);
            }
        }
    }
}

#[test]
fn test_large_value_durability() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    let large_data = "x".repeat(500_000); // 500KB

    // Write large value
    {
        let db = Database::create(&path).unwrap();
        let item = ItemBuilder::new()
            .string("large", &large_data)
            .number("size", 500_000)
            .build();
        db.put(b"large_key", item).unwrap();
    }

    // Verify large value persisted
    {
        let db = Database::open(&path).unwrap();
        let result = db.get(b"large_key").unwrap().unwrap();
        match result.get("large").unwrap() {
            kstone_api::KeystoneValue::S(s) => {
                assert_eq!(s.len(), 500_000);
                assert_eq!(s, &large_data);
            }
            _ => panic!("Expected string"),
        }
    }
}

#[test]
fn test_flush_multiple_times() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Multiple flush cycles in one session
    {
        let db = Database::create(&path).unwrap();

        for round in 0..5 {
            // Write 1500 items (triggers automatic flush)
            for i in 0..1500 {
                let item = ItemBuilder::new()
                    .number("round", round)
                    .number("index", i)
                    .build();
                db.put(format!("r{}:k{}", round, i).as_bytes(), item).unwrap();
            }
            db.flush().unwrap();
        }
    }

    // Verify all rounds persisted
    {
        let db = Database::open(&path).unwrap();
        for round in 0..5 {
            for i in 0..1500 {
                let result = db.get(format!("r{}:k{}", round, i).as_bytes()).unwrap();
                assert!(result.is_some(), "Lost r{}:k{}", round, i);
            }
        }
    }
}

#[test]
fn test_overwrite_durability() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write, overwrite, close
    {
        let db = Database::create(&path).unwrap();

        // Initial values
        for i in 0..200 {
            let item = ItemBuilder::new().number("version", 1).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }

        // Overwrite with v2
        for i in 0..200 {
            let item = ItemBuilder::new().number("version", 2).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }

        // Overwrite with v3
        for i in 0..200 {
            let item = ItemBuilder::new().number("version", 3).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }
    }

    // Verify latest version persisted
    {
        let db = Database::open(&path).unwrap();
        for i in 0..200 {
            let result = db.get(format!("key{}", i).as_bytes()).unwrap().unwrap();
            match result.get("version").unwrap() {
                kstone_api::KeystoneValue::N(n) => {
                    assert_eq!(n, "3", "key{} has wrong version", i);
                }
                _ => panic!("Expected number"),
            }
        }
    }
}

#[test]
fn test_composite_key_durability() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write composite keys
    {
        let db = Database::create(&path).unwrap();

        for pk_id in 0..50 {
            for sk_id in 0..20 {
                let pk = format!("user{}", pk_id);
                let sk = format!("post{}", sk_id);
                let item = ItemBuilder::new()
                    .number("pk_id", pk_id)
                    .number("sk_id", sk_id)
                    .string("content", format!("Content {} {}", pk_id, sk_id))
                    .build();
                db.put_with_sk(pk.as_bytes(), sk.as_bytes(), item).unwrap();
            }
        }
    }

    // Verify all composite keys persisted
    {
        let db = Database::open(&path).unwrap();
        for pk_id in 0..50 {
            for sk_id in 0..20 {
                let pk = format!("user{}", pk_id);
                let sk = format!("post{}", sk_id);
                let result = db.get_with_sk(pk.as_bytes(), sk.as_bytes()).unwrap();
                assert!(result.is_some(), "Lost pk={} sk={}", pk, sk);
            }
        }
    }
}

#[test]
fn test_empty_database_durability() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Create empty database
    {
        let _db = Database::create(&path).unwrap();
        // Close immediately without writing
    }

    // Reopen empty database
    {
        let db = Database::open(&path).unwrap();
        let result = db.get(b"nonexistent").unwrap();
        assert!(result.is_none());

        // Should be able to write after reopening
        let item = ItemBuilder::new().string("data", "first").build();
        db.put(b"key", item).unwrap();
    }

    // Verify the write persisted
    {
        let db = Database::open(&path).unwrap();
        let result = db.get(b"key").unwrap();
        assert!(result.is_some());
    }
}

#[test]
fn test_interleaved_writes_and_reopens() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write, close, write, close pattern
    for batch in 0..10 {
        let db = if batch == 0 {
            Database::create(&path).unwrap()
        } else {
            Database::open(&path).unwrap()
        };

        // Write 50 items for this batch
        for i in 0..50 {
            let item = ItemBuilder::new()
                .number("batch", batch)
                .number("index", i)
                .build();
            db.put(format!("b{}:i{}", batch, i).as_bytes(), item).unwrap();
        }

        // Implicit close by dropping
    }

    // Final verification of all batches
    {
        let db = Database::open(&path).unwrap();
        for batch in 0..10 {
            for i in 0..50 {
                let result = db.get(format!("b{}:i{}", batch, i).as_bytes()).unwrap();
                assert!(result.is_some(), "Lost b{}:i{}", batch, i);
            }
        }
    }
}

#[test]
fn test_mixed_operations_durability() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Mix of puts, deletes, overwrites
    {
        let db = Database::create(&path).unwrap();

        // Initial write
        for i in 0..300 {
            let item = ItemBuilder::new().number("value", i).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }

        // Delete every 3rd
        for i in (0..300).step_by(3) {
            db.delete(format!("key{}", i).as_bytes()).unwrap();
        }

        // Overwrite every 5th
        for i in (0..300).step_by(5) {
            let item = ItemBuilder::new().number("value", i * 100).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }
    }

    // Verify final state
    {
        let db = Database::open(&path).unwrap();
        for i in 0..300 {
            let result = db.get(format!("key{}", i).as_bytes()).unwrap();

            if i % 3 == 0 && i % 5 != 0 {
                // Deleted (and not overwritten)
                assert!(result.is_none(), "key{} should be deleted", i);
            } else {
                // Should exist
                assert!(result.is_some(), "key{} should exist", i);
            }
        }
    }
}

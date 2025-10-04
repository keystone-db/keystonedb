/// Large dataset and stress tests for KeystoneDB
///
/// These tests verify behavior with large volumes of data and
/// ensure proper handling across all 256 stripes.

use kstone_api::{Database, ItemBuilder};
use tempfile::TempDir;

#[test]
fn test_100k_records() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    println!("Writing 100,000 records...");
    for i in 0..100_000 {
        let key = format!("key{:08}", i);
        let item = ItemBuilder::new()
            .number("index", i)
            .string("data", format!("value{}", i))
            .build();
        db.put(key.as_bytes(), item).unwrap();

        if i % 10_000 == 0 {
            println!("  Written {} records", i);
        }
    }

    println!("Verifying 100,000 records...");
    for i in 0..100_000 {
        let key = format!("key{:08}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Missing key at index {}", i);

        if i % 10_000 == 0 {
            println!("  Verified {} records", i);
        }
    }

    println!("Test complete!");
}

#[test]
#[ignore] // Long-running test, run with --ignored
fn test_1m_records() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    println!("Writing 1,000,000 records...");
    for i in 0..1_000_000 {
        let key = format!("key{:010}", i);
        let item = ItemBuilder::new()
            .number("index", i)
            .bool("active", i % 2 == 0)
            .build();
        db.put(key.as_bytes(), item).unwrap();

        if i % 100_000 == 0 {
            println!("  Written {} records", i);
        }
    }

    println!("Flushing...");
    db.flush().unwrap();

    println!("Verifying 1,000,000 records...");
    for i in (0..1_000_000).step_by(1000) {
        // Sample verification (every 1000th record)
        let key = format!("key{:010}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Missing key at index {}", i);

        if i % 100_000 == 0 {
            println!("  Sampled {} records", i);
        }
    }

    println!("Test complete!");
}

#[test]
fn test_stripe_distribution() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Write keys designed to hit different stripes
    // Using different prefixes to ensure CRC32 distributes them
    let prefixes = vec!["user", "post", "comment", "like", "follow", "message", "photo", "video"];

    for prefix in &prefixes {
        for i in 0..1000 {
            let key = format!("{}#{}", prefix, i);
            let item = ItemBuilder::new()
                .string("prefix", *prefix)
                .number("index", i)
                .build();
            db.put(key.as_bytes(), item).unwrap();
        }
    }

    // Verify all records
    for prefix in &prefixes {
        for i in 0..1000 {
            let key = format!("{}#{}", prefix, i);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some(), "Missing key: {}", key);
        }
    }
}

#[test]
fn test_updates_over_large_dataset() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Initial write
    println!("Initial write of 50,000 records...");
    for i in 0..50_000 {
        let item = ItemBuilder::new()
            .number("version", 1)
            .number("value", i)
            .build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    // Update all records
    println!("Updating all 50,000 records...");
    for i in 0..50_000 {
        let item = ItemBuilder::new()
            .number("version", 2)
            .number("value", i * 2)
            .build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    // Verify updates
    println!("Verifying updates...");
    for i in 0..50_000 {
        let result = db.get(format!("key{}", i).as_bytes()).unwrap().unwrap();
        match result.get("version").unwrap() {
            kstone_api::KeystoneValue::N(n) => {
                assert_eq!(n, "2", "Wrong version at index {}", i);
            }
            _ => panic!("Expected number"),
        }
    }
}

#[test]
fn test_deletes_over_large_dataset() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Write 50,000 records
    println!("Writing 50,000 records...");
    for i in 0..50_000 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    // Delete half of them
    println!("Deleting 25,000 records...");
    for i in (0..50_000).step_by(2) {
        db.delete(format!("key{}", i).as_bytes()).unwrap();
    }

    // Verify deleted records are gone, others remain
    println!("Verifying deletes...");
    for i in 0..50_000 {
        let result = db.get(format!("key{}", i).as_bytes()).unwrap();
        if i % 2 == 0 {
            assert!(result.is_none(), "Key {} should be deleted", i);
        } else {
            assert!(result.is_some(), "Key {} should exist", i);
        }
    }
}

#[test]
fn test_persistence_with_large_dataset() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write and close
    {
        println!("Writing 30,000 records...");
        let db = Database::create(&path).unwrap();
        for i in 0..30_000 {
            let item = ItemBuilder::new()
                .string("name", format!("item{}", i))
                .number("index", i)
                .build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }
        db.flush().unwrap();
    }

    // Reopen and verify
    {
        println!("Reopening and verifying 30,000 records...");
        let db = Database::open(&path).unwrap();
        for i in 0..30_000 {
            let result = db.get(format!("key{}", i).as_bytes()).unwrap();
            assert!(result.is_some(), "Missing key{} after reopen", i);
        }
    }
}

#[test]
fn test_mixed_workload_large_scale() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    println!("Mixed workload: writes, updates, deletes on 20,000 keys...");

    // Initial writes
    for i in 0..20_000 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    // Mix of operations
    for i in 0..20_000 {
        if i % 3 == 0 {
            // Update
            let item = ItemBuilder::new().number("value", i * 2).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        } else if i % 3 == 1 {
            // Delete
            db.delete(format!("key{}", i).as_bytes()).unwrap();
        }
        // else: leave as-is
    }

    // Verify final state
    println!("Verifying final state...");
    for i in 0..20_000 {
        let result = db.get(format!("key{}", i).as_bytes()).unwrap();
        if i % 3 == 1 {
            assert!(result.is_none(), "Key {} should be deleted", i);
        } else {
            assert!(result.is_some(), "Key {} should exist", i);
        }
    }
}

#[test]
fn test_sort_keys_large_scale() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    println!("Writing 10,000 items with sort keys...");

    // 100 partition keys, each with 100 sort keys
    for pk_id in 0..100 {
        for sk_id in 0..100 {
            let pk = format!("partition{}", pk_id);
            let sk = format!("sort{:04}", sk_id);
            let item = ItemBuilder::new()
                .number("pk_id", pk_id)
                .number("sk_id", sk_id)
                .build();
            db.put_with_sk(pk.as_bytes(), sk.as_bytes(), item).unwrap();
        }
    }

    println!("Verifying 10,000 items...");
    for pk_id in 0..100 {
        for sk_id in 0..100 {
            let pk = format!("partition{}", pk_id);
            let sk = format!("sort{:04}", sk_id);
            let result = db.get_with_sk(pk.as_bytes(), sk.as_bytes()).unwrap();
            assert!(result.is_some(), "Missing pk={} sk={}", pk, sk);
        }
    }
}

#[test]
fn test_in_memory_large_dataset() {
    let db = Database::create_in_memory().unwrap();

    println!("Writing 50,000 records to in-memory database...");
    for i in 0..50_000 {
        let item = ItemBuilder::new()
            .number("index", i)
            .string("data", format!("mem{}", i))
            .build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();

        if i % 10_000 == 0 {
            println!("  Written {} records", i);
        }
    }

    println!("Verifying 50,000 records...");
    for i in 0..50_000 {
        let result = db.get(format!("key{}", i).as_bytes()).unwrap();
        assert!(result.is_some(), "Missing key{}", i);

        if i % 10_000 == 0 {
            println!("  Verified {} records", i);
        }
    }
}

#[test]
fn test_sequential_batch_writes() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    println!("Sequential batch writes (10 batches of 5,000 each)...");

    for batch in 0..10 {
        for i in 0..5_000 {
            let key = format!("batch{}:key{}", batch, i);
            let item = ItemBuilder::new()
                .number("batch", batch)
                .number("index", i)
                .build();
            db.put(key.as_bytes(), item).unwrap();
        }
        db.flush().unwrap();
        println!("  Completed batch {}", batch);
    }

    println!("Verifying all batches...");
    for batch in 0..10 {
        for i in 0..5_000 {
            let key = format!("batch{}:key{}", batch, i);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some(), "Missing {}", key);
        }
    }
}

/// Crash recovery and durability tests for KeystoneDB
///
/// These tests simulate crashes at various points during database operations
/// to verify that KeystoneDB can recover correctly and maintains data integrity.

use kstone_api::{Database, ItemBuilder};
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a database, write data, and simulate crash by dropping without flush
fn simulate_crash_during_write(num_records: usize) -> (PathBuf, Vec<String>) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    let written_keys = {
        let db = Database::create(&path).unwrap();

        let mut keys = Vec::new();
        for i in 0..num_records {
            let key = format!("key{}", i);
            let item = ItemBuilder::new()
                .number("index", i as i64)
                .string("data", format!("value{}", i))
                .build();

            db.put(key.as_bytes(), item).unwrap();
            keys.push(key);
        }

        // Don't call flush() - simulate crash by dropping database
        keys
    };

    // Keep TempDir alive by converting to path
    std::mem::forget(dir);

    (path, written_keys)
}

#[test]
fn test_crash_during_memtable_write() {
    // Write 100 records (less than memtable threshold of 1000)
    // Database crashes before flush
    let (path, written_keys) = simulate_crash_during_write(100);

    // Reopen and verify recovery
    let db = Database::open(&path).unwrap();

    // All records should be recovered from WAL
    for key in &written_keys {
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Record lost after crash: {}", key);
    }

    // Cleanup
    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn test_crash_before_flush() {
    // Write exactly at threshold (forces flush)
    let (path, written_keys) = simulate_crash_during_write(1000);

    // Reopen and verify
    let db = Database::open(&path).unwrap();

    // All records should be present (either in SST or WAL)
    for key in &written_keys {
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Record lost: {}", key);
    }

    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn test_crash_after_multiple_writes() {
    // Write 2500 records (triggers multiple flushes)
    let (path, written_keys) = simulate_crash_during_write(2500);

    // Reopen and verify
    let db = Database::open(&path).unwrap();

    // Verify all records
    for (i, key) in written_keys.iter().enumerate() {
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Record {} lost after crash", i);
    }

    std::fs::remove_dir_all(&path).ok();
}

#[test]
fn test_multiple_crashes_and_recoveries() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // First session: write 100 records, crash
    {
        let db = Database::create(path).unwrap();
        for i in 0..100 {
            let item = ItemBuilder::new().number("value", i).build();
            db.put(format!("crash1:key{}", i).as_bytes(), item).unwrap();
        }
        // Crash (drop without flush)
    }

    // Second session: recover, write 100 more, crash again
    {
        let db = Database::open(path).unwrap();

        // Verify first batch recovered
        for i in 0..100 {
            let result = db.get(format!("crash1:key{}", i).as_bytes()).unwrap();
            assert!(result.is_some(), "Lost crash1:key{}", i);
        }

        // Write second batch
        for i in 0..100 {
            let item = ItemBuilder::new().number("value", i).build();
            db.put(format!("crash2:key{}", i).as_bytes(), item).unwrap();
        }
        // Crash again
    }

    // Third session: verify everything
    {
        let db = Database::open(path).unwrap();

        // Verify both batches
        for i in 0..100 {
            let result1 = db.get(format!("crash1:key{}", i).as_bytes()).unwrap();
            let result2 = db.get(format!("crash2:key{}", i).as_bytes()).unwrap();
            assert!(result1.is_some(), "Lost crash1:key{}", i);
            assert!(result2.is_some(), "Lost crash2:key{}", i);
        }
    }
}

#[test]
fn test_crash_with_deletes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Write records and delete some, then crash
    {
        let db = Database::create(path).unwrap();

        // Write 200 records
        for i in 0..200 {
            let item = ItemBuilder::new().number("value", i).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }

        // Delete every other record
        for i in (0..200).step_by(2) {
            db.delete(format!("key{}", i).as_bytes()).unwrap();
        }
        // Crash
    }

    // Reopen and verify deletes persisted
    {
        let db = Database::open(path).unwrap();

        for i in 0..200 {
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
fn test_crash_with_overwrites() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Write, overwrite, then crash
    {
        let db = Database::create(path).unwrap();

        // Initial write
        for i in 0..100 {
            let item = ItemBuilder::new().number("version", 1).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }

        // Overwrite with new version
        for i in 0..100 {
            let item = ItemBuilder::new().number("version", 2).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }
        // Crash
    }

    // Verify latest version recovered
    {
        let db = Database::open(path).unwrap();

        for i in 0..100 {
            let result = db.get(format!("key{}", i).as_bytes()).unwrap().unwrap();
            match result.get("version").unwrap() {
                kstone_api::KeystoneValue::N(n) => {
                    assert_eq!(n, "2", "key{} has wrong version", i);
                }
                _ => panic!("Expected number"),
            }
        }
    }
}

#[test]
fn test_recovery_with_empty_database() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Create database and crash immediately
    {
        let _db = Database::create(path).unwrap();
        // Crash
    }

    // Reopen empty database
    {
        let db = Database::open(path).unwrap();
        let result = db.get(b"anything").unwrap();
        assert!(result.is_none());
    }
}

#[test]
fn test_crash_after_explicit_flush() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Write, flush, write more, crash
    {
        let db = Database::create(path).unwrap();

        // First batch + flush
        for i in 0..500 {
            let item = ItemBuilder::new().number("batch", 1).build();
            db.put(format!("batch1:key{}", i).as_bytes(), item).unwrap();
        }
        db.flush().unwrap();

        // Second batch (no flush)
        for i in 0..500 {
            let item = ItemBuilder::new().number("batch", 2).build();
            db.put(format!("batch2:key{}", i).as_bytes(), item).unwrap();
        }
        // Crash
    }

    // Verify both batches
    {
        let db = Database::open(path).unwrap();

        // Batch 1 (flushed to SST)
        for i in 0..500 {
            let result = db.get(format!("batch1:key{}", i).as_bytes()).unwrap();
            assert!(result.is_some(), "Lost batch1:key{}", i);
        }

        // Batch 2 (only in WAL before crash)
        for i in 0..500 {
            let result = db.get(format!("batch2:key{}", i).as_bytes()).unwrap();
            assert!(result.is_some(), "Lost batch2:key{}", i);
        }
    }
}

#[test]
fn test_crash_with_large_values() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Write large values and crash
    {
        let db = Database::create(path).unwrap();

        for i in 0..50 {
            let large_string = "x".repeat(100_000); // 100KB per value
            let item = ItemBuilder::new()
                .string("large_data", &large_string)
                .number("index", i)
                .build();
            db.put(format!("large{}", i).as_bytes(), item).unwrap();
        }
        // Crash
    }

    // Verify large values recovered
    {
        let db = Database::open(path).unwrap();

        for i in 0..50 {
            let result = db.get(format!("large{}", i).as_bytes()).unwrap();
            assert!(result.is_some(), "Lost large{}", i);

            let item = result.unwrap();
            let data = item.get("large_data").unwrap();
            match data {
                kstone_api::KeystoneValue::S(s) => {
                    assert_eq!(s.len(), 100_000, "Wrong size for large{}", i);
                }
                _ => panic!("Expected string"),
            }
        }
    }
}

#[test]
fn test_crash_with_sort_keys() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Write composite keys and crash
    {
        let db = Database::create(path).unwrap();

        for pk_id in 0..10 {
            for sk_id in 0..20 {
                let pk = format!("partition{}", pk_id);
                let sk = format!("sort{}", sk_id);
                let item = ItemBuilder::new()
                    .number("pk_id", pk_id)
                    .number("sk_id", sk_id)
                    .build();
                db.put_with_sk(pk.as_bytes(), sk.as_bytes(), item).unwrap();
            }
        }
        // Crash
    }

    // Verify all composite keys recovered
    {
        let db = Database::open(path).unwrap();

        for pk_id in 0..10 {
            for sk_id in 0..20 {
                let pk = format!("partition{}", pk_id);
                let sk = format!("sort{}", sk_id);
                let result = db.get_with_sk(pk.as_bytes(), sk.as_bytes()).unwrap();
                assert!(result.is_some(), "Lost pk={} sk={}", pk, sk);
            }
        }
    }
}

#[test]
fn test_repeated_crash_recovery_cycles() {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    // Simulate 10 crash/recovery cycles
    for cycle in 0..10 {
        let db = if cycle == 0 {
            Database::create(path).unwrap()
        } else {
            Database::open(path).unwrap()
        };

        // Write 100 records for this cycle
        for i in 0..100 {
            let key = format!("cycle{}:key{}", cycle, i);
            let item = ItemBuilder::new()
                .number("cycle", cycle)
                .number("index", i)
                .build();
            db.put(key.as_bytes(), item).unwrap();
        }

        // Verify all previous cycles still exist
        for prev_cycle in 0..=cycle {
            for i in 0..100 {
                let key = format!("cycle{}:key{}", prev_cycle, i);
                let result = db.get(key.as_bytes()).unwrap();
                assert!(result.is_some(), "Lost {}", key);
            }
        }

        // Crash (drop db)
    }

    println!("Successfully completed 10 crash/recovery cycles");
}

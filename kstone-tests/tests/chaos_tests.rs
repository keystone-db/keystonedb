//! Chaos and fault injection tests
//!
//! These tests simulate failures and edge conditions to verify
//! the database handles them gracefully.

use kstone_api::{Database, ItemBuilder};
use rand::prelude::*;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ===========================================
// File corruption tests
// ===========================================

#[test]
fn survives_truncated_sst_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write enough data to create SST files
    {
        let db = Database::create(&path).unwrap();
        for i in 0..2000 {
            let item = ItemBuilder::new()
                .number("index", i)
                .string("data", format!("value_{}", i))
                .build();
            db.put(format!("key_{}", i).as_bytes(), item).unwrap();
        }
        db.flush().unwrap();
    }

    // Find and truncate an SST file
    let sst_files: Vec<_> = fs::read_dir(&path)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x == "sst")
                .unwrap_or(false)
        })
        .collect();

    if let Some(sst) = sst_files.first() {
        let metadata = fs::metadata(sst.path()).unwrap();
        let original_size = metadata.len();

        // Truncate to half size
        let file = OpenOptions::new()
            .write(true)
            .open(sst.path())
            .unwrap();
        file.set_len(original_size / 2).unwrap();
    }

    // Try to reopen - should handle gracefully (recover what we can or error cleanly)
    let result = Database::open(&path);
    // Either succeeds with partial data or returns a clear error
    match result {
        Ok(db) => {
            // If it opens, we should be able to do operations
            let item = ItemBuilder::new().string("test", "after_corruption").build();
            db.put(b"new_key", item).unwrap();
            assert!(db.get(b"new_key").unwrap().is_some());
        }
        Err(e) => {
            // Error should be meaningful, not a panic
            let error_msg = format!("{}", e).to_lowercase();
            assert!(
                error_msg.contains("corrupt")
                    || error_msg.contains("invalid")
                    || error_msg.contains("checksum")
                    || error_msg.contains("mismatch")
                    || error_msg.contains("truncat")
                    || error_msg.contains("unexpected")
                    || error_msg.contains("failed"),
                "Error should be descriptive: {}",
                error_msg
            );
        }
    }
}

#[test]
fn survives_corrupted_sst_bytes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Write data to create SST files
    {
        let db = Database::create(&path).unwrap();
        for i in 0..2000 {
            let item = ItemBuilder::new().number("i", i).build();
            db.put(format!("k{}", i).as_bytes(), item).unwrap();
        }
        db.flush().unwrap();
    }

    // Find and corrupt an SST file
    let sst_files: Vec<_> = fs::read_dir(&path)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x == "sst")
                .unwrap_or(false)
        })
        .collect();

    if let Some(sst) = sst_files.first() {
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .open(sst.path())
            .unwrap();

        // Write garbage in the middle
        file.seek(SeekFrom::Start(100)).unwrap();
        file.write_all(&[0xFF, 0xFE, 0xFD, 0xFC, 0x00, 0x00, 0x00, 0x00]).unwrap();
    }

    // Try to reopen
    let result = Database::open(&path);
    match result {
        Ok(db) => {
            // Should still be able to do new operations
            let item = ItemBuilder::new().string("test", "value").build();
            assert!(db.put(b"after_corrupt", item).is_ok());
        }
        Err(_) => {
            // Clean error is acceptable
        }
    }
}

#[test]
fn survives_missing_wal() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Create database with some data
    {
        let db = Database::create(&path).unwrap();
        for i in 0..100 {
            let item = ItemBuilder::new().number("i", i).build();
            db.put(format!("k{}", i).as_bytes(), item).unwrap();
        }
        db.flush().unwrap();
    }

    // Delete the WAL file
    let wal_path = path.join("wal.log");
    if wal_path.exists() {
        fs::remove_file(&wal_path).unwrap();
    }

    // Try to reopen - should either recover from SST or error cleanly
    let result = Database::open(&path);
    match result {
        Ok(db) => {
            // Some data should be recoverable from SST
            // New operations should work
            let item = ItemBuilder::new().string("test", "value").build();
            db.put(b"new_key", item).unwrap();
        }
        Err(e) => {
            // Clean error is acceptable for missing WAL
            let error_msg = format!("{}", e);
            assert!(
                !error_msg.is_empty(),
                "Error should have a message"
            );
        }
    }
}

// ===========================================
// Stress tests under contention
// ===========================================

#[test]
fn handles_rapid_open_close_cycles() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Initial data
    {
        let db = Database::create(&path).unwrap();
        for i in 0..100 {
            let item = ItemBuilder::new().number("v", i).build();
            db.put(format!("k{}", i).as_bytes(), item).unwrap();
        }
    }

    // Rapid open/close cycles
    for cycle in 0..20 {
        let db = Database::open(&path).unwrap();

        // Do some operations
        let item = ItemBuilder::new().number("cycle", cycle).build();
        db.put(format!("cycle_{}", cycle).as_bytes(), item).unwrap();

        // Verify data
        let result = db.get(b"k50").unwrap();
        assert!(result.is_some(), "Original data should persist through cycles");

        // Explicit drop (close)
        drop(db);
    }

    // Final verification
    let db = Database::open(&path).unwrap();
    assert!(db.get(b"k0").unwrap().is_some());
    assert!(db.get(b"k99").unwrap().is_some());
    assert!(db.get(b"cycle_19").unwrap().is_some());
}

#[test]
fn handles_many_small_transactions() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    let num_threads = 4;
    let ops_per_thread = 500;
    let counter = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let db = db.clone();
            let counter = counter.clone();

            thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = format!("t{}k{}", thread_id, i);
                    let item = ItemBuilder::new()
                        .number("thread", thread_id as i64)
                        .number("op", i as i64)
                        .build();

                    db.put(key.as_bytes(), item).unwrap();
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all operations completed
    assert_eq!(
        counter.load(Ordering::Relaxed),
        (num_threads * ops_per_thread) as u64
    );

    // Verify data integrity
    for thread_id in 0..num_threads {
        for i in 0..ops_per_thread {
            let key = format!("t{}k{}", thread_id, i);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some(), "Missing key: {}", key);
        }
    }
}

#[test]
fn handles_concurrent_readers_and_writers() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    // Pre-populate some data
    for i in 0..100 {
        let item = ItemBuilder::new().number("v", i).build();
        db.put(format!("init_{}", i).as_bytes(), item).unwrap();
    }

    let running = Arc::new(AtomicBool::new(true));
    let reads = Arc::new(AtomicU64::new(0));
    let writes = Arc::new(AtomicU64::new(0));

    // Start reader threads
    let reader_handles: Vec<_> = (0..4)
        .map(|_| {
            let db = db.clone();
            let running = running.clone();
            let reads = reads.clone();

            thread::spawn(move || {
                let mut rng = rand::thread_rng();
                while running.load(Ordering::Relaxed) {
                    let key = format!("init_{}", rng.gen_range(0..100));
                    let _ = db.get(key.as_bytes());
                    reads.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    // Start writer threads
    let writer_handles: Vec<_> = (0..2)
        .map(|thread_id| {
            let db = db.clone();
            let running = running.clone();
            let writes = writes.clone();

            thread::spawn(move || {
                let mut counter = 0;
                while running.load(Ordering::Relaxed) {
                    let key = format!("write_{}_{}", thread_id, counter);
                    let item = ItemBuilder::new().number("c", counter).build();
                    db.put(key.as_bytes(), item).unwrap();
                    writes.fetch_add(1, Ordering::Relaxed);
                    counter += 1;
                }
            })
        })
        .collect();

    // Let it run for a bit
    thread::sleep(Duration::from_millis(500));
    running.store(false, Ordering::Relaxed);

    // Wait for all threads
    for handle in reader_handles {
        handle.join().unwrap();
    }
    for handle in writer_handles {
        handle.join().unwrap();
    }

    let total_reads = reads.load(Ordering::Relaxed);
    let total_writes = writes.load(Ordering::Relaxed);

    assert!(total_reads > 0, "Should have done some reads");
    assert!(total_writes > 0, "Should have done some writes");

    // Verify database is still functional
    let item = ItemBuilder::new().string("final", "test").build();
    db.put(b"final_key", item).unwrap();
    assert!(db.get(b"final_key").unwrap().is_some());
}

// ===========================================
// Resource exhaustion tests
// ===========================================

#[test]
fn handles_very_long_keys() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Try progressively longer keys
    for len in [100, 500, 1000, 2000, 4000] {
        let key = "k".repeat(len);
        let item = ItemBuilder::new().number("len", len as i64).build();

        let result = db.put(key.as_bytes(), item.clone());
        assert!(result.is_ok(), "Should handle key of length {}", len);

        let retrieved = db.get(key.as_bytes()).unwrap();
        assert!(retrieved.is_some(), "Should retrieve key of length {}", len);
    }
}

#[test]
fn handles_very_large_values() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Try progressively larger values
    for size_kb in [10, 100, 500, 1000] {
        let value = "x".repeat(size_kb * 1024);
        let item = ItemBuilder::new().string("data", &value).build();

        let key = format!("large_{}", size_kb);
        let result = db.put(key.as_bytes(), item);
        assert!(result.is_ok(), "Should handle value of {}KB", size_kb);

        let retrieved = db.get(key.as_bytes()).unwrap();
        assert!(retrieved.is_some(), "Should retrieve value of {}KB", size_kb);
    }
}

#[test]
fn handles_many_unique_keys() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    let num_keys = 10_000;

    // Write many keys
    for i in 0..num_keys {
        let item = ItemBuilder::new().number("i", i).build();
        db.put(format!("unique_key_{:08}", i).as_bytes(), item).unwrap();
    }

    // Verify random samples
    let mut rng = rand::thread_rng();
    for _ in 0..100 {
        let i = rng.gen_range(0..num_keys);
        let result = db.get(format!("unique_key_{:08}", i).as_bytes()).unwrap();
        assert!(result.is_some(), "Key {} should exist", i);
    }
}

// ===========================================
// Timing and latency tests
// ===========================================

#[test]
fn operations_complete_in_reasonable_time() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Pre-populate
    for i in 0..1000 {
        let item = ItemBuilder::new().number("i", i).build();
        db.put(format!("k{}", i).as_bytes(), item).unwrap();
    }
    db.flush().unwrap();

    // Measure operation times
    let iterations = 100;

    // Put timing
    let start = Instant::now();
    for i in 0..iterations {
        let item = ItemBuilder::new().number("t", i).build();
        db.put(format!("timing_{}", i).as_bytes(), item).unwrap();
    }
    let put_time = start.elapsed();
    let avg_put_us = put_time.as_micros() / iterations as u128;

    // Get timing
    let start = Instant::now();
    for i in 0..iterations {
        db.get(format!("k{}", i % 1000).as_bytes()).unwrap();
    }
    let get_time = start.elapsed();
    let avg_get_us = get_time.as_micros() / iterations as u128;

    // Sanity check - operations should complete in reasonable time
    // (These are very generous limits, mainly to catch infinite loops or deadlocks)
    assert!(
        avg_put_us < 100_000, // 100ms per put is way too slow
        "Average put time too high: {}us",
        avg_put_us
    );
    assert!(
        avg_get_us < 100_000, // 100ms per get is way too slow
        "Average get time too high: {}us",
        avg_get_us
    );
}

// ===========================================
// Edge case interaction tests
// ===========================================

#[test]
fn handles_interleaved_operations() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();
    let mut rng = rand::thread_rng();

    let mut expected: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

    // Random mix of operations
    for _ in 0..1000 {
        let key = format!("k{}", rng.gen_range(0..100));

        match rng.gen_range(0..3) {
            0 => {
                // Put
                let value = rng.gen::<i64>();
                let item = ItemBuilder::new().number("v", value).build();
                db.put(key.as_bytes(), item).unwrap();
                expected.insert(key, value);
            }
            1 => {
                // Get
                let result = db.get(key.as_bytes()).unwrap();
                if let Some(exp_val) = expected.get(&key) {
                    assert!(result.is_some());
                    match result.unwrap().get("v") {
                        Some(kstone_api::KeystoneValue::N(n)) => {
                            let parsed: i64 = n.parse().unwrap();
                            assert_eq!(parsed, *exp_val);
                        }
                        _ => panic!("Wrong type"),
                    }
                }
            }
            2 => {
                // Delete
                db.delete(key.as_bytes()).unwrap();
                expected.remove(&key);
            }
            _ => unreachable!(),
        }
    }

    // Verify final state
    for (key, expected_val) in &expected {
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Key {} should exist", key);
        match result.unwrap().get("v") {
            Some(kstone_api::KeystoneValue::N(n)) => {
                let parsed: i64 = n.parse().unwrap();
                assert_eq!(parsed, *expected_val);
            }
            _ => panic!("Wrong type for key {}", key),
        }
    }
}

//! Long-running stability tests
//!
//! These tests run for extended periods to find issues that only appear
//! under sustained load. They are marked #[ignore] by default and should
//! be run separately with `cargo test --ignored`.

use kstone_api::{Database, ItemBuilder, KeystoneValue};
use rand::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ===========================================
// Sustained workload tests
// ===========================================

/// Run a mixed workload for an extended period
#[test]
#[ignore] // Run with: cargo test --ignored stability
fn long_running_mixed_workload() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    let duration = Duration::from_secs(60); // 1 minute test
    let start = Instant::now();
    let ops = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let running = Arc::new(AtomicBool::new(true));

    let num_threads = 4;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let db = db.clone();
            let ops = ops.clone();
            let errors = errors.clone();
            let running = running.clone();

            thread::spawn(move || {
                let mut rng = rand::thread_rng();
                let mut local_ops = 0u64;

                while running.load(Ordering::Relaxed) {
                    let key = format!("key-{}-{}", thread_id, rng.gen::<u32>() % 10000);

                    let result = match rng.gen_range(0..10) {
                        0..=6 => {
                            // 70% writes
                            let item = ItemBuilder::new()
                                .number("value", rng.gen::<i64>())
                                .string("thread", &format!("{}", thread_id))
                                .build();
                            db.put(key.as_bytes(), item)
                        }
                        7..=8 => {
                            // 20% reads
                            db.get(key.as_bytes()).map(|_| ())
                        }
                        _ => {
                            // 10% deletes
                            db.delete(key.as_bytes())
                        }
                    };

                    if result.is_err() {
                        errors.fetch_add(1, Ordering::Relaxed);
                    }

                    local_ops += 1;
                    if local_ops % 1000 == 0 {
                        ops.fetch_add(1000, Ordering::Relaxed);
                    }
                }

                ops.fetch_add(local_ops % 1000, Ordering::Relaxed);
            })
        })
        .collect();

    // Run for the specified duration
    thread::sleep(duration);
    running.store(false, Ordering::Relaxed);

    for h in handles {
        h.join().unwrap();
    }

    let total_ops = ops.load(Ordering::Relaxed);
    let total_errors = errors.load(Ordering::Relaxed);
    let elapsed = start.elapsed();

    println!(
        "Long running test: {} ops in {:?} ({:.0} ops/sec), {} errors",
        total_ops,
        elapsed,
        total_ops as f64 / elapsed.as_secs_f64(),
        total_errors
    );

    assert_eq!(total_errors, 0, "Should have no errors during sustained load");
    assert!(total_ops > 1000, "Should complete significant operations");

    // Verify database is still functional
    let stats = db.stats().unwrap();
    assert!(stats.total_keys.is_some() || stats.total_sst_files > 0);

    // Can still do operations
    let item = ItemBuilder::new().string("test", "final").build();
    db.put(b"final_test", item).unwrap();
    assert!(db.get(b"final_test").unwrap().is_some());
}

/// Test that the database handles memory pressure from many concurrent operations
#[test]
#[ignore]
fn sustained_memory_pressure() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    let duration = Duration::from_secs(30);
    let running = Arc::new(AtomicBool::new(true));

    // Create large values to pressure memory
    let large_value = "x".repeat(10 * 1024); // 10KB per value

    let handles: Vec<_> = (0..2)
        .map(|thread_id| {
            let db = db.clone();
            let running = running.clone();
            let large_value = large_value.clone();

            thread::spawn(move || {
                let mut counter = 0u64;
                while running.load(Ordering::Relaxed) {
                    let key = format!("large_{}_{}", thread_id, counter % 1000);
                    let item = ItemBuilder::new()
                        .string("data", &large_value)
                        .number("seq", counter as i64)
                        .build();

                    db.put(key.as_bytes(), item).unwrap();
                    counter += 1;

                    // Periodically flush to prevent unbounded memory growth
                    if counter % 100 == 0 {
                        db.flush().unwrap();
                    }
                }
            })
        })
        .collect();

    thread::sleep(duration);
    running.store(false, Ordering::Relaxed);

    for h in handles {
        h.join().unwrap();
    }

    // Database should still be functional
    let item = ItemBuilder::new().string("test", "after_pressure").build();
    db.put(b"pressure_test", item).unwrap();
    assert!(db.get(b"pressure_test").unwrap().is_some());
}

/// Test recovery consistency after many operations
#[test]
#[ignore]
fn recovery_after_sustained_writes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    let num_operations = 50_000;
    let mut expected: HashMap<String, i64> = HashMap::new();

    // Perform many operations
    {
        let db = Database::create(&path).unwrap();
        let mut rng = rand::thread_rng();

        for i in 0..num_operations {
            let key = format!("key_{}", rng.gen::<u32>() % 5000);

            if rng.gen_bool(0.8) {
                // 80% writes
                let value = rng.gen::<i64>();
                let item = ItemBuilder::new().number("v", value).build();
                db.put(key.as_bytes(), item).unwrap();
                expected.insert(key, value);
            } else {
                // 20% deletes
                db.delete(key.as_bytes()).unwrap();
                expected.remove(&key);
            }

            // Periodic flush
            if i % 5000 == 0 {
                db.flush().unwrap();
            }
        }

        db.flush().unwrap();
    }

    // Reopen and verify
    {
        let db = Database::open(&path).unwrap();

        // Verify all expected keys exist with correct values
        for (key, expected_value) in &expected {
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some(), "Key {} should exist", key);

            match result.unwrap().get("v") {
                Some(KeystoneValue::N(n)) => {
                    let parsed: i64 = n.parse().unwrap();
                    assert_eq!(parsed, *expected_value, "Value mismatch for key {}", key);
                }
                _ => panic!("Wrong type for key {}", key),
            }
        }

        // Verify deleted keys don't exist
        let all_possible_keys: std::collections::HashSet<_> =
            (0..5000).map(|i| format!("key_{}", i)).collect();
        let expected_keys: std::collections::HashSet<_> = expected.keys().cloned().collect();
        let deleted_keys: Vec<_> = all_possible_keys.difference(&expected_keys).collect();

        for key in deleted_keys.iter().take(100) {
            // Sample check
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_none(), "Deleted key {} should not exist", key);
        }
    }
}

/// Test that repeated open/close cycles don't cause issues
#[test]
#[ignore]
fn repeated_open_close_stress() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    let cycles = 100;
    let ops_per_cycle = 500;

    for cycle in 0..cycles {
        let db = if cycle == 0 {
            Database::create(&path).unwrap()
        } else {
            Database::open(&path).unwrap()
        };

        // Do operations
        for i in 0..ops_per_cycle {
            let key = format!("cycle_{}_key_{}", cycle, i);
            let item = ItemBuilder::new()
                .number("cycle", cycle as i64)
                .number("op", i as i64)
                .build();
            db.put(key.as_bytes(), item).unwrap();
        }

        // Verify some data from previous cycles
        if cycle > 0 {
            let prev_key = format!("cycle_{}_key_0", cycle - 1);
            let result = db.get(prev_key.as_bytes()).unwrap();
            assert!(
                result.is_some(),
                "Data from cycle {} should persist",
                cycle - 1
            );
        }

        db.flush().unwrap();
    }

    // Final verification
    let db = Database::open(&path).unwrap();
    for cycle in 0..cycles {
        let key = format!("cycle_{}_key_0", cycle);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Cycle {} data should exist", cycle);
    }
}

// ===========================================
// Shorter stability tests (run with normal tests)
// ===========================================

/// Quick stability check - runs with normal tests
#[test]
fn quick_stability_check() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    let duration = Duration::from_secs(5);
    let start = Instant::now();
    let ops = Arc::new(AtomicU64::new(0));
    let running = Arc::new(AtomicBool::new(true));

    let handles: Vec<_> = (0..2)
        .map(|thread_id| {
            let db = db.clone();
            let ops = ops.clone();
            let running = running.clone();

            thread::spawn(move || {
                let mut rng = rand::thread_rng();
                while running.load(Ordering::Relaxed) {
                    let key = format!("qk-{}-{}", thread_id, rng.gen::<u32>() % 1000);
                    let item = ItemBuilder::new().number("v", rng.gen::<i64>()).build();
                    db.put(key.as_bytes(), item).unwrap();
                    ops.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    thread::sleep(duration);
    running.store(false, Ordering::Relaxed);

    for h in handles {
        h.join().unwrap();
    }

    let total_ops = ops.load(Ordering::Relaxed);
    println!(
        "Quick stability: {} ops in {:?}",
        total_ops,
        start.elapsed()
    );

    assert!(total_ops > 100, "Should complete at least 100 ops");

    // Verify functionality
    assert!(db.get(b"qk-0-0").is_ok());
}

/// Test that flush operations under load don't cause issues
#[test]
fn flush_under_concurrent_load() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    let running = Arc::new(AtomicBool::new(true));
    let writes = Arc::new(AtomicU64::new(0));

    // Writer thread
    let writer_db = db.clone();
    let writer_running = running.clone();
    let writer_writes = writes.clone();
    let writer = thread::spawn(move || {
        let mut i = 0u64;
        while writer_running.load(Ordering::Relaxed) {
            let item = ItemBuilder::new().number("i", i as i64).build();
            writer_db
                .put(format!("flush_test_{}", i).as_bytes(), item)
                .unwrap();
            writer_writes.fetch_add(1, Ordering::Relaxed);
            i += 1;
        }
    });

    // Flusher thread
    let flusher_db = db.clone();
    let flusher_running = running.clone();
    let flusher = thread::spawn(move || {
        while flusher_running.load(Ordering::Relaxed) {
            flusher_db.flush().unwrap();
            thread::sleep(Duration::from_millis(50));
        }
    });

    // Run for a bit
    thread::sleep(Duration::from_secs(3));
    running.store(false, Ordering::Relaxed);

    writer.join().unwrap();
    flusher.join().unwrap();

    let total_writes = writes.load(Ordering::Relaxed);
    println!("Flush under load: {} writes with concurrent flushes", total_writes);

    assert!(total_writes > 100, "Should complete writes during concurrent flushes");

    // Verify data integrity
    for i in 0..std::cmp::min(100, total_writes) {
        let result = db.get(format!("flush_test_{}", i).as_bytes()).unwrap();
        assert!(result.is_some(), "Key {} should exist", i);
    }
}

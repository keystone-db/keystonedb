/// Concurrent access integration tests for KeystoneDB
///
/// Tests multi-threaded read and write operations to ensure thread safety
/// and correct behavior under concurrent load.

use kstone_api::{Database, ItemBuilder};
use std::sync::Arc;
use std::thread;
use tempfile::TempDir;

#[test]
fn test_concurrent_writes() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    let num_threads = 10;
    let writes_per_thread = 100;

    let mut handles = vec![];

    // Spawn threads that write concurrently
    for thread_id in 0..num_threads {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            for i in 0..writes_per_thread {
                let key = format!("thread{}:key{}", thread_id, i);
                let item = ItemBuilder::new()
                    .number("thread_id", thread_id)
                    .number("index", i)
                    .build();

                db_clone.put(key.as_bytes(), item).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all writes succeeded
    for thread_id in 0..num_threads {
        for i in 0..writes_per_thread {
            let key = format!("thread{}:key{}", thread_id, i);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some(), "Missing {}", key);
        }
    }
}

#[test]
fn test_concurrent_reads() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    // Pre-populate database
    for i in 0..1000 {
        let item = ItemBuilder::new()
            .number("value", i)
            .string("data", format!("item{}", i))
            .build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    let num_threads = 20;
    let reads_per_thread = 100;

    let mut handles = vec![];

    // Spawn threads that read concurrently
    for thread_id in 0..num_threads {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            for i in 0..reads_per_thread {
                let key_idx = (thread_id * reads_per_thread + i) % 1000;
                let key = format!("key{}", key_idx);
                let result = db_clone.get(key.as_bytes()).unwrap();
                assert!(result.is_some(), "Read failed: {}", key);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_concurrent_read_write_mix() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    // Pre-populate
    for i in 0..500 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(format!("existing{}", i).as_bytes(), item).unwrap();
    }

    let num_reader_threads = 10;
    let num_writer_threads = 5;

    let mut handles = vec![];

    // Spawn reader threads
    for thread_id in 0..num_reader_threads {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            for i in 0..200 {
                let key_idx = (thread_id * 50 + i) % 500;
                let key = format!("existing{}", key_idx);
                let _result = db_clone.get(key.as_bytes()).unwrap();
            }
        });
        handles.push(handle);
    }

    // Spawn writer threads
    for thread_id in 0..num_writer_threads {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            for i in 0..100 {
                let key = format!("new{}:{}", thread_id, i);
                let item = ItemBuilder::new()
                    .number("thread", thread_id)
                    .number("index", i)
                    .build();
                db_clone.put(key.as_bytes(), item).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify new writes
    for thread_id in 0..num_writer_threads {
        for i in 0..100 {
            let key = format!("new{}:{}", thread_id, i);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some(), "Missing write: {}", key);
        }
    }
}

#[test]
fn test_concurrent_deletes() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    // Pre-populate
    for i in 0..1000 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    let num_threads = 10;
    let deletes_per_thread = 100;

    let mut handles = vec![];

    // Each thread deletes a different range
    for thread_id in 0..num_threads {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            let start = thread_id * deletes_per_thread;
            for i in 0..deletes_per_thread {
                let key = format!("key{}", start + i);
                db_clone.delete(key.as_bytes()).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify deletions
    for thread_id in 0..num_threads {
        let start = thread_id * deletes_per_thread;
        for i in 0..deletes_per_thread {
            let key = format!("key{}", start + i);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_none(), "Item not deleted: {}", key);
        }
    }
}

#[test]
fn test_concurrent_overwrites() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    let num_threads = 10;
    let overwrites_per_thread = 50;

    let mut handles = vec![];

    // Multiple threads overwriting the same keys
    for thread_id in 0..num_threads {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            for i in 0..overwrites_per_thread {
                let key = format!("shared{}", i);
                let item = ItemBuilder::new()
                    .number("thread_id", thread_id)
                    .number("counter", i)
                    .build();
                db_clone.put(key.as_bytes(), item).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // All keys should exist with some thread's value
    for i in 0..overwrites_per_thread {
        let key = format!("shared{}", i);
        let result = db.get(key.as_bytes()).unwrap();
        assert!(result.is_some(), "Missing shared key: {}", key);
    }
}

#[test]
fn test_concurrent_with_flush() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    let mut handles = vec![];

    // Writer thread (triggers flushes)
    let db_writer = Arc::clone(&db);
    let writer = thread::spawn(move || {
        for i in 0..2000 {
            let item = ItemBuilder::new().number("value", i).build();
            db_writer.put(format!("key{}", i).as_bytes(), item).unwrap();
        }
    });
    handles.push(writer);

    // Reader threads (read while writes/flushes happening)
    for _ in 0..5 {
        let db_reader = Arc::clone(&db);
        let reader = thread::spawn(move || {
            for _ in 0..100 {
                // Read random keys
                for i in (0..2000).step_by(20) {
                    let _result = db_reader.get(format!("key{}", i).as_bytes()).unwrap();
                }
            }
        });
        handles.push(reader);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all writes
    for i in 0..2000 {
        let result = db.get(format!("key{}", i).as_bytes()).unwrap();
        assert!(result.is_some(), "Missing key{}", i);
    }
}

#[test]
fn test_concurrent_in_memory_mode() {
    let db = Arc::new(Database::create_in_memory().unwrap());

    let num_threads = 8;
    let writes_per_thread = 100;

    let mut handles = vec![];

    // Concurrent writes to in-memory database
    for thread_id in 0..num_threads {
        let db_clone = Arc::clone(&db);
        let handle = thread::spawn(move || {
            for i in 0..writes_per_thread {
                let key = format!("mem{}:{}", thread_id, i);
                let item = ItemBuilder::new()
                    .number("thread", thread_id)
                    .number("index", i)
                    .build();
                db_clone.put(key.as_bytes(), item).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all writes
    for thread_id in 0..num_threads {
        for i in 0..writes_per_thread {
            let key = format!("mem{}:{}", thread_id, i);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some(), "Missing {}", key);
        }
    }
}

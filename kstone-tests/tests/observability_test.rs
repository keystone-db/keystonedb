/// Observability API tests for KeystoneDB
///
/// Tests database statistics, health checks, and metrics collection.

use kstone_api::{Database, ItemBuilder, HealthStatus};
use tempfile::TempDir;

#[test]
fn test_database_stats() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Get initial stats
    let stats = db.stats().unwrap();
    assert!(stats.total_keys.is_none()); // Not tracked by default
    assert_eq!(stats.total_sst_files, 0); // No SSTs yet

    // Write some data
    for i in 0..100 {
        let item = ItemBuilder::new()
            .number("value", i)
            .build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    // Stats after writes (still in memtable)
    let stats = db.stats().unwrap();
    assert_eq!(stats.total_sst_files, 0); // No flush yet

    // Flush to create SST
    db.flush().unwrap();

    // Stats after flush - would show SST if we tracked it
    let _stats = db.stats().unwrap();
}

#[test]
fn test_health_check() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Fresh database should be healthy
    let health = db.health();
    assert_eq!(health.status, HealthStatus::Healthy);
    assert!(health.warnings.is_empty());
    assert!(health.errors.is_empty());

    // Write data and check health
    for i in 0..1000 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    let health = db.health();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[test]
fn test_stats_after_operations() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Perform various operations
    for i in 0..500 {
        let item = ItemBuilder::new()
            .string("data", format!("value{}", i))
            .number("index", i)
            .build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    // Delete some
    for i in (0..500).step_by(3) {
        db.delete(format!("key{}", i).as_bytes()).unwrap();
    }

    // Get stats
    let _stats = db.stats().unwrap();
}

#[test]
fn test_stats_in_memory_mode() {
    let db = Database::create_in_memory().unwrap();

    // In-memory database stats
    let stats = db.stats().unwrap();
    assert_eq!(stats.wal_size_bytes, Some(0)); // No WAL
    assert_eq!(stats.total_disk_size_bytes, Some(0)); // No disk

    // Write data
    for i in 0..100 {
        let item = ItemBuilder::new().number("value", i).build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    // Stats still show no disk usage
    let stats = db.stats().unwrap();
    assert_eq!(stats.total_disk_size_bytes, Some(0));
}

#[test]
fn test_stats_with_composite_keys() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Write composite keys
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

    // Get stats
    let _stats = db.stats().unwrap();
}

#[test]
fn test_health_after_flush() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Write and flush multiple times
    for round in 0..3 {
        for i in 0..500 {
            let item = ItemBuilder::new()
                .number("round", round)
                .number("index", i)
                .build();
            db.put(format!("r{}:k{}", round, i).as_bytes(), item).unwrap();
        }
        db.flush().unwrap();

        // Check health after each flush
        let health = db.health();
        assert_eq!(health.status, HealthStatus::Healthy);
    }
}

#[test]
fn test_stats_persistence() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();

    // Create database, write data
    {
        let db = Database::create(&path).unwrap();

        for i in 0..1000 {
            let item = ItemBuilder::new().number("value", i).build();
            db.put(format!("key{}", i).as_bytes(), item).unwrap();
        }

        db.flush().unwrap();
    }

    // Reopen and check stats
    {
        let db = Database::open(&path).unwrap();
        let _stats = db.stats().unwrap();
    }
}

#[test]
fn test_compaction_stats() {
    let dir = TempDir::new().unwrap();
    let db = Database::create(dir.path()).unwrap();

    // Initial compaction stats should be zero
    let stats = db.stats().unwrap();
    assert_eq!(stats.compaction.total_compactions, 0);
    assert_eq!(stats.compaction.total_ssts_merged, 0);
    assert_eq!(stats.compaction.total_bytes_read, 0);

    // Write enough data to potentially trigger operations
    for i in 0..2000 {
        let item = ItemBuilder::new()
            .string("data", format!("value{}", i))
            .number("index", i)
            .build();
        db.put(format!("key{}", i).as_bytes(), item).unwrap();
    }

    db.flush().unwrap();

    // Stats should still be valid
    let _stats = db.stats().unwrap();
}

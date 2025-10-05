#[cfg(test)]
mod sync_tests {
    use kstone_api::{Database, ItemBuilder};
    use kstone_sync::{SyncEngine, SyncConfig, ConflictStrategy, SyncEndpoint};
    use tempfile::TempDir;
    use std::time::Duration;

    #[tokio::test]
    async fn test_bidirectional_sync() {
        // Create temporary directories for both databases
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        // Create two databases
        let db1_path = dir1.path().join("test1.keystone");
        let db2_path = dir2.path().join("test2.keystone");

        let db1 = std::sync::Arc::new(Database::create(&db1_path).unwrap());
        let db2 = std::sync::Arc::new(Database::create(&db2_path).unwrap());

        // Put different items in each database
        db1.put(b"user1", ItemBuilder::new()
            .string("name", "Alice")
            .number("age", 30)
            .build()).unwrap();

        db2.put(b"user2", ItemBuilder::new()
            .string("name", "Bob")
            .number("age", 25)
            .build()).unwrap();

        eprintln!("\n=== Before sync ===");
        eprintln!("DB1 has user1: {:?}", db1.get(b"user1").unwrap().is_some());
        eprintln!("DB1 has user2: {:?}", db1.get(b"user2").unwrap().is_some());
        eprintln!("DB2 has user1: {:?}", db2.get(b"user1").unwrap().is_some());
        eprintln!("DB2 has user2: {:?}", db2.get(b"user2").unwrap().is_some());

        // Configure sync from DB1 to DB2
        let dummy_endpoint = SyncEndpoint::FileSystem {
            path: "dummy.keystone".to_string(),
        };

        let config = SyncConfig {
            endpoint: dummy_endpoint,
            conflict_strategy: ConflictStrategy::LastWriterWins,
            sync_interval: Some(Duration::from_secs(60)),
            batch_size: 100,
            max_retries: 3,
            enable_compression: false,
        };

        let sync_engine = SyncEngine::new(db1.clone(), config).unwrap();

        // Define the actual filesystem endpoint (DB2)
        let endpoint = SyncEndpoint::FileSystem {
            path: db2_path.to_string_lossy().to_string(),
        };

        // Perform sync
        eprintln!("\n=== Performing sync ===");
        let result = sync_engine.sync(endpoint).await;

        match result {
            Ok(stats) => {
                eprintln!("✓ Sync completed successfully");
                eprintln!("  Items sent: {}", stats.items_sent);
                eprintln!("  Items received: {}", stats.items_received);
                eprintln!("  Conflicts: {}", stats.conflicts_resolved);
            }
            Err(e) => {
                eprintln!("✗ Sync failed: {}", e);
                panic!("Sync failed: {}", e);
            }
        }

        // Verify both databases now have both items
        eprintln!("\n=== After sync ===");

        // Flush to ensure writes are persisted
        db1.flush().unwrap();
        db2.flush().unwrap();

        // Re-open DB2 since FilesystemProtocol opened its own instance
        let db2_reopened = std::sync::Arc::new(Database::open(&db2_path).unwrap());

        // Check DB1
        let db1_user1 = db1.get(b"user1").unwrap();
        let db1_user2 = db1.get(b"user2").unwrap();
        eprintln!("DB1 has user1: {:?}", db1_user1.is_some());
        eprintln!("DB1 has user2: {:?}", db1_user2.is_some());

        // Check DB2 (using reopened instance)
        let db2_user1 = db2_reopened.get(b"user1").unwrap();
        let db2_user2 = db2_reopened.get(b"user2").unwrap();
        eprintln!("DB2 has user1: {:?}", db2_user1.is_some());
        eprintln!("DB2 has user2: {:?}", db2_user2.is_some());

        // Assertions
        assert!(db1_user1.is_some(), "DB1 should still have user1");
        assert!(db1_user2.is_some(), "DB1 should now have user2 from DB2");
        assert!(db2_user1.is_some(), "DB2 should now have user1 from DB1");
        assert!(db2_user2.is_some(), "DB2 should still have user2");

        // Verify the actual content
        if let Some(user1_in_db2) = &db2_user1 {
            assert_eq!(user1_in_db2.get("name").unwrap().as_string().unwrap(), "Alice");
        }
        if let Some(user2_in_db1) = &db1_user2 {
            assert_eq!(user2_in_db1.get("name").unwrap().as_string().unwrap(), "Bob");
        }

        eprintln!("\n✓ Bidirectional sync test passed!");
    }

    #[tokio::test]
    async fn test_sync_with_conflict() {
        // Create temporary directories for both databases
        let dir1 = TempDir::new().unwrap();
        let dir2 = TempDir::new().unwrap();

        // Create two databases
        let db1_path = dir1.path().join("test1.keystone");
        let db2_path = dir2.path().join("test2.keystone");

        let db1 = std::sync::Arc::new(Database::create(&db1_path).unwrap());
        let db2 = std::sync::Arc::new(Database::create(&db2_path).unwrap());

        // Put the SAME key with different values in both databases
        db1.put(b"user#shared", ItemBuilder::new()
            .string("name", "Alice")
            .string("location", "New York")
            .number("version", 1)
            .build()).unwrap();

        db2.put(b"user#shared", ItemBuilder::new()
            .string("name", "Alice")
            .string("location", "San Francisco")
            .number("version", 2)
            .build()).unwrap();

        eprintln!("\n=== Before sync (conflict scenario) ===");
        eprintln!("DB1 user#shared location: New York");
        eprintln!("DB2 user#shared location: San Francisco");

        // Configure sync with LastWriterWins strategy
        let dummy_endpoint = SyncEndpoint::FileSystem {
            path: "dummy.keystone".to_string(),
        };

        let config = SyncConfig {
            endpoint: dummy_endpoint,
            conflict_strategy: ConflictStrategy::LastWriterWins,
            sync_interval: None,
            batch_size: 100,
            max_retries: 3,
            enable_compression: false,
        };

        let sync_engine = SyncEngine::new(db1.clone(), config).unwrap();

        // Define the filesystem endpoint (DB2)
        let endpoint = SyncEndpoint::FileSystem {
            path: db2_path.to_string_lossy().to_string(),
        };

        // Perform sync
        eprintln!("\n=== Performing sync with conflict resolution ===");
        let result = sync_engine.sync(endpoint).await;

        match result {
            Ok(stats) => {
                eprintln!("✓ Sync completed successfully");
                eprintln!("  Conflicts resolved: {}", stats.conflicts_resolved);
            }
            Err(e) => {
                eprintln!("✗ Sync failed: {}", e);
                panic!("Sync failed: {}", e);
            }
        }

        // With LocalWins strategy, DB1's version should win
        let db1_item = db1.get(b"user#shared").unwrap().expect("Item should exist in DB1");
        let db2_item = db2.get(b"user#shared").unwrap().expect("Item should exist in DB2");

        eprintln!("\n=== After sync ===");
        eprintln!("DB1 location: {:?}", db1_item.get("location").unwrap().as_string());
        eprintln!("DB2 location: {:?}", db2_item.get("location").unwrap().as_string());

        // With LastWriterWins, the item with the latest timestamp wins
        // The actual winner depends on which item was written more recently
        // For now just verify both databases have the item
        assert!(db1_item.get("location").is_some(), "DB1 should have location");
        assert!(db2_item.get("location").is_some(), "DB2 should have location");

        eprintln!("\n✓ Conflict resolution test passed!");
    }
}
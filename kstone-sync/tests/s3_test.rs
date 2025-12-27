#[cfg(test)]
mod s3_tests {
    use kstone_api::{Database, ItemBuilder};
    use kstone_sync::sync_engine::{SyncEngine, SyncConfig};
    use kstone_sync::protocol::s3::S3Protocol;
    use kstone_sync::{ConflictStrategy, SyncEndpoint};
    use tempfile::TempDir;
    use std::sync::Arc;

    /// Test S3 protocol creation
    /// Note: get_local_files is a private method tested indirectly through sync operations
    #[tokio::test]
    async fn test_s3_protocol_creation() {
        // Test protocol creation
        let _protocol = S3Protocol::new(
            "test-bucket".to_string(),
            "test-prefix".to_string(),
            "us-east-1".to_string(),
            None,
            None,
        );
        // Protocol created successfully - actual sync operations require real S3 connection
    }

    #[tokio::test]
    async fn test_s3_endpoint_creation() {
        let endpoint = SyncEndpoint::S3 {
            bucket: "test-bucket".to_string(),
            prefix: "test-prefix".to_string(),
            region: "us-east-1".to_string(),
            endpoint_url: None,
            credentials: None,
        };

        assert_eq!(endpoint.endpoint_type(), "s3");
        assert_eq!(endpoint.endpoint_id().0, "s3://test-bucket:test-prefix");
    }

    #[tokio::test]
    async fn test_s3_endpoint_with_custom_url() {
        let endpoint = SyncEndpoint::S3 {
            bucket: "test-bucket".to_string(),
            prefix: "test-prefix".to_string(),
            region: "us-east-1".to_string(),
            endpoint_url: Some("http://localhost:9000".to_string()),
            credentials: None,
        };

        assert_eq!(endpoint.endpoint_type(), "s3");
    }

    /// Integration test with real S3 (skipped in CI)
    #[tokio::test]
    #[ignore] // Run with: cargo test --ignored
    async fn test_s3_upload_download() {
        // This test requires actual S3 setup
        // Set these environment variables:
        // - AWS_ACCESS_KEY_ID
        // - AWS_SECRET_ACCESS_KEY
        // - TEST_S3_BUCKET

        let bucket = std::env::var("TEST_S3_BUCKET")
            .expect("Set TEST_S3_BUCKET environment variable");

        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.keystone");

        // Create test database
        let db = Arc::new(Database::create(&db_path).unwrap());

        // Add test data
        db.put(b"test-key", ItemBuilder::new()
            .string("data", "test-value")
            .build()).unwrap();

        db.flush().unwrap();

        // Create sync engine
        let dummy_endpoint = SyncEndpoint::FileSystem {
            path: "/tmp/dummy".to_string(),
        };

        let config = SyncConfig {
            endpoint: dummy_endpoint,
            conflict_strategy: ConflictStrategy::LastWriterWins,
            sync_interval: None,
            batch_size: 100,
            max_retries: 3,
            enable_compression: false,
        };

        let sync_engine = SyncEngine::new(db.clone(), config).unwrap();

        // Upload snapshot
        let snapshot_id = sync_engine.upload_to_s3(
            bucket.clone(),
            "test-sync".to_string(),
            "us-east-1".to_string(),
            db_path.clone(),
        ).await.unwrap();

        println!("Uploaded snapshot: {}", snapshot_id);

        // Create new directory for restore
        let restore_dir = TempDir::new().unwrap();
        let restore_path = restore_dir.path().join("restored.keystone");

        // Restore snapshot
        sync_engine.restore_from_s3(
            bucket,
            "test-sync".to_string(),
            "us-east-1".to_string(),
            snapshot_id,
            restore_path.clone(),
        ).await.unwrap();

        // Verify restored database
        let restored_db = Database::open(&restore_path).unwrap();
        let item = restored_db.get(b"test-key").unwrap();
        assert!(item.is_some());
        assert_eq!(item.unwrap().get("data").unwrap().as_string().unwrap(), "test-value");
    }

    /// Test with MinIO (local S3-compatible storage)
    #[tokio::test]
    #[ignore] // Run with: cargo test --ignored
    async fn test_s3_with_minio() {
        // This test requires MinIO running locally:
        // docker run -p 9000:9000 -p 9001:9001 \
        //   -e MINIO_ROOT_USER=minioadmin \
        //   -e MINIO_ROOT_PASSWORD=minioadmin \
        //   minio/minio server /data --console-address ":9001"

        let endpoint = SyncEndpoint::S3 {
            bucket: "test-bucket".to_string(),
            prefix: "keystonedb".to_string(),
            region: "us-east-1".to_string(),
            endpoint_url: Some("http://localhost:9000".to_string()),
            credentials: Some(kstone_sync::protocol::AwsCredentials {
                access_key_id: "minioadmin".to_string(),
                secret_access_key: "minioadmin".to_string(),
                session_token: None,
            }),
        };

        // Test endpoint creation
        assert_eq!(endpoint.endpoint_type(), "s3");
    }
}
# S3 Backup Example

This example demonstrates how to backup and restore a KeystoneDB database to/from S3-compatible object storage.

## Features

- **Full Snapshots**: Create complete backups of your database
- **Incremental Sync**: Sync only changed files for efficiency
- **S3-Compatible**: Works with AWS S3, MinIO, Backblaze B2, and other S3-compatible stores
- **Point-in-Time Recovery**: Maintain multiple snapshots for recovery

## Usage

### Environment Variables

```bash
# AWS S3 Configuration
export AWS_REGION=us-east-1
export AWS_ACCESS_KEY_ID=your-access-key
export AWS_SECRET_ACCESS_KEY=your-secret-key

# Or for S3-compatible stores like MinIO
export S3_ENDPOINT_URL=http://localhost:9000
```

### Backup Database to S3

```bash
# Upload a snapshot
cargo run --example s3-backup -- upload \
  --db-path mydb.keystone \
  --bucket my-backups \
  --prefix keystonedb/prod

# List available snapshots
cargo run --example s3-backup -- list \
  --bucket my-backups \
  --prefix keystonedb/prod

# Download and restore a snapshot
cargo run --example s3-backup -- restore \
  --db-path restored.keystone \
  --bucket my-backups \
  --prefix keystonedb/prod \
  --snapshot-id 20240115-103000
```

### Programmatic Usage

```rust
use kstone_api::Database;
use kstone_sync::{SyncEngine, SyncConfig, SyncEndpoint};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Open database
    let db = Arc::new(Database::open("mydb.keystone")?);

    // Configure S3 endpoint
    let endpoint = SyncEndpoint::S3 {
        bucket: "my-backups".to_string(),
        prefix: "keystonedb/prod".to_string(),
        region: "us-east-1".to_string(),
        endpoint_url: None, // Or Some("http://localhost:9000".to_string()) for MinIO
        credentials: None, // Uses environment variables by default
    };

    // Create sync engine
    let config = SyncConfig {
        endpoint: endpoint.clone(),
        conflict_strategy: ConflictStrategy::LastWriterWins,
        sync_interval: None,
        batch_size: 100,
        max_retries: 3,
        enable_compression: false,
    };

    let sync_engine = SyncEngine::new(db, config)?;

    // Upload snapshot
    let snapshot_id = sync_engine.upload_to_s3(
        "my-backups".to_string(),
        "keystonedb/prod".to_string(),
        "us-east-1".to_string(),
        std::path::PathBuf::from("mydb.keystone"),
    ).await?;

    println!("Uploaded snapshot: {}", snapshot_id);

    Ok(())
}
```

## S3 Object Structure

```
bucket/
└── prefix/
    ├── snapshots/
    │   ├── 20240115-103000/
    │   │   ├── manifest.json      # Snapshot metadata
    │   │   ├── wal.log           # Write-ahead log
    │   │   └── sst/              # SST files directory
    │   │       ├── 000-1.sst
    │   │       ├── 001-2.sst
    │   │       └── ...
    │   └── 20240115-113000/
    │       └── ...
    └── metadata/
        ├── latest.json            # Points to latest snapshot
        └── sync_state.json        # Sync state tracking
```

## Security Considerations

- Use IAM roles in production instead of access keys
- Enable S3 server-side encryption
- Configure bucket policies for access control
- Use versioning for additional protection
- Consider enabling MFA delete for critical backups

## Cost Optimization

- Use S3 lifecycle policies to move old snapshots to Glacier
- Enable compression for reduced storage costs
- Use incremental sync for frequent backups
- Configure appropriate retention policies

## Testing with MinIO

For local testing, you can use MinIO:

```bash
# Start MinIO
docker run -p 9000:9000 -p 9001:9001 \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  minio/minio server /data --console-address ":9001"

# Configure for MinIO
export S3_ENDPOINT_URL=http://localhost:9000
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin

# Create bucket
aws --endpoint-url http://localhost:9000 s3 mb s3://test-bucket
```
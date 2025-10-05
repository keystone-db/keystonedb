/// S3 Backup Example for KeystoneDB
///
/// This example demonstrates how to backup a KeystoneDB database to S3.

use anyhow::Result;
use kstone_api::Database;
use kstone_sync::{SyncEngine, SyncConfig, ConflictStrategy, SyncEndpoint};
use std::sync::Arc;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        return Ok(());
    }

    match args[1].as_str() {
        "upload" => {
            if args.len() < 5 {
                println!("Usage: {} upload <db-path> <bucket> <prefix>", args[0]);
                return Ok(());
            }

            let db_path = PathBuf::from(&args[2]);
            let bucket = &args[3];
            let prefix = &args[4];

            upload_snapshot(&db_path, bucket, prefix).await?;
        }
        "list" => {
            if args.len() < 4 {
                println!("Usage: {} list <bucket> <prefix>", args[0]);
                return Ok(());
            }

            let bucket = &args[2];
            let prefix = &args[3];

            list_snapshots(bucket, prefix).await?;
        }
        "restore" => {
            if args.len() < 6 {
                println!("Usage: {} restore <db-path> <bucket> <prefix> <snapshot-id>", args[0]);
                return Ok(());
            }

            let db_path = PathBuf::from(&args[2]);
            let bucket = &args[3];
            let prefix = &args[4];
            let snapshot_id = &args[5];

            restore_snapshot(&db_path, bucket, prefix, snapshot_id).await?;
        }
        _ => {
            print_usage();
        }
    }

    Ok(())
}

fn print_usage() {
    println!("KeystoneDB S3 Backup Tool");
    println!();
    println!("Commands:");
    println!("  upload <db-path> <bucket> <prefix>              - Upload database snapshot to S3");
    println!("  list <bucket> <prefix>                          - List available snapshots");
    println!("  restore <db-path> <bucket> <prefix> <snapshot>  - Restore database from snapshot");
    println!();
    println!("Environment Variables:");
    println!("  AWS_REGION            - AWS region (default: us-east-1)");
    println!("  AWS_ACCESS_KEY_ID     - AWS access key");
    println!("  AWS_SECRET_ACCESS_KEY - AWS secret key");
    println!("  S3_ENDPOINT_URL       - Custom S3 endpoint (for MinIO, etc.)");
}

async fn upload_snapshot(db_path: &PathBuf, bucket: &str, prefix: &str) -> Result<()> {
    println!("Uploading snapshot from {:?} to s3://{}/{}", db_path, bucket, prefix);

    // Open the database
    let db = Arc::new(Database::open(db_path)?);

    // Get region from environment or use default
    let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let endpoint_url = std::env::var("S3_ENDPOINT_URL").ok();

    // Configure S3 endpoint
    let endpoint = SyncEndpoint::S3 {
        bucket: bucket.to_string(),
        prefix: prefix.to_string(),
        region: region.clone(),
        endpoint_url: endpoint_url.clone(),
        credentials: None, // Uses environment variables
    };

    // Create sync engine with dummy endpoint (we'll use specific S3 methods)
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

    let sync_engine = SyncEngine::new(db, config)?;

    // Upload snapshot
    let snapshot_id = sync_engine.upload_to_s3(
        bucket.to_string(),
        prefix.to_string(),
        region,
        db_path.clone(),
    ).await?;

    println!("✅ Successfully uploaded snapshot: {}", snapshot_id);
    println!("   Bucket: {}", bucket);
    println!("   Prefix: {}", prefix);

    Ok(())
}

async fn list_snapshots(bucket: &str, prefix: &str) -> Result<()> {
    use kstone_sync::protocol::s3::S3Protocol;

    println!("Listing snapshots in s3://{}/{}", bucket, prefix);

    let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let endpoint_url = std::env::var("S3_ENDPOINT_URL").ok();

    let mut protocol = S3Protocol::new(
        bucket.to_string(),
        prefix.to_string(),
        region,
        endpoint_url,
        None, // Uses environment variables
    );

    protocol.connect().await?;
    let snapshots = protocol.list_snapshots().await?;
    protocol.disconnect().await?;

    if snapshots.is_empty() {
        println!("No snapshots found");
    } else {
        println!("Found {} snapshot(s):", snapshots.len());
        println!();
        for snapshot in snapshots {
            println!("  ID:        {}", snapshot.id);
            println!("  Timestamp: {}", snapshot.timestamp);
            println!("  Files:     {}", snapshot.file_count);
            println!("  Size:      {} bytes", snapshot.total_size);
            println!();
        }
    }

    Ok(())
}

async fn restore_snapshot(db_path: &PathBuf, bucket: &str, prefix: &str, snapshot_id: &str) -> Result<()> {
    println!("Restoring snapshot {} from s3://{}/{} to {:?}",
             snapshot_id, bucket, prefix, db_path);

    if db_path.exists() {
        println!("⚠️  Warning: Database directory already exists at {:?}", db_path);
        println!("   Please remove it or choose a different path");
        return Ok(());
    }

    // Create the database directory
    std::fs::create_dir_all(db_path)?;

    // Get region from environment or use default
    let region = std::env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let endpoint_url = std::env::var("S3_ENDPOINT_URL").ok();

    // Create a temporary database to satisfy the sync engine requirement
    let db = Arc::new(Database::create(db_path)?);

    // Create sync engine with dummy endpoint
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

    let sync_engine = SyncEngine::new(db, config)?;

    // Restore from S3
    sync_engine.restore_from_s3(
        bucket.to_string(),
        prefix.to_string(),
        region,
        snapshot_id.to_string(),
        db_path.clone(),
    ).await?;

    println!("✅ Successfully restored snapshot: {}", snapshot_id);
    println!("   Database path: {:?}", db_path);

    Ok(())
}
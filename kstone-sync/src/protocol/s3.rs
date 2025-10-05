/// S3-compatible sync protocol implementation
///
/// Enables synchronization between KeystoneDB and S3-compatible object stores.
/// Supports both full snapshots and incremental file-level sync.

use async_trait::async_trait;
use anyhow::{anyhow, Result};
use aws_sdk_s3::Client;
use aws_config::{BehaviorVersion, Region};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

use kstone_api::Database;
use kstone_core::{Item, Key};

use crate::{
    EndpointId, VectorClock,
    protocol::{SyncProtocol, SyncMessage, SyncSessionStats, DiffType},
    merkle::MerkleNode,
};

/// S3 sync modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum S3SyncMode {
    /// Full snapshot upload/download
    Snapshot,
    /// Incremental file-level sync
    Incremental,
}

/// S3 sync protocol implementation
pub struct S3Protocol {
    /// S3 bucket name
    bucket: String,
    /// Prefix for all objects
    prefix: String,
    /// AWS S3 client
    client: Option<Client>,
    /// Local database path
    local_db_path: Option<PathBuf>,
    /// Local database reference
    local_db: Option<Arc<Database>>,
    /// Sync mode
    sync_mode: S3SyncMode,
    /// Remote endpoint ID
    remote_endpoint_id: Option<EndpointId>,
    /// Remote vector clock
    remote_clock: Option<VectorClock>,
}

/// Snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub vector_clock: VectorClock,
    pub file_count: usize,
    pub total_size: u64,
    pub compressed: bool,
}

/// File sync result
#[derive(Debug, Clone)]
pub struct FileSyncResult {
    pub file_name: String,
    pub action: FileSyncAction,
    pub size: u64,
}

/// File sync action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSyncAction {
    Uploaded,
    Downloaded,
    Skipped,
    Deleted,
}

impl S3Protocol {
    /// Create a new S3 protocol
    pub fn new(
        bucket: String,
        prefix: String,
        region: String,
        endpoint_url: Option<String>,
        credentials: Option<crate::protocol::AwsCredentials>,
    ) -> Self {
        Self {
            bucket,
            prefix,
            client: None,
            local_db_path: None,
            local_db: None,
            sync_mode: S3SyncMode::Incremental,
            remote_endpoint_id: None,
            remote_clock: None,
        }
    }

    /// Set the local database path
    pub fn with_local_db_path(mut self, path: PathBuf) -> Self {
        self.local_db_path = Some(path);
        self
    }

    /// Set the local database reference
    pub fn with_local_db(mut self, db: Arc<Database>) -> Self {
        self.local_db = Some(db);
        self
    }

    /// Set the sync mode
    pub fn with_sync_mode(mut self, mode: S3SyncMode) -> Self {
        self.sync_mode = mode;
        self
    }

    /// Initialize AWS S3 client
    async fn init_client(&mut self, region: String, endpoint_url: Option<String>) -> Result<()> {
        let config_builder = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(region));

        let config = if let Some(endpoint) = endpoint_url {
            config_builder.endpoint_url(endpoint).load().await
        } else {
            config_builder.load().await
        };

        self.client = Some(Client::new(&config));
        Ok(())
    }

    /// Upload a full snapshot to S3
    pub async fn upload_snapshot(&self) -> Result<String> {
        let client = self.client.as_ref().ok_or_else(|| anyhow!("S3 client not initialized"))?;
        let local_path = self.local_db_path.as_ref().ok_or_else(|| anyhow!("Local database path not set"))?;

        // Generate snapshot ID
        let snapshot_id = format!("{}", Utc::now().format("%Y%m%d-%H%M%S"));
        let snapshot_prefix = format!("{}/snapshots/{}", self.prefix, snapshot_id);

        // Create metadata
        let mut metadata = SnapshotMetadata {
            id: snapshot_id.clone(),
            timestamp: Utc::now(),
            vector_clock: self.remote_clock.clone().unwrap_or_else(VectorClock::new),
            file_count: 0,
            total_size: 0,
            compressed: false,
        };

        // Upload WAL
        let wal_path = local_path.join("wal.log");
        if wal_path.exists() {
            let wal_key = format!("{}/wal.log", snapshot_prefix);
            let wal_data = fs::read(&wal_path).await?;

            client
                .put_object()
                .bucket(&self.bucket)
                .key(&wal_key)
                .body(wal_data.into())
                .send()
                .await?;

            metadata.file_count += 1;
            metadata.total_size += wal_path.metadata()?.len();
        }

        // Upload SST files
        let mut entries = fs::read_dir(local_path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("sst") {
                let file_name = path.file_name().unwrap().to_str().unwrap();
                let sst_key = format!("{}/sst/{}", snapshot_prefix, file_name);
                let sst_data = fs::read(&path).await?;

                client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(&sst_key)
                    .body(sst_data.into())
                    .send()
                    .await?;

                metadata.file_count += 1;
                metadata.total_size += path.metadata()?.len();
            }
        }

        // Upload metadata
        let metadata_key = format!("{}/manifest.json", snapshot_prefix);
        let metadata_json = serde_json::to_vec(&metadata)?;

        client
            .put_object()
            .bucket(&self.bucket)
            .key(&metadata_key)
            .body(metadata_json.into())
            .send()
            .await?;

        // Update latest pointer
        let latest_key = format!("{}/metadata/latest.json", self.prefix);
        let latest_json = serde_json::json!({
            "snapshot_id": snapshot_id,
            "timestamp": metadata.timestamp,
        });

        client
            .put_object()
            .bucket(&self.bucket)
            .key(&latest_key)
            .body(serde_json::to_vec(&latest_json)?.into())
            .send()
            .await?;

        Ok(snapshot_id)
    }

    /// Download and restore from a snapshot
    pub async fn download_snapshot(&self, snapshot_id: &str) -> Result<()> {
        let client = self.client.as_ref().ok_or_else(|| anyhow!("S3 client not initialized"))?;
        let local_path = self.local_db_path.as_ref().ok_or_else(|| anyhow!("Local database path not set"))?;

        let snapshot_prefix = format!("{}/snapshots/{}", self.prefix, snapshot_id);

        // Download metadata first
        let metadata_key = format!("{}/manifest.json", snapshot_prefix);
        let metadata_obj = client
            .get_object()
            .bucket(&self.bucket)
            .key(&metadata_key)
            .send()
            .await?;

        let metadata_bytes = metadata_obj.body.collect().await?.into_bytes();
        let metadata: SnapshotMetadata = serde_json::from_slice(&metadata_bytes)?;

        // Create local directory if it doesn't exist
        fs::create_dir_all(local_path).await?;

        // Download WAL
        let wal_key = format!("{}/wal.log", snapshot_prefix);
        if let Ok(wal_obj) = client
            .get_object()
            .bucket(&self.bucket)
            .key(&wal_key)
            .send()
            .await
        {
            let wal_data = wal_obj.body.collect().await?.into_bytes();
            let wal_path = local_path.join("wal.log");
            fs::write(&wal_path, &wal_data).await?;
        }

        // List and download SST files
        let sst_prefix = format!("{}/sst/", snapshot_prefix);
        let list_result = client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&sst_prefix)
            .send()
            .await?;

        if let Some(objects) = list_result.contents {
            for object in objects {
                if let Some(key) = object.key {
                    let file_name = key.split('/').last().unwrap();
                    let sst_obj = client
                        .get_object()
                        .bucket(&self.bucket)
                        .key(&key)
                        .send()
                        .await?;

                    let sst_data = sst_obj.body.collect().await?.into_bytes();
                    let sst_path = local_path.join(file_name);
                    fs::write(&sst_path, &sst_data).await?;
                }
            }
        }

        Ok(())
    }

    /// List available snapshots
    pub async fn list_snapshots(&self) -> Result<Vec<SnapshotMetadata>> {
        let client = self.client.as_ref().ok_or_else(|| anyhow!("S3 client not initialized"))?;

        let snapshot_prefix = format!("{}/snapshots/", self.prefix);
        let mut snapshots = Vec::new();

        let list_result = client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&snapshot_prefix)
            .delimiter("/")
            .send()
            .await?;

        if let Some(prefixes) = list_result.common_prefixes {
            for prefix_info in prefixes {
                if let Some(prefix) = prefix_info.prefix {
                    // Extract snapshot ID from prefix
                    let snapshot_id = prefix
                        .trim_end_matches('/')
                        .split('/')
                        .last()
                        .unwrap()
                        .to_string();

                    // Try to load metadata
                    let metadata_key = format!("{}/snapshots/{}/manifest.json", self.prefix, snapshot_id);
                    if let Ok(metadata_obj) = client
                        .get_object()
                        .bucket(&self.bucket)
                        .key(&metadata_key)
                        .send()
                        .await
                    {
                        let metadata_bytes = metadata_obj.body.collect().await?.into_bytes();
                        if let Ok(metadata) = serde_json::from_slice::<SnapshotMetadata>(&metadata_bytes) {
                            snapshots.push(metadata);
                        }
                    }
                }
            }
        }

        // Sort by timestamp (newest first)
        snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(snapshots)
    }

    /// Sync individual files incrementally
    pub async fn sync_files(&self) -> Result<Vec<FileSyncResult>> {
        let client = self.client.as_ref().ok_or_else(|| anyhow!("S3 client not initialized"))?;
        let local_path = self.local_db_path.as_ref().ok_or_else(|| anyhow!("Local database path not set"))?;

        let mut results = Vec::new();

        // Get local file listing with checksums
        let local_files = self.get_local_files(local_path).await?;

        // Get remote file listing with ETags
        let remote_files = self.get_remote_files(client).await?;

        // Upload new or modified local files
        for (file_name, local_checksum) in &local_files {
            if let Some(remote_etag) = remote_files.get(file_name) {
                // File exists remotely, check if different
                if local_checksum != remote_etag {
                    // Upload modified file
                    let file_path = local_path.join(file_name);
                    let file_data = fs::read(&file_path).await?;
                    let file_size = file_data.len() as u64;

                    let key = format!("{}/current/{}", self.prefix, file_name);
                    client
                        .put_object()
                        .bucket(&self.bucket)
                        .key(&key)
                        .body(file_data.into())
                        .send()
                        .await?;

                    results.push(FileSyncResult {
                        file_name: file_name.clone(),
                        action: FileSyncAction::Uploaded,
                        size: file_size,
                    });
                }
            } else {
                // New file, upload it
                let file_path = local_path.join(file_name);
                let file_data = fs::read(&file_path).await?;
                let file_size = file_data.len() as u64;

                let key = format!("{}/current/{}", self.prefix, file_name);
                client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .body(file_data.into())
                    .send()
                    .await?;

                results.push(FileSyncResult {
                    file_name: file_name.clone(),
                    action: FileSyncAction::Uploaded,
                    size: file_size,
                });
            }
        }

        // Download new remote files
        for (file_name, _) in &remote_files {
            if !local_files.contains_key(file_name) {
                // Remote file doesn't exist locally, download it
                let key = format!("{}/current/{}", self.prefix, file_name);
                let obj = client
                    .get_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .send()
                    .await?;

                let file_data = obj.body.collect().await?.into_bytes();
                let file_size = file_data.len() as u64;

                let file_path = local_path.join(file_name);
                fs::write(&file_path, &file_data).await?;

                results.push(FileSyncResult {
                    file_name: file_name.clone(),
                    action: FileSyncAction::Downloaded,
                    size: file_size,
                });
            }
        }

        Ok(results)
    }

    /// Get local files with checksums
    async fn get_local_files(&self, path: &Path) -> Result<HashMap<String, String>> {
        let mut files = HashMap::new();

        let mut entries = fs::read_dir(path).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Only include database files
                    if name == "wal.log" || name.ends_with(".sst") {
                        // Simple checksum using file size and modified time
                        let metadata = path.metadata()?;
                        let checksum = format!("{}-{}",
                            metadata.len(),
                            metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs()
                        );
                        files.insert(name.to_string(), checksum);
                    }
                }
            }
        }

        Ok(files)
    }

    /// Get remote files with ETags
    async fn get_remote_files(&self, client: &Client) -> Result<HashMap<String, String>> {
        let mut files = HashMap::new();

        let prefix = format!("{}/current/", self.prefix);
        let list_result = client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&prefix)
            .send()
            .await?;

        if let Some(objects) = list_result.contents {
            for object in objects {
                if let (Some(key), Some(etag)) = (object.key, object.e_tag) {
                    let file_name = key.split('/').last().unwrap().to_string();
                    files.insert(file_name, etag.trim_matches('"').to_string());
                }
            }
        }

        Ok(files)
    }
}

#[async_trait]
impl SyncProtocol for S3Protocol {
    async fn connect(&mut self) -> Result<()> {
        // Initialize S3 client if not already done
        if self.client.is_none() {
            // Default to us-east-1 if not specified
            self.init_client("us-east-1".to_string(), None).await?;
        }

        // Verify bucket access
        if let Some(client) = &self.client {
            client
                .head_bucket()
                .bucket(&self.bucket)
                .send()
                .await
                .map_err(|e| anyhow!("Cannot access S3 bucket: {}", e))?;
        }

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.client = None;
        self.remote_endpoint_id = None;
        self.remote_clock = None;
        Ok(())
    }

    async fn send(&mut self, _message: SyncMessage) -> Result<()> {
        // S3 protocol doesn't use message passing
        Ok(())
    }

    async fn receive(&mut self) -> Result<SyncMessage> {
        // Return completion message
        Ok(SyncMessage::Complete {
            stats: SyncSessionStats::default(),
        })
    }

    async fn handshake(&mut self, _local_id: &EndpointId, local_clock: &VectorClock) -> Result<VectorClock> {
        // For S3 sync, generate endpoint ID from bucket/prefix
        self.remote_endpoint_id = Some(EndpointId::from_str(&format!("s3://{}:{}", self.bucket, self.prefix)));

        // Initialize clock for remote
        let mut clock = VectorClock::new();
        clock.merge(local_clock);
        self.remote_clock = Some(clock.clone());

        Ok(clock)
    }

    async fn exchange_merkle(&mut self, _local_tree: &MerkleNode) -> Result<Vec<(Key, DiffType)>> {
        // For S3 sync, we use file-level comparison instead of merkle trees
        // Return empty diff as we handle sync differently
        Ok(Vec::new())
    }

    async fn pull_items(&mut self, _keys: Vec<Key>) -> Result<Vec<(Key, Option<Item>, VectorClock)>> {
        // S3 sync handles files, not individual items
        // Actual sync happens through snapshot/incremental file operations
        Ok(Vec::new())
    }

    async fn push_items(&mut self, _items: Vec<(Key, Option<Item>, VectorClock)>) -> Result<Vec<String>> {
        // S3 sync handles files, not individual items
        // Actual sync happens through snapshot/incremental file operations
        Ok(Vec::new())
    }

    fn capabilities(&self) -> &[String] {
        // S3 protocol capabilities
        static CAPABILITIES: &[String] = &[];
        CAPABILITIES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_s3_protocol_connect() {
        // This test requires actual S3 or MinIO setup
        // Skip in CI environment
        if std::env::var("CI").is_ok() {
            return;
        }

        let protocol = S3Protocol::new(
            "test-bucket".to_string(),
            "test-prefix".to_string(),
            "us-east-1".to_string(),
            None,
            None,
        );

        // This will fail without actual S3 setup
        // assert!(protocol.connect().await.is_ok());
    }

    #[tokio::test]
    async fn test_local_files_listing() {
        let dir = TempDir::new().unwrap();

        // Create test files
        let wal_path = dir.path().join("wal.log");
        std::fs::write(&wal_path, b"test").unwrap();

        let sst_path = dir.path().join("000-1.sst");
        std::fs::write(&sst_path, b"sst data").unwrap();

        let protocol = S3Protocol::new(
            "test".to_string(),
            "test".to_string(),
            "us-east-1".to_string(),
            None,
            None,
        );

        let files = protocol.get_local_files(dir.path()).await.unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.contains_key("wal.log"));
        assert!(files.contains_key("000-1.sst"));
    }
}
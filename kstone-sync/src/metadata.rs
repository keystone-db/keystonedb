/// Sync metadata storage in KeystoneDB
///
/// Stores sync-related metadata such as checkpoints, pending changes,
/// and conflict queues in special reserved tables within the database.

use anyhow::Result;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use kstone_api::{Database, ItemBuilder, KeystoneValue as Value};
use kstone_core::{Item, Key};

use crate::{
    EndpointId, VectorClock, SyncRecord, Conflict, ConflictStrategy,
    SyncStats, change_tracker::SyncRecord as TrackedRecord,
};

/// Prefixes for sync metadata tables
const SYNC_METADATA_PREFIX: &str = "_sync#metadata#";
const SYNC_CHECKPOINT_PREFIX: &str = "_sync#checkpoint#";
const SYNC_PENDING_PREFIX: &str = "_sync#pending#";
const SYNC_CONFLICT_PREFIX: &str = "_sync#conflict#";
const SYNC_ENDPOINT_PREFIX: &str = "_sync#endpoint#";

/// Sync metadata stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetadata {
    /// Local endpoint ID
    pub local_endpoint: EndpointId,
    /// Remote endpoints we sync with
    pub remote_endpoints: Vec<EndpointInfo>,
    /// Current vector clock
    pub vector_clock: VectorClock,
    /// Last sync time for each endpoint
    pub last_sync_times: HashMap<EndpointId, i64>,
    /// Sync configuration
    pub config: SyncMetadataConfig,
    /// Statistics
    pub stats: SyncStats,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
}

impl SyncMetadata {
    /// Create new sync metadata
    pub fn new(local_endpoint: EndpointId) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        let mut vector_clock = VectorClock::new();
        vector_clock.update(local_endpoint.clone(), 0);

        Self {
            local_endpoint,
            remote_endpoints: Vec::new(),
            vector_clock,
            last_sync_times: HashMap::new(),
            config: SyncMetadataConfig::default(),
            stats: SyncStats::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a remote endpoint
    pub fn add_endpoint(&mut self, endpoint: EndpointInfo) {
        if !self.remote_endpoints.iter().any(|e| e.id == endpoint.id) {
            self.remote_endpoints.push(endpoint);
            self.updated_at = chrono::Utc::now().timestamp_millis();
        }
    }

    /// Update last sync time for an endpoint
    pub fn update_sync_time(&mut self, endpoint: &EndpointId) {
        self.last_sync_times.insert(
            endpoint.clone(),
            chrono::Utc::now().timestamp_millis(),
        );
        self.updated_at = chrono::Utc::now().timestamp_millis();
    }

    /// Get last sync time for an endpoint
    pub fn get_last_sync_time(&self, endpoint: &EndpointId) -> Option<i64> {
        self.last_sync_times.get(endpoint).copied()
    }
}

/// Information about a remote endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointInfo {
    /// Endpoint ID
    pub id: EndpointId,
    /// Endpoint URL or address
    pub url: String,
    /// Endpoint type (DynamoDB, HTTP, etc.)
    pub endpoint_type: String,
    /// Whether this endpoint is active
    pub active: bool,
    /// Last known vector clock from this endpoint
    pub last_clock: Option<VectorClock>,
}

/// Sync metadata configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetadataConfig {
    /// Default conflict resolution strategy
    pub conflict_strategy: ConflictStrategy,
    /// Whether sync is enabled
    pub sync_enabled: bool,
    /// Sync interval in milliseconds
    pub sync_interval_ms: Option<u64>,
    /// Maximum pending changes
    pub max_pending_changes: usize,
    /// Maximum retry attempts
    pub max_retry_attempts: u32,
}

impl Default for SyncMetadataConfig {
    fn default() -> Self {
        Self {
            conflict_strategy: ConflictStrategy::LastWriterWins,
            sync_enabled: true,
            sync_interval_ms: Some(30_000), // 30 seconds
            max_pending_changes: 10_000,
            max_retry_attempts: 3,
        }
    }
}

/// Checkpoint information for sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCheckpoint {
    /// Endpoint this checkpoint is for
    pub endpoint_id: EndpointId,
    /// Last synced sequence number
    pub last_sequence: u64,
    /// Vector clock at checkpoint
    pub vector_clock: VectorClock,
    /// Merkle tree root hash at checkpoint
    pub merkle_root: Option<Bytes>,
    /// Timestamp of checkpoint
    pub timestamp: i64,
}

/// Storage operations for sync metadata
pub struct SyncMetadataStore<'a> {
    db: &'a Database,
}

impl<'a> SyncMetadataStore<'a> {
    /// Create a new metadata store
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Initialize sync metadata tables
    pub fn initialize(&self) -> Result<()> {
        // Create metadata entry if it doesn't exist
        let key = format!("{}{}", SYNC_METADATA_PREFIX, "main");
        if self.db.get(key.as_bytes())?.is_none() {
            let local_endpoint = EndpointId::new();
            let metadata = SyncMetadata::new(local_endpoint);
            self.save_metadata(&metadata)?;
        }
        Ok(())
    }

    /// Save sync metadata
    pub fn save_metadata(&self, metadata: &SyncMetadata) -> Result<()> {
        let key = format!("{}{}", SYNC_METADATA_PREFIX, "main");
        let json = serde_json::to_string(metadata)?;

        let item = ItemBuilder::new()
            .string("type", "sync_metadata")
            .string("content", json)
            .number("updated_at", metadata.updated_at)
            .build();

        self.db.put(key.as_bytes(), item)?;
        Ok(())
    }

    /// Load sync metadata
    pub fn load_metadata(&self) -> Result<Option<SyncMetadata>> {
        let key = format!("{}{}", SYNC_METADATA_PREFIX, "main");

        if let Some(item) = self.db.get(key.as_bytes())? {
            if let Some(Value::S(content)) = item.get("content") {
                let metadata: SyncMetadata = serde_json::from_str(content)?;
                return Ok(Some(metadata));
            }
        }

        Ok(None)
    }

    /// Save a checkpoint
    pub fn save_checkpoint(&self, checkpoint: &SyncCheckpoint) -> Result<()> {
        let key = format!("{}{}", SYNC_CHECKPOINT_PREFIX, checkpoint.endpoint_id.0);
        let json = serde_json::to_string(checkpoint)?;

        let item = ItemBuilder::new()
            .string("type", "sync_checkpoint")
            .string("endpoint_id", &checkpoint.endpoint_id.0)
            .string("content", json)
            .number("last_sequence", checkpoint.last_sequence as i64)
            .number("timestamp", checkpoint.timestamp)
            .build();

        self.db.put(key.as_bytes(), item)?;
        Ok(())
    }

    /// Load checkpoint for an endpoint
    pub fn load_checkpoint(&self, endpoint_id: &EndpointId) -> Result<Option<SyncCheckpoint>> {
        let key = format!("{}{}", SYNC_CHECKPOINT_PREFIX, endpoint_id.0);

        if let Some(item) = self.db.get(key.as_bytes())? {
            if let Some(Value::S(content)) = item.get("content") {
                let checkpoint: SyncCheckpoint = serde_json::from_str(content)?;
                return Ok(Some(checkpoint));
            }
        }

        Ok(None)
    }

    /// Save a pending sync record
    pub fn save_pending_record(&self, record: &TrackedRecord) -> Result<()> {
        let key = format!("{}{}", SYNC_PENDING_PREFIX, record.id);
        let json = serde_json::to_string(record)?;

        let item = ItemBuilder::new()
            .string("type", "pending_sync")
            .string("record_id", &record.id)
            .string("content", json)
            .number("sequence", record.stream_record.sequence_number as i64)
            .number("created_at", record.created_at)
            .bool("synced", record.synced)
            .build();

        self.db.put(key.as_bytes(), item)?;
        Ok(())
    }

    /// Load pending sync records
    pub fn load_pending_records(&self, limit: Option<usize>) -> Result<Vec<TrackedRecord>> {
        let mut records = Vec::new();

        // This would need a scan operation with prefix filter
        // For now, we'll implement a simple version
        // In production, this would use an index on the pending records

        // Placeholder implementation - would need proper prefix scan
        Ok(records)
    }

    /// Delete a pending record
    pub fn delete_pending_record(&self, record_id: &str) -> Result<()> {
        let key = format!("{}{}", SYNC_PENDING_PREFIX, record_id);
        self.db.delete(key.as_bytes())?;
        Ok(())
    }

    /// Save a conflict
    pub fn save_conflict(&self, conflict: &Conflict) -> Result<()> {
        let key = format!("{}{}", SYNC_CONFLICT_PREFIX, conflict.id);
        let json = serde_json::to_string(conflict)?;

        let item = ItemBuilder::new()
            .string("type", "sync_conflict")
            .string("conflict_id", &conflict.id)
            .string("content", json)
            .number("detected_at", conflict.detected_at)
            .bool("resolved", conflict.resolved)
            .build();

        self.db.put(key.as_bytes(), item)?;
        Ok(())
    }

    /// Load conflicts
    pub fn load_conflicts(&self, only_pending: bool) -> Result<Vec<Conflict>> {
        let mut conflicts = Vec::new();

        // Placeholder implementation - would need proper prefix scan
        // with filtering based on resolved status

        Ok(conflicts)
    }

    /// Delete a conflict
    pub fn delete_conflict(&self, conflict_id: &str) -> Result<()> {
        let key = format!("{}{}", SYNC_CONFLICT_PREFIX, conflict_id);
        self.db.delete(key.as_bytes())?;
        Ok(())
    }

    /// Save endpoint information
    pub fn save_endpoint(&self, endpoint: &EndpointInfo) -> Result<()> {
        let key = format!("{}{}", SYNC_ENDPOINT_PREFIX, endpoint.id.0);
        let json = serde_json::to_string(endpoint)?;

        let item = ItemBuilder::new()
            .string("type", "sync_endpoint")
            .string("endpoint_id", &endpoint.id.0)
            .string("url", &endpoint.url)
            .string("endpoint_type", &endpoint.endpoint_type)
            .bool("active", endpoint.active)
            .string("content", json)
            .build();

        self.db.put(key.as_bytes(), item)?;
        Ok(())
    }

    /// Load endpoint information
    pub fn load_endpoint(&self, endpoint_id: &EndpointId) -> Result<Option<EndpointInfo>> {
        let key = format!("{}{}", SYNC_ENDPOINT_PREFIX, endpoint_id.0);

        if let Some(item) = self.db.get(key.as_bytes())? {
            if let Some(Value::S(content)) = item.get("content") {
                let endpoint: EndpointInfo = serde_json::from_str(content)?;
                return Ok(Some(endpoint));
            }
        }

        Ok(None)
    }

    /// Clean up old metadata
    pub fn cleanup_old_metadata(&self, older_than_ms: i64) -> Result<usize> {
        let cutoff = chrono::Utc::now().timestamp_millis() - older_than_ms;
        let mut deleted = 0;

        // Would implement cleanup of old pending records, resolved conflicts, etc.
        // This requires scanning with prefix and checking timestamps

        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_sync_metadata_store() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();
        let store = SyncMetadataStore::new(&db);

        // Initialize
        store.initialize().unwrap();

        // Load metadata
        let metadata = store.load_metadata().unwrap();
        assert!(metadata.is_some());

        let mut metadata = metadata.unwrap();

        // Add endpoint
        let endpoint = EndpointInfo {
            id: EndpointId::from_str("remote1"),
            url: "https://dynamodb.us-east-1.amazonaws.com".to_string(),
            endpoint_type: "DynamoDB".to_string(),
            active: true,
            last_clock: None,
        };
        metadata.add_endpoint(endpoint.clone());

        // Save and reload
        store.save_metadata(&metadata).unwrap();
        let reloaded = store.load_metadata().unwrap().unwrap();
        assert_eq!(reloaded.remote_endpoints.len(), 1);
    }

    #[test]
    fn test_checkpoint_storage() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();
        let store = SyncMetadataStore::new(&db);

        let checkpoint = SyncCheckpoint {
            endpoint_id: EndpointId::from_str("remote1"),
            last_sequence: 100,
            vector_clock: VectorClock::new(),
            merkle_root: Some(Bytes::from("hash")),
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        store.save_checkpoint(&checkpoint).unwrap();

        let loaded = store.load_checkpoint(&checkpoint.endpoint_id).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().last_sequence, 100);
    }
}
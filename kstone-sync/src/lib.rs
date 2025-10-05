/// Cloud sync functionality for KeystoneDB
///
/// Provides bidirectional synchronization with cloud databases (DynamoDB, etc.)
/// using vector clocks for causality tracking and merkle trees for efficient
/// diff detection.

pub mod vector_clock;
pub mod merkle;
pub mod change_tracker;
pub mod conflict;
pub mod sync_engine;
pub mod offline_queue;
pub mod metadata;
pub mod protocol;

// #[cfg(feature = "dynamodb")]
// pub mod dynamodb;

pub use vector_clock::VectorClock;
pub use merkle::{MerkleTree, MerkleNode};
pub use change_tracker::{ChangeTracker, SyncRecord};
pub use conflict::{ConflictStrategy, ConflictResolver, ConflictResolution, Conflict};
pub use sync_engine::{SyncEngine, SyncConfig, SyncState, SyncEvent};
pub use offline_queue::{OfflineQueue, PendingOperation};
pub use metadata::{SyncMetadata, SyncMetadataStore, EndpointInfo};
pub use protocol::{SyncProtocol, SyncEndpoint};

// #[cfg(feature = "dynamodb")]
// pub use dynamodb::DynamoDBSync;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a sync endpoint
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EndpointId(pub String);

impl EndpointId {
    /// Create a new random endpoint ID
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    /// Create from a string
    pub fn from_str(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl Default for EndpointId {
    fn default() -> Self {
        Self::new()
    }
}

/// Origin of a change
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncOrigin {
    /// Change originated locally
    Local,
    /// Change came from a remote endpoint
    Remote(EndpointId),
}

/// Sync statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncStats {
    pub total_syncs: u64,
    pub successful_syncs: u64,
    pub failed_syncs: u64,
    pub conflicts_detected: u64,
    pub conflicts_resolved: u64,
    pub conflicts_pending: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub items_sent: u64,
    pub items_received: u64,
    pub last_sync_time: Option<i64>,
    pub avg_sync_duration_ms: u64,
}

/// Builder for creating a cloud sync configuration
pub struct CloudSyncBuilder {
    endpoint: Option<SyncEndpoint>,
    conflict_strategy: ConflictStrategy,
    sync_interval: Option<std::time::Duration>,
    batch_size: usize,
    max_retries: u32,
    enable_compression: bool,
}

impl CloudSyncBuilder {
    pub fn new() -> Self {
        Self {
            endpoint: None,
            conflict_strategy: ConflictStrategy::LastWriterWins,
            sync_interval: Some(std::time::Duration::from_secs(30)),
            batch_size: 100,
            max_retries: 3,
            enable_compression: true,
        }
    }

    pub fn with_endpoint(mut self, endpoint: SyncEndpoint) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    pub fn with_conflict_strategy(mut self, strategy: ConflictStrategy) -> Self {
        self.conflict_strategy = strategy;
        self
    }

    pub fn with_sync_interval(mut self, interval: std::time::Duration) -> Self {
        self.sync_interval = Some(interval);
        self
    }

    pub fn with_batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    pub fn with_compression(mut self, enable: bool) -> Self {
        self.enable_compression = enable;
        self
    }

    pub fn build(self) -> Result<SyncEngine> {
        let endpoint = self.endpoint.ok_or_else(|| {
            anyhow::anyhow!("Sync endpoint is required")
        })?;

        let config = SyncConfig {
            endpoint,
            conflict_strategy: self.conflict_strategy,
            sync_interval: self.sync_interval,
            batch_size: self.batch_size,
            max_retries: self.max_retries,
            enable_compression: self.enable_compression,
        };

        SyncEngine::new(config)
    }
}

impl Default for CloudSyncBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_id() {
        let id1 = EndpointId::new();
        let id2 = EndpointId::new();
        assert_ne!(id1, id2);

        let id3 = EndpointId::from_str("test");
        assert_eq!(id3.0, "test");
    }

    #[test]
    fn test_sync_origin() {
        let local = SyncOrigin::Local;
        assert_eq!(local, SyncOrigin::Local);

        let remote = SyncOrigin::Remote(EndpointId::from_str("remote1"));
        match remote {
            SyncOrigin::Remote(id) => assert_eq!(id.0, "remote1"),
            _ => panic!("Expected remote origin"),
        }
    }
}
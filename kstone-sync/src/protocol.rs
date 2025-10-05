/// Sync protocol definitions and endpoint abstractions
///
/// Defines the messages and protocols used for synchronization
/// between KeystoneDB instances and cloud services.

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use kstone_core::{Item, Key};
use crate::{
    EndpointId, VectorClock, SyncRecord, MerkleNode,
    change_tracker::SyncRecord as TrackedRecord,
};

/// Sync endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncEndpoint {
    /// AWS DynamoDB endpoint
    DynamoDB {
        region: String,
        table_name: String,
        credentials: Option<AwsCredentials>,
    },
    /// HTTP/REST endpoint
    Http {
        url: String,
        auth: Option<HttpAuth>,
    },
    /// Another KeystoneDB instance
    Keystone {
        url: String,
        auth_token: Option<String>,
    },
    /// Local file system (for testing)
    FileSystem {
        path: String,
    },
}

impl SyncEndpoint {
    /// Get a unique identifier for this endpoint
    pub fn endpoint_id(&self) -> EndpointId {
        let id_str = match self {
            Self::DynamoDB { region, table_name, .. } => {
                format!("dynamodb:{}:{}", region, table_name)
            }
            Self::Http { url, .. } => {
                format!("http:{}", url)
            }
            Self::Keystone { url, .. } => {
                format!("keystone:{}", url)
            }
            Self::FileSystem { path } => {
                format!("file:{}", path)
            }
        };
        EndpointId::from_str(&id_str)
    }

    /// Get the endpoint type as a string
    pub fn endpoint_type(&self) -> &str {
        match self {
            Self::DynamoDB { .. } => "dynamodb",
            Self::Http { .. } => "http",
            Self::Keystone { .. } => "keystone",
            Self::FileSystem { .. } => "filesystem",
        }
    }
}

/// AWS credentials for DynamoDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

/// HTTP authentication methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpAuth {
    Bearer(String),
    Basic { username: String, password: String },
    ApiKey(String),
}

/// Sync protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessage {
    /// Initial handshake
    Hello {
        endpoint_id: EndpointId,
        vector_clock: VectorClock,
        capabilities: Vec<String>,
    },

    /// Exchange Merkle tree roots for diff detection
    MerkleExchange {
        root_hash: Option<Bytes>,
        level: u32,
        key_range: Option<(Bytes, Bytes)>,
    },

    /// Request specific Merkle nodes
    MerkleRequest {
        level: u32,
        key_range: (Bytes, Bytes),
    },

    /// Response with Merkle nodes
    MerkleResponse {
        nodes: Vec<MerkleNode>,
    },

    /// Request specific items
    ItemsRequest {
        keys: Vec<Key>,
    },

    /// Response with items
    ItemsResponse {
        items: Vec<(Key, Option<Item>, VectorClock)>,
    },

    /// Push changes to remote
    PushChanges {
        changes: Vec<TrackedRecord>,
        checkpoint: Option<u64>,
    },

    /// Acknowledge pushed changes
    PushAck {
        accepted: Vec<String>,
        rejected: Vec<(String, String)>, // (id, reason)
    },

    /// Request changes since checkpoint
    PullRequest {
        since_sequence: Option<u64>,
        limit: usize,
    },

    /// Response with changes
    PullResponse {
        changes: Vec<TrackedRecord>,
        has_more: bool,
        checkpoint: u64,
    },

    /// Sync completed
    Complete {
        stats: SyncSessionStats,
    },

    /// Error occurred
    Error {
        code: String,
        message: String,
    },
}

/// Statistics for a sync session
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncSessionStats {
    pub items_sent: usize,
    pub items_received: usize,
    pub conflicts_detected: usize,
    pub conflicts_resolved: usize,
    pub bytes_sent: usize,
    pub bytes_received: usize,
    pub duration_ms: u64,
}

/// Trait for sync protocol implementations
#[async_trait]
pub trait SyncProtocol: Send + Sync {
    /// Connect to the endpoint
    async fn connect(&mut self) -> Result<()>;

    /// Disconnect from the endpoint
    async fn disconnect(&mut self) -> Result<()>;

    /// Send a message
    async fn send(&mut self, message: SyncMessage) -> Result<()>;

    /// Receive a message
    async fn receive(&mut self) -> Result<SyncMessage>;

    /// Perform handshake
    async fn handshake(&mut self, local_id: &EndpointId, clock: &VectorClock) -> Result<VectorClock>;

    /// Exchange Merkle trees for diff detection
    async fn exchange_merkle(&mut self, local_tree: &MerkleNode) -> Result<Vec<(Key, DiffType)>>;

    /// Pull items from remote
    async fn pull_items(&mut self, keys: Vec<Key>) -> Result<Vec<(Key, Option<Item>, VectorClock)>>;

    /// Push items to remote
    async fn push_items(&mut self, items: Vec<(Key, Option<Item>, VectorClock)>) -> Result<Vec<String>>;

    /// Get remote capabilities
    fn capabilities(&self) -> &[String];

    /// Check if a capability is supported
    fn supports(&self, capability: &str) -> bool {
        self.capabilities().contains(&capability.to_string())
    }
}

/// Type of difference detected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffType {
    /// Item only exists locally
    LocalOnly,
    /// Item only exists remotely
    RemoteOnly,
    /// Item exists in both but differs
    Modified,
}

/// Batch sync request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSyncRequest {
    pub operations: Vec<SyncOperation>,
    pub vector_clock: VectorClock,
    pub checkpoint: Option<u64>,
}

/// Individual sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperation {
    pub operation_id: String,
    pub key: Key,
    pub item: Option<Item>,
    pub operation_type: OperationType,
    pub timestamp: i64,
}

/// Operation type for sync
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationType {
    Put,
    Delete,
}

/// Batch sync response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSyncResponse {
    pub results: Vec<OperationResult>,
    pub remote_clock: VectorClock,
    pub checkpoint: u64,
}

/// Result of a single operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub operation_id: String,
    pub success: bool,
    pub error: Option<String>,
    pub conflict: Option<ConflictInfo>,
}

/// Information about a detected conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub local_version: Option<Item>,
    pub remote_version: Option<Item>,
    pub resolution_used: String,
}

/// Sync protocol capabilities
pub mod capabilities {
    pub const MERKLE_DIFF: &str = "merkle_diff";
    pub const VECTOR_CLOCK: &str = "vector_clock";
    pub const BATCH_SYNC: &str = "batch_sync";
    pub const COMPRESSION: &str = "compression";
    pub const INCREMENTAL: &str = "incremental";
    pub const BIDIRECTIONAL: &str = "bidirectional";
    pub const CONFLICT_RESOLUTION: &str = "conflict_resolution";
    pub const CHECKPOINTS: &str = "checkpoints";

    /// Get all standard capabilities
    pub fn all() -> Vec<String> {
        vec![
            MERKLE_DIFF.to_string(),
            VECTOR_CLOCK.to_string(),
            BATCH_SYNC.to_string(),
            COMPRESSION.to_string(),
            INCREMENTAL.to_string(),
            BIDIRECTIONAL.to_string(),
            CONFLICT_RESOLUTION.to_string(),
            CHECKPOINTS.to_string(),
        ]
    }
}

/// Mock sync protocol for testing
pub struct MockSyncProtocol {
    connected: bool,
    capabilities: Vec<String>,
    items: HashMap<Key, (Option<Item>, VectorClock)>,
}

impl MockSyncProtocol {
    pub fn new() -> Self {
        Self {
            connected: false,
            capabilities: capabilities::all(),
            items: HashMap::new(),
        }
    }
}

#[async_trait]
impl SyncProtocol for MockSyncProtocol {
    async fn connect(&mut self) -> Result<()> {
        self.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }

    async fn send(&mut self, _message: SyncMessage) -> Result<()> {
        Ok(())
    }

    async fn receive(&mut self) -> Result<SyncMessage> {
        Ok(SyncMessage::Complete {
            stats: SyncSessionStats::default(),
        })
    }

    async fn handshake(&mut self, _local_id: &EndpointId, _clock: &VectorClock) -> Result<VectorClock> {
        Ok(VectorClock::new())
    }

    async fn exchange_merkle(&mut self, _local_tree: &MerkleNode) -> Result<Vec<(Key, DiffType)>> {
        Ok(Vec::new())
    }

    async fn pull_items(&mut self, keys: Vec<Key>) -> Result<Vec<(Key, Option<Item>, VectorClock)>> {
        let mut result = Vec::new();
        for key in keys {
            if let Some((item, clock)) = self.items.get(&key) {
                result.push((key, item.clone(), clock.clone()));
            }
        }
        Ok(result)
    }

    async fn push_items(&mut self, items: Vec<(Key, Option<Item>, VectorClock)>) -> Result<Vec<String>> {
        let mut ids = Vec::new();
        for (key, item, clock) in items {
            self.items.insert(key.clone(), (item, clock));
            ids.push(uuid::Uuid::new_v4().to_string());
        }
        Ok(ids)
    }

    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_endpoint_id() {
        let endpoint = SyncEndpoint::DynamoDB {
            region: "us-east-1".to_string(),
            table_name: "test-table".to_string(),
            credentials: None,
        };

        let id = endpoint.endpoint_id();
        assert_eq!(id.0, "dynamodb:us-east-1:test-table");
        assert_eq!(endpoint.endpoint_type(), "dynamodb");
    }

    #[tokio::test]
    async fn test_mock_protocol() {
        let mut protocol = MockSyncProtocol::new();

        protocol.connect().await.unwrap();

        let items = vec![
            (Key::new(b"key1".to_vec()), Some(HashMap::new()), VectorClock::new()),
        ];

        let ids = protocol.push_items(items).await.unwrap();
        assert_eq!(ids.len(), 1);

        let pulled = protocol.pull_items(vec![Key::new(b"key1".to_vec())]).await.unwrap();
        assert_eq!(pulled.len(), 1);
    }
}
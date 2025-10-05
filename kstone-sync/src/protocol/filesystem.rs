/// Filesystem-based sync protocol implementation
///
/// Enables synchronization between two KeystoneDB instances using the filesystem.
/// This protocol directly accesses the remote database file for sync operations.

use async_trait::async_trait;
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;

use kstone_api::Database;
use kstone_core::{Item, Key};

use crate::{
    EndpointId, VectorClock,
    protocol::{SyncProtocol, SyncMessage, SyncSessionStats, DiffType},
    merkle::MerkleNode,
};

/// Filesystem sync protocol
pub struct FilesystemProtocol {
    /// Path to the remote database
    remote_path: PathBuf,
    /// Remote database instance
    remote_db: Option<Arc<Database>>,
    /// Remote endpoint ID
    remote_endpoint_id: Option<EndpointId>,
    /// Remote vector clock
    remote_clock: Option<VectorClock>,
    /// Local database reference for comparisons
    local_db: Option<Arc<Database>>,
}

impl FilesystemProtocol {
    /// Create a new filesystem protocol
    pub fn new(path: String) -> Self {
        Self {
            remote_path: PathBuf::from(path),
            remote_db: None,
            remote_endpoint_id: None,
            remote_clock: None,
            local_db: None,
        }
    }

    /// Set the local database reference for comparisons
    pub fn with_local_db(mut self, db: Arc<Database>) -> Self {
        self.local_db = Some(db);
        self
    }
}

#[async_trait]
impl SyncProtocol for FilesystemProtocol {
    async fn connect(&mut self) -> Result<()> {
        // Open the remote database
        let db = Database::open(&self.remote_path)?;
        self.remote_db = Some(Arc::new(db));
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        self.remote_db = None;
        self.remote_endpoint_id = None;
        self.remote_clock = None;
        Ok(())
    }

    async fn send(&mut self, _message: SyncMessage) -> Result<()> {
        // Filesystem protocol doesn't use message passing
        Ok(())
    }

    async fn receive(&mut self) -> Result<SyncMessage> {
        // Return completion message
        Ok(SyncMessage::Complete {
            stats: SyncSessionStats::default(),
        })
    }

    async fn handshake(&mut self, _local_id: &EndpointId, local_clock: &VectorClock) -> Result<VectorClock> {
        // For filesystem sync, we generate a simple endpoint ID from the path
        self.remote_endpoint_id = Some(EndpointId::from_str(&format!("fs:{}", self.remote_path.display())));

        // Initialize a simple clock for the remote
        let mut clock = VectorClock::new();
        clock.update(self.remote_endpoint_id.as_ref().unwrap().clone(), 0);
        clock.merge(local_clock);

        self.remote_clock = Some(clock.clone());
        Ok(clock)
    }

    async fn exchange_merkle(&mut self, _local_tree: &MerkleNode) -> Result<Vec<(Key, DiffType)>> {
        let mut diffs = Vec::new();

        if let (Some(local_db), Some(remote_db)) = (&self.local_db, &self.remote_db) {
            // Create key mappings
            let mut local_key_map: std::collections::HashMap<bytes::Bytes, kstone_core::Key> = std::collections::HashMap::new();
            let mut remote_key_map: std::collections::HashMap<bytes::Bytes, kstone_core::Key> = std::collections::HashMap::new();

            // Build local merkle tree
            // TODO: scan_with_keys doesn't exist, using scan instead
            use kstone_api::scan::Scan;
            let scan = Scan::new().limit(10000);
            let local_records = local_db.scan(scan)?;
            let mut local_items = Vec::new();

            eprintln!("DEBUG: Local database has {} items", local_records.items.len());
            // TODO: We need keys but scan doesn't return them
            // For now, merkle tree will be empty
            /*
            for (key, item) in local_records {
                let key_bytes = key.encode();
                eprintln!("  Local key: {:?}", String::from_utf8_lossy(&key.pk));
                let value_bytes = serde_json::to_vec(&item)?;
                local_items.push((key_bytes.clone(), bytes::Bytes::from(value_bytes)));
                local_key_map.insert(key_bytes, key);
            }
            */

            let local_tree = crate::merkle::MerkleTree::build(local_items, 16)?;

            // Build remote merkle tree
            // TODO: scan_with_keys doesn't exist, using scan instead
            let scan = Scan::new().limit(10000);
            let remote_records = remote_db.scan(scan)?;
            let mut remote_items = Vec::new();

            eprintln!("DEBUG: Remote database has {} items", remote_records.items.len());
            // TODO: We need keys but scan doesn't return them
            // For now, merkle tree will be empty
            /*
            for (key, item) in remote_records {
                let key_bytes = key.encode();
                eprintln!("  Remote key: {:?}", String::from_utf8_lossy(&key.pk));
                let value_bytes = serde_json::to_vec(&item)?;
                remote_items.push((key_bytes.clone(), bytes::Bytes::from(value_bytes)));
                remote_key_map.insert(key_bytes, key);
            }
            */

            let remote_tree = crate::merkle::MerkleTree::build(remote_items, 16)?;

            // Compare trees using the diff method
            let merkle_diff = local_tree.diff(&remote_tree);

            // Convert MerkleDiff to DiffType
            // only_in_left = LocalOnly (push to remote)
            for (key_bytes, _hash) in merkle_diff.only_in_left {
                if let Some(key) = local_key_map.get(&key_bytes) {
                    diffs.push((key.clone(), DiffType::LocalOnly));
                }
            }

            // only_in_right = RemoteOnly (pull from remote)
            for (key_bytes, _hash) in merkle_diff.only_in_right {
                if let Some(key) = remote_key_map.get(&key_bytes) {
                    diffs.push((key.clone(), DiffType::RemoteOnly));
                }
            }

            // modified = Modified (conflict resolution needed)
            for (key_bytes, _left_hash, _right_hash) in merkle_diff.modified {
                // Use the key from local map (could also use remote, they should be same)
                if let Some(key) = local_key_map.get(&key_bytes) {
                    diffs.push((key.clone(), DiffType::Modified));
                }
            }
        }

        Ok(diffs)
    }

    async fn pull_items(&mut self, keys: Vec<Key>) -> Result<Vec<(Key, Option<Item>, VectorClock)>> {
        let mut items = Vec::new();

        if let Some(remote_db) = &self.remote_db {
            for key in keys {
                let item = if key.sk.is_some() {
                    remote_db.get_with_sk(&key.pk, key.sk.as_ref().unwrap())?
                } else {
                    remote_db.get(&key.pk)?
                };
                let clock = self.remote_clock.clone().unwrap_or_else(VectorClock::new);
                items.push((key, item, clock));
            }
        }

        Ok(items)
    }

    async fn push_items(&mut self, items: Vec<(Key, Option<Item>, VectorClock)>) -> Result<Vec<String>> {
        let mut ids = Vec::new();

        if let Some(remote_db) = &self.remote_db {
            for (key, item_opt, _clock) in items {
                let id = uuid::Uuid::new_v4().to_string();

                match item_opt {
                    Some(item) => {
                        // Put item in remote database
                        if let Some(sk) = &key.sk {
                            remote_db.put_with_sk(&key.pk, sk, item)?;
                        } else {
                            remote_db.put(&key.pk, item)?;
                        }
                    }
                    None => {
                        // Delete from remote database
                        if let Some(sk) = &key.sk {
                            remote_db.delete_with_sk(&key.pk, sk)?;
                        } else {
                            remote_db.delete(&key.pk)?;
                        }
                    }
                }

                ids.push(id);
            }

            // Flush to ensure changes are persisted
            remote_db.flush()?;
        }

        Ok(ids)
    }

    fn capabilities(&self) -> &[String] {
        // Filesystem protocol capabilities
        static CAPABILITIES: &[String] = &[];
        CAPABILITIES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use kstone_api::ItemBuilder;

    #[tokio::test]
    async fn test_filesystem_protocol_connect() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.keystone");

        // Create a database
        let db = Database::create(&db_path).unwrap();
        drop(db);

        // Create protocol and connect
        let mut protocol = FilesystemProtocol::new(db_path.to_string_lossy().to_string());
        assert!(protocol.connect().await.is_ok());
        assert!(protocol.remote_db.is_some());

        // Disconnect
        protocol.disconnect().await.unwrap();
        assert!(protocol.remote_db.is_none());
    }

    #[tokio::test]
    async fn test_filesystem_protocol_push_pull() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.keystone");

        // Create a database
        let _db = Database::create(&db_path).unwrap();

        // Connect protocol
        let mut protocol = FilesystemProtocol::new(db_path.to_string_lossy().to_string());
        protocol.connect().await.unwrap();

        // Push items
        let items = vec![
            (
                Key::new(b"key1".to_vec()),
                Some(ItemBuilder::new().string("value", "test1").build()),
                VectorClock::new(),
            ),
            (
                Key::new(b"key2".to_vec()),
                Some(ItemBuilder::new().string("value", "test2").build()),
                VectorClock::new(),
            ),
        ];

        let ids = protocol.push_items(items).await.unwrap();
        assert_eq!(ids.len(), 2);

        // Pull items back
        let keys = vec![Key::new(b"key1".to_vec()), Key::new(b"key2".to_vec())];
        let pulled = protocol.pull_items(keys).await.unwrap();
        assert_eq!(pulled.len(), 2);

        // Verify items were stored
        assert!(pulled[0].1.is_some());
        assert!(pulled[1].1.is_some());
    }
}
/// Sync engine with state machine for coordinating synchronization
///
/// Manages the overall sync process including change detection,
/// conflict resolution, and data transfer.

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time;

use kstone_api::Database;
use kstone_core::{Item, Key, stream::StreamRecord};

use crate::{
    EndpointId, VectorClock, SyncOrigin, SyncStats,
    change_tracker::{ChangeTracker, SyncRecord},
    conflict::{ConflictManager, ConflictStrategy, Conflict},
    merkle::MerkleTree,
    metadata::{SyncMetadata, SyncMetadataStore, SyncCheckpoint},
    offline_queue::{OfflineQueue, PendingOperation, RetryPolicy},
    protocol::{SyncProtocol, SyncEndpoint, SyncMessage, SyncSessionStats, DiffType},
};

/// Sync engine state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncState {
    /// Not connected
    Idle,
    /// Connecting to endpoint
    Connecting,
    /// Performing handshake
    Handshaking,
    /// Discovering changes via Merkle tree
    Discovering,
    /// Negotiating what to sync
    Negotiating,
    /// Transferring data
    Transferring {
        sent: usize,
        received: usize,
        total: usize,
    },
    /// Resolving conflicts
    ResolvingConflicts,
    /// Committing changes
    Committing,
    /// Sync completed
    Completed,
    /// Error occurred
    Error(String),
}

/// Sync events that can be observed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncEvent {
    /// Sync started
    Started {
        endpoint_id: EndpointId,
    },
    /// State changed
    StateChanged {
        old_state: SyncState,
        new_state: SyncState,
    },
    /// Progress update
    Progress {
        sent: usize,
        received: usize,
        total: usize,
    },
    /// Conflict detected
    ConflictDetected {
        key: Key,
        conflict_id: String,
    },
    /// Conflict resolved
    ConflictResolved {
        conflict_id: String,
    },
    /// Sync completed
    Completed {
        stats: SyncSessionStats,
    },
    /// Error occurred
    Failed {
        error: String,
    },
}

/// Sync configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Endpoint to sync with
    pub endpoint: SyncEndpoint,
    /// Conflict resolution strategy
    pub conflict_strategy: ConflictStrategy,
    /// Automatic sync interval
    pub sync_interval: Option<Duration>,
    /// Batch size for operations
    pub batch_size: usize,
    /// Maximum retries
    pub max_retries: u32,
    /// Enable compression
    pub enable_compression: bool,
}

/// Main sync engine
pub struct SyncEngine {
    /// Database reference
    db: Arc<Database>,
    /// Configuration
    config: SyncConfig,
    /// Current state
    state: Arc<RwLock<SyncState>>,
    /// Change tracker
    change_tracker: Arc<ChangeTracker>,
    /// Conflict manager
    conflict_manager: Arc<ConflictManager>,
    /// Offline queue
    offline_queue: Arc<OfflineQueue>,
    /// Metadata store
    metadata_store: Arc<SyncMetadataStore>,
    /// Sync metadata
    metadata: Arc<RwLock<SyncMetadata>>,
    /// Event channel sender
    event_tx: mpsc::UnboundedSender<SyncEvent>,
    /// Event channel receiver
    event_rx: Option<mpsc::UnboundedReceiver<SyncEvent>>,
    /// Shutdown signal
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl SyncEngine {
    /// Create a new sync engine
    pub fn new(db: Arc<Database>, config: SyncConfig) -> Result<Self> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let local_endpoint = EndpointId::new();

        let change_tracker = Arc::new(ChangeTracker::new(local_endpoint.clone(), 10000));
        let conflict_manager = Arc::new(ConflictManager::new(config.conflict_strategy.clone()));
        let offline_queue = Arc::new(OfflineQueue::new(RetryPolicy::default(), 1000));

        let metadata_store = Arc::new(SyncMetadataStore::new(db.clone()));

        // Initialize metadata if not exists
        metadata_store.initialize()?;

        let metadata = match metadata_store.load_metadata()? {
            Some(existing) => Arc::new(RwLock::new(existing)),
            None => {
                let new_metadata = SyncMetadata::new(local_endpoint.clone());
                metadata_store.save_metadata(&new_metadata)?;
                Arc::new(RwLock::new(new_metadata))
            }
        };

        Ok(Self {
            db,
            config,
            state: Arc::new(RwLock::new(SyncState::Idle)),
            change_tracker,
            conflict_manager,
            offline_queue,
            metadata_store,
            metadata,
            event_tx,
            event_rx: Some(event_rx),
            shutdown_tx: None,
        })
    }

    /// Start the sync engine with automatic syncing
    pub async fn start(&mut self) -> Result<()> {
        if let Some(interval) = self.config.sync_interval {
            let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
            self.shutdown_tx = Some(shutdown_tx);

            let engine = self.clone_for_task();

            tokio::spawn(async move {
                let mut interval = time::interval(interval);

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if let Err(e) = engine.sync_once().await {
                                tracing::error!("Sync error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            tracing::info!("Sync engine shutting down");
                            break;
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Stop the sync engine
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        Ok(())
    }

    /// Upload a snapshot to S3
    #[cfg(feature = "s3-sync")]
    pub async fn upload_to_s3(
        &self,
        bucket: String,
        prefix: String,
        region: String,
        db_path: PathBuf,
    ) -> Result<String> {
        use crate::protocol::s3::S3Protocol;

        let mut protocol = S3Protocol::new(bucket, prefix, region, None, None)
            .with_local_db_path(db_path)
            .with_local_db(self.db.clone());

        protocol.connect().await?;
        let snapshot_id = protocol.upload_snapshot().await?;
        protocol.disconnect().await?;

        Ok(snapshot_id)
    }

    /// Download a snapshot from S3
    #[cfg(feature = "s3-sync")]
    pub async fn restore_from_s3(
        &self,
        bucket: String,
        prefix: String,
        region: String,
        snapshot_id: String,
        db_path: PathBuf,
    ) -> Result<()> {
        use crate::protocol::s3::S3Protocol;

        let mut protocol = S3Protocol::new(bucket, prefix, region, None, None)
            .with_local_db_path(db_path)
            .with_local_db(self.db.clone());

        protocol.connect().await?;
        protocol.download_snapshot(&snapshot_id).await?;
        protocol.disconnect().await?;

        Ok(())
    }

    /// Perform a single sync operation with a specific endpoint
    pub async fn sync(&self, endpoint: SyncEndpoint) -> Result<SyncSessionStats> {
        // Temporarily use the provided endpoint for this sync
        let mut protocol = self.create_protocol_for_endpoint(&endpoint).await?;

        self.set_state(SyncState::Connecting);
        self.emit_event(SyncEvent::Started {
            endpoint_id: endpoint.endpoint_id(),
        });

        // Connect to endpoint
        protocol.connect().await?;
        self.set_state(SyncState::Handshaking);

        // Perform handshake
        let metadata = self.metadata.read().clone();
        let remote_clock = protocol.handshake(
            &metadata.local_endpoint,
            &metadata.vector_clock,
        ).await?;

        // Update vector clock
        self.change_tracker.update_vector_clock(&remote_clock);

        // Discovery phase
        self.set_state(SyncState::Discovering);
        let changes = self.discover_changes(protocol.as_mut()).await?;

        let total_changes = changes.len();

        // Debug: Log discovered changes
        eprintln!("DEBUG: Discovered {} changes", total_changes);
        for (key, diff_type) in &changes {
            eprintln!("  Key: {:?}, Type: {:?}", String::from_utf8_lossy(&key.pk), diff_type);
        }

        if total_changes == 0 {
            self.set_state(SyncState::Completed);
            let stats = SyncSessionStats::default();
            self.emit_event(SyncEvent::Completed {
                stats: stats.clone(),
            });
            return Ok(stats);
        }

        // Transfer phase
        self.set_state(SyncState::Transferring {
            sent: 0,
            received: 0,
            total: total_changes,
        });

        let stats = self.transfer_changes(protocol.as_mut(), changes).await?;

        // Conflict resolution
        if self.conflict_manager.get_stats().pending_count > 0 {
            self.set_state(SyncState::ResolvingConflicts);
            self.resolve_conflicts().await?;
        }

        // Commit phase
        self.set_state(SyncState::Committing);

        // Disconnect
        protocol.disconnect().await?;

        // Update metadata
        let mut metadata = self.metadata.write();
        metadata.stats.last_sync_time = Some(chrono::Utc::now().timestamp_millis());
        metadata.stats.successful_syncs += 1;
        metadata.stats.total_syncs += 1;
        metadata.stats.items_sent += stats.items_sent as u64;
        metadata.stats.items_received += stats.items_received as u64;
        drop(metadata);

        self.metadata_store.save_metadata(&self.metadata.read())?;
        self.db.flush()?;

        self.set_state(SyncState::Completed);
        self.emit_event(SyncEvent::Completed {
            stats: stats.clone(),
        });

        Ok(stats)
    }

    /// Perform a single sync operation
    pub async fn sync_once(&self) -> Result<()> {
        self.set_state(SyncState::Connecting);
        self.emit_event(SyncEvent::Started {
            endpoint_id: self.config.endpoint.endpoint_id(),
        });

        // Create protocol handler based on endpoint type
        let mut protocol = self.create_protocol().await?;

        // Connect to endpoint
        protocol.connect().await?;
        self.set_state(SyncState::Handshaking);

        // Perform handshake
        let metadata = self.metadata.read().clone();
        let remote_clock = protocol.handshake(
            &metadata.local_endpoint,
            &metadata.vector_clock,
        ).await?;

        // Update vector clock
        self.change_tracker.update_vector_clock(&remote_clock);

        // Discovery phase
        self.set_state(SyncState::Discovering);
        let changes = self.discover_changes(protocol.as_mut()).await?;

        let total_changes = changes.len();

        // Debug: Log discovered changes
        eprintln!("DEBUG: Discovered {} changes", total_changes);
        for (key, diff_type) in &changes {
            eprintln!("  Key: {:?}, Type: {:?}", String::from_utf8_lossy(&key.pk), diff_type);
        }

        if total_changes == 0 {
            self.set_state(SyncState::Completed);
            self.emit_event(SyncEvent::Completed {
                stats: SyncSessionStats::default(),
            });
            return Ok(());
        }

        // Transfer phase
        self.set_state(SyncState::Transferring {
            sent: 0,
            received: 0,
            total: total_changes,
        });

        let stats = self.transfer_changes(protocol.as_mut(), changes).await?;

        // Conflict resolution
        if self.conflict_manager.get_stats().pending_count > 0 {
            self.set_state(SyncState::ResolvingConflicts);
            self.resolve_conflicts().await?;
        }

        // Commit phase
        self.set_state(SyncState::Committing);
        self.commit_changes().await?;

        // Complete
        self.set_state(SyncState::Completed);
        self.emit_event(SyncEvent::Completed { stats });

        // Update metadata
        {
            let mut metadata = self.metadata.write();
            metadata.stats.total_syncs += 1;
            metadata.stats.successful_syncs += 1;
            metadata.update_sync_time(&self.config.endpoint.endpoint_id());

            // Save metadata to persist statistics
            self.metadata_store.save_metadata(&*metadata)?;
        } // Lock dropped here

        protocol.disconnect().await?;
        Ok(())
    }

    /// Discover changes using Merkle tree diff
    async fn discover_changes(
        &self,
        protocol: &mut dyn SyncProtocol,
    ) -> Result<Vec<(Key, DiffType)>> {
        // Build local Merkle tree by scanning the database with keys
        let mut local_items = Vec::new();

        // Scan the database to get items with their keys
        let records = self.db.scan_with_keys(10000)?;

        for (key, item) in records {
            let key_bytes = key.encode();
            let value_bytes = serde_json::to_vec(&item)?;
            local_items.push((key_bytes, Bytes::from(value_bytes)));
        }

        let local_tree = MerkleTree::build(local_items, 16)?;

        if let Some(root) = local_tree.root.as_ref() {
            let diffs = protocol.exchange_merkle(root).await?;
            Ok(diffs)
        } else {
            Ok(Vec::new())
        }
    }

    /// Transfer changes between endpoints
    async fn transfer_changes(
        &self,
        protocol: &mut dyn SyncProtocol,
        changes: Vec<(Key, DiffType)>,
    ) -> Result<SyncSessionStats> {
        let mut stats = SyncSessionStats::default();
        let mut sent = 0;
        let mut received = 0;

        for chunk in changes.chunks(self.config.batch_size) {
            let mut to_pull = Vec::new();
            let mut to_push = Vec::new();

            for (key, diff_type) in chunk {
                match diff_type {
                    DiffType::LocalOnly => {
                        // We have it, they don't - push
                        let item = if let Some(ref sk) = key.sk {
                            self.db.get_with_sk(&key.pk, sk)?
                        } else {
                            self.db.get(&key.pk)?
                        };

                        if let Some(item) = item {
                            to_push.push((
                                key.clone(),
                                Some(item),
                                self.change_tracker.get_vector_clock(),
                            ));
                        }
                    }
                    DiffType::RemoteOnly => {
                        // They have it, we don't - pull
                        to_pull.push(key.clone());
                    }
                    DiffType::Modified => {
                        // Both have it but different - need to resolve
                        to_pull.push(key.clone());

                        let item = if let Some(ref sk) = key.sk {
                            self.db.get_with_sk(&key.pk, sk)?
                        } else {
                            self.db.get(&key.pk)?
                        };

                        if let Some(item) = item {
                            to_push.push((
                                key.clone(),
                                Some(item),
                                self.change_tracker.get_vector_clock(),
                            ));
                        }
                    }
                }
            }

            // Pull items from remote
            if !to_pull.is_empty() {
                let pulled = protocol.pull_items(to_pull).await?;
                received += pulled.len();
                stats.items_received += pulled.len();

                // Process pulled items
                for (key, item, remote_clock) in pulled {
                    self.process_remote_item(key, item, remote_clock).await?;
                }
            }

            // Push items to remote
            if !to_push.is_empty() {
                let pushed = protocol.push_items(to_push.clone()).await?;
                sent += pushed.len();
                stats.items_sent += pushed.len();
            }

            // Update progress
            self.set_state(SyncState::Transferring {
                sent,
                received,
                total: changes.len(),
            });

            self.emit_event(SyncEvent::Progress {
                sent,
                received,
                total: changes.len(),
            });
        }

        Ok(stats)
    }

    /// Process an item received from remote
    async fn process_remote_item(
        &self,
        key: Key,
        remote_item: Option<Item>,
        remote_clock: VectorClock,
    ) -> Result<()> {
        // Get the local item using the correct method
        let local_item = if let Some(ref sk) = key.sk {
            self.db.get_with_sk(&key.pk, sk)?
        } else {
            self.db.get(&key.pk)?
        };

        let local_clock = self.change_tracker.get_vector_clock();

        // Check for conflict
        if local_item.is_some() && remote_clock.concurrent_with(&local_clock) {
            let conflict = Conflict::new(
                key.clone(),
                local_item,
                remote_item,
                local_clock,
                remote_clock,
                chrono::Utc::now().timestamp_millis(),
                chrono::Utc::now().timestamp_millis(),
                self.config.conflict_strategy.clone(),
            );

            let conflict_id = self.conflict_manager.add_conflict(conflict)?;

            self.emit_event(SyncEvent::ConflictDetected {
                key,
                conflict_id,
            });
        } else {
            // No conflict, apply remote change
            match remote_item {
                Some(item) => {
                    if let Some(ref sk) = key.sk {
                        self.db.put_with_sk(&key.pk, sk, item)?
                    } else {
                        self.db.put(&key.pk, item)?
                    }
                }
                None => {
                    if let Some(ref sk) = key.sk {
                        self.db.delete_with_sk(&key.pk, sk)?
                    } else {
                        self.db.delete(&key.pk)?
                    }
                }
            }
        }

        Ok(())
    }

    /// Resolve pending conflicts
    async fn resolve_conflicts(&self) -> Result<()> {
        let results = self.conflict_manager.resolve_all();

        for (conflict_id, result) in results {
            if let Ok(resolution) = result {
                self.emit_event(SyncEvent::ConflictResolved {
                    conflict_id: conflict_id.clone(),
                });

                // Apply resolution
                // This would write to the database based on the resolution
            }
        }

        Ok(())
    }

    /// Commit all changes
    async fn commit_changes(&self) -> Result<()> {
        // Flush database
        self.db.flush()?;

        // Save checkpoint
        let checkpoint = SyncCheckpoint {
            endpoint_id: self.config.endpoint.endpoint_id(),
            last_sequence: 0, // Would get from stream
            vector_clock: self.change_tracker.get_vector_clock(),
            merkle_root: None,
            timestamp: chrono::Utc::now().timestamp_millis(),
        };

        self.metadata_store.save_checkpoint(&checkpoint)?;

        Ok(())
    }

    /// Create protocol handler for the configured endpoint
    async fn create_protocol_for_endpoint(&self, endpoint: &SyncEndpoint) -> Result<Box<dyn SyncProtocol>> {
        match endpoint {
            SyncEndpoint::DynamoDB { .. } => {
                #[cfg(feature = "dynamodb")]
                {
                    // Would create DynamoDB protocol
                    Ok(Box::new(crate::protocol::MockSyncProtocol::new()))
                }
                #[cfg(not(feature = "dynamodb"))]
                {
                    Err(anyhow::anyhow!("DynamoDB support not enabled"))
                }
            }
            SyncEndpoint::FileSystem { path } => {
                // Use filesystem protocol for local database sync
                let protocol = crate::protocol::filesystem::FilesystemProtocol::new(path.clone())
                    .with_local_db(self.db.clone());
                Ok(Box::new(protocol))
            }
            #[cfg(feature = "s3-sync")]
            SyncEndpoint::S3 { bucket, prefix, region, endpoint_url, credentials } => {
                // For S3 sync, we need the database path
                // Since path() returns None for now, we'll use a workaround
                // In production, the database path should be provided via configuration

                let mut protocol = crate::protocol::s3::S3Protocol::new(
                    bucket.clone(),
                    prefix.clone(),
                    region.clone(),
                    endpoint_url.clone(),
                    credentials.clone(),
                )
                .with_local_db(self.db.clone());

                // Path will need to be provided separately for now
                // TODO: Properly expose database path from LsmEngine

                Ok(Box::new(protocol))
            }
            _ => {
                Err(anyhow::anyhow!("Unsupported sync endpoint type"))
            }
        }
    }

    async fn create_protocol(&self) -> Result<Box<dyn SyncProtocol>> {
        match &self.config.endpoint {
            SyncEndpoint::DynamoDB { .. } => {
                #[cfg(feature = "dynamodb")]
                {
                    // Would create DynamoDB protocol
                    Ok(Box::new(crate::protocol::MockSyncProtocol::new()))
                }
                #[cfg(not(feature = "dynamodb"))]
                {
                    Err(anyhow::anyhow!("DynamoDB support not enabled"))
                }
            }
            SyncEndpoint::FileSystem { path } => {
                // Use filesystem protocol for local database sync
                let protocol = crate::protocol::filesystem::FilesystemProtocol::new(path.clone())
                    .with_local_db(self.db.clone());
                Ok(Box::new(protocol))
            }
            #[cfg(feature = "s3-sync")]
            SyncEndpoint::S3 { bucket, prefix, region, endpoint_url, credentials } => {
                // For S3 sync, we need the database path
                // Since path() returns None for now, we'll use a workaround
                // In production, the database path should be provided via configuration

                let mut protocol = crate::protocol::s3::S3Protocol::new(
                    bucket.clone(),
                    prefix.clone(),
                    region.clone(),
                    endpoint_url.clone(),
                    credentials.clone(),
                )
                .with_local_db(self.db.clone());

                // Path will need to be provided separately for now
                // TODO: Properly expose database path from LsmEngine

                Ok(Box::new(protocol))
            }
            _ => {
                // Use mock for other protocols (HTTP, Keystone)
                Ok(Box::new(crate::protocol::MockSyncProtocol::new()))
            }
        }
    }

    /// Set the current state
    fn set_state(&self, new_state: SyncState) {
        let old_state = {
            let mut state = self.state.write();
            let old = state.clone();
            *state = new_state.clone();
            old
        };

        self.emit_event(SyncEvent::StateChanged {
            old_state,
            new_state,
        });
    }

    /// Emit an event
    fn emit_event(&self, event: SyncEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Get current state
    pub fn get_state(&self) -> SyncState {
        self.state.read().clone()
    }

    /// Get sync statistics
    pub fn get_stats(&self) -> SyncStats {
        self.metadata.read().stats.clone()
    }

    /// Subscribe to sync events
    pub fn subscribe(&mut self) -> Option<mpsc::UnboundedReceiver<SyncEvent>> {
        self.event_rx.take()
    }

    /// Clone for spawning tasks
    fn clone_for_task(&self) -> Self {
        let (event_tx, _) = mpsc::unbounded_channel();

        Self {
            db: self.db.clone(),
            config: self.config.clone(),
            state: self.state.clone(),
            change_tracker: self.change_tracker.clone(),
            conflict_manager: self.conflict_manager.clone(),
            offline_queue: self.offline_queue.clone(),
            metadata_store: self.metadata_store.clone(),
            metadata: self.metadata.clone(),
            event_tx,
            event_rx: None,
            shutdown_tx: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sync_engine_creation() {
        let db = Arc::new(kstone_api::Database::create_in_memory().unwrap());

        let config = SyncConfig {
            endpoint: SyncEndpoint::FileSystem {
                path: "/tmp/test".to_string(),
            },
            conflict_strategy: ConflictStrategy::LastWriterWins,
            sync_interval: None,
            batch_size: 100,
            max_retries: 3,
            enable_compression: false,
        };

        let engine = SyncEngine::new(db, config).unwrap();
        assert_eq!(engine.get_state(), SyncState::Idle);
    }

    #[tokio::test]
    async fn test_sync_state_transitions() {
        let db = Arc::new(kstone_api::Database::create_in_memory().unwrap());

        let config = SyncConfig {
            endpoint: SyncEndpoint::FileSystem {
                path: "/tmp/test".to_string(),
            },
            conflict_strategy: ConflictStrategy::LastWriterWins,
            sync_interval: None,
            batch_size: 100,
            max_retries: 3,
            enable_compression: false,
        };

        let engine = SyncEngine::new(db, config).unwrap();

        engine.set_state(SyncState::Connecting);
        assert_eq!(engine.get_state(), SyncState::Connecting);

        engine.set_state(SyncState::Handshaking);
        assert_eq!(engine.get_state(), SyncState::Handshaking);
    }
}
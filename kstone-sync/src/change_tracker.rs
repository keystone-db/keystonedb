/// Change tracking layer for synchronization
///
/// Builds on top of KeystoneDB's stream infrastructure to track changes
/// that need to be synchronized with remote endpoints.

use anyhow::Result;
use bytes::Bytes;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use uuid::Uuid;

use kstone_core::{
    stream::{StreamRecord, StreamEventType},
    Item, Key,
};

use crate::{EndpointId, SyncOrigin, VectorClock};

/// Extended stream record with sync metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRecord {
    /// Unique ID for this sync record
    pub id: String,
    /// The underlying stream record
    pub stream_record: StreamRecord,
    /// Vector clock for this change
    pub vector_clock: VectorClock,
    /// Origin of this change
    pub origin: SyncOrigin,
    /// Whether this change has been synced
    pub synced: bool,
    /// Timestamp when this record was created
    pub created_at: i64,
    /// Last sync attempt timestamp
    pub last_sync_attempt: Option<i64>,
    /// Number of sync attempts
    pub sync_attempts: u32,
}

impl SyncRecord {
    /// Create a new sync record from a stream record
    pub fn from_stream_record(
        stream_record: StreamRecord,
        local_endpoint: &EndpointId,
        vector_clock: VectorClock,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            stream_record,
            vector_clock,
            origin: SyncOrigin::Local,
            synced: false,
            created_at: chrono::Utc::now().timestamp_millis(),
            last_sync_attempt: None,
            sync_attempts: 0,
        }
    }

    /// Create a sync record from a remote change
    pub fn from_remote(
        stream_record: StreamRecord,
        remote_endpoint: EndpointId,
        vector_clock: VectorClock,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            stream_record,
            vector_clock,
            origin: SyncOrigin::Remote(remote_endpoint),
            synced: true, // Remote changes are already "synced"
            created_at: chrono::Utc::now().timestamp_millis(),
            last_sync_attempt: None,
            sync_attempts: 0,
        }
    }

    /// Check if this record needs to be synced
    pub fn needs_sync(&self) -> bool {
        !self.synced && matches!(self.origin, SyncOrigin::Local)
    }

    /// Mark this record as synced
    pub fn mark_synced(&mut self) {
        self.synced = true;
    }

    /// Record a sync attempt
    pub fn record_attempt(&mut self) {
        self.sync_attempts += 1;
        self.last_sync_attempt = Some(chrono::Utc::now().timestamp_millis());
    }

    /// Check if we should retry syncing this record
    pub fn should_retry(&self, max_attempts: u32) -> bool {
        self.needs_sync() && self.sync_attempts < max_attempts
    }

    /// Get the key from the underlying stream record
    pub fn key(&self) -> &Key {
        &self.stream_record.key
    }

    /// Get the event type
    pub fn event_type(&self) -> StreamEventType {
        self.stream_record.event_type
    }
}

/// Tracks changes for synchronization
pub struct ChangeTracker {
    /// Local endpoint ID
    local_endpoint: EndpointId,
    /// Current vector clock for this endpoint
    vector_clock: Arc<RwLock<VectorClock>>,
    /// Pending changes to be synced
    pending_changes: Arc<RwLock<VecDeque<SyncRecord>>>,
    /// Recently synced changes (for deduplication)
    recent_synced: Arc<RwLock<HashMap<String, i64>>>,
    /// Maximum number of pending changes to keep
    max_pending: usize,
    /// Time to keep synced records for deduplication (ms)
    dedup_window_ms: i64,
}

impl ChangeTracker {
    /// Create a new change tracker
    pub fn new(local_endpoint: EndpointId, max_pending: usize) -> Self {
        let mut vector_clock = VectorClock::new();
        vector_clock.update(local_endpoint.clone(), 0);

        Self {
            local_endpoint,
            vector_clock: Arc::new(RwLock::new(vector_clock)),
            pending_changes: Arc::new(RwLock::new(VecDeque::new())),
            recent_synced: Arc::new(RwLock::new(HashMap::new())),
            max_pending,
            dedup_window_ms: 300_000, // 5 minutes
        }
    }

    /// Track a new local change from a stream record
    pub fn track_local_change(&self, stream_record: StreamRecord) -> Result<SyncRecord> {
        // Increment vector clock
        let mut clock = self.vector_clock.write();
        clock.increment(&self.local_endpoint);
        let current_clock = clock.clone();
        drop(clock);

        // Create sync record
        let sync_record = SyncRecord::from_stream_record(
            stream_record,
            &self.local_endpoint,
            current_clock,
        );

        // Add to pending changes
        let mut pending = self.pending_changes.write();

        // Enforce max pending limit
        while pending.len() >= self.max_pending {
            pending.pop_front();
        }

        pending.push_back(sync_record.clone());

        Ok(sync_record)
    }

    /// Track a change received from a remote endpoint
    pub fn track_remote_change(
        &self,
        stream_record: StreamRecord,
        remote_endpoint: EndpointId,
        remote_clock: VectorClock,
    ) -> Result<SyncRecord> {
        // Update our vector clock
        let mut clock = self.vector_clock.write();
        clock.merge(&remote_clock);
        clock.increment(&self.local_endpoint);
        drop(clock);

        // Create sync record
        let sync_record = SyncRecord::from_remote(
            stream_record,
            remote_endpoint,
            remote_clock,
        );

        // Add to recent synced for deduplication
        let mut recent = self.recent_synced.write();
        recent.insert(sync_record.id.clone(), sync_record.created_at);

        Ok(sync_record)
    }

    /// Get pending changes that need to be synced
    pub fn get_pending_changes(&self, limit: Option<usize>) -> Vec<SyncRecord> {
        let pending = self.pending_changes.read();
        let limit = limit.unwrap_or(pending.len());

        pending
            .iter()
            .filter(|r| r.needs_sync())
            .take(limit)
            .cloned()
            .collect()
    }

    /// Mark a change as synced
    pub fn mark_synced(&self, record_id: &str) -> Result<()> {
        let mut pending = self.pending_changes.write();

        for record in pending.iter_mut() {
            if record.id == record_id {
                record.mark_synced();

                // Add to recent synced
                let mut recent = self.recent_synced.write();
                recent.insert(record.id.clone(), record.created_at);

                return Ok(());
            }
        }

        Ok(())
    }

    /// Record a sync attempt for a change
    pub fn record_sync_attempt(&self, record_id: &str) -> Result<()> {
        let mut pending = self.pending_changes.write();

        for record in pending.iter_mut() {
            if record.id == record_id {
                record.record_attempt();
                return Ok(());
            }
        }

        Ok(())
    }

    /// Clean up old synced records
    pub fn cleanup_old_records(&self) {
        let now = chrono::Utc::now().timestamp_millis();
        let cutoff = now - self.dedup_window_ms;

        // Clean up recent synced
        let mut recent = self.recent_synced.write();
        recent.retain(|_, timestamp| *timestamp > cutoff);

        // Remove old synced records from pending
        let mut pending = self.pending_changes.write();
        pending.retain(|record| {
            !record.synced || record.created_at > cutoff
        });
    }

    /// Check if a change has been recently synced (for deduplication)
    pub fn is_recently_synced(&self, record_id: &str) -> bool {
        let recent = self.recent_synced.read();
        if let Some(timestamp) = recent.get(record_id) {
            let now = chrono::Utc::now().timestamp_millis();
            *timestamp > now - self.dedup_window_ms
        } else {
            false
        }
    }

    /// Get the current vector clock
    pub fn get_vector_clock(&self) -> VectorClock {
        self.vector_clock.read().clone()
    }

    /// Update the vector clock from a remote endpoint
    pub fn update_vector_clock(&self, remote_clock: &VectorClock) {
        let mut clock = self.vector_clock.write();
        clock.merge(remote_clock);
    }

    /// Get statistics about tracked changes
    pub fn get_stats(&self) -> ChangeTrackerStats {
        let pending = self.pending_changes.read();
        let recent = self.recent_synced.read();

        let pending_count = pending.len();
        let needs_sync = pending.iter().filter(|r| r.needs_sync()).count();
        let synced = pending.iter().filter(|r| r.synced).count();

        ChangeTrackerStats {
            total_pending: pending_count,
            needs_sync,
            synced,
            recent_synced: recent.len(),
            oldest_pending: pending.front().map(|r| r.created_at),
            newest_pending: pending.back().map(|r| r.created_at),
        }
    }

    /// Clear all tracked changes
    pub fn clear(&self) {
        self.pending_changes.write().clear();
        self.recent_synced.write().clear();
    }
}

/// Statistics about the change tracker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeTrackerStats {
    pub total_pending: usize,
    pub needs_sync: usize,
    pub synced: usize,
    pub recent_synced: usize,
    pub oldest_pending: Option<i64>,
    pub newest_pending: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use kstone_core::stream::StreamViewType;

    fn create_test_stream_record(key: &str) -> StreamRecord {
        StreamRecord {
            sequence_number: 1,
            event_type: StreamEventType::Insert,
            key: Key::new(key.as_bytes().to_vec()),
            old_image: None,
            new_image: Some(HashMap::new()),
            timestamp: chrono::Utc::now().timestamp_millis(),
        }
    }

    #[test]
    fn test_track_local_change() {
        let tracker = ChangeTracker::new(EndpointId::from_str("local"), 100);
        let stream_record = create_test_stream_record("key1");

        let sync_record = tracker.track_local_change(stream_record).unwrap();

        assert!(!sync_record.synced);
        assert!(matches!(sync_record.origin, SyncOrigin::Local));
        assert!(sync_record.needs_sync());

        let pending = tracker.get_pending_changes(None);
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_track_remote_change() {
        let tracker = ChangeTracker::new(EndpointId::from_str("local"), 100);
        let stream_record = create_test_stream_record("key1");
        let remote_endpoint = EndpointId::from_str("remote");
        let remote_clock = VectorClock::new();

        let sync_record = tracker.track_remote_change(
            stream_record,
            remote_endpoint.clone(),
            remote_clock,
        ).unwrap();

        assert!(sync_record.synced);
        assert!(matches!(sync_record.origin, SyncOrigin::Remote(_)));
        assert!(!sync_record.needs_sync());

        let pending = tracker.get_pending_changes(None);
        assert_eq!(pending.len(), 0); // Remote changes don't need syncing
    }

    #[test]
    fn test_mark_synced() {
        let tracker = ChangeTracker::new(EndpointId::from_str("local"), 100);
        let stream_record = create_test_stream_record("key1");

        let sync_record = tracker.track_local_change(stream_record).unwrap();
        let record_id = sync_record.id.clone();

        assert_eq!(tracker.get_pending_changes(None).len(), 1);

        tracker.mark_synced(&record_id).unwrap();

        assert_eq!(tracker.get_pending_changes(None).len(), 0);
    }

    #[test]
    fn test_max_pending_limit() {
        let tracker = ChangeTracker::new(EndpointId::from_str("local"), 3);

        // Add 5 changes
        for i in 0..5 {
            let stream_record = create_test_stream_record(&format!("key{}", i));
            tracker.track_local_change(stream_record).unwrap();
        }

        // Should only keep last 3
        let stats = tracker.get_stats();
        assert_eq!(stats.total_pending, 3);
    }
}
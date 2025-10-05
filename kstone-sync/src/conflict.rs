/// Conflict detection and resolution for synchronization
///
/// Provides various strategies for resolving conflicts when the same item
/// is modified concurrently on different endpoints.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use kstone_core::{Item, Key, Value};
use crate::{VectorClock, EndpointId, SyncRecord};

/// Conflict resolution strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictStrategy {
    /// Last writer wins based on timestamp
    LastWriterWins,
    /// First writer wins (keep existing)
    FirstWriterWins,
    /// Use vector clock causality
    VectorClock,
    /// Merge changes at the attribute level
    AttributeMerge,
    /// Custom resolution logic
    Custom(String), // Name of custom resolver
    /// Queue for manual resolution
    Manual,
}

impl Default for ConflictStrategy {
    fn default() -> Self {
        Self::LastWriterWins
    }
}

/// A detected conflict between local and remote versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    /// Unique ID for this conflict
    pub id: String,
    /// The key that has conflicting versions
    pub key: Key,
    /// Local version of the item
    pub local_item: Option<Item>,
    /// Remote version of the item
    pub remote_item: Option<Item>,
    /// Local vector clock
    pub local_clock: VectorClock,
    /// Remote vector clock
    pub remote_clock: VectorClock,
    /// Local modification timestamp
    pub local_timestamp: i64,
    /// Remote modification timestamp
    pub remote_timestamp: i64,
    /// Detected at timestamp
    pub detected_at: i64,
    /// Resolution strategy to use
    pub strategy: ConflictStrategy,
    /// Whether this conflict has been resolved
    pub resolved: bool,
    /// Resolution result if resolved
    pub resolution: Option<ConflictResolution>,
}

impl Conflict {
    /// Create a new conflict
    pub fn new(
        key: Key,
        local_item: Option<Item>,
        remote_item: Option<Item>,
        local_clock: VectorClock,
        remote_clock: VectorClock,
        local_timestamp: i64,
        remote_timestamp: i64,
        strategy: ConflictStrategy,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            key,
            local_item,
            remote_item,
            local_clock,
            remote_clock,
            local_timestamp,
            remote_timestamp,
            detected_at: chrono::Utc::now().timestamp_millis(),
            strategy,
            resolved: false,
            resolution: None,
        }
    }

    /// Check if this is a real conflict (concurrent modifications)
    pub fn is_concurrent(&self) -> bool {
        self.local_clock.concurrent_with(&self.remote_clock)
    }

    /// Resolve this conflict using the configured strategy
    pub fn resolve(&mut self) -> Result<ConflictResolution> {
        let resolution = match &self.strategy {
            ConflictStrategy::LastWriterWins => {
                self.resolve_last_writer_wins()
            }
            ConflictStrategy::FirstWriterWins => {
                self.resolve_first_writer_wins()
            }
            ConflictStrategy::VectorClock => {
                self.resolve_vector_clock()
            }
            ConflictStrategy::AttributeMerge => {
                self.resolve_attribute_merge()
            }
            ConflictStrategy::Manual => {
                ConflictResolution::Deferred
            }
            ConflictStrategy::Custom(name) => {
                // Would call custom resolver here
                ConflictResolution::Deferred
            }
        };

        self.resolved = !matches!(resolution, ConflictResolution::Deferred);
        self.resolution = Some(resolution.clone());
        Ok(resolution)
    }

    /// Resolve using last writer wins strategy
    fn resolve_last_writer_wins(&self) -> ConflictResolution {
        if self.local_timestamp >= self.remote_timestamp {
            ConflictResolution::UseLocal(self.local_item.clone())
        } else {
            ConflictResolution::UseRemote(self.remote_item.clone())
        }
    }

    /// Resolve using first writer wins strategy
    fn resolve_first_writer_wins(&self) -> ConflictResolution {
        if self.local_timestamp <= self.remote_timestamp {
            ConflictResolution::UseLocal(self.local_item.clone())
        } else {
            ConflictResolution::UseRemote(self.remote_item.clone())
        }
    }

    /// Resolve using vector clock causality
    fn resolve_vector_clock(&self) -> ConflictResolution {
        use crate::vector_clock::ClockOrdering;

        match self.local_clock.compare(&self.remote_clock) {
            ClockOrdering::Before => {
                // Local happened before remote, use remote
                ConflictResolution::UseRemote(self.remote_item.clone())
            }
            ClockOrdering::After => {
                // Local happened after remote, use local
                ConflictResolution::UseLocal(self.local_item.clone())
            }
            ClockOrdering::Equal => {
                // Identical clocks, items should be the same
                ConflictResolution::UseLocal(self.local_item.clone())
            }
            ClockOrdering::Concurrent => {
                // True conflict, fall back to timestamp
                self.resolve_last_writer_wins()
            }
        }
    }

    /// Resolve by merging at the attribute level
    fn resolve_attribute_merge(&self) -> ConflictResolution {
        match (&self.local_item, &self.remote_item) {
            (Some(local), Some(remote)) => {
                let merged = Self::merge_items(local, remote, self.local_timestamp, self.remote_timestamp);
                ConflictResolution::Merged(Some(merged))
            }
            (Some(local), None) => {
                // Remote deleted, check timestamps
                if self.remote_timestamp > self.local_timestamp {
                    ConflictResolution::UseRemote(None)
                } else {
                    ConflictResolution::UseLocal(Some(local.clone()))
                }
            }
            (None, Some(remote)) => {
                // Local deleted, check timestamps
                if self.local_timestamp > self.remote_timestamp {
                    ConflictResolution::UseLocal(None)
                } else {
                    ConflictResolution::UseRemote(Some(remote.clone()))
                }
            }
            (None, None) => {
                // Both deleted
                ConflictResolution::UseLocal(None)
            }
        }
    }

    /// Merge two items at the attribute level
    fn merge_items(local: &Item, remote: &Item, local_ts: i64, remote_ts: i64) -> Item {
        let mut merged = HashMap::new();

        // Add all keys from both items
        let mut all_keys: Vec<_> = local.keys().chain(remote.keys()).collect();
        all_keys.sort();
        all_keys.dedup();

        for key in all_keys {
            match (local.get(key), remote.get(key)) {
                (Some(local_val), Some(remote_val)) => {
                    // Both have the attribute, use newer
                    if local_ts >= remote_ts {
                        merged.insert(key.clone(), local_val.clone());
                    } else {
                        merged.insert(key.clone(), remote_val.clone());
                    }
                }
                (Some(val), None) => {
                    // Only local has it
                    merged.insert(key.clone(), val.clone());
                }
                (None, Some(val)) => {
                    // Only remote has it
                    merged.insert(key.clone(), val.clone());
                }
                (None, None) => {} // Shouldn't happen
            }
        }

        merged
    }
}

/// Result of conflict resolution
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConflictResolution {
    /// Use the local version
    UseLocal(Option<Item>),
    /// Use the remote version
    UseRemote(Option<Item>),
    /// Use a merged version
    Merged(Option<Item>),
    /// Resolution deferred (manual intervention needed)
    Deferred,
}

impl ConflictResolution {
    /// Get the resolved item (if any)
    pub fn get_item(&self) -> Option<&Item> {
        match self {
            Self::UseLocal(item) | Self::UseRemote(item) | Self::Merged(item) => {
                item.as_ref()
            }
            Self::Deferred => None,
        }
    }

    /// Check if this is a deletion
    pub fn is_deletion(&self) -> bool {
        match self {
            Self::UseLocal(None) | Self::UseRemote(None) | Self::Merged(None) => true,
            _ => false,
        }
    }
}

/// Trait for custom conflict resolvers
pub trait ConflictResolver: Send + Sync {
    /// Resolve a conflict
    fn resolve(&self, conflict: &Conflict) -> Result<ConflictResolution>;

    /// Get the name of this resolver
    fn name(&self) -> &str;
}

/// Manager for tracking and resolving conflicts
pub struct ConflictManager {
    /// Pending conflicts
    pending: parking_lot::RwLock<HashMap<String, Conflict>>,
    /// Resolved conflicts (kept for history)
    resolved: parking_lot::RwLock<Vec<Conflict>>,
    /// Custom resolvers
    resolvers: HashMap<String, Box<dyn ConflictResolver>>,
    /// Default strategy
    default_strategy: ConflictStrategy,
    /// Maximum resolved conflicts to keep
    max_resolved: usize,
}

impl ConflictManager {
    /// Create a new conflict manager
    pub fn new(default_strategy: ConflictStrategy) -> Self {
        Self {
            pending: parking_lot::RwLock::new(HashMap::new()),
            resolved: parking_lot::RwLock::new(Vec::new()),
            resolvers: HashMap::new(),
            default_strategy,
            max_resolved: 1000,
        }
    }

    /// Register a custom resolver
    pub fn register_resolver(&mut self, resolver: Box<dyn ConflictResolver>) {
        self.resolvers.insert(resolver.name().to_string(), resolver);
    }

    /// Add a new conflict
    pub fn add_conflict(&self, mut conflict: Conflict) -> Result<String> {
        // Auto-resolve if not concurrent
        if !conflict.is_concurrent() {
            conflict.resolve()?;
            if conflict.resolved {
                self.add_resolved(conflict.clone());
                return Ok(conflict.id);
            }
        }

        let id = conflict.id.clone();
        self.pending.write().insert(id.clone(), conflict);
        Ok(id)
    }

    /// Resolve a pending conflict
    pub fn resolve_conflict(&self, conflict_id: &str) -> Result<ConflictResolution> {
        let mut pending = self.pending.write();

        if let Some(mut conflict) = pending.remove(conflict_id) {
            // Try custom resolver if specified
            if let ConflictStrategy::Custom(ref name) = conflict.strategy {
                if let Some(resolver) = self.resolvers.get(name) {
                    let resolution = resolver.resolve(&conflict)?;
                    conflict.resolution = Some(resolution.clone());
                    conflict.resolved = true;
                    self.add_resolved(conflict);
                    return Ok(resolution);
                }
            }

            // Use default resolution
            let resolution = conflict.resolve()?;
            if conflict.resolved {
                self.add_resolved(conflict);
            } else {
                // Put back if not resolved
                pending.insert(conflict_id.to_string(), conflict);
            }
            Ok(resolution)
        } else {
            Err(anyhow::anyhow!("Conflict not found: {}", conflict_id))
        }
    }

    /// Resolve all pending conflicts
    pub fn resolve_all(&self) -> Vec<(String, Result<ConflictResolution>)> {
        let ids: Vec<String> = self.pending.read().keys().cloned().collect();

        ids.into_iter()
            .map(|id| {
                let result = self.resolve_conflict(&id);
                (id, result)
            })
            .collect()
    }

    /// Get pending conflicts
    pub fn get_pending(&self) -> Vec<Conflict> {
        self.pending.read().values().cloned().collect()
    }

    /// Get resolved conflicts
    pub fn get_resolved(&self) -> Vec<Conflict> {
        self.resolved.read().clone()
    }

    /// Add to resolved history
    fn add_resolved(&self, conflict: Conflict) {
        let mut resolved = self.resolved.write();
        resolved.push(conflict);

        // Limit history size
        if resolved.len() > self.max_resolved {
            let drain_count = resolved.len() - self.max_resolved;
            resolved.drain(0..drain_count);
        }
    }

    /// Clear all conflicts
    pub fn clear(&self) {
        self.pending.write().clear();
        self.resolved.write().clear();
    }

    /// Get statistics
    pub fn get_stats(&self) -> ConflictStats {
        ConflictStats {
            pending_count: self.pending.read().len(),
            resolved_count: self.resolved.read().len(),
        }
    }
}

/// Conflict statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictStats {
    pub pending_count: usize,
    pub resolved_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_item(value: &str) -> Item {
        let mut item = HashMap::new();
        item.insert("value".to_string(), Value::string(value));
        item
    }

    #[test]
    fn test_last_writer_wins() {
        let mut conflict = Conflict::new(
            Key::new(b"key1".to_vec()),
            Some(create_test_item("local")),
            Some(create_test_item("remote")),
            VectorClock::new(),
            VectorClock::new(),
            100,
            200, // Remote is newer
            ConflictStrategy::LastWriterWins,
        );

        let resolution = conflict.resolve().unwrap();
        assert!(matches!(resolution, ConflictResolution::UseRemote(_)));
    }

    #[test]
    fn test_first_writer_wins() {
        let mut conflict = Conflict::new(
            Key::new(b"key1".to_vec()),
            Some(create_test_item("local")),
            Some(create_test_item("remote")),
            VectorClock::new(),
            VectorClock::new(),
            100, // Local is older
            200,
            ConflictStrategy::FirstWriterWins,
        );

        let resolution = conflict.resolve().unwrap();
        assert!(matches!(resolution, ConflictResolution::UseLocal(_)));
    }

    #[test]
    fn test_vector_clock_resolution() {
        let local_clock = VectorClock::with_local(EndpointId::from_str("local"), 5);
        let mut remote_clock = VectorClock::with_local(EndpointId::from_str("remote"), 3);
        remote_clock.update(EndpointId::from_str("local"), 3); // Remote is behind local

        let mut conflict = Conflict::new(
            Key::new(b"key1".to_vec()),
            Some(create_test_item("local")),
            Some(create_test_item("remote")),
            local_clock,
            remote_clock,
            100,
            200,
            ConflictStrategy::VectorClock,
        );

        let resolution = conflict.resolve().unwrap();
        assert!(matches!(resolution, ConflictResolution::UseLocal(_)));
    }

    #[test]
    fn test_attribute_merge() {
        let mut local = HashMap::new();
        local.insert("field1".to_string(), Value::string("local1"));
        local.insert("field2".to_string(), Value::string("local2"));

        let mut remote = HashMap::new();
        remote.insert("field2".to_string(), Value::string("remote2"));
        remote.insert("field3".to_string(), Value::string("remote3"));

        let mut conflict = Conflict::new(
            Key::new(b"key1".to_vec()),
            Some(local),
            Some(remote),
            VectorClock::new(),
            VectorClock::new(),
            100,
            200, // Remote is newer
            ConflictStrategy::AttributeMerge,
        );

        let resolution = conflict.resolve().unwrap();
        if let ConflictResolution::Merged(Some(item)) = resolution {
            assert_eq!(item.len(), 3); // Should have all 3 fields
            assert_eq!(item.get("field1").unwrap().as_string(), Some("local1"));
            assert_eq!(item.get("field2").unwrap().as_string(), Some("remote2")); // Remote is newer
            assert_eq!(item.get("field3").unwrap().as_string(), Some("remote3"));
        } else {
            panic!("Expected merged resolution");
        }
    }
}
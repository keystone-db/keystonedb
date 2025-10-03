/// Background compaction for LSM engine
///
/// Compaction merges multiple SST files into one, keeping only the latest version
/// of each key. This reduces read amplification and reclaims disk space.
///
/// # Compaction Strategy
///
/// Uses a simple level-based compaction approach:
/// - Triggers when a stripe has too many SST files (default: ≥10 SSTs)
/// - Merges all SSTs in a stripe into a single new SST
/// - Removes tombstones (deleted records) during merge
/// - Keeps newest version of each key (highest SeqNo)

use crate::{Error, Result, Record, sst::{SstWriter, SstReader}};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Default compaction trigger: compact when stripe has this many SSTs
pub const DEFAULT_SST_THRESHOLD: usize = 10;

/// Minimum SSTs required to trigger compaction (must have at least 2 to merge)
pub const MIN_SSTS_TO_COMPACT: usize = 2;

/// Legacy constant for backward compatibility
pub const COMPACTION_THRESHOLD: usize = DEFAULT_SST_THRESHOLD;

/// Compaction configuration
#[derive(Clone, Debug)]
pub struct CompactionConfig {
    /// Enable/disable automatic background compaction
    pub enabled: bool,

    /// Trigger compaction when stripe has this many SSTs
    pub sst_threshold: usize,

    /// How often to check for compaction opportunities (in seconds)
    pub check_interval_secs: u64,

    /// Maximum number of stripes to compact concurrently
    pub max_concurrent_compactions: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sst_threshold: DEFAULT_SST_THRESHOLD,
            check_interval_secs: 60, // Check every minute
            max_concurrent_compactions: 4, // Compact up to 4 stripes at once
        }
    }
}

impl CompactionConfig {
    /// Create a new compaction config with all defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable automatic compaction
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }

    /// Set SST threshold for triggering compaction
    pub fn with_sst_threshold(mut self, threshold: usize) -> Self {
        self.sst_threshold = threshold.max(MIN_SSTS_TO_COMPACT);
        self
    }

    /// Set check interval in seconds
    pub fn with_check_interval(mut self, seconds: u64) -> Self {
        self.check_interval_secs = seconds;
        self
    }

    /// Set maximum concurrent compactions
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent_compactions = max.max(1);
        self
    }
}

/// Statistics about compaction operations
#[derive(Clone, Debug, Default)]
pub struct CompactionStats {
    /// Total number of compactions performed
    pub total_compactions: u64,

    /// Total number of SSTs merged
    pub total_ssts_merged: u64,

    /// Total number of SSTs created
    pub total_ssts_created: u64,

    /// Total bytes read during compaction
    pub total_bytes_read: u64,

    /// Total bytes written during compaction
    pub total_bytes_written: u64,

    /// Total bytes reclaimed (deleted records)
    pub total_bytes_reclaimed: u64,

    /// Total number of records deduplicated
    pub total_records_deduplicated: u64,

    /// Total number of tombstones removed
    pub total_tombstones_removed: u64,

    /// Number of currently active compactions
    pub active_compactions: u64,
}

/// Thread-safe compaction statistics using atomics
#[derive(Clone)]
pub struct CompactionStatsAtomic {
    total_compactions: Arc<AtomicU64>,
    total_ssts_merged: Arc<AtomicU64>,
    total_ssts_created: Arc<AtomicU64>,
    total_bytes_read: Arc<AtomicU64>,
    total_bytes_written: Arc<AtomicU64>,
    total_bytes_reclaimed: Arc<AtomicU64>,
    total_records_deduplicated: Arc<AtomicU64>,
    total_tombstones_removed: Arc<AtomicU64>,
    active_compactions: Arc<AtomicU64>,
}

impl Default for CompactionStatsAtomic {
    fn default() -> Self {
        Self::new()
    }
}

impl CompactionStatsAtomic {
    /// Create new atomic statistics
    pub fn new() -> Self {
        Self {
            total_compactions: Arc::new(AtomicU64::new(0)),
            total_ssts_merged: Arc::new(AtomicU64::new(0)),
            total_ssts_created: Arc::new(AtomicU64::new(0)),
            total_bytes_read: Arc::new(AtomicU64::new(0)),
            total_bytes_written: Arc::new(AtomicU64::new(0)),
            total_bytes_reclaimed: Arc::new(AtomicU64::new(0)),
            total_records_deduplicated: Arc::new(AtomicU64::new(0)),
            total_tombstones_removed: Arc::new(AtomicU64::new(0)),
            active_compactions: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get a snapshot of current statistics
    pub fn snapshot(&self) -> CompactionStats {
        CompactionStats {
            total_compactions: self.total_compactions.load(Ordering::Relaxed),
            total_ssts_merged: self.total_ssts_merged.load(Ordering::Relaxed),
            total_ssts_created: self.total_ssts_created.load(Ordering::Relaxed),
            total_bytes_read: self.total_bytes_read.load(Ordering::Relaxed),
            total_bytes_written: self.total_bytes_written.load(Ordering::Relaxed),
            total_bytes_reclaimed: self.total_bytes_reclaimed.load(Ordering::Relaxed),
            total_records_deduplicated: self.total_records_deduplicated.load(Ordering::Relaxed),
            total_tombstones_removed: self.total_tombstones_removed.load(Ordering::Relaxed),
            active_compactions: self.active_compactions.load(Ordering::Relaxed),
        }
    }

    /// Increment compaction counter and return guard that decrements active count on drop
    pub fn start_compaction(&self) -> CompactionGuard {
        self.total_compactions.fetch_add(1, Ordering::Relaxed);
        self.active_compactions.fetch_add(1, Ordering::Relaxed);
        CompactionGuard {
            stats: self.clone(),
        }
    }

    /// Record SSTs merged
    pub fn record_ssts_merged(&self, count: u64) {
        self.total_ssts_merged.fetch_add(count, Ordering::Relaxed);
    }

    /// Record SSTs created
    pub fn record_ssts_created(&self, count: u64) {
        self.total_ssts_created.fetch_add(count, Ordering::Relaxed);
    }

    /// Record bytes read
    pub fn record_bytes_read(&self, bytes: u64) {
        self.total_bytes_read.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record bytes written
    pub fn record_bytes_written(&self, bytes: u64) {
        self.total_bytes_written.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record bytes reclaimed
    pub fn record_bytes_reclaimed(&self, bytes: u64) {
        self.total_bytes_reclaimed.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record records deduplicated
    pub fn record_records_deduplicated(&self, count: u64) {
        self.total_records_deduplicated
            .fetch_add(count, Ordering::Relaxed);
    }

    /// Record tombstones removed
    pub fn record_tombstones_removed(&self, count: u64) {
        self.total_tombstones_removed
            .fetch_add(count, Ordering::Relaxed);
    }
}

/// RAII guard that decrements active compaction count on drop
pub struct CompactionGuard {
    stats: CompactionStatsAtomic,
}

impl Drop for CompactionGuard {
    fn drop(&mut self) {
        self.stats
            .active_compactions
            .fetch_sub(1, Ordering::Relaxed);
    }
}

/// Compaction manager for a stripe
pub struct CompactionManager {
    stripe_id: usize,
    dir: PathBuf,
}

impl CompactionManager {
    /// Create a new compaction manager
    pub fn new(stripe_id: usize, dir: PathBuf) -> Self {
        Self { stripe_id, dir }
    }

    /// Check if compaction is needed for this stripe
    pub fn needs_compaction(&self, sst_count: usize) -> bool {
        sst_count >= COMPACTION_THRESHOLD
    }

    /// Compact multiple SST files into a single new SST
    ///
    /// Algorithm:
    /// 1. Read all records from all input SSTs
    /// 2. Merge by key, keeping only the latest version (highest SeqNo)
    /// 3. Filter out tombstones (deleted records)
    /// 4. Write merged records to new SST
    /// 5. Return new SST reader and paths of old SSTs to delete
    pub fn compact(
        &self,
        ssts: &[SstReader],
        next_sst_id: u64,
    ) -> Result<(SstReader, Vec<PathBuf>)> {
        if ssts.is_empty() {
            return Err(Error::InvalidArgument("Cannot compact zero SSTs".into()));
        }

        // Step 1: Collect all records from all SSTs into a map
        // Key: encoded key, Value: latest record for that key
        let mut records_by_key: BTreeMap<Vec<u8>, Record> = BTreeMap::new();

        for sst in ssts {
            for record in sst.scan()? {
                let encoded_key = record.key.encode().to_vec();

                // Keep record with highest SeqNo (latest version)
                records_by_key
                    .entry(encoded_key)
                    .and_modify(|existing| {
                        if record.seq > existing.seq {
                            *existing = record.clone();
                        }
                    })
                    .or_insert(record);
            }
        }

        // Step 2: Filter out tombstones and collect records to write
        let mut records_to_write: Vec<Record> = records_by_key
            .into_values()
            .filter(|record| !record.is_tombstone())
            .collect();

        // Sort by encoded key (already sorted from BTreeMap, but ensure consistency)
        records_to_write.sort_by(|a, b| a.key.encode().cmp(&b.key.encode()));

        // Step 3: Write new SST
        let new_sst_path = self.dir.join(format!("{:03}-{}.sst", self.stripe_id, next_sst_id));
        let mut writer = SstWriter::new();

        for record in records_to_write {
            writer.add(record);
        }

        writer.finish(&new_sst_path)?;

        // Step 4: Open new SST reader
        let new_reader = SstReader::open(&new_sst_path)?;

        // Step 5: Collect paths of old SSTs to delete
        let old_sst_paths: Vec<PathBuf> = ssts
            .iter()
            .map(|sst| sst.path().to_path_buf())
            .collect();

        Ok((new_reader, old_sst_paths))
    }

    /// Delete old SST files after successful compaction
    pub fn cleanup_old_ssts(&self, old_sst_paths: Vec<PathBuf>) -> Result<()> {
        for path in old_sst_paths {
            if path.exists() {
                fs::remove_file(&path).map_err(|e| {
                    Error::Internal(format!("Failed to delete old SST {:?}: {}", path, e))
                })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, Value, SeqNo, sst::SstWriter};
    use tempfile::TempDir;
    use std::collections::HashMap;

    fn create_test_record(pk: &[u8], seq: SeqNo, value: &str) -> Record {
        let mut item = HashMap::new();
        item.insert("value".to_string(), Value::string(value));
        Record::put(Key::new(pk.to_vec()), item, seq)
    }

    fn create_delete_record(pk: &[u8], seq: SeqNo) -> Record {
        Record::delete(Key::new(pk.to_vec()), seq)
    }

    #[test]
    fn test_compaction_manager_needs_compaction() {
        let dir = TempDir::new().unwrap();
        let manager = CompactionManager::new(0, dir.path().to_path_buf());

        assert!(!manager.needs_compaction(5));
        assert!(!manager.needs_compaction(9));
        assert!(manager.needs_compaction(10));
        assert!(manager.needs_compaction(15));
    }

    #[test]
    fn test_compact_multiple_versions() {
        let dir = TempDir::new().unwrap();
        let manager = CompactionManager::new(0, dir.path().to_path_buf());

        // Create SST 1: key1=v1 (seq 1), key2=v1 (seq 1)
        let sst1_path = dir.path().join("000-1.sst");
        let mut writer1 = SstWriter::new();
        writer1.add(create_test_record(b"key1", 1, "v1"));
        writer1.add(create_test_record(b"key2", 1, "v1"));
        writer1.finish(&sst1_path).unwrap();

        // Create SST 2: key1=v2 (seq 2), key3=v1 (seq 2)
        let sst2_path = dir.path().join("000-2.sst");
        let mut writer2 = SstWriter::new();
        writer2.add(create_test_record(b"key1", 2, "v2"));
        writer2.add(create_test_record(b"key3", 2, "v1"));
        writer2.finish(&sst2_path).unwrap();

        // Compact
        let sst1 = SstReader::open(&sst1_path).unwrap();
        let sst2 = SstReader::open(&sst2_path).unwrap();
        let (new_sst, old_paths) = manager.compact(&[sst1, sst2], 3).unwrap();

        // Verify: should have 3 keys, with key1 having latest version (v2)
        let records: Vec<Record> = new_sst.scan().unwrap().collect();
        assert_eq!(records.len(), 3);

        // Find key1
        let key1_record = records.iter().find(|r| r.key.pk.as_ref() == b"key1").unwrap();
        assert_eq!(key1_record.seq, 2);
        assert_eq!(
            key1_record.value.as_ref().unwrap().get("value").unwrap().as_string(),
            Some("v2")
        );

        assert_eq!(old_paths.len(), 2);
    }

    #[test]
    fn test_compact_filters_tombstones() {
        let dir = TempDir::new().unwrap();
        let manager = CompactionManager::new(0, dir.path().to_path_buf());

        // Create SST 1: key1=v1 (seq 1), key2=v1 (seq 1)
        let sst1_path = dir.path().join("000-1.sst");
        let mut writer1 = SstWriter::new();
        writer1.add(create_test_record(b"key1", 1, "v1"));
        writer1.add(create_test_record(b"key2", 1, "v1"));
        writer1.finish(&sst1_path).unwrap();

        // Create SST 2: key1=DELETE (seq 2)
        let sst2_path = dir.path().join("000-2.sst");
        let mut writer2 = SstWriter::new();
        writer2.add(create_delete_record(b"key1", 2));
        writer2.finish(&sst2_path).unwrap();

        // Compact
        let sst1 = SstReader::open(&sst1_path).unwrap();
        let sst2 = SstReader::open(&sst2_path).unwrap();
        let (new_sst, _) = manager.compact(&[sst1, sst2], 3).unwrap();

        // Verify: should only have key2 (key1 was deleted)
        let records: Vec<Record> = new_sst.scan().unwrap().collect();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].key.pk.as_ref(), b"key2");
    }

    #[test]
    fn test_compact_empty_result() {
        let dir = TempDir::new().unwrap();
        let manager = CompactionManager::new(0, dir.path().to_path_buf());

        // Create SST with only a deletion
        let sst1_path = dir.path().join("000-1.sst");
        let mut writer1 = SstWriter::new();
        writer1.add(create_delete_record(b"key1", 1));
        writer1.finish(&sst1_path).unwrap();

        // Compact
        let sst1 = SstReader::open(&sst1_path).unwrap();
        let (new_sst, _) = manager.compact(&[sst1], 2).unwrap();

        // Verify: should be empty (all tombstones filtered)
        let records: Vec<Record> = new_sst.scan().unwrap().collect();
        assert_eq!(records.len(), 0);
    }

    #[test]
    fn test_cleanup_old_ssts() {
        let dir = TempDir::new().unwrap();
        let manager = CompactionManager::new(0, dir.path().to_path_buf());

        // Create test files
        let file1 = dir.path().join("000-1.sst");
        let file2 = dir.path().join("000-2.sst");
        std::fs::write(&file1, b"test").unwrap();
        std::fs::write(&file2, b"test").unwrap();

        assert!(file1.exists());
        assert!(file2.exists());

        // Cleanup
        manager.cleanup_old_ssts(vec![file1.clone(), file2.clone()]).unwrap();

        assert!(!file1.exists());
        assert!(!file2.exists());
    }

    #[test]
    fn test_compact_preserves_order() {
        let dir = TempDir::new().unwrap();
        let manager = CompactionManager::new(0, dir.path().to_path_buf());

        // Create SST with multiple keys in order
        let sst1_path = dir.path().join("000-1.sst");
        let mut writer1 = SstWriter::new();
        writer1.add(create_test_record(b"key1", 1, "v1"));
        writer1.add(create_test_record(b"key3", 1, "v3"));
        writer1.finish(&sst1_path).unwrap();

        let sst2_path = dir.path().join("000-2.sst");
        let mut writer2 = SstWriter::new();
        writer2.add(create_test_record(b"key2", 2, "v2"));
        writer2.add(create_test_record(b"key4", 2, "v4"));
        writer2.finish(&sst2_path).unwrap();

        // Compact
        let sst1 = SstReader::open(&sst1_path).unwrap();
        let sst2 = SstReader::open(&sst2_path).unwrap();
        let (new_sst, _) = manager.compact(&[sst1, sst2], 3).unwrap();

        // Verify: records should be sorted
        let records: Vec<Record> = new_sst.scan().unwrap().collect();
        assert_eq!(records.len(), 4);
        assert_eq!(records[0].key.pk.as_ref(), b"key1");
        assert_eq!(records[1].key.pk.as_ref(), b"key2");
        assert_eq!(records[2].key.pk.as_ref(), b"key3");
        assert_eq!(records[3].key.pk.as_ref(), b"key4");
    }

    #[test]
    fn test_compaction_config_defaults() {
        let config = CompactionConfig::default();
        assert!(config.enabled);
        assert_eq!(config.sst_threshold, DEFAULT_SST_THRESHOLD);
        assert_eq!(config.check_interval_secs, 60);
        assert_eq!(config.max_concurrent_compactions, 4);
    }

    #[test]
    fn test_compaction_config_disabled() {
        let config = CompactionConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn test_compaction_config_builder() {
        let config = CompactionConfig::new()
            .with_sst_threshold(5)
            .with_check_interval(30)
            .with_max_concurrent(2);

        assert_eq!(config.sst_threshold, 5);
        assert_eq!(config.check_interval_secs, 30);
        assert_eq!(config.max_concurrent_compactions, 2);
    }

    #[test]
    fn test_compaction_stats_atomic_snapshot() {
        let stats = CompactionStatsAtomic::new();

        stats.record_ssts_merged(5);
        stats.record_ssts_created(1);
        stats.record_bytes_read(1024);
        stats.record_bytes_written(512);
        stats.record_tombstones_removed(3);

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.total_ssts_merged, 5);
        assert_eq!(snapshot.total_ssts_created, 1);
        assert_eq!(snapshot.total_bytes_read, 1024);
        assert_eq!(snapshot.total_bytes_written, 512);
        assert_eq!(snapshot.total_tombstones_removed, 3);
    }

    #[test]
    fn test_compaction_guard_decrements_on_drop() {
        let stats = CompactionStatsAtomic::new();

        {
            let _guard = stats.start_compaction();
            assert_eq!(stats.active_compactions.load(Ordering::Relaxed), 1);
            assert_eq!(stats.total_compactions.load(Ordering::Relaxed), 1);
        }

        // Guard dropped, active should decrement
        assert_eq!(stats.active_compactions.load(Ordering::Relaxed), 0);
        assert_eq!(stats.total_compactions.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_multiple_compaction_guards() {
        let stats = CompactionStatsAtomic::new();

        let _guard1 = stats.start_compaction();
        let _guard2 = stats.start_compaction();
        let _guard3 = stats.start_compaction();

        assert_eq!(stats.active_compactions.load(Ordering::Relaxed), 3);
        assert_eq!(stats.total_compactions.load(Ordering::Relaxed), 3);

        drop(_guard2);
        assert_eq!(stats.active_compactions.load(Ordering::Relaxed), 2);
    }
}

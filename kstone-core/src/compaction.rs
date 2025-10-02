/// Background compaction for LSM engine
///
/// Compaction merges multiple SST files into one, keeping only the latest version
/// of each key. This reduces read amplification and reclaims disk space.

use crate::{Error, Result, Record, sst::{SstWriter, SstReader}};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::fs;

/// Threshold for triggering compaction (number of SST files per stripe)
pub const COMPACTION_THRESHOLD: usize = 10;

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
}

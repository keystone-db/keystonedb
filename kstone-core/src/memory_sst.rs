/// In-memory Sorted String Table for testing and temporary databases
///
/// Provides the same API as the disk-based SST but stores all records in memory.
/// All data is lost when the MemorySstReader is dropped.

use crate::{Record, Result, Key, bloom::BloomFilter};
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// In-memory SST writer
pub struct MemorySstWriter {
    records: Vec<Record>,
}

impl MemorySstWriter {
    /// Create a new in-memory SST writer
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Add a record to the SST
    pub fn add(&mut self, record: Record) {
        self.records.push(record);
    }

    /// Finish writing and create an in-memory SST reader
    ///
    /// Unlike disk-based SST, this doesn't write to a file.
    /// Instead, it returns a reader with the in-memory data.
    pub fn finish(mut self, name: impl Into<String>) -> Result<MemorySstReader> {
        // Sort records by encoded key
        self.records.sort_by(|a, b| a.key.encode().cmp(&b.key.encode()));

        // Build bloom filter
        let mut bloom = BloomFilter::new(self.records.len().max(1000), 10);
        for record in &self.records {
            bloom.add(&record.key.encode());
        }

        Ok(MemorySstReader {
            name: name.into(),
            records: self.records,
            bloom,
        })
    }
}

impl Default for MemorySstWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory SST reader
#[derive(Clone)]
pub struct MemorySstReader {
    name: String,
    records: Vec<Record>,
    bloom: BloomFilter,
}

impl MemorySstReader {
    /// Create a new empty in-memory SST reader (for testing)
    pub fn empty(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            records: Vec::new(),
            bloom: BloomFilter::new(1000, 10),
        }
    }

    /// Get a record by key
    pub fn get(&self, key: &Key) -> Option<&Record> {
        let key_bytes = key.encode();

        // Check bloom filter first
        if !self.bloom.contains(&key_bytes) {
            return None;
        }

        // Binary search in sorted records
        self.records
            .binary_search_by(|r| r.key.encode().as_ref().cmp(key_bytes.as_ref()))
            .ok()
            .map(|idx| &self.records[idx])
    }

    /// Iterate over all records
    pub fn iter(&self) -> impl Iterator<Item = &Record> {
        self.records.iter()
    }

    /// Scan records with a given partition key prefix
    pub fn scan_prefix<'a>(&'a self, pk: &'a Bytes) -> impl Iterator<Item = &'a Record> + 'a {
        self.records.iter().filter(move |r| r.key.pk == *pk)
    }

    /// Get the name of this SST (virtual "path")
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Scan all records (for compaction)
    pub fn scan(&self) -> Result<impl Iterator<Item = Record> + '_> {
        Ok(self.records.iter().cloned())
    }

    /// Get the number of records
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Global in-memory SST storage (for managing multiple SSTs)
///
/// This acts like a file system for in-memory SSTs, allowing storage and retrieval by name.
#[derive(Clone)]
pub struct MemorySstStore {
    ssts: Arc<Mutex<HashMap<String, MemorySstReader>>>,
}

impl MemorySstStore {
    /// Create a new empty SST store
    pub fn new() -> Self {
        Self {
            ssts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Store an SST
    pub fn store(&self, name: impl Into<String>, sst: MemorySstReader) {
        let mut ssts = self.ssts.lock().unwrap();
        ssts.insert(name.into(), sst);
    }

    /// Retrieve an SST by name
    pub fn get(&self, name: &str) -> Option<MemorySstReader> {
        let ssts = self.ssts.lock().unwrap();
        ssts.get(name).cloned()
    }

    /// Delete an SST by name
    pub fn delete(&self, name: &str) -> bool {
        let mut ssts = self.ssts.lock().unwrap();
        ssts.remove(name).is_some()
    }

    /// List all SST names
    pub fn list_names(&self) -> Vec<String> {
        let ssts = self.ssts.lock().unwrap();
        ssts.keys().cloned().collect()
    }

    /// Get the number of SSTs
    pub fn len(&self) -> usize {
        let ssts = self.ssts.lock().unwrap();
        ssts.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all SSTs
    pub fn clear(&self) {
        let mut ssts = self.ssts.lock().unwrap();
        ssts.clear();
    }
}

impl Default for MemorySstStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use std::collections::HashMap;

    fn create_test_record(pk: &[u8], seq: u64) -> Record {
        let mut item = HashMap::new();
        item.insert("test".to_string(), Value::string("value"));
        Record::put(Key::new(pk.to_vec()), item, seq)
    }

    #[test]
    fn test_memory_sst_writer_finish() {
        let mut writer = MemorySstWriter::new();
        writer.add(create_test_record(b"key1", 1));
        writer.add(create_test_record(b"key2", 2));

        let reader = writer.finish("test.sst").unwrap();
        assert_eq!(reader.len(), 2);
        assert_eq!(reader.name(), "test.sst");
    }

    #[test]
    fn test_memory_sst_reader_get() {
        let mut writer = MemorySstWriter::new();
        let key1 = Key::new(b"key1".to_vec());
        writer.add(create_test_record(b"key1", 1));

        let reader = writer.finish("test.sst").unwrap();

        let record = reader.get(&key1);
        assert!(record.is_some());
        assert_eq!(record.unwrap().seq, 1);

        let missing = reader.get(&Key::new(b"missing".to_vec()));
        assert!(missing.is_none());
    }

    #[test]
    fn test_memory_sst_reader_iter() {
        let mut writer = MemorySstWriter::new();
        writer.add(create_test_record(b"key1", 1));
        writer.add(create_test_record(b"key2", 2));
        writer.add(create_test_record(b"key3", 3));

        let reader = writer.finish("test.sst").unwrap();

        let records: Vec<_> = reader.iter().collect();
        assert_eq!(records.len(), 3);
    }

    #[test]
    fn test_memory_sst_reader_scan_prefix() {
        let mut writer = MemorySstWriter::new();
        writer.add(create_test_record(b"user#1", 1));
        writer.add(create_test_record(b"user#2", 2));
        writer.add(create_test_record(b"post#1", 3));

        let reader = writer.finish("test.sst").unwrap();

        let pk = Bytes::from_static(b"user#1");
        let records: Vec<_> = reader.scan_prefix(&pk).collect();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].key.pk, pk);
    }

    #[test]
    fn test_memory_sst_store() {
        let store = MemorySstStore::new();

        let mut writer = MemorySstWriter::new();
        writer.add(create_test_record(b"key1", 1));
        let sst = writer.finish("test.sst").unwrap();

        store.store("test.sst", sst);
        assert_eq!(store.len(), 1);

        let retrieved = store.get("test.sst");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().len(), 1);

        store.delete("test.sst");
        assert!(store.is_empty());
    }

    #[test]
    fn test_memory_sst_store_list_names() {
        let store = MemorySstStore::new();

        for i in 0..5 {
            let mut writer = MemorySstWriter::new();
            writer.add(create_test_record(format!("key{}", i).as_bytes(), i));
            let sst = writer.finish(format!("sst_{}.sst", i)).unwrap();
            store.store(format!("sst_{}.sst", i), sst);
        }

        let names = store.list_names();
        assert_eq!(names.len(), 5);
    }

    #[test]
    fn test_memory_sst_sorted_order() {
        let mut writer = MemorySstWriter::new();
        // Add in reverse order
        writer.add(create_test_record(b"key3", 3));
        writer.add(create_test_record(b"key1", 1));
        writer.add(create_test_record(b"key2", 2));

        let reader = writer.finish("test.sst").unwrap();

        let records: Vec<_> = reader.iter().collect();
        // Should be sorted by key
        assert_eq!(records[0].key.pk.as_ref(), b"key1");
        assert_eq!(records[1].key.pk.as_ref(), b"key2");
        assert_eq!(records[2].key.pk.as_ref(), b"key3");
    }

    #[test]
    fn test_memory_sst_bloom_filter() {
        let mut writer = MemorySstWriter::new();
        writer.add(create_test_record(b"exists", 1));

        let reader = writer.finish("test.sst").unwrap();

        // Key that exists
        assert!(reader.get(&Key::new(b"exists".to_vec())).is_some());

        // Key that doesn't exist (bloom filter should help)
        assert!(reader.get(&Key::new(b"doesnotexist".to_vec())).is_none());
    }
}

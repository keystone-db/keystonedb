/// In-memory Write-Ahead Log for testing and temporary databases
///
/// Provides the same API as the disk-based WAL but stores all records in memory.
/// All data is lost when the MemoryWal is dropped.

use crate::{Record, Result};
use std::sync::{Arc, Mutex};

/// Log Sequence Number (LSN) - monotonically increasing record ID
pub type Lsn = u64;

/// In-memory Write-Ahead Log
#[derive(Clone)]
pub struct MemoryWal {
    inner: Arc<Mutex<MemoryWalInner>>,
}

struct MemoryWalInner {
    /// All records stored in memory (LSN -> Record)
    records: Vec<(Lsn, Record)>,
    /// Next LSN to assign
    next_lsn: Lsn,
}

impl MemoryWal {
    /// Create a new in-memory WAL
    pub fn create() -> Result<Self> {
        Ok(Self {
            inner: Arc::new(Mutex::new(MemoryWalInner {
                records: Vec::new(),
                next_lsn: 1,
            })),
        })
    }

    /// Open is the same as create for in-memory WAL (no persistence)
    pub fn open() -> Result<Self> {
        Self::create()
    }

    /// Append a record to the WAL
    pub fn append(&self, record: Record) -> Result<Lsn> {
        let mut inner = self.inner.lock().unwrap();
        let lsn = inner.next_lsn;
        inner.next_lsn += 1;
        inner.records.push((lsn, record));
        Ok(lsn)
    }

    /// Flush is a no-op for in-memory WAL
    pub fn flush(&self) -> Result<()> {
        Ok(())
    }

    /// Read all records from the WAL
    pub fn read_all(&self) -> Result<Vec<(Lsn, Record)>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner.records.clone())
    }

    /// Get the next LSN that will be assigned
    pub fn next_lsn(&self) -> Lsn {
        let inner = self.inner.lock().unwrap();
        inner.next_lsn
    }

    /// Clear all records (useful for testing)
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.records.clear();
    }

    /// Get the number of records in the WAL
    pub fn len(&self) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.records.len()
    }

    /// Check if the WAL is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, Value};
    use std::collections::HashMap;

    fn create_test_record(pk: &[u8], seq: u64) -> Record {
        let mut item = HashMap::new();
        item.insert("test".to_string(), Value::string("value"));
        Record::put(Key::new(pk.to_vec()), item, seq)
    }

    #[test]
    fn test_memory_wal_create() {
        let wal = MemoryWal::create().unwrap();
        assert_eq!(wal.next_lsn(), 1);
        assert!(wal.is_empty());
    }

    #[test]
    fn test_memory_wal_append() {
        let wal = MemoryWal::create().unwrap();

        let record = create_test_record(b"key1", 1);
        let lsn = wal.append(record).unwrap();
        assert_eq!(lsn, 1);
        assert_eq!(wal.next_lsn(), 2);
        assert_eq!(wal.len(), 1);
    }

    #[test]
    fn test_memory_wal_read_all() {
        let wal = MemoryWal::create().unwrap();

        let record1 = create_test_record(b"key1", 1);
        let record2 = create_test_record(b"key2", 2);

        wal.append(record1.clone()).unwrap();
        wal.append(record2.clone()).unwrap();

        let records = wal.read_all().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].0, 1);
        assert_eq!(records[1].0, 2);
    }

    #[test]
    fn test_memory_wal_flush_noop() {
        let wal = MemoryWal::create().unwrap();
        let record = create_test_record(b"key1", 1);
        wal.append(record).unwrap();

        // Flush should succeed but do nothing
        wal.flush().unwrap();
        assert_eq!(wal.len(), 1);
    }

    #[test]
    fn test_memory_wal_clear() {
        let wal = MemoryWal::create().unwrap();

        wal.append(create_test_record(b"key1", 1)).unwrap();
        wal.append(create_test_record(b"key2", 2)).unwrap();
        assert_eq!(wal.len(), 2);

        wal.clear();
        assert!(wal.is_empty());
        assert_eq!(wal.next_lsn(), 3); // LSN counter not reset
    }

    #[test]
    fn test_memory_wal_clone() {
        let wal = MemoryWal::create().unwrap();
        wal.append(create_test_record(b"key1", 1)).unwrap();

        let wal2 = wal.clone();
        assert_eq!(wal2.len(), 1);

        // Append to clone affects original (shared Arc)
        wal2.append(create_test_record(b"key2", 2)).unwrap();
        assert_eq!(wal.len(), 2);
    }
}

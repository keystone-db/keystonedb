/// Ring buffer WAL for Phase 1.3+
///
/// Implements a circular write-ahead log using the WAL region from the file layout.
/// Supports wrap-around, group commit, and compaction.

use bytes::{BytesMut, BufMut};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::{Error, Result, Record, Lsn, layout::Region, types::checksum};

/// WAL record header size: lsn(8) + len(4)
const RECORD_HEADER_SIZE: usize = 12;

/// WAL entry in the ring buffer
/// Format: [lsn(8) | len(4) | data(bincode) | crc32c(4)]
struct WalEntry {
    lsn: Lsn,
    record: Record,
}

/// Ring buffer WAL
pub struct WalRing {
    inner: Arc<Mutex<WalRingInner>>,
}

struct WalRingInner {
    file: File,
    region: Region,

    // Ring buffer state
    write_offset: u64,      // Current write position (relative to region start)
    checkpoint_lsn: Lsn,    // Oldest LSN still needed
    next_lsn: Lsn,          // Next LSN to assign

    // Batching state
    pending: VecDeque<WalEntry>,
    last_flush: Instant,
    batch_timeout: Duration,
}

impl WalRing {
    /// Create a new ring buffer WAL
    pub fn create(path: impl AsRef<Path>, region: Region) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        // Initialize ring buffer with zeros
        file.seek(SeekFrom::Start(region.offset))?;
        let zeros = vec![0u8; region.size as usize];
        file.write_all(&zeros)?;
        file.sync_all()?;

        Ok(Self {
            inner: Arc::new(Mutex::new(WalRingInner {
                file,
                region,
                write_offset: 0,
                checkpoint_lsn: 0,
                next_lsn: 1,
                pending: VecDeque::new(),
                last_flush: Instant::now(),
                batch_timeout: Duration::from_millis(10), // 10ms default
            })),
        })
    }

    /// Open existing ring buffer WAL and recover
    pub fn open(path: impl AsRef<Path>, region: Region) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        // Recover all records from the ring
        let records = Self::recover(&mut file, &region)?;

        let max_lsn = if records.is_empty() {
            0
        } else {
            records.iter().map(|(lsn, _)| *lsn).max().unwrap_or(0)
        };

        Ok(Self {
            inner: Arc::new(Mutex::new(WalRingInner {
                file,
                region,
                write_offset: 0, // Reset to beginning, will overwrite on next write
                checkpoint_lsn: 0,
                next_lsn: max_lsn + 1,
                pending: VecDeque::new(),
                last_flush: Instant::now(),
                batch_timeout: Duration::from_millis(10),
            })),
        })
    }

    /// Append a record to the WAL (buffered)
    pub fn append(&self, record: Record) -> Result<Lsn> {
        let mut inner = self.inner.lock();

        let lsn = inner.next_lsn;
        inner.next_lsn += 1;

        inner.pending.push_back(WalEntry { lsn, record });

        // Auto-flush if batch timeout exceeded
        if inner.last_flush.elapsed() >= inner.batch_timeout {
            Self::flush_inner(&mut inner)?;
        }

        Ok(lsn)
    }

    /// Flush pending records to disk (group commit)
    pub fn flush(&self) -> Result<()> {
        let mut inner = self.inner.lock();
        Self::flush_inner(&mut inner)
    }

    /// Internal flush implementation
    fn flush_inner(inner: &mut WalRingInner) -> Result<()> {
        if inner.pending.is_empty() {
            return Ok(());
        }

        // Serialize all pending records
        let mut buf = BytesMut::new();

        for entry in &inner.pending {
            let data = bincode::serialize(&entry.record)
                .map_err(|e| Error::Internal(format!("Serialize error: {}", e)))?;

            // Record: [lsn(8) | len(4) | data | crc32c(4)]
            buf.put_u64_le(entry.lsn);
            buf.put_u32_le(data.len() as u32);
            buf.put_slice(&data);

            let crc = checksum::compute(&data);
            buf.put_u32_le(crc);
        }

        let total_size = buf.len() as u64;

        // Check if we need to wrap around
        if inner.write_offset + total_size > inner.region.size {
            // Wrap to beginning
            inner.write_offset = 0;
        }

        // Write to file
        let file_offset = inner.region.offset + inner.write_offset;
        inner.file.seek(SeekFrom::Start(file_offset))?;
        inner.file.write_all(&buf)?;
        inner.file.sync_all()?;

        // Update state
        inner.write_offset += total_size;
        inner.pending.clear();
        inner.last_flush = Instant::now();

        Ok(())
    }

    /// Recover all records from the ring buffer
    fn recover(file: &mut File, region: &Region) -> Result<Vec<(Lsn, Record)>> {
        let mut records = Vec::new();

        // Read entire ring buffer
        file.seek(SeekFrom::Start(region.offset))?;
        let mut ring_data = vec![0u8; region.size as usize];

        // Read what's available (file might be smaller than ring size)
        let bytes_read = file.read(&mut ring_data)?;
        if bytes_read == 0 {
            return Ok(records); // Empty file
        }

        let mut offset = 0usize;

        // Scan for valid records
        while offset + RECORD_HEADER_SIZE + 4 < ring_data.len() {
            // Try to parse record header
            let lsn = u64::from_le_bytes([
                ring_data[offset],
                ring_data[offset + 1],
                ring_data[offset + 2],
                ring_data[offset + 3],
                ring_data[offset + 4],
                ring_data[offset + 5],
                ring_data[offset + 6],
                ring_data[offset + 7],
            ]);

            // LSN of 0 indicates empty space (uninitialized or wrapped-over)
            if lsn == 0 {
                break;
            }

            let len = u32::from_le_bytes([
                ring_data[offset + 8],
                ring_data[offset + 9],
                ring_data[offset + 10],
                ring_data[offset + 11],
            ]) as usize;

            // Check if we have enough space for data + CRC
            if offset + RECORD_HEADER_SIZE + len + 4 > ring_data.len() {
                break; // Incomplete record at end
            }

            // Extract data and CRC
            let data_start = offset + RECORD_HEADER_SIZE;
            let data_end = data_start + len;
            let data = &ring_data[data_start..data_end];

            let crc_offset = data_end;
            let expected_crc = u32::from_le_bytes([
                ring_data[crc_offset],
                ring_data[crc_offset + 1],
                ring_data[crc_offset + 2],
                ring_data[crc_offset + 3],
            ]);

            // Verify checksum
            if checksum::verify(data, expected_crc) {
                // Valid record
                match bincode::deserialize::<Record>(data) {
                    Ok(record) => {
                        records.push((lsn, record));
                        offset = crc_offset + 4;
                    }
                    Err(_) => {
                        // Corrupted record, skip
                        break;
                    }
                }
            } else {
                // Invalid checksum, likely end of valid data
                break;
            }
        }

        // Sort by LSN (in case of wrap-around)
        records.sort_by_key(|(lsn, _)| *lsn);

        Ok(records)
    }

    /// Read all records from WAL
    pub fn read_all(&self) -> Result<Vec<(Lsn, Record)>> {
        let inner = self.inner.lock();
        let mut file = inner.file.try_clone()?;
        drop(inner);

        let inner = self.inner.lock();
        let region = inner.region;
        drop(inner);

        Self::recover(&mut file, &region)
    }

    /// Set checkpoint LSN (for compaction)
    pub fn set_checkpoint(&self, lsn: Lsn) -> Result<()> {
        let mut inner = self.inner.lock();
        inner.checkpoint_lsn = lsn;
        Ok(())
    }

    /// Compact WAL by removing entries before checkpoint
    /// Note: In a ring buffer, this just means we can overwrite them on next wrap
    pub fn compact(&self) -> Result<()> {
        // In ring buffer WAL, compaction is implicit:
        // - Records before checkpoint_lsn can be overwritten on wrap-around
        // - No explicit truncation needed
        Ok(())
    }

    /// Get next LSN
    pub fn next_lsn(&self) -> Lsn {
        let inner = self.inner.lock();
        inner.next_lsn
    }

    /// Set batch timeout for group commit
    pub fn set_batch_timeout(&self, timeout: Duration) {
        let mut inner = self.inner.lock();
        inner.batch_timeout = timeout;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, Value, layout::Region};
    use tempfile::NamedTempFile;
    use std::collections::HashMap;

    #[test]
    fn test_wal_ring_create_and_append() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 64 * 1024); // 64KB ring

        let wal = WalRing::create(tmp.path(), region).unwrap();

        let key = Key::new(b"test".to_vec());
        let mut item = HashMap::new();
        item.insert("value".to_string(), Value::string("hello"));
        let record = Record::put(key, item, 1);

        let lsn = wal.append(record).unwrap();
        assert_eq!(lsn, 1);

        wal.flush().unwrap();

        let records = wal.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, 1);
    }

    #[test]
    fn test_wal_ring_recovery() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 64 * 1024);

        // Write some records
        {
            let wal = WalRing::create(tmp.path(), region).unwrap();

            for i in 0..5 {
                let key = Key::new(format!("key{}", i).into_bytes());
                let item = HashMap::new();
                let record = Record::put(key, item, i);
                wal.append(record).unwrap();
            }

            wal.flush().unwrap();
        }

        // Reopen and verify recovery
        let wal = WalRing::open(tmp.path(), region).unwrap();
        assert_eq!(wal.next_lsn(), 6);

        let records = wal.read_all().unwrap();
        assert_eq!(records.len(), 5);
    }

    #[test]
    fn test_wal_ring_group_commit() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 64 * 1024);

        let wal = WalRing::create(tmp.path(), region).unwrap();

        // Append multiple records without flushing
        for i in 0..10 {
            let key = Key::new(format!("key{}", i).into_bytes());
            let item = HashMap::new();
            let record = Record::put(key, item, i);
            wal.append(record).unwrap();
        }

        // Single flush commits all
        wal.flush().unwrap();

        let records = wal.read_all().unwrap();
        assert_eq!(records.len(), 10);
    }

    #[test]
    fn test_wal_ring_wrap_around() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 1024); // Small ring to force wrap

        let wal = WalRing::create(tmp.path(), region).unwrap();

        // Write enough to cause wrap-around
        for i in 0..50 {
            let key = Key::new(format!("key{}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("data".to_string(), Value::string("x".repeat(50)));
            let record = Record::put(key, item, i);
            wal.append(record).unwrap();
            wal.flush().unwrap();
        }

        // Should still be able to read (though older records may be overwritten)
        let records = wal.read_all().unwrap();
        assert!(!records.is_empty());
    }

    #[test]
    fn test_wal_ring_checkpoint() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 64 * 1024);

        let wal = WalRing::create(tmp.path(), region).unwrap();

        for i in 0..10 {
            let key = Key::new(format!("key{}", i).into_bytes());
            let item = HashMap::new();
            let record = Record::put(key, item, i);
            wal.append(record).unwrap();
        }

        wal.flush().unwrap();

        // Set checkpoint
        wal.set_checkpoint(5).unwrap();
        wal.compact().unwrap(); // No-op for ring buffer, but should succeed

        let records = wal.read_all().unwrap();
        assert_eq!(records.len(), 10); // Compact doesn't remove in ring buffer
    }
}

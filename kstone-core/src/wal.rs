use crate::{Error, Result, Record, Lsn};
use bytes::{BytesMut, BufMut};
use parking_lot::Mutex;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

const WAL_HEADER_SIZE: usize = 16;
const WAL_MAGIC: u32 = 0x57414C00; // "WAL\0"
const RECORD_HEADER_SIZE: usize = 12; // lsn(8) + len(4)

/// Minimal WAL for walking skeleton
/// Format: [magic(4) | version(4) | reserved(8)] [record...]
/// Record: [lsn(8) | len(4) | data | crc(4)]
pub struct Wal {
    inner: Arc<Mutex<WalInner>>,
}

struct WalInner {
    file: File,
    next_lsn: Lsn,
    pending: Vec<Record>,
}

impl Wal {
    /// Create a new WAL file
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;

        // Write header (big-endian for magic, rest doesn't matter)
        let mut header = BytesMut::with_capacity(WAL_HEADER_SIZE);
        header.put_u32(WAL_MAGIC); // big-endian for magic
        header.put_u32_le(1); // version
        header.put_u64_le(0); // reserved
        file.write_all(&header)?;
        file.sync_all()?;

        Ok(Self {
            inner: Arc::new(Mutex::new(WalInner {
                file,
                next_lsn: 1,
                pending: Vec::new(),
            })),
        })
    }

    /// Open existing WAL file
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        // Verify header
        let mut header = [0u8; WAL_HEADER_SIZE];
        file.read_exact(&mut header)?;
        let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if magic != WAL_MAGIC {
            return Err(Error::Corruption("Invalid WAL magic".to_string()));
        }

        // Scan to find last LSN
        file.seek(SeekFrom::Start(WAL_HEADER_SIZE as u64))?;
        let mut max_lsn = 0u64;

        loop {
            let mut rec_header = [0u8; RECORD_HEADER_SIZE];
            match file.read_exact(&mut rec_header) {
                Ok(_) => {
                    let lsn = u64::from_le_bytes([
                        rec_header[0], rec_header[1], rec_header[2], rec_header[3],
                        rec_header[4], rec_header[5], rec_header[6], rec_header[7],
                    ]);
                    let len = u32::from_le_bytes([
                        rec_header[8], rec_header[9], rec_header[10], rec_header[11],
                    ]) as u64;

                    max_lsn = max_lsn.max(lsn);

                    // Skip data + crc
                    file.seek(SeekFrom::Current(len as i64 + 4))?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(WalInner {
                file,
                next_lsn: max_lsn + 1,
                pending: Vec::new(),
            })),
        })
    }

    /// Append a record (buffered, not yet durable)
    pub fn append(&self, record: Record) -> Result<Lsn> {
        let mut inner = self.inner.lock();
        let lsn = inner.next_lsn;
        inner.next_lsn += 1;
        inner.pending.push(record);
        Ok(lsn)
    }

    /// Flush pending records to disk (group commit)
    pub fn flush(&self) -> Result<()> {
        let mut inner = self.inner.lock();
        if inner.pending.is_empty() {
            return Ok(());
        }

        // Seek to end
        inner.file.seek(SeekFrom::End(0))?;

        // Prepare all records into a single buffer
        let mut full_buf = BytesMut::new();
        let base_lsn = inner.next_lsn - inner.pending.len() as u64;

        for (i, record) in inner.pending.iter().enumerate() {
            let lsn = base_lsn + i as u64;

            let data = bincode::serialize(record)
                .map_err(|e| Error::Internal(format!("Serialize error: {}", e)))?;
            let crc = crc32fast::hash(&data);

            full_buf.put_u64_le(lsn);
            full_buf.put_u32_le(data.len() as u32);
            full_buf.put_slice(&data);
            full_buf.put_u32_le(crc);
        }

        // Write all at once
        inner.file.write_all(&full_buf)?;

        inner.file.sync_all()?;
        inner.pending.clear();

        Ok(())
    }

    /// Read all records from WAL
    pub fn read_all(&self) -> Result<Vec<(Lsn, Record)>> {
        let inner = self.inner.lock();
        let mut file = inner.file.try_clone()?;
        drop(inner);

        file.seek(SeekFrom::Start(WAL_HEADER_SIZE as u64))?;

        let mut records = Vec::new();
        loop {
            let mut rec_header = [0u8; RECORD_HEADER_SIZE];
            match file.read_exact(&mut rec_header) {
                Ok(_) => {
                    let lsn = u64::from_le_bytes([
                        rec_header[0], rec_header[1], rec_header[2], rec_header[3],
                        rec_header[4], rec_header[5], rec_header[6], rec_header[7],
                    ]);
                    let len = u32::from_le_bytes([
                        rec_header[8], rec_header[9], rec_header[10], rec_header[11],
                    ]) as usize;

                    let mut data = vec![0u8; len];
                    file.read_exact(&mut data)?;

                    let mut crc_bytes = [0u8; 4];
                    file.read_exact(&mut crc_bytes)?;
                    let expected_crc = u32::from_le_bytes(crc_bytes);
                    let actual_crc = crc32fast::hash(&data);

                    if expected_crc != actual_crc {
                        return Err(Error::ChecksumMismatch);
                    }

                    let record: Record = bincode::deserialize(&data)
                        .map_err(|e| Error::Corruption(format!("Deserialize error: {}", e)))?;

                    records.push((lsn, record));
                }
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(records)
    }

    pub fn next_lsn(&self) -> Lsn {
        self.inner.lock().next_lsn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, Value};
    use tempfile::TempDir;
    use std::collections::HashMap;

    #[test]
    fn test_wal_create_and_write() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("wal.log");

        let wal = Wal::create(path).unwrap();

        let key = Key::new(b"user#123".to_vec());
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("Alice"));
        let record = Record::put(key, item, 1);

        let lsn = wal.append(record).unwrap();
        assert_eq!(lsn, 1);

        wal.flush().unwrap();

        // Verify
        let records = wal.read_all().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, 1);
    }

    #[test]
    fn test_wal_reopen() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("wal.log");

        {
            let wal = Wal::create(&path).unwrap();
            let key = Key::new(b"test".to_vec());
            let item = HashMap::new();
            let record = Record::put(key, item, 1);
            wal.append(record).unwrap();
            wal.flush().unwrap();
        }

        // Reopen
        let wal = Wal::open(&path).unwrap();
        assert_eq!(wal.next_lsn(), 2);

        let records = wal.read_all().unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn test_wal_group_commit() {
        let tmp = TempDir::new().unwrap();
        let wal = Wal::create(tmp.path().join("wal.log")).unwrap();

        // Append multiple records
        for i in 0..10 {
            let key = Key::new(format!("key{}", i).into_bytes());
            let item = HashMap::new();
            let record = Record::put(key, item, i);
            wal.append(record).unwrap();
        }

        // Single flush
        wal.flush().unwrap();

        let records = wal.read_all().unwrap();
        assert_eq!(records.len(), 10);
    }
}

use crate::{Error, Result, Record, Key};
use bytes::{Bytes, BytesMut, BufMut};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const SST_HEADER_SIZE: usize = 16;
const SST_MAGIC: u32 = 0x53535400; // "SST\0"

/// Minimal SST for walking skeleton
/// Format: [magic(4) | version(4) | count(4) | reserved(4)] [record...] [crc(4)]
/// Records are sorted by key
pub struct SstWriter {
    records: Vec<Record>,
}

impl SstWriter {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn add(&mut self, record: Record) {
        self.records.push(record);
    }

    pub fn finish(mut self, path: impl AsRef<Path>) -> Result<()> {
        // Sort records by key
        self.records.sort_by(|a, b| {
            let a_enc = a.key.encode();
            let b_enc = b.key.encode();
            a_enc.cmp(&b_enc)
        });

        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;

        // Write header (big-endian for magic, little-endian for rest)
        let mut buf = BytesMut::new();
        buf.put_u32(SST_MAGIC); // big-endian for magic
        buf.put_u32_le(1); // version
        buf.put_u32_le(self.records.len() as u32);
        buf.put_u32_le(0); // reserved

        // Serialize all records
        let mut data = Vec::new();
        for record in &self.records {
            let rec_data = bincode::serialize(record)
                .map_err(|e| Error::Internal(format!("Serialize error: {}", e)))?;
            data.extend_from_slice(&(rec_data.len() as u32).to_le_bytes());
            data.extend_from_slice(&rec_data);
        }

        buf.put_slice(&data);

        // Write CRC
        let crc = crc32fast::hash(&data);
        buf.put_u32_le(crc);

        file.write_all(&buf)?;
        file.sync_all()?;

        Ok(())
    }
}

pub struct SstReader {
    records: Vec<Record>,
    path: PathBuf,
}

impl SstReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = File::open(&path)?;

        // Read header
        let mut header = [0u8; SST_HEADER_SIZE];
        file.read_exact(&mut header)?;

        let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        if magic != SST_MAGIC {
            return Err(Error::Corruption("Invalid SST magic".to_string()));
        }

        let count = u32::from_le_bytes([header[8], header[9], header[10], header[11]]) as usize;

        // Read all data
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        // Verify CRC (last 4 bytes)
        if data.len() < 4 {
            return Err(Error::Corruption("SST file too short".to_string()));
        }

        let crc_offset = data.len() - 4;
        let expected_crc = u32::from_le_bytes([
            data[crc_offset],
            data[crc_offset + 1],
            data[crc_offset + 2],
            data[crc_offset + 3],
        ]);

        let actual_crc = crc32fast::hash(&data[..crc_offset]);
        if expected_crc != actual_crc {
            return Err(Error::ChecksumMismatch);
        }

        // Deserialize records
        let mut records = Vec::with_capacity(count);
        let mut offset = 0;
        let data = &data[..crc_offset];

        while offset < data.len() {
            let len = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            offset += 4;

            let record: Record = bincode::deserialize(&data[offset..offset + len])
                .map_err(|e| Error::Corruption(format!("Deserialize error: {}", e)))?;
            offset += len;

            records.push(record);
        }

        if records.len() != count {
            return Err(Error::Corruption(format!(
                "Record count mismatch: expected {}, got {}",
                count,
                records.len()
            )));
        }

        Ok(Self {
            records,
            path: path.as_ref().to_path_buf(),
        })
    }

    /// Get a record by exact key match
    pub fn get(&self, key: &Key) -> Option<&Record> {
        let key_enc = key.encode();
        self.records
            .binary_search_by(|rec| rec.key.encode().cmp(&key_enc))
            .ok()
            .map(|idx| &self.records[idx])
    }

    /// Iterate all records
    pub fn iter(&self) -> impl Iterator<Item = &Record> {
        self.records.iter()
    }

    /// Scan records with key prefix
    pub fn scan_prefix<'a>(&'a self, pk: &'a Bytes) -> impl Iterator<Item = &'a Record> + 'a {
        self.records
            .iter()
            .filter(move |rec| rec.key.pk == *pk)
    }

    /// Get the path to this SST file
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Scan all records (returns owned records for compaction)
    pub fn scan(&self) -> Result<impl Iterator<Item = Record> + '_> {
        Ok(self.records.iter().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_sst_write_and_read() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.sst");

        // Write
        {
            let mut writer = SstWriter::new();
            for i in 0..10 {
                let key = Key::new(format!("key{:03}", i).into_bytes());
                let mut item = HashMap::new();
                item.insert("value".to_string(), Value::number(i));
                writer.add(Record::put(key, item, i));
            }
            writer.finish(&path).unwrap();
        }

        // Read
        let reader = SstReader::open(&path).unwrap();
        assert_eq!(reader.records.len(), 10);

        // Get specific key
        let key = Key::new(b"key005".to_vec());
        let rec = reader.get(&key).unwrap();
        assert_eq!(rec.key, key);

        // Iterate
        let count = reader.iter().count();
        assert_eq!(count, 10);
    }

    #[test]
    fn test_sst_sorted() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.sst");

        // Write in random order
        {
            let mut writer = SstWriter::new();
            for i in [3, 1, 4, 1, 5, 9, 2, 6] {
                let key = Key::new(format!("key{}", i).into_bytes());
                let item = HashMap::new();
                writer.add(Record::put(key, item, i));
            }
            writer.finish(&path).unwrap();
        }

        // Read - should be sorted
        let reader = SstReader::open(&path).unwrap();
        let keys: Vec<_> = reader.iter().map(|r| r.key.pk.clone()).collect();

        let mut sorted_keys = keys.clone();
        sorted_keys.sort();
        assert_eq!(keys, sorted_keys);
    }

    #[test]
    fn test_sst_scan_prefix() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.sst");

        {
            let mut writer = SstWriter::new();
            writer.add(Record::put(
                Key::with_sk(b"user#1".to_vec(), b"a".to_vec()),
                HashMap::new(),
                1,
            ));
            writer.add(Record::put(
                Key::with_sk(b"user#1".to_vec(), b"b".to_vec()),
                HashMap::new(),
                2,
            ));
            writer.add(Record::put(
                Key::with_sk(b"user#2".to_vec(), b"a".to_vec()),
                HashMap::new(),
                3,
            ));
            writer.finish(&path).unwrap();
        }

        let reader = SstReader::open(&path).unwrap();
        let pk = Bytes::from("user#1");
        let user1_recs: Vec<_> = reader.scan_prefix(&pk).collect();
        assert_eq!(user1_recs.len(), 2);
    }
}

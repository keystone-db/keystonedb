/// Manifest system for metadata management (Phase 1.5+)
///
/// Tracks:
/// - SST metadata (extent, stripe, key range)
/// - Checkpoint LSN and SeqNo
/// - Stripe assignments
///
/// Uses a ring buffer format with copy-on-write updates.

use bytes::{Bytes, BytesMut, BufMut};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeMap};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;
use crate::{
    Error, Result, Lsn, SeqNo,
    layout::Region,
    extent::Extent,
    types::checksum,
    sst_block::SstBlockHandle,
};

/// Manifest sequence number
pub type ManifestSeq = u64;

/// Manifest record types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManifestRecord {
    /// Register a new SST
    AddSst {
        sst_id: u64,
        stripe: u8,
        extent: Extent,
        handle: SstBlockHandle,
        first_key: Bytes,
        last_key: Bytes,
    },

    /// Remove an SST (for compaction)
    RemoveSst {
        sst_id: u64,
    },

    /// Update checkpoint
    Checkpoint {
        lsn: Lsn,
        seq: SeqNo,
    },

    /// Stripe assignment (for Phase 1.6)
    AssignStripe {
        stripe: u8,
        sst_id: u64,
    },
}

/// SST metadata
#[derive(Debug, Clone)]
pub struct SstMetadata {
    pub sst_id: u64,
    pub stripe: u8,
    pub extent: Extent,
    pub handle: SstBlockHandle,
    pub first_key: Bytes,
    pub last_key: Bytes,
}

/// Manifest state
#[derive(Debug, Clone)]
pub struct ManifestState {
    /// Active SSTs
    pub ssts: HashMap<u64, SstMetadata>,
    /// Checkpoint LSN
    pub checkpoint_lsn: Lsn,
    /// Checkpoint SeqNo
    pub checkpoint_seq: SeqNo,
    /// Stripe assignments
    pub stripe_assignments: BTreeMap<u8, Vec<u64>>, // stripe -> sst_ids
}

impl Default for ManifestState {
    fn default() -> Self {
        Self {
            ssts: HashMap::new(),
            checkpoint_lsn: 0,
            checkpoint_seq: 0,
            stripe_assignments: BTreeMap::new(),
        }
    }
}

/// Manifest ring buffer
pub struct Manifest {
    inner: Arc<Mutex<ManifestInner>>,
}

struct ManifestInner {
    file: File,
    region: Region,
    state: ManifestState,
    next_seq: ManifestSeq,
    write_offset: u64,
    pending: Vec<(ManifestSeq, ManifestRecord)>,
}

impl Manifest {
    /// Create a new manifest
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
            inner: Arc::new(Mutex::new(ManifestInner {
                file,
                region,
                state: ManifestState::default(),
                next_seq: 1,
                write_offset: 0,
                pending: Vec::new(),
            })),
        })
    }

    /// Open existing manifest and recover state
    pub fn open(path: impl AsRef<Path>, region: Region) -> Result<Self> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;

        // Recover all records
        let records = Self::recover(&mut file, &region)?;

        // Replay to build state
        let mut state = ManifestState::default();
        let mut max_seq = 0;

        for (seq, record) in records {
            max_seq = max_seq.max(seq);
            Self::apply_record(&mut state, record);
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(ManifestInner {
                file,
                region,
                state,
                next_seq: max_seq + 1,
                write_offset: 0, // Reset to beginning
                pending: Vec::new(),
            })),
        })
    }

    /// Append a manifest record
    pub fn append(&self, record: ManifestRecord) -> Result<ManifestSeq> {
        let mut inner = self.inner.lock();

        let seq = inner.next_seq;
        inner.next_seq += 1;

        inner.pending.push((seq, record.clone()));

        // Apply to in-memory state
        Self::apply_record(&mut inner.state, record);

        Ok(seq)
    }

    /// Flush pending records to disk
    pub fn flush(&self) -> Result<()> {
        let mut inner = self.inner.lock();

        if inner.pending.is_empty() {
            return Ok(());
        }

        // Serialize all pending records
        let mut buf = BytesMut::new();

        for (seq, record) in &inner.pending {
            let data = bincode::serialize(record)
                .map_err(|e| Error::Internal(format!("Serialize error: {}", e)))?;

            // Record: [seq(8) | len(4) | data | crc32c(4)]
            buf.put_u64_le(*seq);
            buf.put_u32_le(data.len() as u32);
            buf.put_slice(&data);

            let crc = checksum::compute(&data);
            buf.put_u32_le(crc);
        }

        let total_size = buf.len() as u64;

        // Check if we need to wrap around
        if inner.write_offset + total_size > inner.region.size {
            inner.write_offset = 0;
        }

        // Write to file
        let file_offset = inner.region.offset + inner.write_offset;
        inner.file.seek(SeekFrom::Start(file_offset))?;
        inner.file.write_all(&buf)?;
        inner.file.sync_all()?;

        inner.write_offset += total_size;
        inner.pending.clear();

        Ok(())
    }

    /// Get current manifest state
    pub fn state(&self) -> ManifestState {
        let inner = self.inner.lock();
        inner.state.clone()
    }

    /// Get SST metadata
    pub fn get_sst(&self, sst_id: u64) -> Option<SstMetadata> {
        let inner = self.inner.lock();
        inner.state.ssts.get(&sst_id).cloned()
    }

    /// Compact manifest by rewriting only active records
    pub fn compact(&self) -> Result<()> {
        let mut inner = self.inner.lock();

        // Collect all active SSTs first to avoid borrow checker issues
        let ssts: Vec<_> = inner.state.ssts.values().cloned().collect();
        let mut records = Vec::new();

        for sst in ssts {
            records.push((
                inner.next_seq,
                ManifestRecord::AddSst {
                    sst_id: sst.sst_id,
                    stripe: sst.stripe,
                    extent: sst.extent,
                    handle: sst.handle.clone(),
                    first_key: sst.first_key.clone(),
                    last_key: sst.last_key.clone(),
                },
            ));
            inner.next_seq += 1;
        }

        // Add checkpoint record
        records.push((
            inner.next_seq,
            ManifestRecord::Checkpoint {
                lsn: inner.state.checkpoint_lsn,
                seq: inner.state.checkpoint_seq,
            },
        ));
        inner.next_seq += 1;

        // Write compacted records
        inner.pending = records;
        inner.write_offset = 0; // Start fresh

        drop(inner);
        self.flush()?;

        Ok(())
    }

    // Internal helpers

    fn apply_record(state: &mut ManifestState, record: ManifestRecord) {
        match record {
            ManifestRecord::AddSst {
                sst_id,
                stripe,
                extent,
                handle,
                first_key,
                last_key,
            } => {
                state.ssts.insert(
                    sst_id,
                    SstMetadata {
                        sst_id,
                        stripe,
                        extent,
                        handle,
                        first_key,
                        last_key,
                    },
                );

                state
                    .stripe_assignments
                    .entry(stripe)
                    .or_insert_with(Vec::new)
                    .push(sst_id);
            }

            ManifestRecord::RemoveSst { sst_id } => {
                if let Some(meta) = state.ssts.remove(&sst_id) {
                    if let Some(ssts) = state.stripe_assignments.get_mut(&meta.stripe) {
                        ssts.retain(|&id| id != sst_id);
                    }
                }
            }

            ManifestRecord::Checkpoint { lsn, seq } => {
                state.checkpoint_lsn = lsn;
                state.checkpoint_seq = seq;
            }

            ManifestRecord::AssignStripe { stripe, sst_id } => {
                state
                    .stripe_assignments
                    .entry(stripe)
                    .or_insert_with(Vec::new)
                    .push(sst_id);
            }
        }
    }

    fn recover(file: &mut File, region: &Region) -> Result<Vec<(ManifestSeq, ManifestRecord)>> {
        let mut records = Vec::new();

        // Read entire ring buffer
        file.seek(SeekFrom::Start(region.offset))?;
        let mut ring_data = vec![0u8; region.size as usize];

        let bytes_read = file.read(&mut ring_data)?;
        if bytes_read == 0 {
            return Ok(records);
        }

        let mut offset = 0usize;

        // Scan for valid records
        while offset + 16 < ring_data.len() {
            // Record header: seq(8) + len(4)
            let seq = u64::from_le_bytes([
                ring_data[offset],
                ring_data[offset + 1],
                ring_data[offset + 2],
                ring_data[offset + 3],
                ring_data[offset + 4],
                ring_data[offset + 5],
                ring_data[offset + 6],
                ring_data[offset + 7],
            ]);

            // Seq 0 indicates empty space
            if seq == 0 {
                break;
            }

            let len = u32::from_le_bytes([
                ring_data[offset + 8],
                ring_data[offset + 9],
                ring_data[offset + 10],
                ring_data[offset + 11],
            ]) as usize;

            if offset + 12 + len + 4 > ring_data.len() {
                break;
            }

            // Extract data and CRC
            let data_start = offset + 12;
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
                match bincode::deserialize::<ManifestRecord>(data) {
                    Ok(record) => {
                        records.push((seq, record));
                        offset = crc_offset + 4;
                    }
                    Err(_) => break,
                }
            } else {
                break;
            }
        }

        // Sort by seq
        records.sort_by_key(|(seq, _)| *seq);

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_manifest_create_and_append() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 64 * 1024);

        let manifest = Manifest::create(tmp.path(), region).unwrap();

        let extent = Extent::new(1, 0, 4096);
        let handle = SstBlockHandle {
            extent,
            num_data_blocks: 1,
            index_offset: 0,
            bloom_offset: 0,
            compressed: false,
        };

        manifest
            .append(ManifestRecord::AddSst {
                sst_id: 1,
                stripe: 0,
                extent,
                handle,
                first_key: Bytes::from("a"),
                last_key: Bytes::from("z"),
            })
            .unwrap();

        manifest.flush().unwrap();

        let state = manifest.state();
        assert_eq!(state.ssts.len(), 1);
        assert!(state.ssts.contains_key(&1));
    }

    #[test]
    fn test_manifest_recovery() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 64 * 1024);

        // Write some records
        {
            let manifest = Manifest::create(tmp.path(), region).unwrap();

            for i in 1..=5 {
                let extent = Extent::new(i, 0, 4096);
                let handle = SstBlockHandle {
                    extent,
                    num_data_blocks: 1,
                    index_offset: 0,
                    bloom_offset: 0,
                    compressed: false,
                };

                manifest
                    .append(ManifestRecord::AddSst {
                        sst_id: i,
                        stripe: 0,
                        extent,
                        handle,
                        first_key: Bytes::from(format!("key{}", i)),
                        last_key: Bytes::from(format!("key{}", i + 100)),
                    })
                    .unwrap();
            }

            manifest.flush().unwrap();
        }

        // Reopen and verify
        let manifest = Manifest::open(tmp.path(), region).unwrap();
        let state = manifest.state();
        assert_eq!(state.ssts.len(), 5);
    }

    #[test]
    fn test_manifest_remove_sst() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 64 * 1024);

        let manifest = Manifest::create(tmp.path(), region).unwrap();

        let extent = Extent::new(1, 0, 4096);
        let handle = SstBlockHandle {
            extent,
            num_data_blocks: 1,
            index_offset: 0,
            bloom_offset: 0,
            compressed: false,
        };

        manifest
            .append(ManifestRecord::AddSst {
                sst_id: 1,
                stripe: 0,
                extent,
                handle,
                first_key: Bytes::from("a"),
                last_key: Bytes::from("z"),
            })
            .unwrap();

        manifest.append(ManifestRecord::RemoveSst { sst_id: 1 }).unwrap();
        manifest.flush().unwrap();

        let state = manifest.state();
        assert_eq!(state.ssts.len(), 0);
    }

    #[test]
    fn test_manifest_checkpoint() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 64 * 1024);

        let manifest = Manifest::create(tmp.path(), region).unwrap();

        manifest
            .append(ManifestRecord::Checkpoint { lsn: 100, seq: 50 })
            .unwrap();

        manifest.flush().unwrap();

        let state = manifest.state();
        assert_eq!(state.checkpoint_lsn, 100);
        assert_eq!(state.checkpoint_seq, 50);
    }

    #[test]
    fn test_manifest_compact() {
        let tmp = NamedTempFile::new().unwrap();
        let region = Region::new(0, 64 * 1024);

        let manifest = Manifest::create(tmp.path(), region).unwrap();

        // Add and remove SSTs
        for i in 1..=10 {
            let extent = Extent::new(i, 0, 4096);
            let handle = SstBlockHandle {
                extent,
                num_data_blocks: 1,
                index_offset: 0,
                bloom_offset: 0,
                compressed: false,
            };

            manifest
                .append(ManifestRecord::AddSst {
                    sst_id: i,
                    stripe: 0,
                    extent,
                    handle,
                    first_key: Bytes::from(format!("key{}", i)),
                    last_key: Bytes::from(format!("key{}", i + 100)),
                })
                .unwrap();
        }

        // Remove half
        for i in 1..=5 {
            manifest.append(ManifestRecord::RemoveSst { sst_id: i }).unwrap();
        }

        manifest.flush().unwrap();

        // Compact
        manifest.compact().unwrap();

        // Should still have 5 SSTs
        let state = manifest.state();
        assert_eq!(state.ssts.len(), 5);
    }
}

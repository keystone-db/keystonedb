/// Block-based SST implementation for Phase 1.4+
///
/// Format:
/// - Data blocks (4KB each) with prefix-compressed records
/// - Index block mapping keys to data block offsets
/// - Bloom filter block (one filter per data block)
/// - Footer with metadata
///
/// Each data block contains sorted records with prefix compression.
/// Bloom filters reduce unnecessary block reads.

use bytes::{Bytes, BytesMut, BufMut, Buf};
use std::collections::BTreeMap;
use std::fs::File;
use crate::{
    Error, Result, Record, Key,
    layout::BLOCK_SIZE,
    block::{Block, BlockWriter, BlockReader},
    bloom::BloomFilter,
    extent::{Extent, ExtentAllocator},
    types::checksum,
};

const BITS_PER_KEY: usize = 10; // ~1% false positive rate
const MAX_RECORDS_PER_BLOCK: usize = 100; // Limit for simplicity

/// SST footer (last block)
/// Format: [num_data_blocks(4) | index_offset(8) | bloom_offset(8) | crc32c(4)]
const FOOTER_SIZE: usize = 24;

/// Block-based SST writer
pub struct SstBlockWriter {
    records: Vec<Record>,
    compress: bool,
}

impl SstBlockWriter {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            compress: false,
        }
    }

    pub fn with_compression() -> Self {
        Self {
            records: Vec::new(),
            compress: true,
        }
    }

    pub fn add(&mut self, record: Record) {
        self.records.push(record);
    }

    /// Finish writing SST to extent
    pub fn finish(
        mut self,
        file: &mut File,
        allocator: &ExtentAllocator,
    ) -> Result<SstBlockHandle> {
        // Sort records
        self.records.sort_by(|a, b| {
            a.key.encode().cmp(&b.key.encode())
        });

        // Split into data blocks
        let data_blocks = self.split_into_blocks();

        // Estimate total size
        let estimated_blocks = data_blocks.len() + 2; // data + index + bloom + footer
        let extent = allocator.allocate((estimated_blocks * BLOCK_SIZE) as u64)?;

        let mut writer = BlockWriter::new(file.try_clone()?);
        let mut block_index = Vec::new();
        let mut bloom_filters = Vec::new();
        let mut current_offset = extent.offset;

        // Write data blocks
        for (idx, records) in data_blocks.iter().enumerate() {
            let first_key = records[0].key.encode();

            // Build bloom filter for this block
            let mut bloom = BloomFilter::new(records.len(), BITS_PER_KEY);
            for rec in records {
                bloom.add(&rec.key.encode());
            }
            bloom_filters.push(bloom);

            // Encode block with prefix compression
            let block_data = self.encode_data_block(records)?;

            // Optionally compress
            let final_data = if self.compress {
                self.compress_data(&block_data)?
            } else {
                block_data
            };

            let block = Block::new(idx as u64, final_data);
            writer.write(&block, current_offset)?;

            block_index.push((first_key, current_offset));
            current_offset += BLOCK_SIZE as u64;
        }

        let index_offset = current_offset;

        // Write index block
        let index_data = self.encode_index_block(&block_index)?;
        let index_block = Block::new(data_blocks.len() as u64, index_data);
        writer.write(&index_block, current_offset)?;
        current_offset += BLOCK_SIZE as u64;

        let bloom_offset = current_offset;

        // Write bloom filter block
        let bloom_data = self.encode_bloom_block(&bloom_filters)?;
        let bloom_block = Block::new((data_blocks.len() + 1) as u64, bloom_data);
        writer.write(&bloom_block, current_offset)?;
        current_offset += BLOCK_SIZE as u64;

        // Write footer
        let footer_data = self.encode_footer(data_blocks.len(), index_offset, bloom_offset)?;
        let footer_block = Block::new((data_blocks.len() + 2) as u64, footer_data);
        writer.write(&footer_block, current_offset)?;

        writer.flush()?;

        Ok(SstBlockHandle {
            extent,
            num_data_blocks: data_blocks.len(),
            index_offset,
            bloom_offset,
            compressed: self.compress,
        })
    }

    fn split_into_blocks(&self) -> Vec<Vec<Record>> {
        let mut blocks = Vec::new();
        let mut current_block = Vec::new();

        for record in &self.records {
            current_block.push(record.clone());

            if current_block.len() >= MAX_RECORDS_PER_BLOCK {
                blocks.push(current_block);
                current_block = Vec::new();
            }
        }

        if !current_block.is_empty() {
            blocks.push(current_block);
        }

        blocks
    }

    fn encode_data_block(&self, records: &[Record]) -> Result<Bytes> {
        let mut buf = BytesMut::new();

        buf.put_u32_le(records.len() as u32);

        let mut prev_key = Bytes::new();

        for record in records {
            let key_enc = record.key.encode();

            // Prefix compression: store shared prefix length
            let shared = Self::shared_prefix_len(&prev_key, &key_enc);
            let unshared = key_enc.len() - shared;

            buf.put_u32_le(shared as u32);
            buf.put_u32_le(unshared as u32);
            buf.put_slice(&key_enc[shared..]);

            // Encode record
            let rec_data = bincode::serialize(record)
                .map_err(|e| Error::Internal(format!("Serialize error: {}", e)))?;
            buf.put_u32_le(rec_data.len() as u32);
            buf.put_slice(&rec_data);

            prev_key = key_enc;
        }

        Ok(buf.freeze())
    }

    fn encode_index_block(&self, index: &[(Bytes, u64)]) -> Result<Bytes> {
        let mut buf = BytesMut::new();

        buf.put_u32_le(index.len() as u32);

        for (key, offset) in index {
            buf.put_u32_le(key.len() as u32);
            buf.put_slice(key);
            buf.put_u64_le(*offset);
        }

        Ok(buf.freeze())
    }

    fn encode_bloom_block(&self, blooms: &[BloomFilter]) -> Result<Bytes> {
        let mut buf = BytesMut::new();

        buf.put_u32_le(blooms.len() as u32);

        for bloom in blooms {
            let bloom_data = bloom.encode();
            buf.put_u32_le(bloom_data.len() as u32);
            buf.put_slice(&bloom_data);
        }

        Ok(buf.freeze())
    }

    fn encode_footer(&self, num_blocks: usize, index_offset: u64, bloom_offset: u64) -> Result<Bytes> {
        let mut buf = BytesMut::with_capacity(FOOTER_SIZE);

        buf.put_u32_le(num_blocks as u32);
        buf.put_u64_le(index_offset);
        buf.put_u64_le(bloom_offset);

        let crc = checksum::compute(&buf);
        buf.put_u32_le(crc);

        Ok(buf.freeze())
    }

    fn shared_prefix_len(a: &[u8], b: &[u8]) -> usize {
        let mut i = 0;
        while i < a.len() && i < b.len() && a[i] == b[i] {
            i += 1;
        }
        i
    }

    fn compress_data(&self, data: &Bytes) -> Result<Bytes> {
        // Compression support will be added with zstd dependency
        // For now, return uncompressed data
        Ok(data.clone())
    }
}

/// SST block handle (metadata for reading)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SstBlockHandle {
    pub extent: Extent,
    pub num_data_blocks: usize,
    pub index_offset: u64,
    pub bloom_offset: u64,
    pub compressed: bool,
}

/// Block-based SST reader
pub struct SstBlockReader {
    file: File,
    index: BTreeMap<Bytes, u64>, // key -> block offset
    blooms: Vec<BloomFilter>,
}

impl SstBlockReader {
    pub fn open(file: File, handle: SstBlockHandle) -> Result<Self> {
        let mut reader = BlockReader::new(file.try_clone()?);

        // Read index block
        let index_block = reader.read(handle.num_data_blocks as u64, handle.index_offset)?;
        let index = Self::decode_index_block(&index_block.data)?;

        // Read bloom filters
        let bloom_block = reader.read((handle.num_data_blocks + 1) as u64, handle.bloom_offset)?;
        let blooms = Self::decode_bloom_block(&bloom_block.data)?;

        Ok(Self {
            file,
            index,
            blooms,
        })
    }

    pub fn get(&self, key: &Key) -> Result<Option<Record>> {
        let key_enc = key.encode();

        // Find block containing this key
        let block_offset = self.find_block(&key_enc)?;

        if let Some(offset) = block_offset {
            // Check bloom filter first
            let block_idx = self.index.values().position(|&o| o == offset).unwrap();
            if !self.blooms[block_idx].contains(&key_enc) {
                return Ok(None); // Definitely not present
            }

            // Read and search block
            let mut reader = BlockReader::new(self.file.try_clone()?);
            let block = reader.read(block_idx as u64, offset)?;

            let records = self.decode_data_block(&block.data)?;

            for record in records {
                if record.key == *key {
                    return Ok(Some(record));
                }
            }
        }

        Ok(None)
    }

    fn find_block(&self, key: &Bytes) -> Result<Option<u64>> {
        // Binary search in index
        let mut result = None;

        for (first_key, offset) in &self.index {
            if key >= first_key {
                result = Some(*offset);
            } else {
                break;
            }
        }

        Ok(result)
    }

    fn decode_index_block(data: &Bytes) -> Result<BTreeMap<Bytes, u64>> {
        let mut buf = data.clone();
        let count = buf.get_u32_le() as usize;

        let mut index = BTreeMap::new();

        for _ in 0..count {
            let key_len = buf.get_u32_le() as usize;
            let key = buf.copy_to_bytes(key_len);
            let offset = buf.get_u64_le();
            index.insert(key, offset);
        }

        Ok(index)
    }

    fn decode_bloom_block(data: &Bytes) -> Result<Vec<BloomFilter>> {
        let mut buf = data.clone();
        let count = buf.get_u32_le() as usize;

        let mut blooms = Vec::new();

        for _ in 0..count {
            let bloom_len = buf.get_u32_le() as usize;
            let bloom_data = buf.copy_to_bytes(bloom_len);
            let bloom = BloomFilter::decode(&bloom_data)
                .ok_or_else(|| Error::Corruption("Invalid bloom filter".to_string()))?;
            blooms.push(bloom);
        }

        Ok(blooms)
    }

    fn decode_data_block(&self, data: &Bytes) -> Result<Vec<Record>> {
        let mut buf = data.clone();
        let count = buf.get_u32_le() as usize;

        let mut records = Vec::new();
        let mut prev_key = BytesMut::new();

        for _ in 0..count {
            let shared = buf.get_u32_le() as usize;
            let unshared = buf.get_u32_le() as usize;

            // Reconstruct key
            prev_key.truncate(shared);
            prev_key.extend_from_slice(&buf.copy_to_bytes(unshared));

            // Decode record
            let rec_len = buf.get_u32_le() as usize;
            let rec_data = buf.copy_to_bytes(rec_len);

            let record: Record = bincode::deserialize(&rec_data)
                .map_err(|e| Error::Corruption(format!("Deserialize error: {}", e)))?;

            records.push(record);
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use tempfile::NamedTempFile;
    use std::collections::HashMap;

    #[test]
    fn test_sst_block_write_read() {
        let tmp = NamedTempFile::new().unwrap();
        let mut file = tmp.reopen().unwrap();

        let allocator = ExtentAllocator::new(0);
        let mut writer = SstBlockWriter::new();

        for i in 0..10 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("value".to_string(), Value::number(i));
            writer.add(Record::put(key, item, i));
        }

        let handle = writer.finish(&mut file, &allocator).unwrap();

        // Read
        let file = tmp.reopen().unwrap();
        let reader = SstBlockReader::open(file, handle).unwrap();

        let key = Key::new(b"key005".to_vec());
        let rec = reader.get(&key).unwrap().unwrap();
        assert_eq!(rec.key, key);
    }
}

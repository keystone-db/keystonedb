/// Single-file database layout
///
/// File structure:
/// ```text
/// [Header (4KB)] [WAL Ring] [Manifest Ring] [SST Heap]
/// ```
///
/// - Header: Database metadata and region offsets
/// - WAL Ring: Circular buffer for write-ahead log
/// - Manifest Ring: Circular buffer for metadata/catalog
/// - SST Heap: Variable-size extents for SST blocks

use bytes::{Bytes, BytesMut, BufMut};
use crate::{Error, Result};

/// Block size (4KB) - fundamental unit of I/O
pub const BLOCK_SIZE: usize = 4096;

/// File header size (4KB)
pub const HEADER_SIZE: usize = BLOCK_SIZE;

/// Magic number for database file (big-endian)
pub const DB_MAGIC: u32 = 0x4B53544E; // "KSTN"

/// Current file format version
pub const DB_VERSION: u32 = 1;

/// Default WAL ring size (64MB)
pub const DEFAULT_WAL_SIZE: u64 = 64 * 1024 * 1024;

/// Default Manifest ring size (4MB)
pub const DEFAULT_MANIFEST_SIZE: u64 = 4 * 1024 * 1024;

/// File layout header
///
/// Format (big-endian for magic, little-endian for rest):
/// ```text
/// [magic(4)] [version(4)]
/// [wal_offset(8)] [wal_size(8)]
/// [manifest_offset(8)] [manifest_size(8)]
/// [sst_offset(8)] [sst_size(8)]
/// [reserved(3968)]
/// [crc32c(4)]
/// ```
#[derive(Debug, Clone)]
pub struct FileHeader {
    pub magic: u32,
    pub version: u32,
    pub wal_region: Region,
    pub manifest_region: Region,
    pub sst_region: Region,
}

/// Region in the file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    /// Byte offset from start of file
    pub offset: u64,
    /// Size in bytes
    pub size: u64,
}

impl Region {
    pub fn new(offset: u64, size: u64) -> Self {
        Self { offset, size }
    }

    pub fn end(&self) -> u64 {
        self.offset + self.size
    }

    pub fn contains(&self, offset: u64) -> bool {
        offset >= self.offset && offset < self.end()
    }
}

impl FileHeader {
    /// Create a new file header with default sizes
    pub fn new() -> Self {
        let wal_offset = HEADER_SIZE as u64;
        let wal_size = DEFAULT_WAL_SIZE;

        let manifest_offset = wal_offset + wal_size;
        let manifest_size = DEFAULT_MANIFEST_SIZE;

        let sst_offset = manifest_offset + manifest_size;
        let sst_size = 0; // Will grow as needed

        Self {
            magic: DB_MAGIC,
            version: DB_VERSION,
            wal_region: Region::new(wal_offset, wal_size),
            manifest_region: Region::new(manifest_offset, manifest_size),
            sst_region: Region::new(sst_offset, sst_size),
        }
    }

    /// Create with custom region sizes
    pub fn with_sizes(wal_size: u64, manifest_size: u64) -> Self {
        let wal_offset = HEADER_SIZE as u64;
        let manifest_offset = wal_offset + wal_size;
        let sst_offset = manifest_offset + manifest_size;

        Self {
            magic: DB_MAGIC,
            version: DB_VERSION,
            wal_region: Region::new(wal_offset, wal_size),
            manifest_region: Region::new(manifest_offset, manifest_size),
            sst_region: Region::new(sst_offset, 0),
        }
    }

    /// Serialize header to bytes
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::with_capacity(HEADER_SIZE);

        // Magic and version
        buf.put_u32(self.magic); // big-endian for magic
        buf.put_u32_le(self.version);

        // WAL region
        buf.put_u64_le(self.wal_region.offset);
        buf.put_u64_le(self.wal_region.size);

        // Manifest region
        buf.put_u64_le(self.manifest_region.offset);
        buf.put_u64_le(self.manifest_region.size);

        // SST region
        buf.put_u64_le(self.sst_region.offset);
        buf.put_u64_le(self.sst_region.size);

        // Reserved space (zero-filled)
        let used = 4 + 4 + 8 + 8 + 8 + 8 + 8 + 8; // 56 bytes
        let reserved = HEADER_SIZE - used - 4; // -4 for CRC at end
        buf.put_bytes(0, reserved);

        // CRC32C of header (excluding CRC field itself)
        let crc = crate::types::checksum::compute(&buf);
        buf.put_u32_le(crc);

        buf.freeze()
    }

    /// Deserialize header from bytes
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(Error::Corruption("Header too short".to_string()));
        }

        // Verify magic
        let magic = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if magic != DB_MAGIC {
            return Err(Error::Corruption(format!("Invalid magic: 0x{:08X}", magic)));
        }

        // Verify CRC
        let expected_crc = u32::from_le_bytes([
            data[HEADER_SIZE - 4],
            data[HEADER_SIZE - 3],
            data[HEADER_SIZE - 2],
            data[HEADER_SIZE - 1],
        ]);

        let actual_crc = crate::types::checksum::compute(&data[..HEADER_SIZE - 4]);
        if expected_crc != actual_crc {
            return Err(Error::ChecksumMismatch);
        }

        // Parse fields
        let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

        let wal_offset = u64::from_le_bytes([
            data[8], data[9], data[10], data[11],
            data[12], data[13], data[14], data[15],
        ]);
        let wal_size = u64::from_le_bytes([
            data[16], data[17], data[18], data[19],
            data[20], data[21], data[22], data[23],
        ]);

        let manifest_offset = u64::from_le_bytes([
            data[24], data[25], data[26], data[27],
            data[28], data[29], data[30], data[31],
        ]);
        let manifest_size = u64::from_le_bytes([
            data[32], data[33], data[34], data[35],
            data[36], data[37], data[38], data[39],
        ]);

        let sst_offset = u64::from_le_bytes([
            data[40], data[41], data[42], data[43],
            data[44], data[45], data[46], data[47],
        ]);
        let sst_size = u64::from_le_bytes([
            data[48], data[49], data[50], data[51],
            data[52], data[53], data[54], data[55],
        ]);

        Ok(Self {
            magic,
            version,
            wal_region: Region::new(wal_offset, wal_size),
            manifest_region: Region::new(manifest_offset, manifest_size),
            sst_region: Region::new(sst_offset, sst_size),
        })
    }
}

impl Default for FileHeader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_encode_decode() {
        let header = FileHeader::new();
        let encoded = header.encode();

        assert_eq!(encoded.len(), HEADER_SIZE);

        let decoded = FileHeader::decode(&encoded).unwrap();
        assert_eq!(decoded.magic, DB_MAGIC);
        assert_eq!(decoded.version, DB_VERSION);
        assert_eq!(decoded.wal_region, header.wal_region);
        assert_eq!(decoded.manifest_region, header.manifest_region);
        assert_eq!(decoded.sst_region, header.sst_region);
    }

    #[test]
    fn test_header_with_custom_sizes() {
        let header = FileHeader::with_sizes(128 * 1024 * 1024, 8 * 1024 * 1024);

        assert_eq!(header.wal_region.offset, HEADER_SIZE as u64);
        assert_eq!(header.wal_region.size, 128 * 1024 * 1024);

        assert_eq!(header.manifest_region.offset, HEADER_SIZE as u64 + 128 * 1024 * 1024);
        assert_eq!(header.manifest_region.size, 8 * 1024 * 1024);

        let expected_sst_offset = HEADER_SIZE as u64 + 128 * 1024 * 1024 + 8 * 1024 * 1024;
        assert_eq!(header.sst_region.offset, expected_sst_offset);
    }

    #[test]
    fn test_region_contains() {
        let region = Region::new(1000, 500);

        assert!(!region.contains(999));
        assert!(region.contains(1000));
        assert!(region.contains(1250));
        assert!(region.contains(1499));
        assert!(!region.contains(1500));
    }

    #[test]
    fn test_invalid_magic() {
        let mut data = vec![0u8; HEADER_SIZE];
        data[0..4].copy_from_slice(&0x12345678u32.to_be_bytes());

        let result = FileHeader::decode(&data);
        assert!(matches!(result, Err(Error::Corruption(_))));
    }

    #[test]
    fn test_corrupted_checksum() {
        let header = FileHeader::new();
        let mut encoded = header.encode().to_vec();

        // Corrupt the CRC
        encoded[HEADER_SIZE - 1] ^= 0xFF;

        let result = FileHeader::decode(&encoded);
        assert!(matches!(result, Err(Error::ChecksumMismatch)));
    }
}

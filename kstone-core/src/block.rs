/// Block-based I/O with optional encryption
///
/// All disk I/O is performed in 4KB blocks with CRC32C checksums.
/// Optional AES-256-GCM encryption can be enabled per-block.

use bytes::{Bytes, BytesMut, BufMut};
use std::fs::File;
use std::io::{Read, Write, Seek, SeekFrom};
use crate::{Error, Result, layout::BLOCK_SIZE, types::checksum};

/// Block ID - logical block number
pub type BlockId = u64;

/// Block header (16 bytes)
///
/// Format:
/// ```text
/// [flags(1)] [reserved(3)] [data_len(4)] [reserved(8)]
/// ```
const BLOCK_HEADER_SIZE: usize = 16;

/// Block flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockFlags(u8);

impl BlockFlags {
    const ENCRYPTED: u8 = 0x01;
    const COMPRESSED: u8 = 0x02; // Reserved for future use

    pub fn new() -> Self {
        Self(0)
    }

    pub fn with_encryption(mut self) -> Self {
        self.0 |= Self::ENCRYPTED;
        self
    }

    pub fn is_encrypted(&self) -> bool {
        self.0 & Self::ENCRYPTED != 0
    }

    pub fn is_compressed(&self) -> bool {
        self.0 & Self::COMPRESSED != 0
    }

    fn as_byte(&self) -> u8 {
        self.0
    }

    fn from_byte(b: u8) -> Self {
        Self(b)
    }
}

/// In-memory block
#[derive(Debug, Clone)]
pub struct Block {
    pub id: BlockId,
    pub data: Bytes,
    pub flags: BlockFlags,
}

impl Block {
    /// Create a new block
    pub fn new(id: BlockId, data: Bytes) -> Self {
        Self {
            id,
            data,
            flags: BlockFlags::new(),
        }
    }

    /// Create with encryption flag
    pub fn with_encryption(id: BlockId, data: Bytes) -> Self {
        Self {
            id,
            data,
            flags: BlockFlags::new().with_encryption(),
        }
    }

    /// Maximum data size in a block (accounting for header + footer)
    pub fn max_data_size() -> usize {
        BLOCK_SIZE - BLOCK_HEADER_SIZE - 4 // -4 for CRC at end
    }
}

/// Block writer
pub struct BlockWriter {
    file: File,
    encryption_key: Option<[u8; 32]>,
}

impl BlockWriter {
    /// Create a new block writer
    pub fn new(file: File) -> Self {
        Self {
            file,
            encryption_key: None,
        }
    }

    /// Create with encryption key (AES-256)
    pub fn with_encryption(file: File, key: [u8; 32]) -> Self {
        Self {
            file,
            encryption_key: Some(key),
        }
    }

    /// Write a block at the given offset
    pub fn write(&mut self, block: &Block, offset: u64) -> Result<()> {
        self.file.seek(SeekFrom::Start(offset))?;

        let mut buf = BytesMut::with_capacity(BLOCK_SIZE);

        // Determine if we should encrypt
        let should_encrypt = self.encryption_key.is_some() && block.flags.is_encrypted();

        let (final_data, final_flags) = if should_encrypt {
            let key = self.encryption_key.unwrap();
            let encrypted = encrypt_data(&block.data, &key, block.id)?;
            (encrypted, block.flags)
        } else {
            (block.data.clone(), block.flags)
        };

        // Write header (16 bytes total)
        buf.put_u8(final_flags.as_byte());
        buf.put_bytes(0, 3); // reserved
        buf.put_u32_le(final_data.len() as u32);
        buf.put_bytes(0, 8); // reserved (complete 16-byte header)

        // Write data
        buf.put_slice(&final_data);

        // Pad to block size (minus CRC)
        let padding = BLOCK_SIZE - buf.len() - 4;
        buf.put_bytes(0, padding);

        // Compute and write CRC (excluding CRC field itself)
        let crc = checksum::compute(&buf);
        buf.put_u32_le(crc);

        self.file.write_all(&buf)?;
        Ok(())
    }

    /// Flush writes
    pub fn flush(&mut self) -> Result<()> {
        self.file.sync_all()?;
        Ok(())
    }
}

/// Block reader
pub struct BlockReader {
    file: File,
    encryption_key: Option<[u8; 32]>,
}

impl BlockReader {
    /// Create a new block reader
    pub fn new(file: File) -> Self {
        Self {
            file,
            encryption_key: None,
        }
    }

    /// Create with encryption key
    pub fn with_encryption(file: File, key: [u8; 32]) -> Self {
        Self {
            file,
            encryption_key: Some(key),
        }
    }

    /// Read a block from the given offset
    pub fn read(&mut self, block_id: BlockId, offset: u64) -> Result<Block> {
        self.file.seek(SeekFrom::Start(offset))?;

        let mut buf = vec![0u8; BLOCK_SIZE];
        self.file.read_exact(&mut buf)?;

        // Verify CRC
        let expected_crc = u32::from_le_bytes([
            buf[BLOCK_SIZE - 4],
            buf[BLOCK_SIZE - 3],
            buf[BLOCK_SIZE - 2],
            buf[BLOCK_SIZE - 1],
        ]);

        if !checksum::verify(&buf[..BLOCK_SIZE - 4], expected_crc) {
            return Err(Error::ChecksumMismatch);
        }

        // Parse header
        let flags = BlockFlags::from_byte(buf[0]);
        let data_len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;

        if data_len > Block::max_data_size() {
            return Err(Error::Corruption(format!("Invalid data length: {}", data_len)));
        }

        // Extract data
        let data_start = BLOCK_HEADER_SIZE;
        let data_end = data_start + data_len;
        let data = Bytes::copy_from_slice(&buf[data_start..data_end]);

        // Decrypt if necessary
        let final_data = if flags.is_encrypted() {
            if let Some(key) = self.encryption_key {
                decrypt_data(&data, &key, block_id)?
            } else {
                return Err(Error::EncryptionError("Block is encrypted but no key provided".to_string()));
            }
        } else {
            data
        };

        Ok(Block {
            id: block_id,
            data: final_data,
            flags,
        })
    }
}

/// Encrypt data using AES-256-GCM
fn encrypt_data(data: &[u8], key: &[u8; 32], block_id: BlockId) -> Result<Bytes> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };

    let cipher = Aes256Gcm::new(key.into());

    // Use block_id as part of nonce (12 bytes)
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[0..8].copy_from_slice(&block_id.to_le_bytes());
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| Error::EncryptionError(format!("Encryption failed: {}", e)))?;

    Ok(Bytes::from(ciphertext))
}

/// Decrypt data using AES-256-GCM
fn decrypt_data(data: &[u8], key: &[u8; 32], block_id: BlockId) -> Result<Bytes> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };

    let cipher = Aes256Gcm::new(key.into());

    // Use block_id as part of nonce (12 bytes)
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes[0..8].copy_from_slice(&block_id.to_le_bytes());
    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, data)
        .map_err(|e| Error::EncryptionError(format!("Decryption failed: {}", e)))?;

    Ok(Bytes::from(plaintext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_block_flags() {
        let flags = BlockFlags::new();
        assert!(!flags.is_encrypted());
        assert!(!flags.is_compressed());

        let flags = flags.with_encryption();
        assert!(flags.is_encrypted());
        assert!(!flags.is_compressed());
    }

    #[test]
    fn test_block_write_read() {
        let tmp = NamedTempFile::new().unwrap();
        let data = Bytes::from("hello world");

        // Write
        {
            let file = tmp.reopen().unwrap();
            let mut writer = BlockWriter::new(file);
            let block = Block::new(1, data.clone());
            writer.write(&block, 0).unwrap();
            writer.flush().unwrap();
        }

        // Read
        {
            let file = tmp.reopen().unwrap();
            let mut reader = BlockReader::new(file);
            let block = reader.read(1, 0).unwrap();
            assert_eq!(block.id, 1);
            assert_eq!(block.data, data);
            assert!(!block.flags.is_encrypted());
        }
    }

    #[test]
    fn test_block_encryption() {
        let tmp = NamedTempFile::new().unwrap();
        let data = Bytes::from("secret data");
        let key = [42u8; 32];

        // Write encrypted
        {
            let file = tmp.reopen().unwrap();
            let mut writer = BlockWriter::with_encryption(file, key);
            let block = Block::with_encryption(1, data.clone());
            writer.write(&block, 0).unwrap();
            writer.flush().unwrap();
        }

        // Read encrypted
        {
            let file = tmp.reopen().unwrap();
            let mut reader = BlockReader::with_encryption(file, key);
            let block = reader.read(1, 0).unwrap();
            assert_eq!(block.id, 1);
            assert_eq!(block.data, data);
            assert!(block.flags.is_encrypted());
        }
    }

    #[test]
    fn test_block_encryption_wrong_key() {
        let tmp = NamedTempFile::new().unwrap();
        let data = Bytes::from("secret data");
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];

        // Write with key1
        {
            let file = tmp.reopen().unwrap();
            let mut writer = BlockWriter::with_encryption(file, key1);
            let block = Block::with_encryption(1, data.clone());
            writer.write(&block, 0).unwrap();
            writer.flush().unwrap();
        }

        // Try to read with key2
        {
            let file = tmp.reopen().unwrap();
            let mut reader = BlockReader::with_encryption(file, key2);
            let result = reader.read(1, 0);
            assert!(matches!(result, Err(Error::EncryptionError(_))));
        }
    }

    #[test]
    fn test_block_checksum_corruption() {
        let tmp = NamedTempFile::new().unwrap();
        let data = Bytes::from("test");

        // Write valid block
        {
            let file = tmp.reopen().unwrap();
            let mut writer = BlockWriter::new(file);
            let block = Block::new(1, data.clone());
            writer.write(&block, 0).unwrap();
            writer.flush().unwrap();
        }

        // Corrupt the checksum
        {
            let mut file = tmp.reopen().unwrap();
            file.seek(SeekFrom::Start(BLOCK_SIZE as u64 - 1)).unwrap();
            file.write_all(&[0xFF]).unwrap();
            file.sync_all().unwrap();
        }

        // Try to read
        {
            let file = tmp.reopen().unwrap();
            let mut reader = BlockReader::new(file);
            let result = reader.read(1, 0);
            assert!(matches!(result, Err(Error::ChecksumMismatch)));
        }
    }

    #[test]
    fn test_block_max_data_size() {
        let max_size = Block::max_data_size();
        assert!(max_size > 0);
        assert!(max_size < BLOCK_SIZE);

        // Should be able to fit this much data
        let tmp = NamedTempFile::new().unwrap();
        let data = Bytes::from(vec![0xAB; max_size]);

        let file = tmp.reopen().unwrap();
        let mut writer = BlockWriter::new(file);
        let block = Block::new(1, data.clone());
        writer.write(&block, 0).unwrap();

        let file = tmp.reopen().unwrap();
        let mut reader = BlockReader::new(file);
        let read_block = reader.read(1, 0).unwrap();
        assert_eq!(read_block.data, data);
    }
}

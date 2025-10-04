/// Bloom filter implementation for SST blocks
///
/// Uses a bit array with multiple hash functions to test set membership.
/// Supports false positives but no false negatives.

use bytes::{Bytes, BytesMut, BufMut};

/// Bloom filter with configurable bits per key
#[derive(Clone)]
pub struct BloomFilter {
    /// Bit array
    bits: Vec<u8>,
    /// Number of bits
    num_bits: usize,
    /// Number of hash functions
    num_hashes: u32,
}

impl BloomFilter {
    /// Create a new bloom filter
    ///
    /// # Arguments
    /// * `num_items` - Expected number of items
    /// * `bits_per_key` - Bits to use per key (typically 10 for ~1% false positive rate)
    pub fn new(num_items: usize, bits_per_key: usize) -> Self {
        let num_bits = num_items * bits_per_key;
        let num_bytes = (num_bits + 7) / 8;

        // Optimal number of hash functions: k = (m/n) * ln(2)
        // For bits_per_key = 10: k ≈ 7
        let num_hashes = ((bits_per_key as f64) * 0.693).ceil() as u32;
        let num_hashes = num_hashes.max(1).min(30); // Clamp to reasonable range

        Self {
            bits: vec![0u8; num_bytes],
            num_bits,
            num_hashes,
        }
    }

    /// Add a key to the bloom filter
    pub fn add(&mut self, key: &[u8]) {
        let hash = Self::hash(key);
        for i in 0..self.num_hashes {
            let bit_pos = Self::bloom_hash(hash, i) % (self.num_bits as u64);
            self.set_bit(bit_pos as usize);
        }
    }

    /// Test if a key might be in the set
    pub fn contains(&self, key: &[u8]) -> bool {
        let hash = Self::hash(key);
        for i in 0..self.num_hashes {
            let bit_pos = Self::bloom_hash(hash, i) % (self.num_bits as u64);
            if !self.get_bit(bit_pos as usize) {
                return false; // Definitely not present
            }
        }
        true // Might be present
    }

    /// Serialize bloom filter to bytes
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u32_le(self.num_bits as u32);
        buf.put_u32_le(self.num_hashes);
        buf.put_slice(&self.bits);
        buf.freeze()
    }

    /// Deserialize bloom filter from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 8 {
            return None;
        }

        let num_bits = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let num_hashes = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

        // Must have at least 1 bit
        if num_bits == 0 {
            return None;
        }

        let num_bytes = (num_bits + 7) / 8;
        if data.len() < 8 + num_bytes {
            return None;
        }

        let bits = data[8..8 + num_bytes].to_vec();

        Some(Self {
            bits,
            num_bits,
            num_hashes,
        })
    }

    /// Get size in bytes
    pub fn size(&self) -> usize {
        8 + self.bits.len()
    }

    // Internal helpers

    fn set_bit(&mut self, pos: usize) {
        let byte_idx = pos / 8;
        let bit_idx = pos % 8;
        if byte_idx < self.bits.len() {
            self.bits[byte_idx] |= 1 << bit_idx;
        }
    }

    fn get_bit(&self, pos: usize) -> bool {
        let byte_idx = pos / 8;
        let bit_idx = pos % 8;
        if byte_idx < self.bits.len() {
            (self.bits[byte_idx] & (1 << bit_idx)) != 0
        } else {
            false
        }
    }

    /// Primary hash function
    fn hash(key: &[u8]) -> u64 {
        // Use FNV-1a hash
        let mut hash = 0xcbf29ce484222325u64;
        for &byte in key {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Bloom hash function (double hashing)
    fn bloom_hash(hash: u64, i: u32) -> u64 {
        let h1 = hash;
        let h2 = hash.wrapping_shr(32);
        h1.wrapping_add((i as u64).wrapping_mul(h2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_basic() {
        let mut bloom = BloomFilter::new(100, 10);

        // Add some keys
        bloom.add(b"key1");
        bloom.add(b"key2");
        bloom.add(b"key3");

        // Should find added keys
        assert!(bloom.contains(b"key1"));
        assert!(bloom.contains(b"key2"));
        assert!(bloom.contains(b"key3"));

        // Should not find keys that weren't added (with high probability)
        assert!(!bloom.contains(b"key4"));
        assert!(!bloom.contains(b"key5"));
    }

    #[test]
    fn test_bloom_encode_decode() {
        let mut bloom = BloomFilter::new(50, 10);

        bloom.add(b"test1");
        bloom.add(b"test2");

        let encoded = bloom.encode();
        let decoded = BloomFilter::decode(&encoded).unwrap();

        assert_eq!(decoded.num_bits, bloom.num_bits);
        assert_eq!(decoded.num_hashes, bloom.num_hashes);
        assert_eq!(decoded.bits, bloom.bits);

        // Functionality should be preserved
        assert!(decoded.contains(b"test1"));
        assert!(decoded.contains(b"test2"));
        assert!(!decoded.contains(b"test3"));
    }

    #[test]
    fn test_bloom_false_positive_rate() {
        let mut bloom = BloomFilter::new(1000, 10);

        // Add 1000 keys
        for i in 0..1000 {
            let key = format!("key{}", i);
            bloom.add(key.as_bytes());
        }

        // Test for false positives with keys not added
        let mut false_positives = 0;
        let test_count = 10000;

        for i in 1000..1000 + test_count {
            let key = format!("key{}", i);
            if bloom.contains(key.as_bytes()) {
                false_positives += 1;
            }
        }

        // With 10 bits per key, false positive rate should be ~1%
        let fp_rate = (false_positives as f64) / (test_count as f64);
        assert!(fp_rate < 0.02, "False positive rate too high: {}", fp_rate);
    }

    #[test]
    fn test_bloom_empty() {
        let bloom = BloomFilter::new(10, 10);

        // Empty bloom filter should not contain anything
        assert!(!bloom.contains(b"test"));
    }

    #[test]
    fn test_bloom_size() {
        let bloom = BloomFilter::new(100, 10);
        let size = bloom.size();

        // Should have header (8 bytes) + bit array
        let expected_bytes = (100 * 10 + 7) / 8;
        assert_eq!(size, 8 + expected_bytes);
    }

    #[test]
    fn test_bloom_decode_invalid() {
        // Too short
        assert!(BloomFilter::decode(&[0, 1, 2]).is_none());

        // Header only
        let data = vec![0u8; 8];
        assert!(BloomFilter::decode(&data).is_none());
    }

    #[test]
    fn test_bloom_num_hashes() {
        // With 10 bits per key, should get ~7 hash functions (10 * 0.693 = 6.93 ≈ 7)
        let bloom = BloomFilter::new(100, 10);
        assert_eq!(bloom.num_hashes, 7);

        // With 5 bits per key, should get ~4 hash functions (5 * 0.693 = 3.465 ≈ 4)
        let bloom = BloomFilter::new(100, 5);
        assert_eq!(bloom.num_hashes, 4);
    }
}

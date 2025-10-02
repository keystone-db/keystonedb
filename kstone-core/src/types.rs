use bytes::{Bytes, BytesMut, BufMut};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Logical Sequence Number - monotonic commit order
pub type Lsn = u64;

/// Sequence Number - MVCC version
pub type SeqNo = u64;

/// DynamoDB-style typed value
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Number (stored as string for precision)
    N(String),
    /// String
    S(String),
    /// Binary
    B(Bytes),
    /// Boolean
    Bool(bool),
    /// Null
    Null,
    /// List
    L(Vec<Value>),
    /// Map
    M(HashMap<String, Value>),
    /// Vector of f32 (for embeddings/vector search)
    VecF32(Vec<f32>),
    /// Timestamp (i64 milliseconds since epoch)
    Ts(i64),
}

impl Value {
    pub fn string(s: impl Into<String>) -> Self {
        Value::S(s.into())
    }

    pub fn number(n: impl ToString) -> Self {
        Value::N(n.to_string())
    }

    pub fn binary(b: impl Into<Bytes>) -> Self {
        Value::B(b.into())
    }

    pub fn map(m: HashMap<String, Value>) -> Self {
        Value::M(m)
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::S(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::M(m) => Some(m),
            _ => None,
        }
    }

    pub fn vector(v: Vec<f32>) -> Self {
        Value::VecF32(v)
    }

    pub fn timestamp(ts: i64) -> Self {
        Value::Ts(ts)
    }

    pub fn as_vector(&self) -> Option<&Vec<f32>> {
        match self {
            Value::VecF32(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_timestamp(&self) -> Option<i64> {
        match self {
            Value::Ts(ts) => Some(*ts),
            _ => None,
        }
    }
}

/// Item - a map of attribute names to values
pub type Item = HashMap<String, Value>;

/// Composite key: partition key + optional sort key
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Key {
    pub pk: Bytes,
    pub sk: Option<Bytes>,
}

impl Key {
    pub fn new(pk: impl Into<Bytes>) -> Self {
        Self {
            pk: pk.into(),
            sk: None,
        }
    }

    pub fn with_sk(pk: impl Into<Bytes>, sk: impl Into<Bytes>) -> Self {
        Self {
            pk: pk.into(),
            sk: Some(sk.into()),
        }
    }

    /// Encode key for storage (length-prefixed)
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u32(self.pk.len() as u32);
        buf.put(self.pk.clone());
        if let Some(sk) = &self.sk {
            buf.put_u32(sk.len() as u32);
            buf.put(sk.clone());
        } else {
            buf.put_u32(0);
        }
        buf.freeze()
    }

    /// Hash for stripe selection (256 stripes)
    pub fn stripe(&self) -> u8 {
        let hash = crc32fast::hash(&self.pk);
        (hash % 256) as u8
    }
}

/// Record stored in WAL/SST
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub key: Key,
    pub value: Option<Item>,  // None = tombstone (delete)
    pub seq: SeqNo,
}

impl Record {
    pub fn put(key: Key, item: Item, seq: SeqNo) -> Self {
        Self {
            key,
            value: Some(item),
            seq,
        }
    }

    pub fn delete(key: Key, seq: SeqNo) -> Self {
        Self {
            key,
            value: None,
            seq,
        }
    }

    pub fn is_tombstone(&self) -> bool {
        self.value.is_none()
    }
}

/// CRC32C checksum helpers (hardware-accelerated when available)
pub mod checksum {
    /// Compute CRC32C checksum of data
    pub fn compute(data: &[u8]) -> u32 {
        crc32c::crc32c(data)
    }

    /// Verify CRC32C checksum
    pub fn verify(data: &[u8], expected: u32) -> bool {
        crc32c::crc32c(data) == expected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_encode() {
        let key = Key::new(b"user#123".to_vec());
        let encoded = key.encode();
        assert!(!encoded.is_empty());

        let key_with_sk = Key::with_sk(b"user#123".to_vec(), b"post#456".to_vec());
        let encoded = key_with_sk.encode();
        assert!(!encoded.is_empty());
    }

    #[test]
    fn test_key_stripe() {
        let key1 = Key::new(b"test1".to_vec());
        let key2 = Key::new(b"test2".to_vec());
        // Just verify that stripe() returns a value (it's deterministic)
        assert_eq!(key1.stripe(), key1.stripe());
        // Different keys may have different stripes
        let _ = key2.stripe();
    }

    #[test]
    fn test_value_types() {
        let s = Value::string("hello");
        assert_eq!(s.as_string(), Some("hello"));

        let mut map = HashMap::new();
        map.insert("name".to_string(), Value::string("Alice"));
        let v = Value::map(map);
        assert!(v.as_map().is_some());
    }

    #[test]
    fn test_value_vector() {
        let vec = vec![1.0, 2.5, 3.14];
        let v = Value::vector(vec.clone());
        assert_eq!(v.as_vector(), Some(&vec));

        // Test with embedding-like data
        let embedding = Value::vector(vec![0.1, -0.2, 0.3, 0.4]);
        assert!(embedding.as_vector().is_some());
    }

    #[test]
    fn test_value_timestamp() {
        let now = 1609459200000i64; // 2021-01-01 00:00:00 UTC
        let ts = Value::timestamp(now);
        assert_eq!(ts.as_timestamp(), Some(now));

        // Test negative timestamp (before epoch)
        let before_epoch = Value::timestamp(-1000);
        assert_eq!(before_epoch.as_timestamp(), Some(-1000));
    }

    #[test]
    fn test_crc32c_compute() {
        let data = b"hello world";
        let crc = checksum::compute(data);

        // CRC32C is deterministic
        assert_eq!(crc, checksum::compute(data));

        // Different data should produce different checksums
        let crc2 = checksum::compute(b"hello world!");
        assert_ne!(crc, crc2);
    }

    #[test]
    fn test_crc32c_verify() {
        let data = b"test data";
        let crc = checksum::compute(data);

        // Valid checksum should verify
        assert!(checksum::verify(data, crc));

        // Invalid checksum should fail
        assert!(!checksum::verify(data, crc + 1));

        // Modified data should fail
        assert!(!checksum::verify(b"test datx", crc));
    }
}

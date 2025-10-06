/// Index support for LSI (Local Secondary Index) and GSI (Global Secondary Index)
///
/// Phase 3.1: LSI - alternative sort key on same partition key
/// Phase 3.2: GSI - alternative partition key and sort key

use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// Index projection type - which attributes to include in index
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndexProjection {
    /// Project all attributes (default)
    All,
    /// Project only key attributes
    KeysOnly,
    /// Project specific attributes
    Include(Vec<String>),
}

impl Default for IndexProjection {
    fn default() -> Self {
        IndexProjection::All
    }
}

/// Local Secondary Index definition
///
/// LSI shares the same partition key as the base table but uses
/// a different attribute value as the sort key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalSecondaryIndex {
    /// Index name (unique per table)
    pub name: String,
    /// Attribute name to use as alternative sort key
    pub sort_key_attribute: String,
    /// Which attributes to project into the index
    pub projection: IndexProjection,
}

impl LocalSecondaryIndex {
    /// Create a new LSI with all attributes projected
    pub fn new(name: impl Into<String>, sort_key_attribute: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sort_key_attribute: sort_key_attribute.into(),
            projection: IndexProjection::All,
        }
    }

    /// Set projection to keys only
    pub fn keys_only(mut self) -> Self {
        self.projection = IndexProjection::KeysOnly;
        self
    }

    /// Set projection to include specific attributes
    pub fn include(mut self, attributes: Vec<String>) -> Self {
        self.projection = IndexProjection::Include(attributes);
        self
    }
}

/// Global Secondary Index definition (Phase 3.2+)
///
/// GSI can use different partition key and sort key from the base table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalSecondaryIndex {
    /// Index name (unique per table)
    pub name: String,
    /// Attribute name to use as partition key
    pub partition_key_attribute: String,
    /// Optional attribute name to use as sort key
    pub sort_key_attribute: Option<String>,
    /// Which attributes to project into the index
    pub projection: IndexProjection,
}

impl GlobalSecondaryIndex {
    /// Create a new GSI with partition key only
    pub fn new(name: impl Into<String>, partition_key_attribute: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            partition_key_attribute: partition_key_attribute.into(),
            sort_key_attribute: None,
            projection: IndexProjection::All,
        }
    }

    /// Create a new GSI with partition key and sort key
    pub fn with_sort_key(
        name: impl Into<String>,
        partition_key_attribute: impl Into<String>,
        sort_key_attribute: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            partition_key_attribute: partition_key_attribute.into(),
            sort_key_attribute: Some(sort_key_attribute.into()),
            projection: IndexProjection::All,
        }
    }

    /// Set projection to keys only
    pub fn keys_only(mut self) -> Self {
        self.projection = IndexProjection::KeysOnly;
        self
    }

    /// Set projection to include specific attributes
    pub fn include(mut self, attributes: Vec<String>) -> Self {
        self.projection = IndexProjection::Include(attributes);
        self
    }
}

/// Table schema with index definitions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TableSchema {
    /// Local secondary indexes
    pub local_indexes: Vec<LocalSecondaryIndex>,
    /// Global secondary indexes (Phase 3.2+)
    pub global_indexes: Vec<GlobalSecondaryIndex>,
    /// TTL attribute name (Phase 3.3+)
    /// When set, items with this attribute containing a timestamp in the past are considered expired
    pub ttl_attribute_name: Option<String>,
    /// Stream configuration (Phase 3.4+)
    #[serde(default)]
    pub stream_config: crate::stream::StreamConfig,
    /// Attribute schemas for validation
    #[serde(default)]
    pub attribute_schemas: Vec<crate::validation::AttributeSchema>,
}

impl TableSchema {
    /// Create an empty schema
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a local secondary index
    pub fn add_local_index(mut self, index: LocalSecondaryIndex) -> Self {
        self.local_indexes.push(index);
        self
    }

    /// Get LSI by name
    pub fn get_local_index(&self, name: &str) -> Option<&LocalSecondaryIndex> {
        self.local_indexes.iter().find(|idx| idx.name == name)
    }

    /// Add a global secondary index (Phase 3.2+)
    pub fn add_global_index(mut self, index: GlobalSecondaryIndex) -> Self {
        self.global_indexes.push(index);
        self
    }

    /// Get GSI by name (Phase 3.2+)
    pub fn get_global_index(&self, name: &str) -> Option<&GlobalSecondaryIndex> {
        self.global_indexes.iter().find(|idx| idx.name == name)
    }

    /// Enable TTL (Time To Live) with the specified attribute name (Phase 3.3+)
    ///
    /// Items with this attribute containing a Unix timestamp (seconds since epoch)
    /// in the past will be considered expired and automatically deleted.
    pub fn with_ttl(mut self, attribute_name: impl Into<String>) -> Self {
        self.ttl_attribute_name = Some(attribute_name.into());
        self
    }

    /// Enable streams (Change Data Capture) with the specified configuration (Phase 3.4+)
    ///
    /// Streams capture all item-level modifications (INSERT, MODIFY, REMOVE) and
    /// make them available for processing.
    pub fn with_stream(mut self, config: crate::stream::StreamConfig) -> Self {
        self.stream_config = config;
        self
    }

    /// Check if an item is expired based on TTL (Phase 3.3+)
    ///
    /// Returns true if:
    /// - TTL is enabled AND
    /// - Item has the TTL attribute AND
    /// - The TTL timestamp is in the past
    pub fn is_expired(&self, item: &crate::Item) -> bool {
        use crate::Value;

        if let Some(ttl_attr) = &self.ttl_attribute_name {
            if let Some(ttl_value) = item.get(ttl_attr) {
                // Get current time in seconds since epoch
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;

                // Extract expiration timestamp from item
                let expires_at = match ttl_value {
                    Value::N(n) => n.parse::<i64>().ok(),
                    Value::Ts(ts) => Some(ts / 1000), // Convert millis to seconds
                    _ => None,
                };

                if let Some(expires) = expires_at {
                    return now > expires;
                }
            }
        }

        false
    }

    /// Add an attribute schema for validation
    pub fn with_attribute(mut self, schema: crate::validation::AttributeSchema) -> Self {
        self.attribute_schemas.push(schema);
        self
    }

    /// Validate an item against the attribute schemas
    pub fn validate_item(&self, item: &crate::Item) -> crate::Result<()> {
        if self.attribute_schemas.is_empty() {
            return Ok(()); // No validation if no schemas defined
        }

        let validator = crate::validation::Validator::from_schemas(self.attribute_schemas.clone());
        validator.validate(item)
    }
}

/// Encode an index key for storage
///
/// Format: [INDEX_MARKER | index_name_len | index_name | pk_len | pk | index_sk_len | index_sk]
pub fn encode_index_key(index_name: &str, pk: &Bytes, index_sk: &Bytes) -> Vec<u8> {
    const INDEX_MARKER: u8 = 0xFF;

    let index_name_bytes = index_name.as_bytes();
    let capacity = 1 + 4 + index_name_bytes.len() + 4 + pk.len() + 4 + index_sk.len();
    let mut buf = Vec::with_capacity(capacity);

    buf.push(INDEX_MARKER);
    buf.extend_from_slice(&(index_name_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(index_name_bytes);
    buf.extend_from_slice(&(pk.len() as u32).to_le_bytes());
    buf.extend_from_slice(pk);
    buf.extend_from_slice(&(index_sk.len() as u32).to_le_bytes());
    buf.extend_from_slice(index_sk);

    buf
}

/// Decode an index key
///
/// Returns (index_name, pk, index_sk) or None if not an index key
pub fn decode_index_key(encoded: &[u8]) -> Option<(String, Bytes, Bytes)> {
    const INDEX_MARKER: u8 = 0xFF;

    if encoded.is_empty() || encoded[0] != INDEX_MARKER {
        return None;
    }

    let mut pos = 1;

    // Read index name
    if encoded.len() < pos + 4 {
        return None;
    }
    let name_len = u32::from_le_bytes(encoded[pos..pos + 4].try_into().ok()?) as usize;
    pos += 4;

    if encoded.len() < pos + name_len {
        return None;
    }
    let index_name = String::from_utf8(encoded[pos..pos + name_len].to_vec()).ok()?;
    pos += name_len;

    // Read pk
    if encoded.len() < pos + 4 {
        return None;
    }
    let pk_len = u32::from_le_bytes(encoded[pos..pos + 4].try_into().ok()?) as usize;
    pos += 4;

    if encoded.len() < pos + pk_len {
        return None;
    }
    let pk = Bytes::copy_from_slice(&encoded[pos..pos + pk_len]);
    pos += pk_len;

    // Read index_sk
    if encoded.len() < pos + 4 {
        return None;
    }
    let index_sk_len = u32::from_le_bytes(encoded[pos..pos + 4].try_into().ok()?) as usize;
    pos += 4;

    if encoded.len() < pos + index_sk_len {
        return None;
    }
    let index_sk = Bytes::copy_from_slice(&encoded[pos..pos + index_sk_len]);

    Some((index_name, pk, index_sk))
}

/// Check if an encoded key is an index key
pub fn is_index_key(encoded: &[u8]) -> bool {
    const INDEX_MARKER: u8 = 0xFF;
    !encoded.is_empty() && encoded[0] == INDEX_MARKER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsi_creation() {
        let lsi = LocalSecondaryIndex::new("email-index", "email");
        assert_eq!(lsi.name, "email-index");
        assert_eq!(lsi.sort_key_attribute, "email");
        assert_eq!(lsi.projection, IndexProjection::All);
    }

    #[test]
    fn test_lsi_keys_only() {
        let lsi = LocalSecondaryIndex::new("email-index", "email").keys_only();
        assert_eq!(lsi.projection, IndexProjection::KeysOnly);
    }

    #[test]
    fn test_lsi_include() {
        let lsi = LocalSecondaryIndex::new("email-index", "email")
            .include(vec!["name".to_string(), "age".to_string()]);
        assert_eq!(
            lsi.projection,
            IndexProjection::Include(vec!["name".to_string(), "age".to_string()])
        );
    }

    #[test]
    fn test_table_schema() {
        let schema = TableSchema::new()
            .add_local_index(LocalSecondaryIndex::new("idx1", "attr1"))
            .add_local_index(LocalSecondaryIndex::new("idx2", "attr2"));

        assert_eq!(schema.local_indexes.len(), 2);
        assert!(schema.get_local_index("idx1").is_some());
        assert!(schema.get_local_index("idx3").is_none());
    }

    #[test]
    fn test_encode_decode_index_key() {
        let index_name = "email-index";
        let pk = Bytes::from("user#123");
        let index_sk = Bytes::from("alice@example.com");

        let encoded = encode_index_key(index_name, &pk, &index_sk);
        assert!(is_index_key(&encoded));

        let (decoded_name, decoded_pk, decoded_sk) = decode_index_key(&encoded).unwrap();
        assert_eq!(decoded_name, index_name);
        assert_eq!(decoded_pk, pk);
        assert_eq!(decoded_sk, index_sk);
    }

    #[test]
    fn test_is_not_index_key() {
        // Base table key encoding (from types.rs Key::encode)
        let base_key = vec![0, 0, 0, 4, b'u', b's', b'e', b'r'];
        assert!(!is_index_key(&base_key));
    }

    #[test]
    fn test_gsi_creation() {
        let gsi = GlobalSecondaryIndex::new("status-index", "status");
        assert_eq!(gsi.name, "status-index");
        assert_eq!(gsi.partition_key_attribute, "status");
        assert_eq!(gsi.sort_key_attribute, None);
        assert_eq!(gsi.projection, IndexProjection::All);
    }

    #[test]
    fn test_gsi_with_sort_key() {
        let gsi = GlobalSecondaryIndex::with_sort_key("user-index", "userId", "timestamp");
        assert_eq!(gsi.name, "user-index");
        assert_eq!(gsi.partition_key_attribute, "userId");
        assert_eq!(gsi.sort_key_attribute, Some("timestamp".to_string()));
    }

    #[test]
    fn test_gsi_keys_only() {
        let gsi = GlobalSecondaryIndex::new("status-index", "status").keys_only();
        assert_eq!(gsi.projection, IndexProjection::KeysOnly);
    }

    #[test]
    fn test_table_schema_with_gsi() {
        let schema = TableSchema::new()
            .add_local_index(LocalSecondaryIndex::new("lsi1", "attr1"))
            .add_global_index(GlobalSecondaryIndex::new("gsi1", "attr2"))
            .add_global_index(GlobalSecondaryIndex::with_sort_key("gsi2", "attr3", "attr4"));

        assert_eq!(schema.local_indexes.len(), 1);
        assert_eq!(schema.global_indexes.len(), 2);
        assert!(schema.get_global_index("gsi1").is_some());
        assert!(schema.get_global_index("gsi3").is_none());
    }

    #[test]
    fn test_ttl_schema() {
        let schema = TableSchema::new().with_ttl("expiresAt");

        assert_eq!(schema.ttl_attribute_name, Some("expiresAt".to_string()));
    }

    #[test]
    fn test_ttl_expired_item() {
        use crate::Value;
        use std::collections::HashMap;

        let schema = TableSchema::new().with_ttl("ttl");

        // Item expired 100 seconds ago
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let expired_time = now - 100;

        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("test"));
        item.insert("ttl".to_string(), Value::number(expired_time));

        assert!(schema.is_expired(&item));
    }

    #[test]
    fn test_ttl_not_expired_item() {
        use crate::Value;
        use std::collections::HashMap;

        let schema = TableSchema::new().with_ttl("ttl");

        // Item expires 100 seconds in the future
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let future_time = now + 100;

        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("test"));
        item.insert("ttl".to_string(), Value::number(future_time));

        assert!(!schema.is_expired(&item));
    }

    #[test]
    fn test_ttl_no_ttl_attribute() {
        use crate::Value;
        use std::collections::HashMap;

        let schema = TableSchema::new().with_ttl("ttl");

        // Item has no ttl attribute
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("test"));

        assert!(!schema.is_expired(&item));
    }

    #[test]
    fn test_ttl_disabled() {
        use crate::Value;
        use std::collections::HashMap;

        let schema = TableSchema::new(); // No TTL configured

        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("test"));
        item.insert("ttl".to_string(), Value::number(0)); // Ancient timestamp

        assert!(!schema.is_expired(&item));
    }

    #[test]
    fn test_ttl_with_timestamp_value() {
        use crate::Value;
        use std::collections::HashMap;

        let schema = TableSchema::new().with_ttl("expiresAt");

        // Item with Timestamp value type (milliseconds)
        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let expired_millis = now_millis - 100_000; // 100 seconds ago

        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("test"));
        item.insert("expiresAt".to_string(), Value::Ts(expired_millis));

        assert!(schema.is_expired(&item));
    }
}

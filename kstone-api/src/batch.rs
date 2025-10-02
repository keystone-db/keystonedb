/// Batch operations for DynamoDB-style BatchGetItem and BatchWriteItem
///
/// Provides APIs for getting or writing multiple items in a single operation.

use kstone_core::{Item, Key};
use bytes::Bytes;
use std::collections::HashMap;

/// Batch get request
#[derive(Debug, Clone)]
pub struct BatchGetRequest {
    /// Keys to retrieve
    pub keys: Vec<Key>,
}

impl BatchGetRequest {
    /// Create a new batch get request
    pub fn new() -> Self {
        Self { keys: Vec::new() }
    }

    /// Add a key with partition key only
    pub fn add_key(mut self, pk: &[u8]) -> Self {
        self.keys.push(Key::new(Bytes::copy_from_slice(pk)));
        self
    }

    /// Add a key with partition key and sort key
    pub fn add_key_with_sk(mut self, pk: &[u8], sk: &[u8]) -> Self {
        self.keys.push(Key::with_sk(
            Bytes::copy_from_slice(pk),
            Bytes::copy_from_slice(sk),
        ));
        self
    }

    /// Get the keys
    pub(crate) fn keys(&self) -> &[Key] {
        &self.keys
    }
}

impl Default for BatchGetRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch get response
#[derive(Debug, Clone)]
pub struct BatchGetResponse {
    /// Items retrieved (key -> item)
    pub items: HashMap<Key, Item>,
    /// Keys that were not found
    pub unprocessed_keys: Vec<Key>,
}

impl BatchGetResponse {
    pub(crate) fn new(items: HashMap<Key, Item>) -> Self {
        Self {
            items,
            unprocessed_keys: Vec::new(),
        }
    }
}

/// Batch write request item
#[derive(Debug, Clone)]
pub enum BatchWriteItem {
    /// Put an item
    Put { key: Key, item: Item },
    /// Delete an item
    Delete { key: Key },
}

/// Batch write request
#[derive(Debug, Clone)]
pub struct BatchWriteRequest {
    /// Write items
    pub items: Vec<BatchWriteItem>,
}

impl BatchWriteRequest {
    /// Create a new batch write request
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add a put request with partition key
    pub fn put(mut self, pk: &[u8], item: Item) -> Self {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.items.push(BatchWriteItem::Put { key, item });
        self
    }

    /// Add a put request with partition key and sort key
    pub fn put_with_sk(mut self, pk: &[u8], sk: &[u8], item: Item) -> Self {
        let key = Key::with_sk(
            Bytes::copy_from_slice(pk),
            Bytes::copy_from_slice(sk),
        );
        self.items.push(BatchWriteItem::Put { key, item });
        self
    }

    /// Add a delete request with partition key
    pub fn delete(mut self, pk: &[u8]) -> Self {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.items.push(BatchWriteItem::Delete { key });
        self
    }

    /// Add a delete request with partition key and sort key
    pub fn delete_with_sk(mut self, pk: &[u8], sk: &[u8]) -> Self {
        let key = Key::with_sk(
            Bytes::copy_from_slice(pk),
            Bytes::copy_from_slice(sk),
        );
        self.items.push(BatchWriteItem::Delete { key });
        self
    }

    /// Get the items
    pub(crate) fn items(&self) -> &[BatchWriteItem] {
        &self.items
    }
}

impl Default for BatchWriteRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch write response
#[derive(Debug, Clone)]
pub struct BatchWriteResponse {
    /// Number of items successfully written
    pub processed_count: usize,
    /// Items that failed to write
    pub unprocessed_items: Vec<BatchWriteItem>,
}

impl BatchWriteResponse {
    pub(crate) fn new(processed_count: usize) -> Self {
        Self {
            processed_count,
            unprocessed_items: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kstone_core::Value;

    #[test]
    fn test_batch_get_builder() {
        let request = BatchGetRequest::new()
            .add_key(b"user#1")
            .add_key_with_sk(b"user#2", b"profile");

        assert_eq!(request.keys().len(), 2);
    }

    #[test]
    fn test_batch_write_builder() {
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("Alice"));

        let request = BatchWriteRequest::new()
            .put(b"user#1", item.clone())
            .delete(b"user#2")
            .put_with_sk(b"user#3", b"profile", item);

        assert_eq!(request.items().len(), 3);
    }
}

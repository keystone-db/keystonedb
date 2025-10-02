use kstone_core::{Result, Key, Item, Value, lsm::LsmEngine};
use bytes::Bytes;
use std::path::Path;
use std::collections::HashMap;

pub use kstone_core::{Error as KeystoneError, Value as KeystoneValue};

pub mod query;
pub use query::{Query, QueryResponse};

pub mod scan;
pub use scan::{Scan, ScanResponse};

/// KeystoneDB Database handle
pub struct Database {
    engine: LsmEngine,
}

impl Database {
    /// Create a new database at the specified path
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let engine = LsmEngine::create(path)?;
        Ok(Self { engine })
    }

    /// Open an existing database
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let engine = LsmEngine::open(path)?;
        Ok(Self { engine })
    }

    /// Put an item with a simple partition key
    pub fn put(&self, pk: &[u8], item: Item) -> Result<()> {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.engine.put(key, item)
    }

    /// Put an item with partition key and sort key
    pub fn put_with_sk(
        &self,
        pk: &[u8],
        sk: &[u8],
        item: Item,
    ) -> Result<()> {
        let key = Key::with_sk(Bytes::copy_from_slice(pk), Bytes::copy_from_slice(sk));
        self.engine.put(key, item)
    }

    /// Get an item by partition key
    pub fn get(&self, pk: &[u8]) -> Result<Option<Item>> {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.engine.get(&key)
    }

    /// Get an item by partition key and sort key
    pub fn get_with_sk(
        &self,
        pk: &[u8],
        sk: &[u8],
    ) -> Result<Option<Item>> {
        let key = Key::with_sk(Bytes::copy_from_slice(pk), Bytes::copy_from_slice(sk));
        self.engine.get(&key)
    }

    /// Delete an item by partition key
    pub fn delete(&self, pk: &[u8]) -> Result<()> {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.engine.delete(key)
    }

    /// Delete an item by partition key and sort key
    pub fn delete_with_sk(
        &self,
        pk: &[u8],
        sk: &[u8],
    ) -> Result<()> {
        let key = Key::with_sk(Bytes::copy_from_slice(pk), Bytes::copy_from_slice(sk));
        self.engine.delete(key)
    }

    /// Flush any pending writes
    pub fn flush(&self) -> Result<()> {
        self.engine.flush()
    }

    /// Query items within a partition (Phase 2.1+)
    pub fn query(&self, query: Query) -> Result<QueryResponse> {
        let params = query.into_params();
        let result = self.engine.query(params)?;
        Ok(QueryResponse::from_result(result))
    }

    /// Scan all items in the table (Phase 2.2+)
    pub fn scan(&self, scan: Scan) -> Result<ScanResponse> {
        let params = scan.into_params();
        let result = self.engine.scan(params)?;
        Ok(ScanResponse::from_result(result))
    }
}

/// Helper to build items
pub struct ItemBuilder {
    item: HashMap<String, Value>,
}

impl ItemBuilder {
    pub fn new() -> Self {
        Self {
            item: HashMap::new(),
        }
    }

    pub fn string(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.item.insert(key.into(), Value::string(value.into()));
        self
    }

    pub fn number(mut self, key: impl Into<String>, value: impl ToString) -> Self {
        self.item.insert(key.into(), Value::number(value));
        self
    }

    pub fn bool(mut self, key: impl Into<String>, value: bool) -> Self {
        self.item.insert(key.into(), Value::Bool(value));
        self
    }

    pub fn build(self) -> Item {
        self.item
    }
}

impl Default for ItemBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_database_create_and_put_get() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new()
            .string("name", "Alice")
            .number("age", 30)
            .bool("active", true)
            .build();

        db.put(b"user#123", item.clone()).unwrap();

        let result = db.get(b"user#123").unwrap();
        assert_eq!(result, Some(item));
    }

    #[test]
    fn test_database_with_sort_key() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new()
            .string("content", "Hello world")
            .build();

        db.put_with_sk(b"user#123", b"post#456", item.clone())
            .unwrap();

        let result = db.get_with_sk(b"user#123", b"post#456").unwrap();
        assert_eq!(result, Some(item));
    }

    #[test]
    fn test_database_delete() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().string("test", "data").build();

        db.put(b"key1", item).unwrap();
        assert!(db.get(b"key1").unwrap().is_some());

        db.delete(b"key1").unwrap();
        assert!(db.get(b"key1").unwrap().is_none());
    }

    #[test]
    fn test_database_query_basic() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert items with different sort keys
        for i in 0..10 {
            let sk = format!("item#{:03}", i);
            let item = ItemBuilder::new()
                .string("name", format!("Item {}", i))
                .number("index", i)
                .build();
            db.put_with_sk(b"user#123", sk.as_bytes(), item).unwrap();
        }

        // Query all items
        let query = Query::new(b"user#123");
        let response = db.query(query).unwrap();

        assert_eq!(response.items.len(), 10);
        assert_eq!(response.scanned_count, 10);
    }

    #[test]
    fn test_database_query_with_limit() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert 20 items
        for i in 0..20 {
            let sk = format!("item#{:03}", i);
            let item = ItemBuilder::new().number("id", i).build();
            db.put_with_sk(b"user#456", sk.as_bytes(), item).unwrap();
        }

        // Query with limit
        let query = Query::new(b"user#456").limit(5);
        let response = db.query(query).unwrap();

        assert_eq!(response.items.len(), 5);
        assert!(response.last_key.is_some());
    }

    #[test]
    fn test_database_query_with_sk_condition() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert items
        for i in 0..10 {
            let sk = format!("post#{:03}", i);
            let item = ItemBuilder::new().number("id", i).build();
            db.put_with_sk(b"user#789", sk.as_bytes(), item).unwrap();
        }

        // Query with begins_with
        let query = Query::new(b"user#789").sk_begins_with(b"post#00");
        let response = db.query(query).unwrap();

        // Should match post#000 through post#009
        assert_eq!(response.items.len(), 10);
    }

    #[test]
    fn test_database_query_pagination() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert 15 items
        for i in 0..15 {
            let sk = format!("item#{:03}", i);
            let item = ItemBuilder::new().number("id", i).build();
            db.put_with_sk(b"user#999", sk.as_bytes(), item).unwrap();
        }

        // First page (5 items)
        let query1 = Query::new(b"user#999").limit(5);
        let response1 = db.query(query1).unwrap();
        assert_eq!(response1.items.len(), 5);
        assert!(response1.last_key.is_some());

        // Second page (using pagination)
        let (last_pk, last_sk) = response1.last_key.unwrap();
        let query2 = Query::new(b"user#999")
            .limit(5)
            .start_after(&last_pk, last_sk.as_deref());
        let response2 = db.query(query2).unwrap();
        assert_eq!(response2.items.len(), 5);

        // Third page
        let (last_pk2, last_sk2) = response2.last_key.unwrap();
        let query3 = Query::new(b"user#999")
            .limit(5)
            .start_after(&last_pk2, last_sk2.as_deref());
        let response3 = db.query(query3).unwrap();
        assert_eq!(response3.items.len(), 5);
    }

    #[test]
    fn test_database_scan_basic() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert items across multiple partitions
        for i in 0..20 {
            let pk = format!("user#{}", i);
            let item = ItemBuilder::new()
                .string("name", format!("User {}", i))
                .number("id", i)
                .build();
            db.put(pk.as_bytes(), item).unwrap();
        }

        // Scan all items
        let scan = Scan::new();
        let response = db.scan(scan).unwrap();

        assert_eq!(response.items.len(), 20);
        assert_eq!(response.scanned_count, 20);
    }

    #[test]
    fn test_database_scan_with_limit() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert 50 items
        for i in 0..50 {
            let pk = format!("item#{:03}", i);
            let item = ItemBuilder::new().number("value", i).build();
            db.put(pk.as_bytes(), item).unwrap();
        }

        // Scan with limit
        let scan = Scan::new().limit(10);
        let response = db.scan(scan).unwrap();

        assert_eq!(response.items.len(), 10);
        assert!(response.last_key.is_some());
    }

    #[test]
    fn test_database_scan_pagination() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert 30 items
        for i in 0..30 {
            let pk = format!("key#{:03}", i);
            let item = ItemBuilder::new().number("id", i).build();
            db.put(pk.as_bytes(), item).unwrap();
        }

        // First page
        let scan1 = Scan::new().limit(10);
        let response1 = db.scan(scan1).unwrap();
        assert_eq!(response1.items.len(), 10);
        assert!(response1.last_key.is_some());

        // Second page
        let (last_pk, last_sk) = response1.last_key.unwrap();
        let scan2 = Scan::new()
            .limit(10)
            .start_after(&last_pk, last_sk.as_deref());
        let response2 = db.scan(scan2).unwrap();
        assert_eq!(response2.items.len(), 10);

        // Third page
        let (last_pk2, last_sk2) = response2.last_key.unwrap();
        let scan3 = Scan::new()
            .limit(10)
            .start_after(&last_pk2, last_sk2.as_deref());
        let response3 = db.scan(scan3).unwrap();
        assert_eq!(response3.items.len(), 10);
    }

    #[test]
    fn test_database_scan_parallel() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert 100 items
        for i in 0..100 {
            let pk = format!("key{}", i);
            let item = ItemBuilder::new().number("value", i).build();
            db.put(pk.as_bytes(), item).unwrap();
        }

        // Parallel scan with 4 segments
        let mut total_items = 0;
        for segment in 0..4 {
            let scan = Scan::new().segment(segment, 4);
            let response = db.scan(scan).unwrap();
            total_items += response.items.len();
        }

        // All segments together should return all items
        assert_eq!(total_items, 100);
    }
}

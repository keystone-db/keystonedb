use kstone_core::{Result, Key, Item, Value, lsm::LsmEngine};
use bytes::Bytes;
use std::path::Path;
use std::collections::HashMap;

pub use kstone_core::{Error as KeystoneError, Value as KeystoneValue};

pub mod query;
pub use query::{Query, QueryResponse};

pub mod scan;
pub use scan::{Scan, ScanResponse};

pub mod update;
pub use update::{Update, UpdateResponse};

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

    /// Put an item with a condition expression (Phase 2.5+)
    pub fn put_conditional(
        &self,
        pk: &[u8],
        item: Item,
        condition: &str,
        context: kstone_core::expression::ExpressionContext,
    ) -> Result<()> {
        let key = Key::new(Bytes::copy_from_slice(pk));
        let expr = kstone_core::expression::ExpressionParser::parse(condition)?;
        self.engine.put_conditional(key, item, &expr, &context)
    }

    /// Put an item with partition key, sort key, and condition (Phase 2.5+)
    pub fn put_conditional_with_sk(
        &self,
        pk: &[u8],
        sk: &[u8],
        item: Item,
        condition: &str,
        context: kstone_core::expression::ExpressionContext,
    ) -> Result<()> {
        let key = Key::with_sk(Bytes::copy_from_slice(pk), Bytes::copy_from_slice(sk));
        let expr = kstone_core::expression::ExpressionParser::parse(condition)?;
        self.engine.put_conditional(key, item, &expr, &context)
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

    /// Delete an item with a condition expression (Phase 2.5+)
    pub fn delete_conditional(
        &self,
        pk: &[u8],
        condition: &str,
        context: kstone_core::expression::ExpressionContext,
    ) -> Result<()> {
        let key = Key::new(Bytes::copy_from_slice(pk));
        let expr = kstone_core::expression::ExpressionParser::parse(condition)?;
        self.engine.delete_conditional(key, &expr, &context)
    }

    /// Delete an item with partition key, sort key, and condition (Phase 2.5+)
    pub fn delete_conditional_with_sk(
        &self,
        pk: &[u8],
        sk: &[u8],
        condition: &str,
        context: kstone_core::expression::ExpressionContext,
    ) -> Result<()> {
        let key = Key::with_sk(Bytes::copy_from_slice(pk), Bytes::copy_from_slice(sk));
        let expr = kstone_core::expression::ExpressionParser::parse(condition)?;
        self.engine.delete_conditional(key, &expr, &context)
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

    /// Update an item using update expression (Phase 2.4+)
    pub fn update(&self, update: Update) -> Result<UpdateResponse> {
        let key = update.key().clone();
        let (actions, condition_expr, context) = update.into_actions()?;

        let updated_item = if let Some(condition_str) = condition_expr {
            // Parse condition and call conditional update
            let condition = kstone_core::expression::ExpressionParser::parse(&condition_str)?;
            self.engine.update_conditional(&key, &actions, &condition, &context)?
        } else {
            // No condition, regular update
            self.engine.update(&key, &actions, &context)?
        };

        Ok(UpdateResponse::new(updated_item))
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

    #[test]
    fn test_database_update_set() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial item
        let item = ItemBuilder::new()
            .string("name", "Alice")
            .number("age", 25)
            .build();
        db.put(b"user#123", item).unwrap();

        // Update with SET
        let update = Update::new(b"user#123")
            .expression("SET age = :new_age")
            .value(":new_age", Value::number(30));

        let response = db.update(update).unwrap();

        match response.item.get("age").unwrap() {
            Value::N(n) => assert_eq!(n, "30"),
            _ => panic!("Expected number"),
        }
        assert_eq!(response.item.get("name").unwrap().as_string(), Some("Alice"));
    }

    #[test]
    fn test_database_update_increment() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial item
        let item = ItemBuilder::new().number("score", 100).build();
        db.put(b"game#456", item).unwrap();

        // Increment score
        let update = Update::new(b"game#456")
            .expression("SET score = score + :inc")
            .value(":inc", Value::number(50));

        let response = db.update(update).unwrap();

        match response.item.get("score").unwrap() {
            Value::N(n) => assert_eq!(n, "150"),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_database_update_remove() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial item
        let item = ItemBuilder::new()
            .string("name", "Bob")
            .string("temp", "delete_me")
            .build();
        db.put(b"user#789", item).unwrap();

        // Remove temp attribute
        let update = Update::new(b"user#789")
            .expression("REMOVE temp");

        let response = db.update(update).unwrap();

        assert!(!response.item.contains_key("temp"));
        assert_eq!(response.item.get("name").unwrap().as_string(), Some("Bob"));
    }

    #[test]
    fn test_database_update_multiple_actions() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial item
        let item = ItemBuilder::new()
            .number("age", 25)
            .number("score", 100)
            .string("temp", "delete")
            .build();
        db.put(b"user#999", item).unwrap();

        // Multiple actions
        let update = Update::new(b"user#999")
            .expression("SET age = :new_age, active = :is_active REMOVE temp ADD score :bonus")
            .value(":new_age", Value::number(26))
            .value(":is_active", Value::Bool(true))
            .value(":bonus", Value::number(50));

        let response = db.update(update).unwrap();

        match response.item.get("age").unwrap() {
            Value::N(n) => assert_eq!(n, "26"),
            _ => panic!("Expected number"),
        }
        assert_eq!(response.item.get("active").unwrap(), &Value::Bool(true));
        assert!(!response.item.contains_key("temp"));
        match response.item.get("score").unwrap() {
            Value::N(n) => assert_eq!(n, "150"),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_database_put_if_not_exists() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // First put should succeed
        let item = ItemBuilder::new().string("name", "Alice").build();
        let context = kstone_core::expression::ExpressionContext::new();

        db.put_conditional(
            b"user#123",
            item.clone(),
            "attribute_not_exists(name)",
            context.clone(),
        )
        .unwrap();

        // Second put should fail (item exists)
        let result = db.put_conditional(
            b"user#123",
            item,
            "attribute_not_exists(name)",
            context,
        );

        assert!(matches!(result, Err(kstone_core::Error::ConditionalCheckFailed(_))));
    }

    #[test]
    fn test_database_update_with_condition() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial item
        let item = ItemBuilder::new()
            .string("name", "Bob")
            .number("age", 25)
            .build();
        db.put(b"user#456", item).unwrap();

        // Update with condition that passes
        let update = Update::new(b"user#456")
            .expression("SET age = :new_age")
            .condition("age = :old_age")
            .value(":new_age", Value::number(26))
            .value(":old_age", Value::number(25));

        let response = db.update(update).unwrap();

        match response.item.get("age").unwrap() {
            Value::N(n) => assert_eq!(n, "26"),
            _ => panic!("Expected number"),
        }

        // Update with condition that fails
        let update2 = Update::new(b"user#456")
            .expression("SET age = :new_age")
            .condition("age = :wrong_age")
            .value(":new_age", Value::number(30))
            .value(":wrong_age", Value::number(99));

        let result = db.update(update2);
        assert!(matches!(result, Err(kstone_core::Error::ConditionalCheckFailed(_))));
    }

    #[test]
    fn test_database_delete_with_condition() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert item
        let item = ItemBuilder::new()
            .string("status", "inactive")
            .build();
        db.put(b"user#789", item).unwrap();

        // Try to delete with failing condition
        let context = kstone_core::expression::ExpressionContext::new()
            .with_value(":status", Value::string("active"));

        let result = db.delete_conditional(
            b"user#789",
            "status = :status",
            context,
        );

        assert!(matches!(result, Err(kstone_core::Error::ConditionalCheckFailed(_))));

        // Delete with passing condition
        let context2 = kstone_core::expression::ExpressionContext::new()
            .with_value(":status", Value::string("inactive"));

        db.delete_conditional(
            b"user#789",
            "status = :status",
            context2,
        )
        .unwrap();

        // Verify deleted
        let result = db.get(b"user#789").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_database_conditional_attribute_exists() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert item with email
        let item = ItemBuilder::new()
            .string("name", "Charlie")
            .string("email", "charlie@example.com")
            .build();
        db.put(b"user#999", item).unwrap();

        // Update only if email exists
        let update = Update::new(b"user#999")
            .expression("SET verified = :val")
            .condition("attribute_exists(email)")
            .value(":val", Value::Bool(true));

        let response = db.update(update).unwrap();
        assert_eq!(response.item.get("verified").unwrap(), &Value::Bool(true));

        // Try to update non-existent item (should fail)
        let update2 = Update::new(b"user#000")
            .expression("SET verified = :val")
            .condition("attribute_exists(email)")
            .value(":val", Value::Bool(true));

        let result = db.update(update2);
        assert!(matches!(result, Err(kstone_core::Error::ConditionalCheckFailed(_))));
    }
}

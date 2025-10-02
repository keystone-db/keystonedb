use kstone_core::{Result, Key, Item, Value, lsm::LsmEngine};
use bytes::Bytes;
use std::path::Path;
use std::collections::HashMap;

pub use kstone_core::{
    Error as KeystoneError,
    Value as KeystoneValue,
    index::{LocalSecondaryIndex, GlobalSecondaryIndex, IndexProjection, TableSchema},
    stream::{StreamRecord, StreamEventType, StreamViewType, StreamConfig},
};

pub mod query;
pub use query::{Query, QueryResponse};

pub mod scan;
pub use scan::{Scan, ScanResponse};

pub mod update;
pub use update::{Update, UpdateResponse};

pub mod batch;
pub use batch::{BatchGetRequest, BatchGetResponse, BatchWriteRequest, BatchWriteResponse, BatchWriteItem};

pub mod transaction;
pub use transaction::{TransactGetRequest, TransactGetResponse, TransactWriteRequest, TransactWriteResponse, TransactWriteOp};

pub mod partiql;
pub use partiql::{ExecuteStatementRequest, ExecuteStatementResponse};

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

    /// Create a new database with a table schema (Phase 3.1+)
    pub fn create_with_schema(path: impl AsRef<Path>, schema: TableSchema) -> Result<Self> {
        let engine = LsmEngine::create_with_schema(path, schema)?;
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

    /// Batch get multiple items (Phase 2.6+)
    pub fn batch_get(&self, request: BatchGetRequest) -> Result<BatchGetResponse> {
        let results = self.engine.batch_get(request.keys())?;

        let mut items = std::collections::HashMap::new();
        for (key, item_opt) in results {
            if let Some(item) = item_opt {
                items.insert(key, item);
            }
        }

        Ok(BatchGetResponse::new(items))
    }

    /// Batch write multiple items (Phase 2.6+)
    pub fn batch_write(&self, request: BatchWriteRequest) -> Result<BatchWriteResponse> {
        // Convert batch write request to operations
        let mut operations = Vec::new();

        for item in request.items() {
            match item {
                BatchWriteItem::Put { key, item } => {
                    operations.push((key.clone(), Some(item.clone())));
                }
                BatchWriteItem::Delete { key } => {
                    operations.push((key.clone(), None));
                }
            }
        }

        let processed = self.engine.batch_write(&operations)?;
        Ok(BatchWriteResponse::new(processed))
    }

    /// Transactional get - read multiple items atomically (Phase 2.7+)
    pub fn transact_get(&self, request: TransactGetRequest) -> Result<TransactGetResponse> {
        let items = self.engine.transact_get(request.keys())?;
        Ok(TransactGetResponse::new(items))
    }

    /// Transactional write - write multiple items atomically with conditions (Phase 2.7+)
    pub fn transact_write(&self, request: TransactWriteRequest) -> Result<TransactWriteResponse> {
        use kstone_core::{TransactWriteOperation, expression::ExpressionParser};

        // Convert API operations to core operations
        let mut operations = Vec::new();

        for op in request.operations() {
            match op {
                TransactWriteOp::Put { key, item, condition } => {
                    let condition_expr = if let Some(cond_str) = condition {
                        Some(ExpressionParser::parse(cond_str)?)
                    } else {
                        None
                    };
                    operations.push((
                        key.clone(),
                        TransactWriteOperation::Put {
                            item: item.clone(),
                            condition: condition_expr,
                        },
                    ));
                }
                TransactWriteOp::Update { key, update_expression, condition } => {
                    let actions = kstone_core::expression::UpdateExpressionParser::parse(update_expression)?;
                    let condition_expr = if let Some(cond_str) = condition {
                        Some(ExpressionParser::parse(cond_str)?)
                    } else {
                        None
                    };
                    operations.push((
                        key.clone(),
                        TransactWriteOperation::Update {
                            actions,
                            condition: condition_expr,
                        },
                    ));
                }
                TransactWriteOp::Delete { key, condition } => {
                    let condition_expr = if let Some(cond_str) = condition {
                        Some(ExpressionParser::parse(cond_str)?)
                    } else {
                        None
                    };
                    operations.push((
                        key.clone(),
                        TransactWriteOperation::Delete {
                            condition: condition_expr,
                        },
                    ));
                }
                TransactWriteOp::ConditionCheck { key, condition } => {
                    let condition_expr = ExpressionParser::parse(condition)?;
                    operations.push((
                        key.clone(),
                        TransactWriteOperation::ConditionCheck {
                            condition: condition_expr,
                        },
                    ));
                }
            }
        }

        let committed = self.engine.transact_write(&operations, request.context())?;
        Ok(TransactWriteResponse::new(committed))
    }

    /// Read stream records (Phase 3.4+)
    ///
    /// Returns all stream records in the buffer.
    /// Optionally provide after_sequence_number to only get records after that sequence number.
    pub fn read_stream(&self, after_sequence_number: Option<u64>) -> Result<Vec<StreamRecord>> {
        self.engine.read_stream(after_sequence_number)
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

    #[test]
    fn test_database_batch_get() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert some items
        db.put(b"user#1", ItemBuilder::new().string("name", "Alice").build()).unwrap();
        db.put(b"user#2", ItemBuilder::new().string("name", "Bob").build()).unwrap();
        db.put(b"user#3", ItemBuilder::new().string("name", "Charlie").build()).unwrap();

        // Batch get
        let request = BatchGetRequest::new()
            .add_key(b"user#1")
            .add_key(b"user#2")
            .add_key(b"user#4"); // Doesn't exist

        let response = db.batch_get(request).unwrap();

        assert_eq!(response.items.len(), 2); // Only 2 found
        assert!(response.items.contains_key(&Key::new(b"user#1".to_vec())));
        assert!(response.items.contains_key(&Key::new(b"user#2".to_vec())));
        assert!(!response.items.contains_key(&Key::new(b"user#4".to_vec())));
    }

    #[test]
    fn test_database_batch_write() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial item
        db.put(b"user#1", ItemBuilder::new().string("name", "Alice").build()).unwrap();

        // Batch write: put new items and delete one
        let request = BatchWriteRequest::new()
            .put(b"user#2", ItemBuilder::new().string("name", "Bob").build())
            .put(b"user#3", ItemBuilder::new().string("name", "Charlie").build())
            .delete(b"user#1");

        let response = db.batch_write(request).unwrap();
        assert_eq!(response.processed_count, 3);

        // Verify results
        assert!(db.get(b"user#1").unwrap().is_none()); // Deleted
        assert!(db.get(b"user#2").unwrap().is_some());
        assert!(db.get(b"user#3").unwrap().is_some());
    }

    #[test]
    fn test_database_batch_write_mixed() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Batch write with puts and deletes
        let request = BatchWriteRequest::new()
            .put(b"key#1", ItemBuilder::new().number("value", 1).build())
            .put(b"key#2", ItemBuilder::new().number("value", 2).build())
            .put_with_sk(b"user#1", b"profile", ItemBuilder::new().string("bio", "test").build())
            .delete(b"key#1"); // Delete what we just put

        let response = db.batch_write(request).unwrap();
        assert_eq!(response.processed_count, 4);

        // Verify
        assert!(db.get(b"key#1").unwrap().is_none());
        assert!(db.get(b"key#2").unwrap().is_some());
        assert!(db.get_with_sk(b"user#1", b"profile").unwrap().is_some());
    }

    #[test]
    fn test_database_transact_get_basic() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert items
        db.put(b"user#1", ItemBuilder::new().string("name", "Alice").build()).unwrap();
        db.put(b"user#2", ItemBuilder::new().string("name", "Bob").build()).unwrap();

        // Transact get
        let request = TransactGetRequest::new()
            .get(b"user#1")
            .get(b"user#2");

        let response = db.transact_get(request).unwrap();
        assert_eq!(response.items.len(), 2);
        assert!(response.items[0].is_some());
        assert!(response.items[1].is_some());
    }

    #[test]
    fn test_database_transact_get_missing_items() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert only one item
        db.put(b"user#1", ItemBuilder::new().string("name", "Alice").build()).unwrap();

        // Get existing and non-existing
        let request = TransactGetRequest::new()
            .get(b"user#1")
            .get(b"user#2"); // Doesn't exist

        let response = db.transact_get(request).unwrap();
        assert_eq!(response.items.len(), 2);
        assert!(response.items[0].is_some());
        assert!(response.items[1].is_none());
    }

    #[test]
    fn test_database_transact_write_puts() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Transaction with multiple puts
        let request = TransactWriteRequest::new()
            .put(b"user#1", ItemBuilder::new().string("name", "Alice").build())
            .put(b"user#2", ItemBuilder::new().string("name", "Bob").build());

        let response = db.transact_write(request).unwrap();
        assert_eq!(response.committed_count, 2);

        // Verify both items exist
        assert!(db.get(b"user#1").unwrap().is_some());
        assert!(db.get(b"user#2").unwrap().is_some());
    }

    #[test]
    fn test_database_transact_write_with_condition_success() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Put initial item
        db.put(b"account#1", ItemBuilder::new().number("balance", 100).build()).unwrap();

        // Transaction: put if not exists (should fail), update if balance >= amount (should succeed)
        let request = TransactWriteRequest::new()
            .put(b"account#2", ItemBuilder::new().number("balance", 200).build())
            .update_with_condition(
                b"account#1",
                "SET balance = balance - :amount",
                "balance >= :amount"
            )
            .value(":amount", kstone_core::Value::number(50));

        let response = db.transact_write(request).unwrap();
        assert_eq!(response.committed_count, 2);

        // Verify account#1 balance decreased
        let item = db.get(b"account#1").unwrap().unwrap();
        match item.get("balance").unwrap() {
            kstone_core::Value::N(n) => assert_eq!(n, "50"),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_database_transact_write_condition_failure() {
        use kstone_core::Error;

        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Put item with low balance
        db.put(b"account#1", ItemBuilder::new().number("balance", 10).build()).unwrap();

        // Try to withdraw more than balance (should fail)
        let request = TransactWriteRequest::new()
            .update_with_condition(
                b"account#1",
                "SET balance = balance - :amount",
                "balance >= :amount"
            )
            .value(":amount", kstone_core::Value::number(100));

        let result = db.transact_write(request);
        assert!(matches!(result, Err(Error::TransactionCanceled(_))));

        // Verify balance unchanged
        let item = db.get(b"account#1").unwrap().unwrap();
        match item.get("balance").unwrap() {
            kstone_core::Value::N(n) => assert_eq!(n, "10"),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_database_transact_write_mixed_operations() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial data
        db.put(b"user#1", ItemBuilder::new().string("status", "active").build()).unwrap();
        db.put(b"user#2", ItemBuilder::new().string("status", "inactive").build()).unwrap();

        // Transaction with put, update, delete, and condition check
        let request = TransactWriteRequest::new()
            .put(b"user#3", ItemBuilder::new().string("name", "Charlie").build())
            .update(b"user#1", "SET status = :status")
            .delete(b"user#2")
            .condition_check(b"user#1", "attribute_exists(status)")
            .value(":status", kstone_core::Value::string("premium"));

        let response = db.transact_write(request).unwrap();
        assert_eq!(response.committed_count, 4);

        // Verify all operations
        assert!(db.get(b"user#3").unwrap().is_some()); // New item
        let user1 = db.get(b"user#1").unwrap().unwrap();
        assert_eq!(user1.get("status").unwrap().as_string().unwrap(), "premium"); // Updated
        assert!(db.get(b"user#2").unwrap().is_none()); // Deleted
    }

    #[test]
    fn test_database_transact_write_condition_check_only() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Put item
        db.put(b"user#1", ItemBuilder::new().string("email", "alice@example.com").build()).unwrap();

        // Transaction with only condition check (no actual write)
        let request = TransactWriteRequest::new()
            .condition_check(b"user#1", "attribute_exists(email)");

        let response = db.transact_write(request).unwrap();
        assert_eq!(response.committed_count, 1);

        // Item should be unchanged
        let item = db.get(b"user#1").unwrap().unwrap();
        assert_eq!(item.get("email").unwrap().as_string().unwrap(), "alice@example.com");
    }

    #[test]
    fn test_database_transact_write_atomicity() {
        use kstone_core::Error;

        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Put initial items
        db.put(b"item#1", ItemBuilder::new().number("value", 1).build()).unwrap();

        // Transaction where second operation fails
        let request = TransactWriteRequest::new()
            .put(b"item#2", ItemBuilder::new().number("value", 2).build())
            .put_with_condition(
                b"item#3",
                ItemBuilder::new().number("value", 3).build(),
                "attribute_exists(nonexistent)" // This will fail
            );

        let result = db.transact_write(request);
        assert!(matches!(result, Err(Error::TransactionCanceled(_))));

        // Verify nothing was committed (atomicity)
        assert!(db.get(b"item#2").unwrap().is_none()); // First put should be rolled back
        assert!(db.get(b"item#3").unwrap().is_none());
    }

    #[test]
    fn test_database_create_with_lsi() {
        let dir = TempDir::new().unwrap();

        // Create schema with LSI on email attribute
        let schema = TableSchema::new()
            .add_local_index(LocalSecondaryIndex::new("email-index", "email"));

        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put an item with email attribute
        let item = ItemBuilder::new()
            .string("name", "Alice")
            .string("email", "alice@example.com")
            .number("age", 30)
            .build();

        db.put(b"user#123", item).unwrap();

        // Verify the base record was stored
        let retrieved = db.get(b"user#123").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().get("email").unwrap().as_string().unwrap(), "alice@example.com");
    }

    #[test]
    fn test_database_query_by_lsi() {
        let dir = TempDir::new().unwrap();

        // Create schema with LSI on email attribute
        let schema = TableSchema::new()
            .add_local_index(LocalSecondaryIndex::new("email-index", "email"));

        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put multiple items for the same partition key
        for i in 0..5 {
            let email = format!("user{}@example.com", i);
            let item = ItemBuilder::new()
                .string("name", format!("User {}", i))
                .string("email", &email)
                .number("age", 20 + i)
                .build();

            db.put(b"org#acme", item).unwrap();
        }

        // Query by email using the LSI
        let query = Query::new(b"org#acme")
            .index("email-index")
            .sk_begins_with(b"user");

        let response = db.query(query).unwrap();

        // Should find all 5 items
        assert_eq!(response.items.len(), 5);

        // Verify items are sorted by email
        for (i, item) in response.items.iter().enumerate() {
            let expected_email = format!("user{}@example.com", i);
            assert_eq!(
                item.get("email").unwrap().as_string().unwrap(),
                expected_email
            );
        }
    }

    #[test]
    fn test_database_query_lsi_with_condition() {
        let dir = TempDir::new().unwrap();

        // Create schema with LSI on score attribute
        let schema = TableSchema::new()
            .add_local_index(LocalSecondaryIndex::new("score-index", "score"));

        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put items with different scores
        let scores = vec![100, 250, 500, 750, 900];
        for (i, score) in scores.iter().enumerate() {
            let item = ItemBuilder::new()
                .string("player", format!("Player {}", i))
                .number("score", *score)
                .build();

            db.put(b"game#123", item).unwrap();
        }

        // Query for scores >= 500 using LSI
        let query = Query::new(b"game#123")
            .index("score-index")
            .sk_gte(b"500");

        let response = db.query(query).unwrap();

        // Should find 3 items (500, 750, 900)
        assert_eq!(response.items.len(), 3);
    }

    #[test]
    fn test_database_create_with_gsi() {
        let dir = TempDir::new().unwrap();

        // Create schema with GSI on status attribute
        let schema = TableSchema::new()
            .add_global_index(GlobalSecondaryIndex::new("status-index", "status"));

        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put items with different partition keys but same status
        db.put(b"user#1", ItemBuilder::new()
            .string("name", "Alice")
            .string("status", "active")
            .build()).unwrap();

        db.put(b"user#2", ItemBuilder::new()
            .string("name", "Bob")
            .string("status", "active")
            .build()).unwrap();

        db.put(b"user#3", ItemBuilder::new()
            .string("name", "Charlie")
            .string("status", "inactive")
            .build()).unwrap();

        // Verify base records
        assert!(db.get(b"user#1").unwrap().is_some());
        assert!(db.get(b"user#2").unwrap().is_some());
        assert!(db.get(b"user#3").unwrap().is_some());
    }

    #[test]
    fn test_database_query_by_gsi() {
        let dir = TempDir::new().unwrap();

        // Create schema with GSI on status attribute (PK) and timestamp (SK)
        let schema = TableSchema::new()
            .add_global_index(GlobalSecondaryIndex::with_sort_key(
                "status-time-index",
                "status",
                "timestamp"
            ));

        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put items with different base PKs but same status
        for i in 1..=5 {
            db.put(format!("user#{}", i).as_bytes(), ItemBuilder::new()
                .string("name", format!("User {}", i))
                .string("status", "active")
                .number("timestamp", 1000 + i)
                .build()).unwrap();
        }

        // Put items with different status
        for i in 6..=8 {
            db.put(format!("user#{}", i).as_bytes(), ItemBuilder::new()
                .string("name", format!("User {}", i))
                .string("status", "inactive")
                .number("timestamp", 1000 + i)
                .build()).unwrap();
        }

        // Query by status="active" using GSI
        let query = Query::new(b"active")
            .index("status-time-index");

        let response = db.query(query).unwrap();

        // Should find all 5 active users
        assert_eq!(response.items.len(), 5);

        // Verify all have status=active
        for item in &response.items {
            assert_eq!(item.get("status").unwrap().as_string().unwrap(), "active");
        }
    }

    #[test]
    fn test_database_query_gsi_with_sort_key_condition() {
        let dir = TempDir::new().unwrap();

        // Create schema with GSI
        let schema = TableSchema::new()
            .add_global_index(GlobalSecondaryIndex::with_sort_key(
                "status-time-index",
                "status",
                "timestamp"
            ));

        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put items
        for i in 1..=10 {
            db.put(format!("order#{}", i).as_bytes(), ItemBuilder::new()
                .string("status", "shipped")
                .number("timestamp", 1000 + i * 100)
                .number("amount", i * 10)
                .build()).unwrap();
        }

        // Query for status="shipped" AND timestamp >= 1500
        let query = Query::new(b"shipped")
            .index("status-time-index")
            .sk_gte(b"1500");

        let response = db.query(query).unwrap();

        // Should find items with timestamp >= 1500 (items 5-10 = 6 items)
        assert_eq!(response.items.len(), 6);
    }

    #[test]
    fn test_database_gsi_different_stripes() {
        let dir = TempDir::new().unwrap();

        // Create schema with GSI on category
        let schema = TableSchema::new()
            .add_global_index(GlobalSecondaryIndex::new("category-index", "category"));

        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put items with different base PKs that will route to different stripes
        // but same GSI partition key (category)
        db.put(b"product#electronics#laptop", ItemBuilder::new()
            .string("name", "Laptop")
            .string("category", "electronics")
            .build()).unwrap();

        db.put(b"product#books#novel", ItemBuilder::new()
            .string("name", "Novel")
            .string("category", "books")
            .build()).unwrap();

        db.put(b"product#electronics#phone", ItemBuilder::new()
            .string("name", "Phone")
            .string("category", "electronics")
            .build()).unwrap();

        // Query GSI by category="electronics"
        let query = Query::new(b"electronics")
            .index("category-index");

        let response = db.query(query).unwrap();

        // Should find both electronics items regardless of base table stripe
        assert_eq!(response.items.len(), 2);

        // Verify both have category=electronics
        for item in &response.items {
            assert_eq!(item.get("category").unwrap().as_string().unwrap(), "electronics");
        }
    }

    #[test]
    fn test_database_ttl_lazy_deletion() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        // Create schema with TTL enabled on "expiresAt" attribute
        let schema = TableSchema::new().with_ttl("expiresAt");
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Get current time
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Put item that expired 100 seconds ago
        let expired_item = ItemBuilder::new()
            .string("name", "Expired Item")
            .number("expiresAt", now - 100)
            .build();
        db.put(b"item#1", expired_item).unwrap();

        // Put item that expires in the future
        let valid_item = ItemBuilder::new()
            .string("name", "Valid Item")
            .number("expiresAt", now + 1000)
            .build();
        db.put(b"item#2", valid_item).unwrap();

        // Get expired item - should return None (lazy deletion)
        let result = db.get(b"item#1").unwrap();
        assert!(result.is_none());

        // Get valid item - should return the item
        let result = db.get(b"item#2").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().get("name").unwrap().as_string().unwrap(), "Valid Item");
    }

    #[test]
    fn test_database_ttl_query_filter() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        // Create schema with TTL
        let schema = TableSchema::new().with_ttl("ttl");
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Put multiple items with same PK, different SK, different expiration times
        for i in 1..=5 {
            let ttl_value = if i <= 2 {
                now - 100 // Expired
            } else {
                now + 1000 // Valid
            };

            db.put_with_sk(
                b"user#123",
                format!("item#{}", i).as_bytes(),
                ItemBuilder::new()
                    .string("name", format!("User {}", i))
                    .number("ttl", ttl_value)
                    .build()
            ).unwrap();
        }

        // Query - should only return non-expired items
        let query = Query::new(b"user#123");
        let response = db.query(query).unwrap();

        assert_eq!(response.items.len(), 3); // Only items 3, 4, 5
    }

    #[test]
    fn test_database_ttl_scan_filter() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        // Create schema with TTL
        let schema = TableSchema::new().with_ttl("expiresAt");
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Put items with mixed expiration
        db.put(b"item#1", ItemBuilder::new()
            .string("name", "Item 1")
            .number("expiresAt", now - 100) // Expired
            .build()).unwrap();

        db.put(b"item#2", ItemBuilder::new()
            .string("name", "Item 2")
            .number("expiresAt", now + 1000) // Valid
            .build()).unwrap();

        db.put(b"item#3", ItemBuilder::new()
            .string("name", "Item 3")
            .number("expiresAt", now + 1000) // Valid
            .build()).unwrap();

        // Scan - should only return non-expired items
        let scan = Scan::new();
        let response = db.scan(scan).unwrap();

        assert_eq!(response.items.len(), 2); // Only items 2 and 3
    }

    #[test]
    fn test_database_ttl_no_ttl_attribute() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        // Create schema with TTL enabled
        let schema = TableSchema::new().with_ttl("expiresAt");
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put item without TTL attribute
        db.put(b"item#1", ItemBuilder::new()
            .string("name", "No TTL Item")
            .build()).unwrap();

        // Should still be retrievable (no expiration)
        let result = db.get(b"item#1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().get("name").unwrap().as_string().unwrap(), "No TTL Item");
    }

    #[test]
    fn test_database_ttl_timestamp_value_type() {
        use tempfile::TempDir;
        use kstone_core::Value;

        let dir = TempDir::new().unwrap();

        // Create schema with TTL
        let schema = TableSchema::new().with_ttl("expiresAt");
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Put item with Timestamp value type (milliseconds)
        let mut item = ItemBuilder::new()
            .string("name", "Timestamp Item")
            .build();
        item.insert("expiresAt".to_string(), Value::Ts(now_millis - 100_000)); // 100 seconds ago

        db.put(b"item#1", item).unwrap();

        // Should be expired and return None
        let result = db.get(b"item#1").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_database_stream_insert() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        // Create schema with streams enabled
        let schema = TableSchema::new()
            .with_stream(StreamConfig::enabled());
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put an item
        db.put(b"user#1", ItemBuilder::new()
            .string("name", "Alice")
            .build()).unwrap();

        // Read stream
        let records = db.read_stream(None).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].event_type, StreamEventType::Insert);
        assert!(records[0].new_image.is_some());
        assert!(records[0].old_image.is_none());
    }

    #[test]
    fn test_database_stream_modify() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        let schema = TableSchema::new()
            .with_stream(StreamConfig::enabled());
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put initial item
        db.put(b"user#1", ItemBuilder::new()
            .string("name", "Alice")
            .build()).unwrap();

        // Update the item
        db.put(b"user#1", ItemBuilder::new()
            .string("name", "Bob")
            .build()).unwrap();

        // Read stream
        let records = db.read_stream(None).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].event_type, StreamEventType::Insert);
        assert_eq!(records[1].event_type, StreamEventType::Modify);
        assert!(records[1].old_image.is_some());
        assert!(records[1].new_image.is_some());
    }

    #[test]
    fn test_database_stream_remove() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        let schema = TableSchema::new()
            .with_stream(StreamConfig::enabled());
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put an item
        db.put(b"user#1", ItemBuilder::new()
            .string("name", "Alice")
            .build()).unwrap();

        // Delete the item
        db.delete(b"user#1").unwrap();

        // Read stream
        let records = db.read_stream(None).unwrap();

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].event_type, StreamEventType::Insert);
        assert_eq!(records[1].event_type, StreamEventType::Remove);
        assert!(records[1].old_image.is_some());
        assert!(records[1].new_image.is_none());
    }

    #[test]
    fn test_database_stream_view_type_keys_only() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        let schema = TableSchema::new()
            .with_stream(StreamConfig::enabled().with_view_type(StreamViewType::KeysOnly));
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put an item
        db.put(b"user#1", ItemBuilder::new()
            .string("name", "Alice")
            .build()).unwrap();

        // Read stream
        let records = db.read_stream(None).unwrap();

        assert_eq!(records.len(), 1);
        // Keys only - no images
        assert!(records[0].old_image.is_none());
        assert!(records[0].new_image.is_none());
    }

    #[test]
    fn test_database_stream_after_sequence() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        let schema = TableSchema::new()
            .with_stream(StreamConfig::enabled());
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put multiple items
        db.put(b"item#1", ItemBuilder::new().string("data", "1").build()).unwrap();
        db.put(b"item#2", ItemBuilder::new().string("data", "2").build()).unwrap();
        db.put(b"item#3", ItemBuilder::new().string("data", "3").build()).unwrap();

        // Get all records
        let all_records = db.read_stream(None).unwrap();
        assert_eq!(all_records.len(), 3);

        // Get only records after first sequence number
        let first_seq = all_records[0].sequence_number;
        let filtered_records = db.read_stream(Some(first_seq)).unwrap();
        assert_eq!(filtered_records.len(), 2);
    }

    #[test]
    fn test_database_stream_buffer_limit() {
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();

        // Create schema with small buffer
        let schema = TableSchema::new()
            .with_stream(StreamConfig::enabled().with_buffer_size(5));
        let db = Database::create_with_schema(dir.path(), schema).unwrap();

        // Put 10 items (exceeds buffer size)
        for i in 1..=10 {
            db.put(format!("item#{}", i).as_bytes(), ItemBuilder::new()
                .string("data", format!("{}", i))
                .build()).unwrap();
        }

        // Stream should only contain last 5 records
        let records = db.read_stream(None).unwrap();
        assert_eq!(records.len(), 5);

        // Verify it's the most recent records
        assert_eq!(records[0].sequence_number, 6);
        assert_eq!(records[4].sequence_number, 10);
    }
}



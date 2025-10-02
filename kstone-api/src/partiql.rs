/// PartiQL ExecuteStatement API for KeystoneDB
///
/// Provides a high-level API for executing PartiQL (SQL-compatible) queries against KeystoneDB.
/// Supports SELECT, INSERT, UPDATE, and DELETE operations.

use crate::{Database, Item, Query, Scan, Update};
use bytes::Bytes;
use kstone_core::{
    partiql::{
        PartiQLParser, PartiQLStatement, PartiQLTranslator, SelectTranslation,
        SortKeyConditionType,
    },
    Result,
};

/// Request to execute a PartiQL statement
#[allow(dead_code)]
pub struct ExecuteStatementRequest {
    sql: String,
}

impl ExecuteStatementRequest {
    pub fn new(sql: impl Into<String>) -> Self {
        Self { sql: sql.into() }
    }
}

/// Response from executing a PartiQL statement
#[derive(Debug)]
pub enum ExecuteStatementResponse {
    /// SELECT statement result
    Select {
        items: Vec<Item>,
        count: usize,
        scanned_count: usize,
        last_key: Option<(Bytes, Option<Bytes>)>,
    },
    /// INSERT statement result
    Insert { success: bool },
    /// UPDATE statement result
    Update { item: Item },
    /// DELETE statement result
    Delete { success: bool },
}

impl Database {
    /// Execute a PartiQL statement
    ///
    /// Parses, validates, and executes a PartiQL SQL statement against the database.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use kstone_api::Database;
    /// use tempfile::TempDir;
    ///
    /// let dir = TempDir::new().unwrap();
    /// let db = Database::create(dir.path()).unwrap();
    ///
    /// // INSERT
    /// let sql = "INSERT INTO users VALUE {'pk': 'user#123', 'name': 'Alice', 'age': 30}";
    /// db.execute_statement(sql).unwrap();
    ///
    /// // SELECT
    /// let sql = "SELECT * FROM users WHERE pk = 'user#123'";
    /// let response = db.execute_statement(sql).unwrap();
    ///
    /// // UPDATE
    /// let sql = "UPDATE users SET age = age + 1 WHERE pk = 'user#123'";
    /// db.execute_statement(sql).unwrap();
    ///
    /// // DELETE
    /// let sql = "DELETE FROM users WHERE pk = 'user#123'";
    /// db.execute_statement(sql).unwrap();
    /// ```
    pub fn execute_statement(&self, sql: &str) -> Result<ExecuteStatementResponse> {
        // Parse the SQL statement
        let statement = PartiQLParser::parse(sql)?;

        // Execute based on statement type
        match statement {
            PartiQLStatement::Select(select_stmt) => {
                // Translate SELECT to Query or Scan
                let translation = PartiQLTranslator::translate_select(&select_stmt)?;

                match translation {
                    SelectTranslation::Query {
                        pk,
                        sk_condition,
                        index_name,
                        forward,
                    } => {
                        // Execute Query operation
                        let mut query = Query::new(&pk);

                        if let Some(index) = index_name {
                            query = query.index(&index);
                        }

                        // Add sort key condition if present
                        if let Some(sk_cond) = sk_condition {
                            query = match sk_cond {
                                SortKeyConditionType::Equal(sk) => query.sk_eq(&sk),
                                SortKeyConditionType::LessThan(sk) => query.sk_lt(&sk),
                                SortKeyConditionType::LessThanOrEqual(sk) => {
                                    query.sk_lte(&sk)
                                }
                                SortKeyConditionType::GreaterThan(sk) => {
                                    query.sk_gt(&sk)
                                }
                                SortKeyConditionType::GreaterThanOrEqual(sk) => {
                                    query.sk_gte(&sk)
                                }
                                SortKeyConditionType::Between(low, high) => {
                                    query.sk_between(&low, &high)
                                }
                            };
                        }

                        // Set scan direction
                        query = query.forward(forward);

                        // Apply LIMIT - if both LIMIT and OFFSET, fetch enough records
                        if let Some(limit) = select_stmt.limit {
                            let fetch_limit = if let Some(offset) = select_stmt.offset {
                                limit + offset
                            } else {
                                limit
                            };
                            query = query.limit(fetch_limit);
                        }

                        // Execute query
                        let mut response = self.query(query)?;

                        // Apply OFFSET if specified (by skipping items)
                        if let Some(offset) = select_stmt.offset {
                            if offset < response.items.len() {
                                response.items = response.items.into_iter().skip(offset).collect();
                                response.count = response.items.len();
                            } else {
                                // Offset is beyond results, return empty
                                response.items.clear();
                                response.count = 0;
                            }
                        }

                        // Apply LIMIT if specified (truncate after offset)
                        if let Some(limit) = select_stmt.limit {
                            if response.items.len() > limit {
                                response.items.truncate(limit);
                                response.count = limit;
                            }
                        }

                        // Apply projection
                        response.items = apply_projection(response.items, &select_stmt.select_list);
                        response.count = response.items.len();

                        Ok(ExecuteStatementResponse::Select {
                            items: response.items,
                            count: response.count,
                            scanned_count: response.scanned_count,
                            last_key: response.last_key,
                        })
                    }
                    SelectTranslation::MultiGet { keys, index_name } => {
                        // Execute multiple get operations
                        // For now, we'll execute a query for each pk and merge results
                        // TODO: Optimize with batch_get when available
                        let mut all_items = Vec::new();
                        let mut total_scanned = 0;

                        for pk in keys {
                            let mut query = Query::new(&pk);
                            if let Some(ref index) = index_name {
                                query = query.index(index);
                            }

                            let response = self.query(query)?;
                            total_scanned += response.scanned_count;
                            all_items.extend(response.items);
                        }

                        // Apply OFFSET if specified
                        if let Some(offset) = select_stmt.offset {
                            if offset < all_items.len() {
                                all_items = all_items.into_iter().skip(offset).collect();
                            } else {
                                all_items.clear();
                            }
                        }

                        // Apply LIMIT if specified
                        if let Some(limit) = select_stmt.limit {
                            all_items.truncate(limit);
                        }

                        // Apply projection
                        all_items = apply_projection(all_items, &select_stmt.select_list);

                        Ok(ExecuteStatementResponse::Select {
                            count: all_items.len(),
                            scanned_count: total_scanned,
                            items: all_items,
                            last_key: None,
                        })
                    }
                    SelectTranslation::Scan { filter_conditions } => {
                        // Execute Scan operation
                        let mut scan = Scan::new();

                        // Apply LIMIT - if both LIMIT and OFFSET, fetch enough records
                        if let Some(limit) = select_stmt.limit {
                            let fetch_limit = if let Some(offset) = select_stmt.offset {
                                limit + offset
                            } else {
                                limit
                            };
                            scan = scan.limit(fetch_limit);
                        }

                        let mut response = self.scan(scan)?;

                        // Apply filter conditions (WHERE clause filtering)
                        if !filter_conditions.is_empty() {
                            response.items = apply_filter_conditions(response.items, &filter_conditions);
                            response.count = response.items.len();
                        }

                        // Apply OFFSET if specified (by skipping items)
                        if let Some(offset) = select_stmt.offset {
                            if offset < response.items.len() {
                                response.items = response.items.into_iter().skip(offset).collect();
                                response.count = response.items.len();
                            } else {
                                // Offset is beyond results, return empty
                                response.items.clear();
                                response.count = 0;
                            }
                        }

                        // Apply LIMIT if specified (truncate after offset)
                        if let Some(limit) = select_stmt.limit {
                            if response.items.len() > limit {
                                response.items.truncate(limit);
                                response.count = limit;
                            }
                        }

                        // Apply projection
                        response.items = apply_projection(response.items, &select_stmt.select_list);
                        response.count = response.items.len();

                        Ok(ExecuteStatementResponse::Select {
                            items: response.items,
                            count: response.count,
                            scanned_count: response.scanned_count,
                            last_key: response.last_key,
                        })
                    }
                }
            }
            PartiQLStatement::Insert(insert_stmt) => {
                // Translate INSERT to Put operation
                let translation = PartiQLTranslator::translate_insert(&insert_stmt)?;

                // Execute put
                let pk = translation.key.pk.as_ref();
                if let Some(sk) = translation.key.sk.as_ref() {
                    self.put_with_sk(pk, sk.as_ref(), translation.item)?;
                } else {
                    self.put(pk, translation.item)?;
                }

                Ok(ExecuteStatementResponse::Insert { success: true })
            }
            PartiQLStatement::Update(update_stmt) => {
                // Translate UPDATE to Update operation
                let translation = PartiQLTranslator::translate_update(&update_stmt)?;

                // Build Update request
                let mut update = Update::new_from_key(translation.key);
                update = update.expression(&translation.expression);

                // Add placeholder values
                for (placeholder, value) in translation.values {
                    update = update.value(&placeholder, value);
                }

                // Execute update
                let response = self.update(update)?;

                Ok(ExecuteStatementResponse::Update {
                    item: response.item,
                })
            }
            PartiQLStatement::Delete(delete_stmt) => {
                // Translate DELETE to Delete operation
                let translation = PartiQLTranslator::translate_delete(&delete_stmt)?;

                // Execute delete
                let pk = translation.key.pk.as_ref();
                if let Some(sk) = translation.key.sk.as_ref() {
                    self.delete_with_sk(pk, sk.as_ref())?;
                } else {
                    self.delete(pk)?;
                }

                Ok(ExecuteStatementResponse::Delete { success: true })
            }
        }
    }
}

/// Apply projection to filter items to only include selected attributes
fn apply_projection(
    items: Vec<Item>,
    select_list: &kstone_core::partiql::SelectList,
) -> Vec<Item> {
    use kstone_core::partiql::SelectList;
    use std::collections::HashMap;

    match select_list {
        SelectList::All => items,
        SelectList::Attributes(attrs) => {
            items
                .into_iter()
                .map(|item| {
                    let mut projected = HashMap::new();
                    for attr in attrs {
                        if let Some(value) = item.get(attr) {
                            projected.insert(attr.clone(), value.clone());
                        }
                    }
                    projected
                })
                .collect()
        }
    }
}

/// Apply filter conditions to items (for Scan filtering)
fn apply_filter_conditions(
    items: Vec<Item>,
    conditions: &[kstone_core::partiql::Condition],
) -> Vec<Item> {
    use kstone_core::partiql::CompareOp;

    items
        .into_iter()
        .filter(|item| {
            // Item must match ALL conditions (AND logic)
            conditions.iter().all(|condition| {
                // Get attribute value from item
                let item_value = match item.get(&condition.attribute) {
                    Some(v) => v,
                    None => return false, // Attribute not present, doesn't match
                };

                // Compare based on operator
                match &condition.operator {
                    CompareOp::Equal => compare_values_eq(item_value, &condition.value),
                    CompareOp::NotEqual => !compare_values_eq(item_value, &condition.value),
                    CompareOp::LessThan => compare_values_lt(item_value, &condition.value),
                    CompareOp::LessThanOrEqual => {
                        compare_values_lt(item_value, &condition.value)
                            || compare_values_eq(item_value, &condition.value)
                    }
                    CompareOp::GreaterThan => {
                        !compare_values_lt(item_value, &condition.value)
                            && !compare_values_eq(item_value, &condition.value)
                    }
                    CompareOp::GreaterThanOrEqual => {
                        !compare_values_lt(item_value, &condition.value)
                    }
                    CompareOp::In => {
                        // Check if item_value is in the list
                        if let kstone_core::partiql::SqlValue::List(values) = &condition.value {
                            values.iter().any(|v| compare_values_eq(item_value, v))
                        } else {
                            false
                        }
                    }
                    CompareOp::Between => {
                        // BETWEEN x AND y
                        if let kstone_core::partiql::SqlValue::List(values) = &condition.value {
                            if values.len() == 2 {
                                let lower = &values[0];
                                let upper = &values[1];
                                (compare_values_eq(item_value, lower)
                                    || !compare_values_lt(item_value, lower))
                                    && (compare_values_eq(item_value, upper)
                                        || compare_values_lt(item_value, upper))
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    }
                }
            })
        })
        .collect()
}

/// Compare two values for equality
fn compare_values_eq(
    item_value: &crate::Value,
    sql_value: &kstone_core::partiql::SqlValue,
) -> bool {
    match (item_value, sql_value) {
        (crate::Value::N(n1), kstone_core::partiql::SqlValue::Number(n2)) => n1 == n2,
        (crate::Value::S(s1), kstone_core::partiql::SqlValue::String(s2)) => s1 == s2,
        (crate::Value::Bool(b1), kstone_core::partiql::SqlValue::Boolean(b2)) => b1 == b2,
        (crate::Value::Null, kstone_core::partiql::SqlValue::Null) => true,
        _ => false,
    }
}

/// Compare two values for less-than
fn compare_values_lt(
    item_value: &crate::Value,
    sql_value: &kstone_core::partiql::SqlValue,
) -> bool {
    match (item_value, sql_value) {
        (crate::Value::N(n1), kstone_core::partiql::SqlValue::Number(n2)) => {
            // Parse as f64 for numeric comparison
            if let (Ok(num1), Ok(num2)) = (n1.parse::<f64>(), n2.parse::<f64>()) {
                num1 < num2
            } else {
                // Fallback to string comparison if parsing fails
                n1 < n2
            }
        }
        (crate::Value::S(s1), kstone_core::partiql::SqlValue::String(s2)) => s1 < s2,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ItemBuilder;
    use tempfile::TempDir;

    #[test]
    fn test_execute_statement_insert() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let sql = "INSERT INTO users VALUE {'pk': 'user#123', 'name': 'Alice', 'age': 30}";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Insert { success } => assert!(success),
            _ => panic!("Expected Insert response"),
        }

        // Verify item was inserted
        let item = db.get(b"user#123").unwrap().unwrap();
        assert_eq!(item.get("name").unwrap().as_string().unwrap(), "Alice");
    }

    #[test]
    fn test_execute_statement_select_query() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert an item
        db.put(
            b"user#123",
            ItemBuilder::new()
                .string("name", "Bob")
                .number("age", 25)
                .build(),
        )
        .unwrap();

        // Query with SELECT
        let sql = "SELECT * FROM users WHERE pk = 'user#123'";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Select {
                items,
                count,
                scanned_count,
                ..
            } => {
                assert_eq!(count, 1);
                assert_eq!(scanned_count, 1);
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].get("name").unwrap().as_string().unwrap(), "Bob");
            }
            _ => panic!("Expected Select response"),
        }
    }

    #[test]
    fn test_execute_statement_select_scan() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert multiple items
        db.put(
            b"user#1",
            ItemBuilder::new().string("name", "Alice").build(),
        )
        .unwrap();
        db.put(
            b"user#2",
            ItemBuilder::new().string("name", "Bob").build(),
        )
        .unwrap();

        // Scan with SELECT
        let sql = "SELECT * FROM users";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Select { items, count, .. } => {
                assert_eq!(count, 2);
                assert_eq!(items.len(), 2);
            }
            _ => panic!("Expected Select response"),
        }
    }

    #[test]
    fn test_execute_statement_update_set() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial item
        db.put(
            b"user#123",
            ItemBuilder::new()
                .string("name", "Charlie")
                .number("age", 30)
                .build(),
        )
        .unwrap();

        // Update with SET
        let sql = "UPDATE users SET name = 'Charles', age = 31 WHERE pk = 'user#123'";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Update { item } => {
                assert_eq!(item.get("name").unwrap().as_string().unwrap(), "Charles");
                match item.get("age").unwrap() {
                    kstone_core::Value::N(n) => assert_eq!(n, "31"),
                    _ => panic!("Expected number"),
                }
            }
            _ => panic!("Expected Update response"),
        }
    }

    #[test]
    fn test_execute_statement_update_arithmetic() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial item
        db.put(
            b"user#456",
            ItemBuilder::new().number("score", 100).build(),
        )
        .unwrap();

        // Update with arithmetic
        let sql = "UPDATE users SET score = score + 50 WHERE pk = 'user#456'";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Update { item } => match item.get("score").unwrap() {
                kstone_core::Value::N(n) => assert_eq!(n, "150"),
                _ => panic!("Expected number"),
            },
            _ => panic!("Expected Update response"),
        }
    }

    #[test]
    fn test_execute_statement_update_remove() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert initial item
        db.put(
            b"user#789",
            ItemBuilder::new()
                .string("name", "Dave")
                .string("temp", "delete_me")
                .build(),
        )
        .unwrap();

        // Update with REMOVE
        let sql = "UPDATE users REMOVE temp WHERE pk = 'user#789'";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Update { item } => {
                assert!(!item.contains_key("temp"));
                assert_eq!(item.get("name").unwrap().as_string().unwrap(), "Dave");
            }
            _ => panic!("Expected Update response"),
        }
    }

    #[test]
    fn test_execute_statement_delete() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert an item
        db.put(
            b"user#999",
            ItemBuilder::new().string("name", "Eve").build(),
        )
        .unwrap();

        // Delete with DELETE
        let sql = "DELETE FROM users WHERE pk = 'user#999'";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Delete { success } => assert!(success),
            _ => panic!("Expected Delete response"),
        }

        // Verify item was deleted
        assert!(db.get(b"user#999").unwrap().is_none());
    }

    #[test]
    fn test_execute_statement_invalid_sql() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let sql = "INVALID SQL STATEMENT";
        let result = db.execute_statement(sql);

        assert!(result.is_err());
    }

    #[test]
    fn test_execute_statement_missing_pk_in_update() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // UPDATE without pk in WHERE should fail
        let sql = "UPDATE users SET name = 'Test' WHERE age = 30";
        let result = db.execute_statement(sql);

        assert!(result.is_err());
    }

    #[test]
    fn test_execute_statement_select_with_limit() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert multiple items
        for i in 0..20 {
            db.put(
                format!("user#{:03}", i).as_bytes(),
                ItemBuilder::new()
                    .string("name", format!("User{}", i))
                    .number("seq", i)
                    .build(),
            )
            .unwrap();
        }

        // SELECT with LIMIT
        let sql = "SELECT * FROM users LIMIT 5";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Select { items, count, .. } => {
                assert_eq!(count, 5);
                assert_eq!(items.len(), 5);
            }
            _ => panic!("Expected Select response"),
        }
    }

    #[test]
    fn test_execute_statement_select_with_offset() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert items
        for i in 0..10 {
            db.put(
                format!("user#{:03}", i).as_bytes(),
                ItemBuilder::new().number("seq", i).build(),
            )
            .unwrap();
        }

        // SELECT with OFFSET
        let sql = "SELECT * FROM users OFFSET 5";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Select { items, count, .. } => {
                assert_eq!(count, 5);
                assert_eq!(items.len(), 5);
            }
            _ => panic!("Expected Select response"),
        }
    }

    #[test]
    fn test_execute_statement_select_with_limit_and_offset() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert items
        for i in 0..20 {
            db.put(
                format!("user#{:03}", i).as_bytes(),
                ItemBuilder::new().number("seq", i).build(),
            )
            .unwrap();
        }

        // SELECT with LIMIT and OFFSET
        let sql = "SELECT * FROM users LIMIT 5 OFFSET 10";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Select { items, count, .. } => {
                assert_eq!(count, 5);
                assert_eq!(items.len(), 5);
            }
            _ => panic!("Expected Select response"),
        }
    }

    #[test]
    fn test_execute_statement_select_with_projection() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert items with multiple attributes
        for i in 0..5 {
            db.put(
                format!("user#{:03}", i).as_bytes(),
                ItemBuilder::new()
                    .string("name", format!("User{}", i))
                    .number("age", 20 + i)
                    .string("email", format!("user{}@example.com", i))
                    .string("city", "New York")
                    .build(),
            )
            .unwrap();
        }

        // SELECT with projection (only name and age)
        let sql = "SELECT name, age FROM users WHERE pk = 'user#001'";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Select { items, count, .. } => {
                assert_eq!(count, 1);
                assert_eq!(items.len(), 1);

                let item = &items[0];
                // Should have name and age
                assert!(item.contains_key("name"));
                assert!(item.contains_key("age"));

                // Should NOT have email and city
                assert!(!item.contains_key("email"));
                assert!(!item.contains_key("city"));

                // Verify only 2 attributes present
                assert_eq!(item.len(), 2);
            }
            _ => panic!("Expected Select response"),
        }
    }

    #[test]
    fn test_execute_statement_scan_with_filter() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Insert items with various ages
        for i in 0..10 {
            db.put(
                format!("user#{:03}", i).as_bytes(),
                ItemBuilder::new()
                    .string("name", format!("User{}", i))
                    .number("age", 20 + i)
                    .build(),
            )
            .unwrap();
        }

        // Scan with filter: age > 25 (should match users with age 26, 27, 28, 29)
        let sql = "SELECT * FROM users WHERE age > 25";
        let response = db.execute_statement(sql).unwrap();

        match response {
            ExecuteStatementResponse::Select { items, count, .. } => {
                assert_eq!(count, 4);
                assert_eq!(items.len(), 4);

                // Verify all returned items have age > 25
                for item in items {
                    if let Some(crate::Value::N(age_str)) = item.get("age") {
                        let age: i32 = age_str.parse().unwrap();
                        assert!(age > 25);
                    } else {
                        panic!("Expected age attribute");
                    }
                }
            }
            _ => panic!("Expected Select response"),
        }
    }
}

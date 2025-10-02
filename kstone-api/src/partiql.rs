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

                        // Execute query
                        let response = self.query(query)?;

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

                        Ok(ExecuteStatementResponse::Select {
                            count: all_items.len(),
                            scanned_count: total_scanned,
                            items: all_items,
                            last_key: None,
                        })
                    }
                    SelectTranslation::Scan { filter_conditions } => {
                        // Execute Scan operation
                        let scan = Scan::new();

                        // TODO: Apply filter conditions when scan supports filtering
                        // For now, we'll just execute the scan
                        let _ = filter_conditions; // Suppress unused warning

                        let response = self.scan(scan)?;

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
}

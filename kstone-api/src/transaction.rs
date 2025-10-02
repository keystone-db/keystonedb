/// Transaction operations for DynamoDB-style TransactGetItems and TransactWriteItems
///
/// Provides ACID transaction support with atomic reads and writes.

use kstone_core::{Item, Key};
use bytes::Bytes;

#[cfg(test)]
use std::collections::HashMap;

/// Transaction get request - read multiple items atomically
#[derive(Debug, Clone)]
pub struct TransactGetRequest {
    /// Keys to retrieve
    pub keys: Vec<Key>,
}

impl TransactGetRequest {
    /// Create a new transaction get request
    pub fn new() -> Self {
        Self { keys: Vec::new() }
    }

    /// Add a key with partition key only
    pub fn get(mut self, pk: &[u8]) -> Self {
        self.keys.push(Key::new(Bytes::copy_from_slice(pk)));
        self
    }

    /// Add a key with partition key and sort key
    pub fn get_with_sk(mut self, pk: &[u8], sk: &[u8]) -> Self {
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

impl Default for TransactGetRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Transaction get response
#[derive(Debug, Clone)]
pub struct TransactGetResponse {
    /// Items retrieved (in same order as request)
    pub items: Vec<Option<Item>>,
}

impl TransactGetResponse {
    pub(crate) fn new(items: Vec<Option<Item>>) -> Self {
        Self { items }
    }
}

/// Transaction write operation
#[derive(Debug, Clone)]
pub enum TransactWriteOp {
    /// Put an item with optional condition
    Put {
        key: Key,
        item: Item,
        condition: Option<String>,
    },
    /// Update an item with optional condition
    Update {
        key: Key,
        update_expression: String,
        condition: Option<String>,
    },
    /// Delete an item with optional condition
    Delete {
        key: Key,
        condition: Option<String>,
    },
    /// Condition check only (no write)
    ConditionCheck {
        key: Key,
        condition: String,
    },
}

/// Transaction write request - write multiple items atomically
#[derive(Debug, Clone)]
pub struct TransactWriteRequest {
    /// Write operations
    pub operations: Vec<TransactWriteOp>,
    /// Shared expression context for all operations
    pub context: kstone_core::expression::ExpressionContext,
}

impl TransactWriteRequest {
    /// Create a new transaction write request
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
            context: kstone_core::expression::ExpressionContext::new(),
        }
    }

    /// Add a put operation
    pub fn put(mut self, pk: &[u8], item: Item) -> Self {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.operations.push(TransactWriteOp::Put {
            key,
            item,
            condition: None,
        });
        self
    }

    /// Add a put operation with condition
    pub fn put_with_condition(mut self, pk: &[u8], item: Item, condition: impl Into<String>) -> Self {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.operations.push(TransactWriteOp::Put {
            key,
            item,
            condition: Some(condition.into()),
        });
        self
    }

    /// Add an update operation
    pub fn update(mut self, pk: &[u8], update_expression: impl Into<String>) -> Self {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.operations.push(TransactWriteOp::Update {
            key,
            update_expression: update_expression.into(),
            condition: None,
        });
        self
    }

    /// Add an update operation with condition
    pub fn update_with_condition(
        mut self,
        pk: &[u8],
        update_expression: impl Into<String>,
        condition: impl Into<String>,
    ) -> Self {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.operations.push(TransactWriteOp::Update {
            key,
            update_expression: update_expression.into(),
            condition: Some(condition.into()),
        });
        self
    }

    /// Add a delete operation
    pub fn delete(mut self, pk: &[u8]) -> Self {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.operations.push(TransactWriteOp::Delete {
            key,
            condition: None,
        });
        self
    }

    /// Add a delete operation with condition
    pub fn delete_with_condition(mut self, pk: &[u8], condition: impl Into<String>) -> Self {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.operations.push(TransactWriteOp::Delete {
            key,
            condition: Some(condition.into()),
        });
        self
    }

    /// Add a condition check (no write, just verify condition)
    pub fn condition_check(mut self, pk: &[u8], condition: impl Into<String>) -> Self {
        let key = Key::new(Bytes::copy_from_slice(pk));
        self.operations.push(TransactWriteOp::ConditionCheck {
            key,
            condition: condition.into(),
        });
        self
    }

    /// Add expression attribute value
    pub fn value(mut self, placeholder: impl Into<String>, value: kstone_core::Value) -> Self {
        self.context = self.context.with_value(placeholder, value);
        self
    }

    /// Add expression attribute name
    pub fn name(mut self, placeholder: impl Into<String>, name: impl Into<String>) -> Self {
        self.context = self.context.with_name(placeholder, name);
        self
    }

    /// Get the operations
    pub(crate) fn operations(&self) -> &[TransactWriteOp] {
        &self.operations
    }

    /// Get the context
    pub(crate) fn context(&self) -> &kstone_core::expression::ExpressionContext {
        &self.context
    }
}

impl Default for TransactWriteRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Transaction write response
#[derive(Debug, Clone)]
pub struct TransactWriteResponse {
    /// Number of operations committed
    pub committed_count: usize,
}

impl TransactWriteResponse {
    pub(crate) fn new(committed_count: usize) -> Self {
        Self { committed_count }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kstone_core::Value;

    #[test]
    fn test_transact_get_builder() {
        let request = TransactGetRequest::new()
            .get(b"user#1")
            .get_with_sk(b"user#2", b"profile");

        assert_eq!(request.keys().len(), 2);
    }

    #[test]
    fn test_transact_write_builder() {
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("Alice"));

        let request = TransactWriteRequest::new()
            .put(b"user#1", item)
            .delete(b"user#2")
            .condition_check(b"user#3", "attribute_exists(email)")
            .value(":val", Value::number(100));

        assert_eq!(request.operations().len(), 3);
    }

    #[test]
    fn test_transact_write_with_conditions() {
        let mut item = HashMap::new();
        item.insert("balance".to_string(), Value::number(100));

        let request = TransactWriteRequest::new()
            .put_with_condition(b"account#1", item, "attribute_not_exists(balance)")
            .update_with_condition(
                b"account#2",
                "SET balance = balance - :amount",
                "balance >= :amount"
            )
            .value(":amount", Value::number(50));

        assert_eq!(request.operations().len(), 2);
    }
}

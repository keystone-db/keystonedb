/// Update builder for DynamoDB-style update operations
///
/// Provides a high-level API for updating items with update expressions.

use kstone_core::{Item, Key, expression::{UpdateAction, UpdateExpressionParser, ExpressionContext}};
use bytes::Bytes;

/// Update builder
pub struct Update {
    key: Key,
    expression: String,
    condition: Option<String>,
    context: ExpressionContext,
}

impl Update {
    /// Create a new update operation for a partition key
    pub fn new(pk: &[u8]) -> Self {
        Self {
            key: Key::new(Bytes::copy_from_slice(pk)),
            expression: String::new(),
            condition: None,
            context: ExpressionContext::new(),
        }
    }

    /// Create update with partition key and sort key
    pub fn with_sk(pk: &[u8], sk: &[u8]) -> Self {
        Self {
            key: Key::with_sk(Bytes::copy_from_slice(pk), Bytes::copy_from_slice(sk)),
            expression: String::new(),
            condition: None,
            context: ExpressionContext::new(),
        }
    }

    /// Create update from an existing Key (used internally by PartiQL)
    pub(crate) fn new_from_key(key: Key) -> Self {
        Self {
            key,
            expression: String::new(),
            condition: None,
            context: ExpressionContext::new(),
        }
    }

    /// Set the update expression
    pub fn expression(mut self, expr: impl Into<String>) -> Self {
        self.expression = expr.into();
        self
    }

    /// Set a condition expression (Phase 2.5+)
    pub fn condition(mut self, condition: impl Into<String>) -> Self {
        self.condition = Some(condition.into());
        self
    }

    /// Add an expression attribute value
    pub fn value(mut self, placeholder: impl Into<String>, value: kstone_core::Value) -> Self {
        self.context = self.context.with_value(placeholder, value);
        self
    }

    /// Add an expression attribute name
    pub fn name(mut self, placeholder: impl Into<String>, name: impl Into<String>) -> Self {
        self.context = self.context.with_name(placeholder, name);
        self
    }

    /// Get the key
    pub(crate) fn key(&self) -> &Key {
        &self.key
    }

    /// Parse the update expression into actions
    pub(crate) fn into_actions(self) -> kstone_core::Result<(Vec<UpdateAction>, Option<String>, ExpressionContext)> {
        let actions = UpdateExpressionParser::parse(&self.expression)?;
        Ok((actions, self.condition, self.context))
    }
}

/// Update response
pub struct UpdateResponse {
    /// The updated item
    pub item: Item,
}

impl UpdateResponse {
    pub(crate) fn new(item: Item) -> Self {
        Self { item }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kstone_core::Value;

    #[test]
    fn test_update_builder() {
        let update = Update::new(b"user#123")
            .expression("SET age = :new_age")
            .value(":new_age", Value::number(30));

        let (actions, _condition, _context) = update.into_actions().unwrap();
        assert_eq!(actions.len(), 1);
    }

    #[test]
    fn test_update_builder_with_sk() {
        let update = Update::with_sk(b"user#123", b"profile")
            .expression("SET score = score + :inc REMOVE temp")
            .value(":inc", Value::number(10));

        let (actions, _condition, _context) = update.into_actions().unwrap();
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn test_update_builder_with_condition() {
        let update = Update::new(b"user#456")
            .expression("SET age = :new_age")
            .condition("age = :old_age")
            .value(":new_age", Value::number(30))
            .value(":old_age", Value::number(25));

        let (actions, condition, _context) = update.into_actions().unwrap();
        assert_eq!(actions.len(), 1);
        assert!(condition.is_some());
        assert_eq!(condition.unwrap(), "age = :old_age");
    }
}

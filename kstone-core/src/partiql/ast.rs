/// Simplified AST for PartiQL statements
///
/// This module provides a simplified Abstract Syntax Tree that wraps sqlparser's AST
/// with types specific to DynamoDB PartiQL operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level PartiQL statement
#[derive(Debug, Clone, PartialEq)]
pub enum PartiQLStatement {
    Select(SelectStatement),
    Insert(InsertStatement),
    Update(UpdateStatement),
    Delete(DeleteStatement),
}

/// SELECT statement
#[derive(Debug, Clone, PartialEq)]
pub struct SelectStatement {
    /// Table name (DynamoDB table)
    pub table_name: String,
    /// Optional index name (for LSI/GSI queries)
    pub index_name: Option<String>,
    /// Attributes to select (None = SELECT *)
    pub select_list: SelectList,
    /// WHERE clause conditions
    pub where_clause: Option<WhereClause>,
    /// ORDER BY clause
    pub order_by: Option<OrderBy>,
}

/// SELECT attribute list
#[derive(Debug, Clone, PartialEq)]
pub enum SelectList {
    /// SELECT *
    All,
    /// SELECT attr1, attr2, ...
    Attributes(Vec<String>),
}

/// WHERE clause with conditions
#[derive(Debug, Clone, PartialEq)]
pub struct WhereClause {
    /// List of conditions (implicitly AND-ed)
    pub conditions: Vec<Condition>,
}

impl WhereClause {
    /// Get condition for a specific attribute
    pub fn get_condition(&self, attr_name: &str) -> Option<&Condition> {
        self.conditions.iter().find(|c| c.attribute == attr_name)
    }

    /// Check if clause contains a condition for given attribute
    pub fn has_condition(&self, attr_name: &str) -> bool {
        self.get_condition(attr_name).is_some()
    }
}

/// Single condition in WHERE clause
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    /// Attribute name
    pub attribute: String,
    /// Comparison operator
    pub operator: CompareOp,
    /// Value(s) to compare against
    pub value: SqlValue,
}

impl Condition {
    /// Check if this is a key attribute condition (pk or sk)
    pub fn is_key_attribute(&self) -> bool {
        self.attribute == "pk" || self.attribute == "sk"
    }
}

/// Comparison operators
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CompareOp {
    /// =
    Equal,
    /// <>
    NotEqual,
    /// <
    LessThan,
    /// <=
    LessThanOrEqual,
    /// >
    GreaterThan,
    /// >=
    GreaterThanOrEqual,
    /// IN (...)
    In,
    /// BETWEEN x AND y
    Between,
}

/// SQL values (simplified from sqlparser)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlValue {
    /// Number (stored as string for precision)
    Number(String),
    /// String
    String(String),
    /// Boolean
    Boolean(bool),
    /// Null
    Null,
    /// List/Array
    List(Vec<SqlValue>),
    /// Map/Object
    Map(HashMap<String, SqlValue>),
}

impl SqlValue {
    /// Convert to KeystoneDB Value type
    pub fn to_kstone_value(&self) -> crate::Value {
        match self {
            SqlValue::Number(s) => crate::Value::N(s.clone()),
            SqlValue::String(s) => crate::Value::S(s.clone()),
            SqlValue::Boolean(b) => crate::Value::Bool(*b),
            SqlValue::Null => crate::Value::Null,
            SqlValue::List(items) => {
                let values: Vec<crate::Value> = items.iter()
                    .map(|item| item.to_kstone_value())
                    .collect();
                crate::Value::L(values)
            }
            SqlValue::Map(map) => {
                let mut kv_map = std::collections::HashMap::new();
                for (k, v) in map {
                    kv_map.insert(k.clone(), v.to_kstone_value());
                }
                crate::Value::M(kv_map)
            }
        }
    }

    /// Create from KeystoneDB Value
    pub fn from_kstone_value(value: &crate::Value) -> Self {
        match value {
            crate::Value::N(s) => SqlValue::Number(s.clone()),
            crate::Value::S(s) => SqlValue::String(s.clone()),
            crate::Value::Bool(b) => SqlValue::Boolean(*b),
            crate::Value::Null => SqlValue::Null,
            crate::Value::L(items) => {
                let sql_values: Vec<SqlValue> = items.iter()
                    .map(SqlValue::from_kstone_value)
                    .collect();
                SqlValue::List(sql_values)
            }
            crate::Value::M(map) => {
                let mut sql_map = HashMap::new();
                for (k, v) in map {
                    sql_map.insert(k.clone(), SqlValue::from_kstone_value(v));
                }
                SqlValue::Map(sql_map)
            }
            crate::Value::B(bytes) => {
                // Encode binary as base64 string
                SqlValue::String(base64_encode(bytes))
            }
            crate::Value::VecF32(vec) => {
                // Convert to list of numbers
                let numbers: Vec<SqlValue> = vec.iter()
                    .map(|f| SqlValue::Number(f.to_string()))
                    .collect();
                SqlValue::List(numbers)
            }
            crate::Value::Ts(ts) => SqlValue::Number(ts.to_string()),
        }
    }
}

fn base64_encode(bytes: &bytes::Bytes) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    {
        let mut encoder = base64::write::EncoderWriter::new(&mut buf, &base64::engine::general_purpose::STANDARD);
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap();
    }
    String::from_utf8(buf).unwrap()
}

/// ORDER BY clause
#[derive(Debug, Clone, PartialEq)]
pub struct OrderBy {
    /// Attribute to order by
    pub attribute: String,
    /// Sort direction (true = ASC, false = DESC)
    pub ascending: bool,
}

/// INSERT statement
#[derive(Debug, Clone, PartialEq)]
pub struct InsertStatement {
    /// Table name
    pub table_name: String,
    /// Value to insert (must be a Map with pk and optional sk)
    pub value: SqlValue,
}

/// UPDATE statement
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateStatement {
    /// Table name
    pub table_name: String,
    /// WHERE clause (must contain pk, optional sk)
    pub where_clause: WhereClause,
    /// SET assignments
    pub set_assignments: Vec<SetAssignment>,
    /// REMOVE attributes
    pub remove_attributes: Vec<String>,
}

/// SET assignment (SET attr = value)
#[derive(Debug, Clone, PartialEq)]
pub struct SetAssignment {
    /// Attribute name
    pub attribute: String,
    /// Value expression (can be literal or arithmetic like "age + 1")
    pub value: SetValue,
}

/// Value in SET clause
#[derive(Debug, Clone, PartialEq)]
pub enum SetValue {
    /// Literal value
    Literal(SqlValue),
    /// Arithmetic expression (attribute + value)
    Add {
        attribute: String,
        value: SqlValue,
    },
    /// Arithmetic expression (attribute - value)
    Subtract {
        attribute: String,
        value: SqlValue,
    },
}

/// DELETE statement
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteStatement {
    /// Table name
    pub table_name: String,
    /// WHERE clause (must contain full key: pk and optional sk)
    pub where_clause: WhereClause,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_where_clause_get_condition() {
        let where_clause = WhereClause {
            conditions: vec![
                Condition {
                    attribute: "pk".to_string(),
                    operator: CompareOp::Equal,
                    value: SqlValue::String("user#123".to_string()),
                },
                Condition {
                    attribute: "age".to_string(),
                    operator: CompareOp::GreaterThan,
                    value: SqlValue::Number("18".to_string()),
                },
            ],
        };

        assert!(where_clause.get_condition("pk").is_some());
        assert!(where_clause.get_condition("age").is_some());
        assert!(where_clause.get_condition("name").is_none());
    }

    #[test]
    fn test_condition_is_key_attribute() {
        let pk_cond = Condition {
            attribute: "pk".to_string(),
            operator: CompareOp::Equal,
            value: SqlValue::String("user#123".to_string()),
        };

        let sk_cond = Condition {
            attribute: "sk".to_string(),
            operator: CompareOp::Equal,
            value: SqlValue::String("profile".to_string()),
        };

        let data_cond = Condition {
            attribute: "age".to_string(),
            operator: CompareOp::GreaterThan,
            value: SqlValue::Number("18".to_string()),
        };

        assert!(pk_cond.is_key_attribute());
        assert!(sk_cond.is_key_attribute());
        assert!(!data_cond.is_key_attribute());
    }

    #[test]
    fn test_sql_value_to_kstone_value() {
        let sql_num = SqlValue::Number("42".to_string());
        assert!(matches!(sql_num.to_kstone_value(), crate::Value::N(s) if s == "42"));

        let sql_str = SqlValue::String("hello".to_string());
        assert!(matches!(sql_str.to_kstone_value(), crate::Value::S(s) if s == "hello"));

        let sql_bool = SqlValue::Boolean(true);
        assert!(matches!(sql_bool.to_kstone_value(), crate::Value::Bool(true)));

        let sql_null = SqlValue::Null;
        assert!(matches!(sql_null.to_kstone_value(), crate::Value::Null));
    }

    #[test]
    fn test_sql_value_from_kstone_value() {
        let kv_num = crate::Value::N("42".to_string());
        assert_eq!(SqlValue::from_kstone_value(&kv_num), SqlValue::Number("42".to_string()));

        let kv_str = crate::Value::S("hello".to_string());
        assert_eq!(SqlValue::from_kstone_value(&kv_str), SqlValue::String("hello".to_string()));

        let kv_bool = crate::Value::Bool(true);
        assert_eq!(SqlValue::from_kstone_value(&kv_bool), SqlValue::Boolean(true));
    }
}

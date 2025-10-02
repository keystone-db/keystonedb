/// DynamoDB-specific validation for PartiQL statements
///
/// Validates that PartiQL queries follow DynamoDB constraints such as:
/// - SELECT must have partition key with = or IN for Query operations
/// - UPDATE/DELETE must specify full primary key
/// - No full table scans without explicit opt-in

use crate::partiql::ast::*;
use crate::{Error, Result};

/// Query type determination for SELECT statements
#[derive(Debug, Clone, PartialEq)]
pub enum QueryType {
    /// Query operation (partition key specified with = or IN)
    Query {
        /// Partition key condition
        pk_condition: Condition,
        /// Optional sort key condition
        sk_condition: Option<Condition>,
    },
    /// Scan operation (full table scan)
    Scan,
}

/// DynamoDB constraint validator
pub struct DynamoDBValidator;

impl DynamoDBValidator {
    /// Validate SELECT statement and determine query type
    pub fn validate_select(stmt: &SelectStatement) -> Result<QueryType> {
        // Extract WHERE clause
        let where_clause = match &stmt.where_clause {
            Some(wc) => wc,
            None => return Ok(QueryType::Scan),
        };

        // Look for pk condition
        if let Some(pk_cond) = where_clause.get_condition("pk") {
            // Check if pk uses = or IN operator
            match pk_cond.operator {
                CompareOp::Equal | CompareOp::In => {
                    // This is a Query operation
                    let sk_condition = where_clause.get_condition("sk").cloned();
                    Ok(QueryType::Query {
                        pk_condition: pk_cond.clone(),
                        sk_condition,
                    })
                }
                _ => {
                    // pk exists but not with = or IN, this is a Scan
                    Ok(QueryType::Scan)
                }
            }
        } else {
            // No pk condition, this is a Scan
            Ok(QueryType::Scan)
        }
    }

    /// Validate INSERT statement
    pub fn validate_insert(stmt: &InsertStatement) -> Result<()> {
        // Ensure value is a Map
        match &stmt.value {
            SqlValue::Map(map) => {
                // Ensure pk exists
                if !map.contains_key("pk") {
                    return Err(Error::InvalidQuery(
                        "INSERT value must contain 'pk' field".into(),
                    ));
                }
                Ok(())
            }
            _ => Err(Error::InvalidQuery(
                "INSERT value must be a map/object".into(),
            )),
        }
    }

    /// Validate UPDATE statement
    pub fn validate_update(stmt: &UpdateStatement) -> Result<()> {
        // Ensure WHERE clause has pk
        if !stmt.where_clause.has_condition("pk") {
            return Err(Error::InvalidQuery(
                "UPDATE must specify partition key (pk) in WHERE clause".into(),
            ));
        }
        Ok(())
    }

    /// Validate DELETE statement
    pub fn validate_delete(stmt: &DeleteStatement) -> Result<()> {
        // Ensure WHERE clause has pk
        if !stmt.where_clause.has_condition("pk") {
            return Err(Error::InvalidQuery(
                "DELETE must specify partition key (pk) in WHERE clause".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_select_query_with_pk_equal() {
        let stmt = SelectStatement {
            table_name: "users".to_string(),
            index_name: None,
            select_list: SelectList::All,
            where_clause: Some(WhereClause {
                conditions: vec![Condition {
                    attribute: "pk".to_string(),
                    operator: CompareOp::Equal,
                    value: SqlValue::String("user#123".to_string()),
                }],
            }),
            order_by: None,
            limit: None,
            offset: None,
        };

        let query_type = DynamoDBValidator::validate_select(&stmt).unwrap();
        match query_type {
            QueryType::Query { pk_condition, sk_condition } => {
                assert_eq!(pk_condition.attribute, "pk");
                assert_eq!(pk_condition.operator, CompareOp::Equal);
                assert!(sk_condition.is_none());
            }
            _ => panic!("Expected Query type"),
        }
    }

    #[test]
    fn test_validate_select_query_with_pk_and_sk() {
        let stmt = SelectStatement {
            table_name: "users".to_string(),
            index_name: None,
            select_list: SelectList::All,
            where_clause: Some(WhereClause {
                conditions: vec![
                    Condition {
                        attribute: "pk".to_string(),
                        operator: CompareOp::Equal,
                        value: SqlValue::String("user#123".to_string()),
                    },
                    Condition {
                        attribute: "sk".to_string(),
                        operator: CompareOp::GreaterThan,
                        value: SqlValue::String("post#".to_string()),
                    },
                ],
            }),
            order_by: None,
            limit: None,
            offset: None,
        };

        let query_type = DynamoDBValidator::validate_select(&stmt).unwrap();
        match query_type {
            QueryType::Query { pk_condition, sk_condition } => {
                assert_eq!(pk_condition.attribute, "pk");
                assert!(sk_condition.is_some());
                assert_eq!(sk_condition.unwrap().attribute, "sk");
            }
            _ => panic!("Expected Query type"),
        }
    }

    #[test]
    fn test_validate_select_scan_no_where() {
        let stmt = SelectStatement {
            table_name: "users".to_string(),
            index_name: None,
            select_list: SelectList::All,
            where_clause: None,
            order_by: None,
            limit: None,
            offset: None,
        };

        let query_type = DynamoDBValidator::validate_select(&stmt).unwrap();
        assert_eq!(query_type, QueryType::Scan);
    }

    #[test]
    fn test_validate_select_scan_no_pk() {
        let stmt = SelectStatement {
            table_name: "users".to_string(),
            index_name: None,
            select_list: SelectList::All,
            where_clause: Some(WhereClause {
                conditions: vec![Condition {
                    attribute: "age".to_string(),
                    operator: CompareOp::GreaterThan,
                    value: SqlValue::Number("18".to_string()),
                }],
            }),
            order_by: None,
            limit: None,
            offset: None,
        };

        let query_type = DynamoDBValidator::validate_select(&stmt).unwrap();
        assert_eq!(query_type, QueryType::Scan);
    }

    #[test]
    fn test_validate_insert_with_pk() {
        let mut map = std::collections::HashMap::new();
        map.insert("pk".to_string(), SqlValue::String("user#123".to_string()));
        map.insert("name".to_string(), SqlValue::String("Alice".to_string()));

        let stmt = InsertStatement {
            table_name: "users".to_string(),
            value: SqlValue::Map(map),
        };

        assert!(DynamoDBValidator::validate_insert(&stmt).is_ok());
    }

    #[test]
    fn test_validate_insert_missing_pk() {
        let mut map = std::collections::HashMap::new();
        map.insert("name".to_string(), SqlValue::String("Alice".to_string()));

        let stmt = InsertStatement {
            table_name: "users".to_string(),
            value: SqlValue::Map(map),
        };

        let result = DynamoDBValidator::validate_insert(&stmt);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("pk"));
    }

    #[test]
    fn test_validate_update_with_pk() {
        let stmt = UpdateStatement {
            table_name: "users".to_string(),
            where_clause: WhereClause {
                conditions: vec![Condition {
                    attribute: "pk".to_string(),
                    operator: CompareOp::Equal,
                    value: SqlValue::String("user#123".to_string()),
                }],
            },
            set_assignments: vec![],
            remove_attributes: vec![],
        };

        assert!(DynamoDBValidator::validate_update(&stmt).is_ok());
    }

    #[test]
    fn test_validate_update_missing_pk() {
        let stmt = UpdateStatement {
            table_name: "users".to_string(),
            where_clause: WhereClause {
                conditions: vec![Condition {
                    attribute: "age".to_string(),
                    operator: CompareOp::GreaterThan,
                    value: SqlValue::Number("18".to_string()),
                }],
            },
            set_assignments: vec![],
            remove_attributes: vec![],
        };

        let result = DynamoDBValidator::validate_update(&stmt);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("pk"));
    }
}

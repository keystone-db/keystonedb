/// Translates PartiQL AST to KeystoneDB operations
///
/// Maps SELECT to Query/Scan, INSERT to Put, UPDATE to Update, DELETE to Delete.

use crate::partiql::ast::*;
use crate::partiql::validator::{DynamoDBValidator, QueryType};
use crate::{Error, Key, Result};
use bytes::Bytes;

/// PartiQL to KeystoneDB translator
pub struct PartiQLTranslator;

impl PartiQLTranslator {
    /// Translate SELECT statement to Query or Scan parameters
    pub fn translate_select(stmt: &SelectStatement) -> Result<SelectTranslation> {
        // Validate and determine query type
        let query_type = DynamoDBValidator::validate_select(stmt)?;

        match query_type {
            QueryType::Query { pk_condition, sk_condition } => {
                // Translate to Query
                let (pk_bytes, multiple_pks) = Self::extract_pk_bytes(&pk_condition)?;

                if multiple_pks {
                    // IN clause with multiple PKs - need to execute multiple gets
                    Ok(SelectTranslation::MultiGet {
                        keys: pk_bytes,
                        index_name: stmt.index_name.clone(),
                    })
                } else {
                    // Single PK - regular Query
                    let pk = pk_bytes.into_iter().next().unwrap();
                    let sk_condition_translated = sk_condition
                        .as_ref()
                        .map(Self::translate_sk_condition)
                        .transpose()?;

                    Ok(SelectTranslation::Query {
                        pk,
                        sk_condition: sk_condition_translated,
                        index_name: stmt.index_name.clone(),
                        forward: stmt.order_by.as_ref().map_or(true, |o| o.ascending),
                    })
                }
            }
            QueryType::Scan => {
                // Translate to Scan
                Ok(SelectTranslation::Scan {
                    filter_conditions: stmt
                        .where_clause
                        .as_ref()
                        .map(|wc| wc.conditions.clone())
                        .unwrap_or_default(),
                })
            }
        }
    }

    /// Extract partition key bytes from condition
    fn extract_pk_bytes(condition: &Condition) -> Result<(Vec<Bytes>, bool)> {
        match &condition.operator {
            CompareOp::Equal => {
                let bytes = Self::value_to_bytes(&condition.value)?;
                Ok((vec![bytes], false))
            }
            CompareOp::In => {
                // IN clause - extract all values
                match &condition.value {
                    SqlValue::List(values) => {
                        let bytes_vec: Result<Vec<Bytes>> = values
                            .iter()
                            .map(Self::value_to_bytes)
                            .collect();
                        Ok((bytes_vec?, true))
                    }
                    _ => Err(Error::InvalidQuery("IN value must be a list".into())),
                }
            }
            _ => Err(Error::InvalidQuery(
                "Partition key must use = or IN operator".into(),
            )),
        }
    }

    /// Convert SqlValue to Bytes for key
    fn value_to_bytes(value: &SqlValue) -> Result<Bytes> {
        match value {
            SqlValue::String(s) => Ok(Bytes::copy_from_slice(s.as_bytes())),
            SqlValue::Number(n) => Ok(Bytes::copy_from_slice(n.as_bytes())),
            _ => Err(Error::InvalidQuery(format!(
                "Unsupported key value type: {:?}",
                value
            ))),
        }
    }

    /// Translate sort key condition to KeystoneDB SortKeyCondition
    fn translate_sk_condition(condition: &Condition) -> Result<SortKeyConditionType> {
        let sk_bytes = Self::value_to_bytes(&condition.value)?;

        match condition.operator {
            CompareOp::Equal => Ok(SortKeyConditionType::Equal(sk_bytes)),
            CompareOp::LessThan => Ok(SortKeyConditionType::LessThan(sk_bytes)),
            CompareOp::LessThanOrEqual => Ok(SortKeyConditionType::LessThanOrEqual(sk_bytes)),
            CompareOp::GreaterThan => Ok(SortKeyConditionType::GreaterThan(sk_bytes)),
            CompareOp::GreaterThanOrEqual => Ok(SortKeyConditionType::GreaterThanOrEqual(sk_bytes)),
            CompareOp::Between => {
                match &condition.value {
                    SqlValue::List(values) if values.len() == 2 => {
                        let low = Self::value_to_bytes(&values[0])?;
                        let high = Self::value_to_bytes(&values[1])?;
                        Ok(SortKeyConditionType::Between(low, high))
                    }
                    _ => Err(Error::InvalidQuery("BETWEEN requires exactly 2 values".into())),
                }
            }
            _ => Err(Error::InvalidQuery(format!(
                "Unsupported sort key operator: {:?}",
                condition.operator
            ))),
        }
    }

    /// Translate INSERT statement
    pub fn translate_insert(stmt: &InsertStatement) -> Result<InsertTranslation> {
        // Validate
        DynamoDBValidator::validate_insert(stmt)?;

        // Extract key and item from value map
        let value_map = match &stmt.value {
            SqlValue::Map(map) => map,
            _ => return Err(Error::InvalidQuery("INSERT value must be a map".into())),
        };

        // Extract pk
        let pk_value = value_map
            .get("pk")
            .ok_or_else(|| Error::InvalidQuery("INSERT value must contain 'pk'".into()))?;
        let pk_bytes = Self::value_to_bytes(pk_value)?;

        // Extract optional sk
        let sk_bytes = value_map
            .get("sk")
            .map(Self::value_to_bytes)
            .transpose()?;

        // Build key
        let key = if let Some(sk) = sk_bytes {
            Key::with_sk(pk_bytes.to_vec(), sk.to_vec())
        } else {
            Key::new(pk_bytes.to_vec())
        };

        // Convert remaining attributes to Item
        let mut item = std::collections::HashMap::new();
        for (attr_name, attr_value) in value_map {
            if attr_name != "pk" && attr_name != "sk" {
                item.insert(attr_name.clone(), attr_value.to_kstone_value());
            }
        }

        Ok(InsertTranslation { key, item })
    }

    /// Translate UPDATE statement
    pub fn translate_update(_stmt: &UpdateStatement) -> Result<UpdateTranslation> {
        // TODO: Implement UPDATE translation
        Err(Error::InvalidQuery("UPDATE translation not yet implemented".into()))
    }

    /// Translate DELETE statement
    pub fn translate_delete(stmt: &DeleteStatement) -> Result<DeleteTranslation> {
        // Validate
        DynamoDBValidator::validate_delete(stmt)?;

        // Extract key from WHERE clause
        let pk_cond = stmt
            .where_clause
            .get_condition("pk")
            .ok_or_else(|| Error::InvalidQuery("DELETE must specify pk in WHERE clause".into()))?;
        let pk_bytes = Self::value_to_bytes(&pk_cond.value)?;

        let sk_bytes = stmt
            .where_clause
            .get_condition("sk")
            .map(|c| Self::value_to_bytes(&c.value))
            .transpose()?;

        let key = if let Some(sk) = sk_bytes {
            Key::with_sk(pk_bytes.to_vec(), sk.to_vec())
        } else {
            Key::new(pk_bytes.to_vec())
        };

        Ok(DeleteTranslation { key })
    }
}

/// SELECT statement translation result
#[derive(Debug)]
pub enum SelectTranslation {
    /// Query operation (single partition)
    Query {
        pk: Bytes,
        sk_condition: Option<SortKeyConditionType>,
        index_name: Option<String>,
        forward: bool,
    },
    /// Multiple get operations (IN clause on pk)
    MultiGet {
        keys: Vec<Bytes>,
        index_name: Option<String>,
    },
    /// Scan operation (full table scan)
    Scan {
        filter_conditions: Vec<Condition>,
    },
}

/// Sort key condition type
#[derive(Debug, Clone)]
pub enum SortKeyConditionType {
    Equal(Bytes),
    LessThan(Bytes),
    LessThanOrEqual(Bytes),
    GreaterThan(Bytes),
    GreaterThanOrEqual(Bytes),
    Between(Bytes, Bytes),
}

/// INSERT translation result
#[derive(Debug)]
pub struct InsertTranslation {
    pub key: Key,
    pub item: crate::Item,
}

/// UPDATE translation result
#[derive(Debug)]
pub struct UpdateTranslation {
    pub key: Key,
    pub expression: String,
    pub values: std::collections::HashMap<String, crate::Value>,
}

/// DELETE translation result
#[derive(Debug)]
pub struct DeleteTranslation {
    pub key: Key,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_select_query() {
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
        };

        let translation = PartiQLTranslator::translate_select(&stmt).unwrap();
        match translation {
            SelectTranslation::Query { pk, .. } => {
                assert_eq!(pk, Bytes::from("user#123"));
            }
            _ => panic!("Expected Query translation"),
        }
    }

    #[test]
    fn test_translate_select_scan() {
        let stmt = SelectStatement {
            table_name: "users".to_string(),
            index_name: None,
            select_list: SelectList::All,
            where_clause: None,
            order_by: None,
        };

        let translation = PartiQLTranslator::translate_select(&stmt).unwrap();
        match translation {
            SelectTranslation::Scan { .. } => {}
            _ => panic!("Expected Scan translation"),
        }
    }

    #[test]
    fn test_translate_insert() {
        let mut map = std::collections::HashMap::new();
        map.insert("pk".to_string(), SqlValue::String("user#123".to_string()));
        map.insert("name".to_string(), SqlValue::String("Alice".to_string()));
        map.insert("age".to_string(), SqlValue::Number("30".to_string()));

        let stmt = InsertStatement {
            table_name: "users".to_string(),
            value: SqlValue::Map(map),
        };

        let translation = PartiQLTranslator::translate_insert(&stmt).unwrap();
        assert_eq!(translation.key.pk.as_ref(), "user#123".as_bytes());
        assert_eq!(translation.item.len(), 2); // name and age (pk/sk excluded)
    }

    #[test]
    fn test_translate_delete() {
        let stmt = DeleteStatement {
            table_name: "users".to_string(),
            where_clause: WhereClause {
                conditions: vec![Condition {
                    attribute: "pk".to_string(),
                    operator: CompareOp::Equal,
                    value: SqlValue::String("user#123".to_string()),
                }],
            },
        };

        let translation = PartiQLTranslator::translate_delete(&stmt).unwrap();
        assert_eq!(translation.key.pk.as_ref(), "user#123".as_bytes());
    }
}

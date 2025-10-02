/// PartiQL parser implementation using sqlparser-rs
///
/// Parses SQL statements and converts them to our simplified AST.
/// Validates DynamoDB-specific constraints (e.g., no JOINs, no subqueries).

use crate::partiql::ast::*;
use crate::{Error, Result};
use sqlparser::ast::{self as sql_ast, Statement};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser as SqlParser;

/// PartiQL statement parser
pub struct PartiQLParser;

impl PartiQLParser {
    /// Parse a PartiQL statement
    pub fn parse(sql: &str) -> Result<PartiQLStatement> {
        // Validate statement length (1-8192 chars per DynamoDB spec)
        if sql.is_empty() {
            return Err(Error::InvalidQuery("Statement cannot be empty".into()));
        }
        if sql.len() > 8192 {
            return Err(Error::InvalidQuery(format!(
                "Statement too long: {} chars (max 8192)",
                sql.len()
            )));
        }

        // Parse SQL using sqlparser
        let dialect = GenericDialect {};
        let statements = SqlParser::parse_sql(&dialect, sql).map_err(|e| {
            Error::InvalidQuery(format!("Failed to parse SQL: {}", e))
        })?;

        // Expect exactly one statement
        if statements.is_empty() {
            return Err(Error::InvalidQuery("No statement found".into()));
        }
        if statements.len() > 1 {
            return Err(Error::InvalidQuery("Multiple statements not supported".into()));
        }

        // Convert to PartiQL AST
        Self::convert_statement(&statements[0])
    }

    /// Convert sqlparser Statement to PartiQLStatement
    fn convert_statement(stmt: &Statement) -> Result<PartiQLStatement> {
        match stmt {
            Statement::Query(query) => {
                let select_stmt = Self::convert_select(query)?;
                Ok(PartiQLStatement::Select(select_stmt))
            }
            Statement::Insert(insert) => {
                let insert_stmt = Self::convert_insert(insert)?;
                Ok(PartiQLStatement::Insert(insert_stmt))
            }
            Statement::Update { .. } => {
                // TODO: Implement UPDATE parsing
                Err(Error::InvalidQuery("UPDATE not yet implemented".into()))
            }
            Statement::Delete(delete) => {
                let delete_stmt = Self::convert_delete(delete)?;
                Ok(PartiQLStatement::Delete(delete_stmt))
            }
            _ => Err(Error::InvalidQuery(format!(
                "Unsupported statement type: {:?}",
                stmt
            ))),
        }
    }

    /// Convert SELECT query
    fn convert_select(query: &sql_ast::Query) -> Result<SelectStatement> {
        // Validate no unsupported features
        if query.with.is_some() {
            return Err(Error::InvalidQuery("WITH clause not supported".into()));
        }
        // ORDER BY is supported, we'll handle it below
        if query.limit.is_some() {
            return Err(Error::InvalidQuery("Use LIMIT in application code, not in PartiQL".into()));
        }
        if query.fetch.is_some() {
            return Err(Error::InvalidQuery("FETCH clause not supported".into()));
        }
        if query.offset.is_some() {
            return Err(Error::InvalidQuery("OFFSET clause not supported".into()));
        }

        // Extract SELECT body
        let set_expr = match &*query.body {
            sql_ast::SetExpr::Select(select) => select,
            _ => return Err(Error::InvalidQuery("Unsupported query type (no UNION/INTERSECT/EXCEPT)".into())),
        };

        // Validate no unsupported SELECT features
        if !set_expr.cluster_by.is_empty() {
            return Err(Error::InvalidQuery("CLUSTER BY not supported".into()));
        }
        if !set_expr.distribute_by.is_empty() {
            return Err(Error::InvalidQuery("DISTRIBUTE BY not supported".into()));
        }
        if set_expr.group_by != sql_ast::GroupByExpr::Expressions(vec![], vec![]) {
            return Err(Error::InvalidQuery("GROUP BY not supported".into()));
        }
        if set_expr.having.is_some() {
            return Err(Error::InvalidQuery("HAVING clause not supported".into()));
        }
        if !set_expr.named_window.is_empty() {
            return Err(Error::InvalidQuery("Window functions not supported".into()));
        }
        if !set_expr.qualify.is_none() {
            return Err(Error::InvalidQuery("QUALIFY clause not supported".into()));
        }
        if let Some(_top) = &set_expr.top {
            return Err(Error::InvalidQuery("TOP clause not supported".into()));
        }

        // Extract FROM clause (must be single table reference)
        let (table_name, index_name) = if set_expr.from.is_empty() {
            return Err(Error::InvalidQuery("FROM clause required".into()));
        } else if set_expr.from.len() > 1 {
            return Err(Error::InvalidQuery("Multiple tables not supported (no JOINs)".into()));
        } else {
            Self::extract_table_reference(&set_expr.from[0])?
        };

        // Extract SELECT list
        let select_list = Self::convert_select_list(&set_expr.projection)?;

        // Extract WHERE clause
        let where_clause = match &set_expr.selection {
            Some(expr) => Some(Self::convert_where_clause(expr)?),
            None => None,
        };

        // Extract ORDER BY
        let order_by = match &query.order_by {
            Some(order_by_clause) => Some(Self::convert_order_by(&order_by_clause.exprs)?),
            None => None,
        };

        Ok(SelectStatement {
            table_name,
            index_name,
            select_list,
            where_clause,
            order_by,
        })
    }

    /// Extract table name and optional index from FROM clause
    fn extract_table_reference(from: &sql_ast::TableWithJoins) -> Result<(String, Option<String>)> {
        // Check no JOINs
        if !from.joins.is_empty() {
            return Err(Error::InvalidQuery("JOIN not supported".into()));
        }

        // Extract table name
        match &from.relation {
            sql_ast::TableFactor::Table { name, .. } => {
                let table_parts: Vec<&sql_ast::Ident> = name.0.iter().collect();

                if table_parts.is_empty() {
                    return Err(Error::InvalidQuery("Empty table name".into()));
                }

                // Check for "table.index" syntax
                if table_parts.len() == 1 {
                    Ok((table_parts[0].value.clone(), None))
                } else if table_parts.len() == 2 {
                    Ok((table_parts[0].value.clone(), Some(table_parts[1].value.clone())))
                } else {
                    Err(Error::InvalidQuery(format!(
                        "Invalid table reference: expected 'table' or 'table.index', got {:?}",
                        name
                    )))
                }
            }
            _ => Err(Error::InvalidQuery("Unsupported FROM clause (subqueries not allowed)".into())),
        }
    }

    /// Convert SELECT projection list
    fn convert_select_list(projection: &[sql_ast::SelectItem]) -> Result<SelectList> {
        if projection.is_empty() {
            return Err(Error::InvalidQuery("Empty SELECT list".into()));
        }

        // Check for SELECT *
        if projection.len() == 1 {
            if let sql_ast::SelectItem::Wildcard(_) = &projection[0] {
                return Ok(SelectList::All);
            }
        }

        // Extract attribute names
        let mut attributes = Vec::new();
        for item in projection {
            match item {
                sql_ast::SelectItem::UnnamedExpr(expr) => {
                    let attr_name = Self::extract_attribute_name(expr)?;
                    attributes.push(attr_name);
                }
                sql_ast::SelectItem::ExprWithAlias { expr, alias: _ } => {
                    let attr_name = Self::extract_attribute_name(expr)?;
                    attributes.push(attr_name);
                }
                sql_ast::SelectItem::Wildcard(_) => {
                    return Err(Error::InvalidQuery("Cannot mix * with other columns".into()));
                }
                _ => {
                    return Err(Error::InvalidQuery("Unsupported SELECT item".into()));
                }
            }
        }

        Ok(SelectList::Attributes(attributes))
    }

    /// Extract attribute name from expression
    fn extract_attribute_name(expr: &sql_ast::Expr) -> Result<String> {
        match expr {
            sql_ast::Expr::Identifier(ident) => Ok(ident.value.clone()),
            sql_ast::Expr::CompoundIdentifier(parts) => {
                if parts.len() == 1 {
                    Ok(parts[0].value.clone())
                } else {
                    Err(Error::InvalidQuery(format!(
                        "Compound identifiers not supported: {:?}",
                        parts
                    )))
                }
            }
            _ => Err(Error::InvalidQuery(format!(
                "Unsupported expression in SELECT list: {:?}",
                expr
            ))),
        }
    }

    /// Convert WHERE clause expression to WhereClause
    fn convert_where_clause(expr: &sql_ast::Expr) -> Result<WhereClause> {
        let mut conditions = Vec::new();
        Self::extract_conditions(expr, &mut conditions)?;
        Ok(WhereClause { conditions })
    }

    /// Recursively extract AND-connected conditions
    fn extract_conditions(expr: &sql_ast::Expr, conditions: &mut Vec<Condition>) -> Result<()> {
        match expr {
            sql_ast::Expr::BinaryOp { left, op, right } => {
                use sqlparser::ast::BinaryOperator;

                match op {
                    BinaryOperator::And => {
                        // Recursively extract from both sides
                        Self::extract_conditions(left, conditions)?;
                        Self::extract_conditions(right, conditions)?;
                    }
                    BinaryOperator::Or => {
                        return Err(Error::InvalidQuery("OR not supported in WHERE clause (use AND only)".into()));
                    }
                    BinaryOperator::Eq
                    | BinaryOperator::NotEq
                    | BinaryOperator::Lt
                    | BinaryOperator::LtEq
                    | BinaryOperator::Gt
                    | BinaryOperator::GtEq => {
                        // Extract comparison condition
                        let condition = Self::convert_comparison(left, op, right)?;
                        conditions.push(condition);
                    }
                    _ => {
                        return Err(Error::InvalidQuery(format!(
                            "Unsupported operator in WHERE clause: {:?}",
                            op
                        )));
                    }
                }
            }
            sql_ast::Expr::InList { expr, list, negated } => {
                if *negated {
                    return Err(Error::InvalidQuery("NOT IN not supported".into()));
                }
                let condition = Self::convert_in_condition(expr, list)?;
                conditions.push(condition);
            }
            sql_ast::Expr::Between { expr, negated, low, high } => {
                if *negated {
                    return Err(Error::InvalidQuery("NOT BETWEEN not supported".into()));
                }
                let condition = Self::convert_between_condition(expr, low, high)?;
                conditions.push(condition);
            }
            _ => {
                return Err(Error::InvalidQuery(format!(
                    "Unsupported WHERE clause expression: {:?}",
                    expr
                )));
            }
        }
        Ok(())
    }

    /// Convert comparison expression to Condition
    fn convert_comparison(
        left: &sql_ast::Expr,
        op: &sql_ast::BinaryOperator,
        right: &sql_ast::Expr,
    ) -> Result<Condition> {
        use sqlparser::ast::BinaryOperator;

        let attribute = Self::extract_attribute_name(left)?;
        let compare_op = match op {
            BinaryOperator::Eq => CompareOp::Equal,
            BinaryOperator::NotEq => CompareOp::NotEqual,
            BinaryOperator::Lt => CompareOp::LessThan,
            BinaryOperator::LtEq => CompareOp::LessThanOrEqual,
            BinaryOperator::Gt => CompareOp::GreaterThan,
            BinaryOperator::GtEq => CompareOp::GreaterThanOrEqual,
            _ => unreachable!(),
        };
        let value = Self::convert_value(right)?;

        Ok(Condition {
            attribute,
            operator: compare_op,
            value,
        })
    }

    /// Convert IN condition
    fn convert_in_condition(
        expr: &sql_ast::Expr,
        list: &[sql_ast::Expr],
    ) -> Result<Condition> {
        let attribute = Self::extract_attribute_name(expr)?;
        let values: Result<Vec<SqlValue>> = list.iter().map(Self::convert_value).collect();

        Ok(Condition {
            attribute,
            operator: CompareOp::In,
            value: SqlValue::List(values?),
        })
    }

    /// Convert BETWEEN condition
    fn convert_between_condition(
        expr: &sql_ast::Expr,
        low: &sql_ast::Expr,
        high: &sql_ast::Expr,
    ) -> Result<Condition> {
        let attribute = Self::extract_attribute_name(expr)?;
        let low_val = Self::convert_value(low)?;
        let high_val = Self::convert_value(high)?;

        Ok(Condition {
            attribute,
            operator: CompareOp::Between,
            value: SqlValue::List(vec![low_val, high_val]),
        })
    }

    /// Convert SQL expression to SqlValue
    fn convert_value(expr: &sql_ast::Expr) -> Result<SqlValue> {
        match expr {
            sql_ast::Expr::Value(val) => Self::convert_sql_value(val),
            _ => Err(Error::InvalidQuery(format!(
                "Unsupported value expression: {:?}",
                expr
            ))),
        }
    }

    /// Convert sqlparser Value to SqlValue
    fn convert_sql_value(val: &sql_ast::Value) -> Result<SqlValue> {
        match val {
            sql_ast::Value::Number(n, _) => Ok(SqlValue::Number(n.clone())),
            sql_ast::Value::SingleQuotedString(s) | sql_ast::Value::DoubleQuotedString(s) => {
                Ok(SqlValue::String(s.clone()))
            }
            sql_ast::Value::Boolean(b) => Ok(SqlValue::Boolean(*b)),
            sql_ast::Value::Null => Ok(SqlValue::Null),
            _ => Err(Error::InvalidQuery(format!("Unsupported SQL value: {:?}", val))),
        }
    }

    /// Convert ORDER BY clause
    fn convert_order_by(order_by: &[sql_ast::OrderByExpr]) -> Result<OrderBy> {
        if order_by.is_empty() {
            return Err(Error::InvalidQuery("Empty ORDER BY clause".into()));
        }
        if order_by.len() > 1 {
            return Err(Error::InvalidQuery("ORDER BY multiple columns not supported".into()));
        }

        let expr = &order_by[0];
        let attribute = Self::extract_attribute_name(&expr.expr)?;
        let ascending = expr.asc.unwrap_or(true);

        Ok(OrderBy {
            attribute,
            ascending,
        })
    }

    /// Convert INSERT statement
    fn convert_insert(_insert: &sql_ast::Insert) -> Result<InsertStatement> {
        // TODO: Implement INSERT parsing
        Err(Error::InvalidQuery("INSERT not yet implemented".into()))
    }

    /// Convert DELETE statement
    fn convert_delete(_delete: &sql_ast::Delete) -> Result<DeleteStatement> {
        // TODO: Implement DELETE parsing
        Err(Error::InvalidQuery("DELETE not yet implemented".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_select() {
        let sql = "SELECT * FROM users WHERE pk = 'user#123'";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Select(select) => {
                assert_eq!(select.table_name, "users");
                assert_eq!(select.index_name, None);
                assert_eq!(select.select_list, SelectList::All);

                let where_clause = select.where_clause.unwrap();
                assert_eq!(where_clause.conditions.len(), 1);
                assert_eq!(where_clause.conditions[0].attribute, "pk");
                assert_eq!(where_clause.conditions[0].operator, CompareOp::Equal);
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_select_with_index() {
        let sql = "SELECT * FROM users.email_index WHERE pk = 'org#acme'";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Select(select) => {
                assert_eq!(select.table_name, "users");
                assert_eq!(select.index_name, Some("email_index".to_string()));
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_select_with_attributes() {
        let sql = "SELECT name, age FROM users WHERE pk = 'user#123'";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Select(select) => {
                match select.select_list {
                    SelectList::Attributes(attrs) => {
                        assert_eq!(attrs, vec!["name", "age"]);
                    }
                    _ => panic!("Expected attribute list"),
                }
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_select_with_order_by() {
        let sql = "SELECT * FROM users WHERE pk = 'user#123' ORDER BY sk DESC";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Select(select) => {
                let order_by = select.order_by.unwrap();
                assert_eq!(order_by.attribute, "sk");
                assert_eq!(order_by.ascending, false);
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_select_with_in() {
        let sql = "SELECT * FROM users WHERE pk IN ('user#1', 'user#2')";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Select(select) => {
                let where_clause = select.where_clause.unwrap();
                assert_eq!(where_clause.conditions[0].operator, CompareOp::In);
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_parse_select_with_between() {
        let sql = "SELECT * FROM users WHERE pk = 'user#123' AND age BETWEEN 18 AND 65";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Select(select) => {
                let where_clause = select.where_clause.unwrap();
                assert_eq!(where_clause.conditions.len(), 2);

                // Find BETWEEN condition
                let between_cond = where_clause.conditions.iter()
                    .find(|c| c.operator == CompareOp::Between)
                    .unwrap();
                assert_eq!(between_cond.attribute, "age");
            }
            _ => panic!("Expected SELECT statement"),
        }
    }

    #[test]
    fn test_reject_join() {
        let sql = "SELECT * FROM users JOIN orders ON users.pk = orders.user_id";
        let result = PartiQLParser::parse(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("JOIN"));
    }

    #[test]
    fn test_reject_or() {
        let sql = "SELECT * FROM users WHERE pk = 'user#123' OR pk = 'user#456'";
        let result = PartiQLParser::parse(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("OR"));
    }

    #[test]
    fn test_reject_group_by() {
        let sql = "SELECT COUNT(*) FROM users GROUP BY status";
        let result = PartiQLParser::parse(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("GROUP BY"));
    }

    #[test]
    fn test_reject_too_long() {
        let sql = "SELECT * FROM users WHERE pk = '".to_string() + &"x".repeat(10000) + "'";
        let result = PartiQLParser::parse(&sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }
}

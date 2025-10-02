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

        // Special handling for INSERT with JSON map
        if sql.trim().to_uppercase().starts_with("INSERT") && sql.contains('{') {
            return Self::parse_insert_with_json_map(sql);
        }

        // Special handling for UPDATE with REMOVE clause
        if sql.trim().to_uppercase().starts_with("UPDATE") && sql.to_uppercase().contains(" REMOVE ") {
            return Self::parse_update_with_remove(sql);
        }

        // Pre-process SQL for DynamoDB compatibility
        // DynamoDB uses VALUE (singular) instead of VALUES (plural)
        let normalized_sql = sql.replace(" VALUE ", " VALUES ");

        // Parse SQL using sqlparser
        let dialect = GenericDialect {};
        let statements = SqlParser::parse_sql(&dialect, &normalized_sql).map_err(|e| {
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

    /// Special parser for INSERT statements with JSON map syntax
    /// Handles: INSERT INTO table VALUE {'pk': 'value', ...}
    fn parse_insert_with_json_map(sql: &str) -> Result<PartiQLStatement> {
        // Extract table name using regex or simple parsing
        let sql_upper = sql.to_uppercase();

        // Find INSERT INTO ... VALUE
        let into_idx = sql_upper.find("INTO ").ok_or_else(|| {
            Error::InvalidQuery("INSERT requires INTO clause".into())
        })?;
        let value_idx = sql_upper.find(" VALUE ").or_else(|| sql_upper.find(" VALUES ")).ok_or_else(|| {
            Error::InvalidQuery("INSERT requires VALUE clause".into())
        })?;

        // Extract table name (between INTO and VALUE)
        let table_part = sql[into_idx + 5..value_idx].trim();
        let table_name = table_part.split_whitespace().next().ok_or_else(|| {
            Error::InvalidQuery("Could not extract table name".into())
        })?.to_string();

        // Find the JSON map (between { and })
        let brace_start = sql.find('{').ok_or_else(|| {
            Error::InvalidQuery("Expected JSON map starting with {".into())
        })?;
        let brace_end = sql.rfind('}').ok_or_else(|| {
            Error::InvalidQuery("Expected JSON map ending with }".into())
        })?;

        if brace_end <= brace_start {
            return Err(Error::InvalidQuery("Invalid JSON map syntax".into()));
        }

        let json_str = &sql[brace_start..=brace_end];
        let value_map = Self::parse_json_string(json_str)?;

        Ok(PartiQLStatement::Insert(InsertStatement {
            table_name,
            value: value_map,
        }))
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
            Statement::Update {
                table,
                assignments,
                selection,
                ..
            } => {
                let update_stmt = Self::convert_update(table, assignments, selection, &[])?;
                Ok(PartiQLStatement::Update(update_stmt))
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
    fn convert_insert(insert: &sql_ast::Insert) -> Result<InsertStatement> {
        // Extract table name
        let table_name = match &insert.table_name {
            sql_ast::ObjectName(parts) => {
                if parts.is_empty() {
                    return Err(Error::InvalidQuery("Empty table name in INSERT".into()));
                }
                parts[0].value.clone()
            }
        };

        // Extract VALUE clause
        // INSERT INTO table VALUE {...} uses the source field
        let value_map = match &insert.source {
            Some(source) => {
                // source is a Box<Query>
                // We expect VALUES clause with a single row
                match &*source.body {
                    sql_ast::SetExpr::Values(values) => {
                        if values.rows.is_empty() {
                            return Err(Error::InvalidQuery("INSERT requires VALUE clause".into()));
                        }
                        if values.rows.len() > 1 {
                            return Err(Error::InvalidQuery(
                                "INSERT can only insert one item at a time".into(),
                            ));
                        }

                        // Parse the single row as a map literal
                        Self::parse_map_literal(&values.rows[0])?
                    }
                    _ => {
                        return Err(Error::InvalidQuery(
                            "INSERT only supports VALUE clause, not SELECT".into(),
                        ));
                    }
                }
            }
            None => return Err(Error::InvalidQuery("INSERT requires VALUE clause".into())),
        };

        Ok(InsertStatement {
            table_name,
            value: value_map,
        })
    }

    /// Parse a map literal from PartiQL VALUE clause
    /// Expects something like: {'pk': 'value', 'name': 'Alice', 'age': 30}
    fn parse_map_literal(row: &[sql_ast::Expr]) -> Result<SqlValue> {
        // Expect a single expression that's a struct/function call representing the map
        // sqlparser might parse {...} differently depending on version

        if row.len() != 1 {
            return Err(Error::InvalidQuery(format!(
                "Expected single map literal in INSERT, got {} expressions",
                row.len()
            )));
        }

        // Try to extract the map from the expression
        // This is a bit tricky - sqlparser might parse it as:
        // - A function call with name being empty and args being the key-value pairs
        // - A composite type
        // - Or something else

        match &row[0] {
            // Check if it's parsed as a function/type constructor
            sql_ast::Expr::Function(func) => {
                // Extract key-value pairs from function arguments
                Self::parse_function_as_map(func)
            }
            // Check if it's a JSON-like expression (newer sqlparser versions)
            sql_ast::Expr::JsonAccess { .. } | sql_ast::Expr::CompositeAccess { .. } => {
                Err(Error::InvalidQuery(
                    "Composite/JSON access expressions not yet supported for INSERT".into(),
                ))
            }
            // Try to handle as a string that can be JSON-parsed
            sql_ast::Expr::Value(sql_ast::Value::SingleQuotedString(s)) => {
                Self::parse_json_string(s)
            }
            _ => {
                Err(Error::InvalidQuery(format!(
                    "Unsupported INSERT VALUE format. Expression type: {:?}",
                    row[0]
                )))
            }
        }
    }

    /// Parse a function expression as a map (for constructs like MAP(...))
    fn parse_function_as_map(func: &sql_ast::Function) -> Result<SqlValue> {
        // If function name is empty or is "MAP", treat arguments as key-value pairs
        let mut map = std::collections::HashMap::new();

        // Access the args based on FunctionArguments enum
        let args_list = match &func.args {
            sql_ast::FunctionArguments::List(args) => &args.args,
            _ => return Err(Error::InvalidQuery("Expected argument list in function".into())),
        };

        for arg in args_list {
            match arg {
                sql_ast::FunctionArg::Unnamed(expr_wrapper) => {
                    let expr = match expr_wrapper {
                        sql_ast::FunctionArgExpr::Expr(e) => e,
                        _ => return Err(Error::InvalidQuery("Expected expression in argument".into())),
                    };

                    // Each arg might be a binary expression like 'key' = 'value'
                    if let sql_ast::Expr::BinaryOp { left, op, right } = expr {
                        if matches!(op, sql_ast::BinaryOperator::Eq) {
                            let key = Self::extract_string_literal(&**left)?;
                            let value = Self::convert_value(&**right)?;
                            map.insert(key, value);
                        } else {
                            return Err(Error::InvalidQuery(
                                "Expected key = value pairs in map".into(),
                            ));
                        }
                    } else {
                        return Err(Error::InvalidQuery(
                            "Expected key = value pairs in map".into(),
                        ));
                    }
                }
                _ => {
                    return Err(Error::InvalidQuery(
                        "Unsupported function argument in map".into(),
                    ));
                }
            }
        }

        Ok(SqlValue::Map(map))
    }

    /// Extract a string literal from an expression
    fn extract_string_literal(expr: &sql_ast::Expr) -> Result<String> {
        match expr {
            sql_ast::Expr::Value(sql_ast::Value::SingleQuotedString(s))
            | sql_ast::Expr::Value(sql_ast::Value::DoubleQuotedString(s)) => Ok(s.clone()),
            sql_ast::Expr::Identifier(ident) => Ok(ident.value.clone()),
            _ => Err(Error::InvalidQuery(format!(
                "Expected string literal, got: {:?}",
                expr
            ))),
        }
    }

    /// Parse a JSON string as SqlValue::Map
    fn parse_json_string(s: &str) -> Result<SqlValue> {
        // DynamoDB uses single quotes, but JSON requires double quotes
        // Convert single quotes to double quotes (simple approach - may need refinement)
        let json_normalized = s.replace('\'', "\"");

        let json_value: serde_json::Value = serde_json::from_str(&json_normalized).map_err(|e| {
            Error::InvalidQuery(format!("Failed to parse JSON: {}", e))
        })?;

        Self::json_to_sql_value(&json_value)
    }

    /// Convert serde_json::Value to SqlValue
    fn json_to_sql_value(value: &serde_json::Value) -> Result<SqlValue> {
        match value {
            serde_json::Value::Null => Ok(SqlValue::Null),
            serde_json::Value::Bool(b) => Ok(SqlValue::Boolean(*b)),
            serde_json::Value::Number(n) => Ok(SqlValue::Number(n.to_string())),
            serde_json::Value::String(s) => Ok(SqlValue::String(s.clone())),
            serde_json::Value::Array(arr) => {
                let items: Result<Vec<SqlValue>> = arr.iter().map(Self::json_to_sql_value).collect();
                Ok(SqlValue::List(items?))
            }
            serde_json::Value::Object(obj) => {
                let mut map = std::collections::HashMap::new();
                for (k, v) in obj {
                    map.insert(k.clone(), Self::json_to_sql_value(v)?);
                }
                Ok(SqlValue::Map(map))
            }
        }
    }

    /// Convert DELETE statement
    fn convert_delete(delete: &sql_ast::Delete) -> Result<DeleteStatement> {
        // Extract table name from DELETE FROM clause
        // FromTable enum has WithFromKeyword and WithoutKeyword variants
        let from_tables = match &delete.from {
            sql_ast::FromTable::WithFromKeyword(tables) => tables,
            sql_ast::FromTable::WithoutKeyword(tables) => tables,
        };

        if from_tables.is_empty() {
            return Err(Error::InvalidQuery("DELETE requires table name".into()));
        }

        // Same extraction logic as SELECT FROM clause
        let (table_name, index_name) = Self::extract_table_reference(&from_tables[0])?;

        // DELETE doesn't support index queries
        if index_name.is_some() {
            return Err(Error::InvalidQuery("DELETE does not support index syntax".into()));
        }

        // Extract WHERE clause (required for DELETE)
        let where_clause = match &delete.selection {
            Some(expr) => Self::convert_where_clause(expr)?,
            None => return Err(Error::InvalidQuery("DELETE requires WHERE clause".into())),
        };

        Ok(DeleteStatement {
            table_name,
            where_clause,
        })
    }

    /// Special parser for UPDATE statements with REMOVE clause
    /// Handles: UPDATE table SET ... REMOVE attr1, attr2 WHERE ...
    /// Also handles: UPDATE table REMOVE attr1, attr2 WHERE ... (REMOVE only)
    fn parse_update_with_remove(sql: &str) -> Result<PartiQLStatement> {
        let sql_upper = sql.to_uppercase();

        // Find REMOVE and WHERE positions
        let remove_idx = sql_upper.find(" REMOVE ").ok_or_else(|| {
            Error::InvalidQuery("Expected REMOVE clause".into())
        })?;
        let where_idx = sql_upper.find(" WHERE ").ok_or_else(|| {
            Error::InvalidQuery("UPDATE requires WHERE clause".into())
        })?;

        if where_idx <= remove_idx {
            return Err(Error::InvalidQuery("REMOVE must come before WHERE".into()));
        }

        // Extract REMOVE attributes (between REMOVE and WHERE)
        let remove_part = &sql[remove_idx + 8..where_idx].trim();
        let remove_attributes: Vec<String> = remove_part
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if remove_attributes.is_empty() {
            return Err(Error::InvalidQuery("REMOVE requires at least one attribute".into()));
        }

        // Check if there's a SET clause before REMOVE
        let before_remove = &sql[..remove_idx];
        let has_set = before_remove.to_uppercase().contains(" SET ");

        // Build modified SQL without REMOVE clause for sqlparser
        let sql_without_remove = if has_set {
            // Has SET: just remove REMOVE clause
            format!(
                "{} {}",
                &sql[..remove_idx].trim(),
                &sql[where_idx..].trim()
            )
        } else {
            // No SET: add dummy SET clause so sqlparser accepts it
            // We'll ignore the dummy assignment in convert_update
            format!(
                "{} SET __dummy__ = 0 {}",
                &sql[..remove_idx].trim(),
                &sql[where_idx..].trim()
            )
        };

        // Parse the modified SQL
        let dialect = GenericDialect {};
        let statements = SqlParser::parse_sql(&dialect, &sql_without_remove).map_err(|e| {
            Error::InvalidQuery(format!("Failed to parse UPDATE: {}", e))
        })?;

        if statements.is_empty() {
            return Err(Error::InvalidQuery("No statement found".into()));
        }

        // Extract UPDATE components
        match &statements[0] {
            Statement::Update {
                table,
                assignments,
                selection,
                ..
            } => {
                let update_stmt = Self::convert_update(table, assignments, selection, &remove_attributes)?;
                Ok(PartiQLStatement::Update(update_stmt))
            }
            _ => Err(Error::InvalidQuery("Expected UPDATE statement".into())),
        }
    }

    /// Convert UPDATE statement
    fn convert_update(
        table: &sql_ast::TableWithJoins,
        assignments: &[sql_ast::Assignment],
        selection: &Option<sql_ast::Expr>,
        remove_attributes: &[String],
    ) -> Result<UpdateStatement> {
        // Extract table name
        let (table_name, index_name) = Self::extract_table_reference(table)?;

        // UPDATE doesn't support index queries
        if index_name.is_some() {
            return Err(Error::InvalidQuery("UPDATE does not support index syntax".into()));
        }

        // Extract WHERE clause (required for UPDATE)
        let where_clause = match selection {
            Some(expr) => Self::convert_where_clause(expr)?,
            None => return Err(Error::InvalidQuery("UPDATE requires WHERE clause".into())),
        };

        // Convert SET assignments, filtering out dummy assignments
        let set_assignments: Vec<SetAssignment> = assignments
            .iter()
            .map(Self::convert_assignment)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|a| a.attribute != "__dummy__")
            .collect();

        Ok(UpdateStatement {
            table_name,
            where_clause,
            set_assignments,
            remove_attributes: remove_attributes.to_vec(),
        })
    }

    /// Convert a SET assignment
    fn convert_assignment(assignment: &sql_ast::Assignment) -> Result<SetAssignment> {
        // Extract attribute name from target
        let attribute = match &assignment.target {
            sql_ast::AssignmentTarget::ColumnName(name) => {
                match name {
                    sql_ast::ObjectName(parts) => {
                        if parts.is_empty() {
                            return Err(Error::InvalidQuery("Empty attribute name".into()));
                        }
                        parts[0].value.clone()
                    }
                }
            }
            _ => return Err(Error::InvalidQuery("Unsupported assignment target".into())),
        };

        // Convert value expression
        let value = Self::convert_set_value(&assignment.value)?;

        Ok(SetAssignment { attribute, value })
    }

    /// Convert SET value (literal or arithmetic expression)
    fn convert_set_value(expr: &sql_ast::Expr) -> Result<SetValue> {
        match expr {
            // Literal value
            sql_ast::Expr::Value(v) => {
                let sql_value = Self::convert_sql_value(v)?;
                Ok(SetValue::Literal(sql_value))
            }
            // Binary operation (attribute +/- value)
            sql_ast::Expr::BinaryOp { left, op, right } => {
                // Left side should be an identifier (attribute name)
                let attribute = match &**left {
                    sql_ast::Expr::Identifier(ident) => ident.value.clone(),
                    _ => return Err(Error::InvalidQuery(
                        "Arithmetic expression left side must be an attribute".into()
                    )),
                };

                // Right side should be a literal value
                let value = match &**right {
                    sql_ast::Expr::Value(v) => Self::convert_sql_value(v)?,
                    _ => return Err(Error::InvalidQuery(
                        "Arithmetic expression right side must be a literal".into()
                    )),
                };

                // Check operator
                match op {
                    sql_ast::BinaryOperator::Plus => Ok(SetValue::Add { attribute, value }),
                    sql_ast::BinaryOperator::Minus => Ok(SetValue::Subtract { attribute, value }),
                    _ => Err(Error::InvalidQuery(format!(
                        "Unsupported arithmetic operator in SET: {:?}",
                        op
                    ))),
                }
            }
            _ => Err(Error::InvalidQuery(format!(
                "Unsupported SET value expression: {:?}",
                expr
            ))),
        }
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

    // DELETE tests
    #[test]
    fn test_parse_delete_with_pk() {
        let sql = "DELETE FROM users WHERE pk = 'user#123'";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Delete(delete) => {
                assert_eq!(delete.table_name, "users");
                assert_eq!(delete.where_clause.conditions.len(), 1);
                assert_eq!(delete.where_clause.conditions[0].attribute, "pk");
                assert_eq!(delete.where_clause.conditions[0].operator, CompareOp::Equal);
            }
            _ => panic!("Expected DELETE statement"),
        }
    }

    #[test]
    fn test_parse_delete_with_pk_and_sk() {
        let sql = "DELETE FROM users WHERE pk = 'user#123' AND sk = 'profile'";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Delete(delete) => {
                assert_eq!(delete.table_name, "users");
                assert_eq!(delete.where_clause.conditions.len(), 2);

                let pk_cond = delete.where_clause.get_condition("pk").unwrap();
                assert_eq!(pk_cond.operator, CompareOp::Equal);

                let sk_cond = delete.where_clause.get_condition("sk").unwrap();
                assert_eq!(sk_cond.operator, CompareOp::Equal);
            }
            _ => panic!("Expected DELETE statement"),
        }
    }

    #[test]
    fn test_reject_delete_without_where() {
        let sql = "DELETE FROM users";
        let result = PartiQLParser::parse(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("WHERE"));
    }

    // INSERT tests
    #[test]
    fn test_parse_insert_simple() {
        let sql = "INSERT INTO users VALUE {'pk': 'user#123', 'name': 'Alice', 'age': 30}";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Insert(insert) => {
                assert_eq!(insert.table_name, "users");
                match &insert.value {
                    SqlValue::Map(map) => {
                        assert_eq!(map.len(), 3);
                        assert_eq!(map.get("pk"), Some(&SqlValue::String("user#123".to_string())));
                        assert_eq!(map.get("name"), Some(&SqlValue::String("Alice".to_string())));
                        assert_eq!(map.get("age"), Some(&SqlValue::Number("30".to_string())));
                    }
                    _ => panic!("Expected Map value"),
                }
            }
            _ => panic!("Expected INSERT statement"),
        }
    }

    #[test]
    fn test_parse_insert_with_sk() {
        let sql = "INSERT INTO users VALUE {'pk': 'user#123', 'sk': 'profile', 'email': 'alice@example.com'}";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Insert(insert) => {
                assert_eq!(insert.table_name, "users");
                match &insert.value {
                    SqlValue::Map(map) => {
                        assert!(map.contains_key("pk"));
                        assert!(map.contains_key("sk"));
                        assert_eq!(map.get("sk"), Some(&SqlValue::String("profile".to_string())));
                    }
                    _ => panic!("Expected Map value"),
                }
            }
            _ => panic!("Expected INSERT statement"),
        }
    }

    #[test]
    fn test_parse_insert_nested_values() {
        let sql = r#"INSERT INTO users VALUE {'pk': 'user#123', 'profile': {'name': 'Alice', 'age': 30}, 'tags': ['admin', 'active']}"#;
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Insert(insert) => {
                match &insert.value {
                    SqlValue::Map(map) => {
                        // Check nested map
                        match map.get("profile") {
                            Some(SqlValue::Map(profile)) => {
                                assert_eq!(profile.get("name"), Some(&SqlValue::String("Alice".to_string())));
                            }
                            _ => panic!("Expected nested map for profile"),
                        }

                        // Check list
                        match map.get("tags") {
                            Some(SqlValue::List(tags)) => {
                                assert_eq!(tags.len(), 2);
                            }
                            _ => panic!("Expected list for tags"),
                        }
                    }
                    _ => panic!("Expected Map value"),
                }
            }
            _ => panic!("Expected INSERT statement"),
        }
    }

    #[test]
    fn test_parse_insert_various_types() {
        let sql = "INSERT INTO items VALUE {'pk': 'item#1', 'price': 29.99, 'active': true, 'description': null}";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Insert(insert) => {
                match &insert.value {
                    SqlValue::Map(map) => {
                        // Number
                        match map.get("price") {
                            Some(SqlValue::Number(n)) => assert_eq!(n, "29.99"),
                            _ => panic!("Expected number for price"),
                        }

                        // Boolean
                        assert_eq!(map.get("active"), Some(&SqlValue::Boolean(true)));

                        // Null
                        assert_eq!(map.get("description"), Some(&SqlValue::Null));
                    }
                    _ => panic!("Expected Map value"),
                }
            }
            _ => panic!("Expected INSERT statement"),
        }
    }

    #[test]
    fn test_reject_insert_without_map() {
        let sql = "INSERT INTO users VALUE 'not a map'";
        let result = PartiQLParser::parse(sql);
        assert!(result.is_err());
    }

    // UPDATE tests
    #[test]
    fn test_parse_update_simple() {
        let sql = "UPDATE users SET name = 'Alice', age = 30 WHERE pk = 'user#123'";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Update(update) => {
                assert_eq!(update.table_name, "users");
                assert_eq!(update.set_assignments.len(), 2);
                assert_eq!(update.remove_attributes.len(), 0);

                // Check first assignment
                assert_eq!(update.set_assignments[0].attribute, "name");
                match &update.set_assignments[0].value {
                    SetValue::Literal(SqlValue::String(s)) => assert_eq!(s, "Alice"),
                    _ => panic!("Expected string literal"),
                }

                // Check WHERE clause
                assert!(update.where_clause.has_condition("pk"));
            }
            _ => panic!("Expected UPDATE statement"),
        }
    }

    #[test]
    fn test_parse_update_with_arithmetic() {
        let sql = "UPDATE users SET age = age + 1, count = count - 5 WHERE pk = 'user#123'";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Update(update) => {
                assert_eq!(update.set_assignments.len(), 2);

                // Check Add operation
                match &update.set_assignments[0].value {
                    SetValue::Add { attribute, value } => {
                        assert_eq!(attribute, "age");
                        match value {
                            SqlValue::Number(n) => assert_eq!(n, "1"),
                            _ => panic!("Expected number"),
                        }
                    }
                    _ => panic!("Expected Add operation"),
                }

                // Check Subtract operation
                match &update.set_assignments[1].value {
                    SetValue::Subtract { attribute, value } => {
                        assert_eq!(attribute, "count");
                        match value {
                            SqlValue::Number(n) => assert_eq!(n, "5"),
                            _ => panic!("Expected number"),
                        }
                    }
                    _ => panic!("Expected Subtract operation"),
                }
            }
            _ => panic!("Expected UPDATE statement"),
        }
    }

    #[test]
    fn test_parse_update_with_remove() {
        let sql = "UPDATE users SET name = 'Alice' REMOVE tags, metadata WHERE pk = 'user#123'";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Update(update) => {
                assert_eq!(update.table_name, "users");
                assert_eq!(update.set_assignments.len(), 1);
                assert_eq!(update.remove_attributes.len(), 2);
                assert_eq!(update.remove_attributes[0], "tags");
                assert_eq!(update.remove_attributes[1], "metadata");
            }
            _ => panic!("Expected UPDATE statement"),
        }
    }

    #[test]
    fn test_parse_update_remove_only() {
        let sql = "UPDATE users REMOVE tags, metadata WHERE pk = 'user#123' AND sk = 'profile'";
        let stmt = PartiQLParser::parse(sql).unwrap();

        match stmt {
            PartiQLStatement::Update(update) => {
                assert_eq!(update.set_assignments.len(), 0);
                assert_eq!(update.remove_attributes.len(), 2);
                assert_eq!(update.where_clause.conditions.len(), 2);
            }
            _ => panic!("Expected UPDATE statement"),
        }
    }

    #[test]
    fn test_reject_update_without_where() {
        let sql = "UPDATE users SET name = 'Alice'";
        let result = PartiQLParser::parse(sql);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("WHERE"));
    }
}

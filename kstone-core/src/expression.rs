/// Expression system for DynamoDB-style condition and update expressions
///
/// Supports:
/// - **Condition expressions**: =, <>, <, <=, >, >=, AND, OR, NOT, functions
/// - **Update expressions**: SET, REMOVE, ADD, DELETE actions
/// - Attribute paths and value placeholders
///
/// # Condition Expression Examples
///
/// ```ignore
/// // Parse: age > :min_age AND active = :is_active
/// let expr = ConditionExpressionParser::parse("age > :min_age AND active = :is_active")?;
///
/// // Evaluate with context
/// let context = ExpressionContext::new()
///     .with_value(":min_age", Value::number(18))
///     .with_value(":is_active", Value::Bool(true));
///
/// let evaluator = ExpressionEvaluator::new(&item, &context);
/// let result = evaluator.evaluate(&expr)?;
/// ```
///
/// # Update Expression Examples
///
/// ```ignore
/// // Parse: SET age = age + :inc, active = :val REMOVE temp
/// let actions = UpdateExpressionParser::parse("SET age = age + :inc REMOVE temp")?;
///
/// let context = ExpressionContext::new()
///     .with_value(":inc", Value::number(1));
///
/// let executor = UpdateExecutor::new(&context);
/// let updated_item = executor.execute(&item, &actions)?;
/// ```

use crate::{Item, Value, Error, Result};
use std::collections::HashMap;

/// Expression AST node
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    // Comparison operators
    Equal(Box<Expr>, Box<Expr>),
    NotEqual(Box<Expr>, Box<Expr>),
    LessThan(Box<Expr>, Box<Expr>),
    LessThanOrEqual(Box<Expr>, Box<Expr>),
    GreaterThan(Box<Expr>, Box<Expr>),
    GreaterThanOrEqual(Box<Expr>, Box<Expr>),

    // Logical operators
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),

    // Functions
    AttributeExists(String),
    AttributeNotExists(String),
    BeginsWith(Box<Expr>, Box<Expr>),

    // Operands
    AttributePath(String),
    ValuePlaceholder(String),
    Literal(Value),
}

/// Expression context with attribute values and names
#[derive(Debug, Clone, Default)]
pub struct ExpressionContext {
    /// Expression attribute values (:value1 -> Value)
    pub values: HashMap<String, Value>,
    /// Expression attribute names (#name -> actual_name)
    pub names: HashMap<String, String>,
}

impl ExpressionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_value(mut self, placeholder: impl Into<String>, value: Value) -> Self {
        self.values.insert(placeholder.into(), value);
        self
    }

    pub fn with_name(mut self, placeholder: impl Into<String>, name: impl Into<String>) -> Self {
        self.names.insert(placeholder.into(), name.into());
        self
    }
}

/// Expression evaluator
pub struct ExpressionEvaluator<'a> {
    item: &'a Item,
    context: &'a ExpressionContext,
}

impl<'a> ExpressionEvaluator<'a> {
    pub fn new(item: &'a Item, context: &'a ExpressionContext) -> Self {
        Self { item, context }
    }

    /// Evaluate expression against item
    pub fn evaluate(&self, expr: &Expr) -> Result<bool> {
        match expr {
            Expr::Equal(left, right) => {
                let l = self.resolve_value(left)?;
                let r = self.resolve_value(right)?;
                Ok(l == r)
            }
            Expr::NotEqual(left, right) => {
                let l = self.resolve_value(left)?;
                let r = self.resolve_value(right)?;
                Ok(l != r)
            }
            Expr::LessThan(left, right) => {
                let l = self.resolve_value(left)?;
                let r = self.resolve_value(right)?;
                Ok(self.compare_values(&l, &r)? < 0)
            }
            Expr::LessThanOrEqual(left, right) => {
                let l = self.resolve_value(left)?;
                let r = self.resolve_value(right)?;
                Ok(self.compare_values(&l, &r)? <= 0)
            }
            Expr::GreaterThan(left, right) => {
                let l = self.resolve_value(left)?;
                let r = self.resolve_value(right)?;
                Ok(self.compare_values(&l, &r)? > 0)
            }
            Expr::GreaterThanOrEqual(left, right) => {
                let l = self.resolve_value(left)?;
                let r = self.resolve_value(right)?;
                Ok(self.compare_values(&l, &r)? >= 0)
            }
            Expr::And(left, right) => {
                Ok(self.evaluate(left)? && self.evaluate(right)?)
            }
            Expr::Or(left, right) => {
                Ok(self.evaluate(left)? || self.evaluate(right)?)
            }
            Expr::Not(expr) => {
                Ok(!self.evaluate(expr)?)
            }
            Expr::AttributeExists(path) => {
                let attr_name = self.resolve_attribute_name(path);
                Ok(self.item.contains_key(&attr_name))
            }
            Expr::AttributeNotExists(path) => {
                let attr_name = self.resolve_attribute_name(path);
                Ok(!self.item.contains_key(&attr_name))
            }
            Expr::BeginsWith(path_expr, value_expr) => {
                let path_value = self.resolve_value(path_expr)?;
                let prefix_value = self.resolve_value(value_expr)?;

                match (&path_value, &prefix_value) {
                    (Value::S(s), Value::S(prefix)) => Ok(s.starts_with(prefix)),
                    (Value::B(b), Value::B(prefix)) => Ok(b.starts_with(prefix.as_ref())),
                    _ => Err(Error::InvalidExpression("begins_with requires string or binary operands".into()))
                }
            }
            Expr::AttributePath(_) | Expr::ValuePlaceholder(_) | Expr::Literal(_) => {
                Err(Error::InvalidExpression("Cannot evaluate operand as boolean expression".into()))
            }
        }
    }

    /// Resolve an expression to a value
    fn resolve_value(&self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::AttributePath(path) => {
                let attr_name = self.resolve_attribute_name(path);
                self.item.get(&attr_name)
                    .cloned()
                    .ok_or_else(|| Error::InvalidExpression(format!("Attribute '{}' not found", attr_name)))
            }
            Expr::ValuePlaceholder(placeholder) => {
                self.context.values.get(placeholder)
                    .cloned()
                    .ok_or_else(|| Error::InvalidExpression(format!("Value placeholder '{}' not found", placeholder)))
            }
            Expr::Literal(value) => Ok(value.clone()),
            _ => Err(Error::InvalidExpression("Cannot resolve non-value expression to value".into()))
        }
    }

    /// Resolve attribute name (handle #placeholder)
    fn resolve_attribute_name(&self, path: &str) -> String {
        if path.starts_with('#') {
            self.context.names.get(path)
                .cloned()
                .unwrap_or_else(|| path.to_string())
        } else {
            path.to_string()
        }
    }

    /// Compare two values (returns -1, 0, or 1)
    fn compare_values(&self, left: &Value, right: &Value) -> Result<i32> {
        match (left, right) {
            (Value::N(l), Value::N(r)) => {
                let l_num: f64 = l.parse().map_err(|_| Error::InvalidExpression("Invalid number".into()))?;
                let r_num: f64 = r.parse().map_err(|_| Error::InvalidExpression("Invalid number".into()))?;
                Ok(if l_num < r_num { -1 } else if l_num > r_num { 1 } else { 0 })
            }
            (Value::S(l), Value::S(r)) => {
                Ok(if l < r { -1 } else if l > r { 1 } else { 0 })
            }
            (Value::B(l), Value::B(r)) => {
                Ok(if l < r { -1 } else if l > r { 1 } else { 0 })
            }
            _ => Err(Error::InvalidExpression("Cannot compare different types".into()))
        }
    }
}

/// Update expression actions
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateAction {
    /// SET path = value (or SET path = path + value for increment)
    Set(String, UpdateValue),
    /// REMOVE path
    Remove(String),
    /// ADD path value (for numbers or sets)
    Add(String, UpdateValue),
    /// DELETE path value (for sets)
    Delete(String, UpdateValue),
}

/// Value in update expression (can be literal, placeholder, or arithmetic)
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateValue {
    /// Direct value (from placeholder or literal)
    Value(Value),
    /// Placeholder to be resolved
    Placeholder(String),
    /// Attribute path reference
    Path(String),
    /// Arithmetic: path + value or path - value
    Add(String, Box<UpdateValue>),
    Sub(String, Box<UpdateValue>),
}

/// Update executor
pub struct UpdateExecutor<'a> {
    context: &'a ExpressionContext,
}

impl<'a> UpdateExecutor<'a> {
    pub fn new(context: &'a ExpressionContext) -> Self {
        Self { context }
    }

    /// Execute update actions on an item
    pub fn execute(&self, item: &Item, actions: &[UpdateAction]) -> Result<Item> {
        let mut result = item.clone();

        for action in actions {
            match action {
                UpdateAction::Set(path, value) => {
                    let attr_name = self.resolve_attribute_name(path);
                    let resolved_value = self.resolve_update_value(value, &result)?;
                    result.insert(attr_name, resolved_value);
                }
                UpdateAction::Remove(path) => {
                    let attr_name = self.resolve_attribute_name(path);
                    result.remove(&attr_name);
                }
                UpdateAction::Add(path, value) => {
                    let attr_name = self.resolve_attribute_name(path);
                    let add_value = self.resolve_update_value(value, &result)?;

                    if let Some(existing) = result.get(&attr_name) {
                        // Add to existing number
                        match (existing, &add_value) {
                            (Value::N(n1), Value::N(n2)) => {
                                let num1: f64 = n1.parse().map_err(|_| Error::InvalidExpression("Invalid number".into()))?;
                                let num2: f64 = n2.parse().map_err(|_| Error::InvalidExpression("Invalid number".into()))?;
                                result.insert(attr_name, Value::number(num1 + num2));
                            }
                            _ => return Err(Error::InvalidExpression("ADD requires numbers".into()))
                        }
                    } else {
                        // Initialize with value
                        result.insert(attr_name, add_value);
                    }
                }
                UpdateAction::Delete(_path, _value) => {
                    // DELETE is for sets - not implementing full set support in this phase
                    return Err(Error::InvalidExpression("DELETE action not yet supported".into()));
                }
            }
        }

        Ok(result)
    }

    fn resolve_update_value(&self, value: &UpdateValue, item: &Item) -> Result<Value> {
        match value {
            UpdateValue::Value(v) => Ok(v.clone()),
            UpdateValue::Placeholder(placeholder) => {
                self.context.values.get(placeholder)
                    .cloned()
                    .ok_or_else(|| Error::InvalidExpression(format!("Placeholder {} not found", placeholder)))
            }
            UpdateValue::Path(path) => {
                let attr_name = self.resolve_attribute_name(path);
                item.get(&attr_name)
                    .cloned()
                    .ok_or_else(|| Error::InvalidExpression(format!("Attribute {} not found", attr_name)))
            }
            UpdateValue::Add(path, val) => {
                let attr_name = self.resolve_attribute_name(path);
                let base = item.get(&attr_name)
                    .cloned()
                    .ok_or_else(|| Error::InvalidExpression(format!("Attribute {} not found", attr_name)))?;
                let increment = self.resolve_update_value(val, item)?;

                match (&base, &increment) {
                    (Value::N(n1), Value::N(n2)) => {
                        let num1: f64 = n1.parse().map_err(|_| Error::InvalidExpression("Invalid number".into()))?;
                        let num2: f64 = n2.parse().map_err(|_| Error::InvalidExpression("Invalid number".into()))?;
                        Ok(Value::number(num1 + num2))
                    }
                    _ => Err(Error::InvalidExpression("Addition requires numbers".into()))
                }
            }
            UpdateValue::Sub(path, val) => {
                let attr_name = self.resolve_attribute_name(path);
                let base = item.get(&attr_name)
                    .cloned()
                    .ok_or_else(|| Error::InvalidExpression(format!("Attribute {} not found", attr_name)))?;
                let decrement = self.resolve_update_value(val, item)?;

                match (&base, &decrement) {
                    (Value::N(n1), Value::N(n2)) => {
                        let num1: f64 = n1.parse().map_err(|_| Error::InvalidExpression("Invalid number".into()))?;
                        let num2: f64 = n2.parse().map_err(|_| Error::InvalidExpression("Invalid number".into()))?;
                        Ok(Value::number(num1 - num2))
                    }
                    _ => Err(Error::InvalidExpression("Subtraction requires numbers".into()))
                }
            }
        }
    }

    fn resolve_attribute_name(&self, path: &str) -> String {
        if path.starts_with('#') {
            self.context.names.get(path)
                .cloned()
                .unwrap_or_else(|| path.to_string())
        } else {
            path.to_string()
        }
    }
}

/// Token for lexer
#[derive(Debug, Clone, PartialEq)]
enum Token {
    // Operators
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,

    // Keywords
    And,
    Or,
    Not,

    // Update keywords
    Set,
    Remove,
    Add,
    Delete,

    // Arithmetic
    Plus,
    Minus,

    // Functions
    AttributeExists,
    AttributeNotExists,
    BeginsWith,

    // Identifiers and literals
    Identifier(String),
    NamePlaceholder(String),    // #name
    ValuePlaceholder(String),   // :value

    // Delimiters
    LeftParen,
    RightParen,
    Comma,

    Eof,
}

/// Simple lexer
struct Lexer {
    input: Vec<char>,
    pos: usize,
}

impl Lexer {
    fn new(input: &str) -> Self {
        Self {
            input: input.chars().collect(),
            pos: 0,
        }
    }

    fn current(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_identifier(&mut self) -> String {
        let start = self.pos;
        while let Some(ch) = self.current() {
            if ch.is_alphanumeric() || ch == '_' {
                self.advance();
            } else {
                break;
            }
        }
        self.input[start..self.pos].iter().collect()
    }

    fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace();

        match self.current() {
            None => Ok(Token::Eof),
            Some('(') => {
                self.advance();
                Ok(Token::LeftParen)
            }
            Some(')') => {
                self.advance();
                Ok(Token::RightParen)
            }
            Some(',') => {
                self.advance();
                Ok(Token::Comma)
            }
            Some('+') => {
                self.advance();
                Ok(Token::Plus)
            }
            Some('-') => {
                self.advance();
                Ok(Token::Minus)
            }
            Some('=') => {
                self.advance();
                Ok(Token::Equal)
            }
            Some('<') => {
                self.advance();
                if self.current() == Some('>') {
                    self.advance();
                    Ok(Token::NotEqual)
                } else if self.current() == Some('=') {
                    self.advance();
                    Ok(Token::LessThanOrEqual)
                } else {
                    Ok(Token::LessThan)
                }
            }
            Some('>') => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Ok(Token::GreaterThanOrEqual)
                } else {
                    Ok(Token::GreaterThan)
                }
            }
            Some('#') => {
                self.advance();
                let name = self.read_identifier();
                Ok(Token::NamePlaceholder(format!("#{}", name)))
            }
            Some(':') => {
                self.advance();
                let name = self.read_identifier();
                Ok(Token::ValuePlaceholder(format!(":{}", name)))
            }
            Some(ch) if ch.is_alphabetic() => {
                let ident = self.read_identifier();
                match ident.to_uppercase().as_str() {
                    "AND" => Ok(Token::And),
                    "OR" => Ok(Token::Or),
                    "NOT" => Ok(Token::Not),
                    "SET" => Ok(Token::Set),
                    "REMOVE" => Ok(Token::Remove),
                    "ADD" => Ok(Token::Add),
                    "DELETE" => Ok(Token::Delete),
                    "ATTRIBUTE_EXISTS" => Ok(Token::AttributeExists),
                    "ATTRIBUTE_NOT_EXISTS" => Ok(Token::AttributeNotExists),
                    "BEGINS_WITH" => Ok(Token::BeginsWith),
                    _ => Ok(Token::Identifier(ident)),
                }
            }
            Some(ch) => Err(Error::InvalidExpression(format!("Unexpected character: {}", ch)))
        }
    }
}

/// Expression parser
pub struct ExpressionParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl ExpressionParser {
    /// Parse a condition expression string into AST
    pub fn parse(input: &str) -> Result<Expr> {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token()?;
            let is_eof = token == Token::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        let mut parser = Self { tokens, pos: 0 };
        parser.parse_expr()
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn expect(&mut self, expected: Token) -> Result<()> {
        if self.current() == &expected {
            self.advance();
            Ok(())
        } else {
            Err(Error::InvalidExpression(format!("Expected {:?}, got {:?}", expected, self.current())))
        }
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;

        while self.current() == &Token::Or {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_not()?;

        while self.current() == &Token::And {
            self.advance();
            let right = self.parse_not()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr> {
        if self.current() == &Token::Not {
            self.advance();
            let expr = self.parse_not()?;
            Ok(Expr::Not(Box::new(expr)))
        } else {
            self.parse_comparison()
        }
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let left = self.parse_operand()?;

        let op = self.current().clone();
        match op {
            Token::Equal => {
                self.advance();
                let right = self.parse_operand()?;
                Ok(Expr::Equal(Box::new(left), Box::new(right)))
            }
            Token::NotEqual => {
                self.advance();
                let right = self.parse_operand()?;
                Ok(Expr::NotEqual(Box::new(left), Box::new(right)))
            }
            Token::LessThan => {
                self.advance();
                let right = self.parse_operand()?;
                Ok(Expr::LessThan(Box::new(left), Box::new(right)))
            }
            Token::LessThanOrEqual => {
                self.advance();
                let right = self.parse_operand()?;
                Ok(Expr::LessThanOrEqual(Box::new(left), Box::new(right)))
            }
            Token::GreaterThan => {
                self.advance();
                let right = self.parse_operand()?;
                Ok(Expr::GreaterThan(Box::new(left), Box::new(right)))
            }
            Token::GreaterThanOrEqual => {
                self.advance();
                let right = self.parse_operand()?;
                Ok(Expr::GreaterThanOrEqual(Box::new(left), Box::new(right)))
            }
            _ => Ok(left) // Could be a function call that returns bool
        }
    }

    fn parse_operand(&mut self) -> Result<Expr> {
        match self.current().clone() {
            Token::LeftParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(Token::RightParen)?;
                Ok(expr)
            }
            Token::Identifier(name) => {
                self.advance();
                Ok(Expr::AttributePath(name))
            }
            Token::NamePlaceholder(name) => {
                self.advance();
                Ok(Expr::AttributePath(name))
            }
            Token::ValuePlaceholder(name) => {
                self.advance();
                Ok(Expr::ValuePlaceholder(name))
            }
            Token::AttributeExists => {
                self.advance();
                self.expect(Token::LeftParen)?;
                let path = match self.current().clone() {
                    Token::Identifier(p) => p,
                    Token::NamePlaceholder(p) => p,
                    _ => return Err(Error::InvalidExpression("Expected attribute path".into()))
                };
                self.advance();
                self.expect(Token::RightParen)?;
                Ok(Expr::AttributeExists(path))
            }
            Token::AttributeNotExists => {
                self.advance();
                self.expect(Token::LeftParen)?;
                let path = match self.current().clone() {
                    Token::Identifier(p) => p,
                    Token::NamePlaceholder(p) => p,
                    _ => return Err(Error::InvalidExpression("Expected attribute path".into()))
                };
                self.advance();
                self.expect(Token::RightParen)?;
                Ok(Expr::AttributeNotExists(path))
            }
            Token::BeginsWith => {
                self.advance();
                self.expect(Token::LeftParen)?;
                let path = self.parse_operand()?;
                self.expect(Token::Comma)?;
                let prefix = self.parse_operand()?;
                self.expect(Token::RightParen)?;
                Ok(Expr::BeginsWith(Box::new(path), Box::new(prefix)))
            }
            _ => Err(Error::InvalidExpression(format!("Unexpected token: {:?}", self.current())))
        }
    }
}

/// Update expression parser
pub struct UpdateExpressionParser {
    tokens: Vec<Token>,
    pos: usize,
}

impl UpdateExpressionParser {
    /// Parse an update expression string into actions
    /// Example: "SET age = age + :inc, active = :val REMOVE temp ADD score :points"
    pub fn parse(input: &str) -> Result<Vec<UpdateAction>> {
        let mut lexer = Lexer::new(input);
        let mut tokens = Vec::new();

        loop {
            let token = lexer.next_token()?;
            let is_eof = token == Token::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }

        let mut parser = Self { tokens, pos: 0 };
        parser.parse_update_expr()
    }

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn parse_update_expr(&mut self) -> Result<Vec<UpdateAction>> {
        let mut actions = Vec::new();

        while self.current() != &Token::Eof {
            match self.current() {
                Token::Set => {
                    self.advance();
                    actions.extend(self.parse_set_actions()?);
                }
                Token::Remove => {
                    self.advance();
                    actions.extend(self.parse_remove_actions()?);
                }
                Token::Add => {
                    self.advance();
                    actions.extend(self.parse_add_actions()?);
                }
                Token::Delete => {
                    self.advance();
                    actions.extend(self.parse_delete_actions()?);
                }
                _ => return Err(Error::InvalidExpression(format!("Unexpected token in update expression: {:?}", self.current())))
            }
        }

        Ok(actions)
    }

    fn parse_set_actions(&mut self) -> Result<Vec<UpdateAction>> {
        let mut actions = Vec::new();

        loop {
            // Get attribute path
            let path = match self.current() {
                Token::Identifier(p) => p.clone(),
                Token::NamePlaceholder(p) => p.clone(),
                _ => return Err(Error::InvalidExpression("Expected attribute path in SET".into()))
            };
            self.advance();

            // Expect =
            if self.current() != &Token::Equal {
                return Err(Error::InvalidExpression("Expected = in SET action".into()));
            }
            self.advance();

            // Parse value (could be path, placeholder, or arithmetic)
            let value = self.parse_update_value()?;

            actions.push(UpdateAction::Set(path, value));

            // Check for comma (more SET actions) or end
            if self.current() == &Token::Comma {
                self.advance();
            } else {
                break;
            }
        }

        Ok(actions)
    }

    fn parse_remove_actions(&mut self) -> Result<Vec<UpdateAction>> {
        let mut actions = Vec::new();

        loop {
            let path = match self.current() {
                Token::Identifier(p) => p.clone(),
                Token::NamePlaceholder(p) => p.clone(),
                _ => return Err(Error::InvalidExpression("Expected attribute path in REMOVE".into()))
            };
            self.advance();

            actions.push(UpdateAction::Remove(path));

            if self.current() == &Token::Comma {
                self.advance();
            } else {
                break;
            }
        }

        Ok(actions)
    }

    fn parse_add_actions(&mut self) -> Result<Vec<UpdateAction>> {
        let mut actions = Vec::new();

        loop {
            let path = match self.current() {
                Token::Identifier(p) => p.clone(),
                Token::NamePlaceholder(p) => p.clone(),
                _ => return Err(Error::InvalidExpression("Expected attribute path in ADD".into()))
            };
            self.advance();

            let value = self.parse_update_value()?;

            actions.push(UpdateAction::Add(path, value));

            if self.current() == &Token::Comma {
                self.advance();
            } else {
                break;
            }
        }

        Ok(actions)
    }

    fn parse_delete_actions(&mut self) -> Result<Vec<UpdateAction>> {
        let mut actions = Vec::new();

        loop {
            let path = match self.current() {
                Token::Identifier(p) => p.clone(),
                Token::NamePlaceholder(p) => p.clone(),
                _ => return Err(Error::InvalidExpression("Expected attribute path in DELETE".into()))
            };
            self.advance();

            let value = self.parse_update_value()?;

            actions.push(UpdateAction::Delete(path, value));

            if self.current() == &Token::Comma {
                self.advance();
            } else {
                break;
            }
        }

        Ok(actions)
    }

    fn parse_update_value(&mut self) -> Result<UpdateValue> {
        // First, get the base value (path or placeholder)
        let base = match self.current() {
            Token::Identifier(p) => {
                let path = p.clone();
                self.advance();

                // Check for arithmetic
                match self.current() {
                    Token::Plus => {
                        self.advance();
                        let operand = self.parse_update_value()?;
                        return Ok(UpdateValue::Add(path, Box::new(operand)));
                    }
                    Token::Minus => {
                        self.advance();
                        let operand = self.parse_update_value()?;
                        return Ok(UpdateValue::Sub(path, Box::new(operand)));
                    }
                    _ => UpdateValue::Path(path)
                }
            }
            Token::NamePlaceholder(p) => {
                let path = p.clone();
                self.advance();
                UpdateValue::Path(path)
            }
            Token::ValuePlaceholder(p) => {
                let placeholder = p.clone();
                self.advance();
                UpdateValue::Placeholder(placeholder)
            }
            _ => return Err(Error::InvalidExpression(format!("Unexpected token in update value: {:?}", self.current())))
        };

        Ok(base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_exists() {
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("Alice"));

        let expr = Expr::AttributeExists("name".to_string());
        let context = ExpressionContext::new();
        let evaluator = ExpressionEvaluator::new(&item, &context);

        assert!(evaluator.evaluate(&expr).unwrap());
    }

    #[test]
    fn test_attribute_not_exists() {
        let item = HashMap::new();

        let expr = Expr::AttributeNotExists("missing".to_string());
        let context = ExpressionContext::new();
        let evaluator = ExpressionEvaluator::new(&item, &context);

        assert!(evaluator.evaluate(&expr).unwrap());
    }

    #[test]
    fn test_equal_with_placeholder() {
        let mut item = HashMap::new();
        item.insert("age".to_string(), Value::number(30));

        let expr = Expr::Equal(
            Box::new(Expr::AttributePath("age".to_string())),
            Box::new(Expr::ValuePlaceholder(":val".to_string())),
        );

        let context = ExpressionContext::new()
            .with_value(":val", Value::number(30));

        let evaluator = ExpressionEvaluator::new(&item, &context);
        assert!(evaluator.evaluate(&expr).unwrap());
    }

    #[test]
    fn test_greater_than() {
        let mut item = HashMap::new();
        item.insert("score".to_string(), Value::number(85));

        let expr = Expr::GreaterThan(
            Box::new(Expr::AttributePath("score".to_string())),
            Box::new(Expr::Literal(Value::number(70))),
        );

        let context = ExpressionContext::new();
        let evaluator = ExpressionEvaluator::new(&item, &context);

        assert!(evaluator.evaluate(&expr).unwrap());
    }

    #[test]
    fn test_and_operator() {
        let mut item = HashMap::new();
        item.insert("age".to_string(), Value::number(25));
        item.insert("active".to_string(), Value::Bool(true));

        let expr = Expr::And(
            Box::new(Expr::GreaterThan(
                Box::new(Expr::AttributePath("age".to_string())),
                Box::new(Expr::Literal(Value::number(18))),
            )),
            Box::new(Expr::Equal(
                Box::new(Expr::AttributePath("active".to_string())),
                Box::new(Expr::Literal(Value::Bool(true))),
            )),
        );

        let context = ExpressionContext::new();
        let evaluator = ExpressionEvaluator::new(&item, &context);

        assert!(evaluator.evaluate(&expr).unwrap());
    }

    #[test]
    fn test_begins_with() {
        let mut item = HashMap::new();
        item.insert("email".to_string(), Value::string("alice@example.com"));

        let expr = Expr::BeginsWith(
            Box::new(Expr::AttributePath("email".to_string())),
            Box::new(Expr::Literal(Value::string("alice"))),
        );

        let context = ExpressionContext::new();
        let evaluator = ExpressionEvaluator::new(&item, &context);

        assert!(evaluator.evaluate(&expr).unwrap());
    }

    #[test]
    fn test_name_placeholder() {
        let mut item = HashMap::new();
        item.insert("user-name".to_string(), Value::string("Alice"));

        let expr = Expr::AttributeExists("#name".to_string());

        let context = ExpressionContext::new()
            .with_name("#name", "user-name");

        let evaluator = ExpressionEvaluator::new(&item, &context);
        assert!(evaluator.evaluate(&expr).unwrap());
    }

    // Parser tests
    #[test]
    fn test_parse_simple_comparison() {
        let expr = ExpressionParser::parse("age > :min_age").unwrap();
        assert!(matches!(expr, Expr::GreaterThan(_, _)));
    }

    #[test]
    fn test_parse_and_expression() {
        let expr = ExpressionParser::parse("age > :min AND active = :is_active").unwrap();
        assert!(matches!(expr, Expr::And(_, _)));
    }

    #[test]
    fn test_parse_or_expression() {
        let expr = ExpressionParser::parse("age < :young OR age > :old").unwrap();
        assert!(matches!(expr, Expr::Or(_, _)));
    }

    #[test]
    fn test_parse_not_expression() {
        let expr = ExpressionParser::parse("NOT active").unwrap();
        assert!(matches!(expr, Expr::Not(_)));
    }

    #[test]
    fn test_parse_attribute_exists() {
        let expr = ExpressionParser::parse("attribute_exists(email)").unwrap();
        assert!(matches!(expr, Expr::AttributeExists(_)));
    }

    #[test]
    fn test_parse_begins_with() {
        let expr = ExpressionParser::parse("begins_with(email, :prefix)").unwrap();
        assert!(matches!(expr, Expr::BeginsWith(_, _)));
    }

    #[test]
    fn test_parse_complex_expression() {
        let expr = ExpressionParser::parse(
            "(age >= :min_age AND age <= :max_age) OR attribute_exists(verified)"
        ).unwrap();
        assert!(matches!(expr, Expr::Or(_, _)));
    }

    #[test]
    fn test_parse_with_name_placeholder() {
        let expr = ExpressionParser::parse("attribute_exists(#name)").unwrap();
        match expr {
            Expr::AttributeExists(path) => assert_eq!(path, "#name"),
            _ => panic!("Expected AttributeExists"),
        }
    }

    #[test]
    fn test_parse_and_evaluate() {
        let mut item = HashMap::new();
        item.insert("age".to_string(), Value::number(25));
        item.insert("active".to_string(), Value::Bool(true));

        let expr = ExpressionParser::parse("age > :min AND active = :is_active").unwrap();

        let context = ExpressionContext::new()
            .with_value(":min", Value::number(18))
            .with_value(":is_active", Value::Bool(true));

        let evaluator = ExpressionEvaluator::new(&item, &context);
        assert!(evaluator.evaluate(&expr).unwrap());
    }

    // Update expression tests
    #[test]
    fn test_update_set_simple() {
        let mut item = HashMap::new();
        item.insert("age".to_string(), Value::number(25));

        let actions = UpdateExpressionParser::parse("SET age = :new_age").unwrap();
        assert_eq!(actions.len(), 1);

        let context = ExpressionContext::new()
            .with_value(":new_age", Value::number(30));

        let executor = UpdateExecutor::new(&context);
        let result = executor.execute(&item, &actions).unwrap();

        match result.get("age").unwrap() {
            Value::N(n) => assert_eq!(n, "30"),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_update_set_increment() {
        let mut item = HashMap::new();
        item.insert("score".to_string(), Value::number(100));

        let actions = UpdateExpressionParser::parse("SET score = score + :inc").unwrap();

        let context = ExpressionContext::new()
            .with_value(":inc", Value::number(50));

        let executor = UpdateExecutor::new(&context);
        let result = executor.execute(&item, &actions).unwrap();

        match result.get("score").unwrap() {
            Value::N(n) => assert_eq!(n, "150"),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_update_remove() {
        let mut item = HashMap::new();
        item.insert("temp".to_string(), Value::string("delete_me"));
        item.insert("keep".to_string(), Value::string("keep_me"));

        let actions = UpdateExpressionParser::parse("REMOVE temp").unwrap();

        let context = ExpressionContext::new();
        let executor = UpdateExecutor::new(&context);
        let result = executor.execute(&item, &actions).unwrap();

        assert!(!result.contains_key("temp"));
        assert!(result.contains_key("keep"));
    }

    #[test]
    fn test_update_add() {
        let mut item = HashMap::new();
        item.insert("count".to_string(), Value::number(10));

        let actions = UpdateExpressionParser::parse("ADD count :inc").unwrap();

        let context = ExpressionContext::new()
            .with_value(":inc", Value::number(5));

        let executor = UpdateExecutor::new(&context);
        let result = executor.execute(&item, &actions).unwrap();

        match result.get("count").unwrap() {
            Value::N(n) => assert_eq!(n, "15"),
            _ => panic!("Expected number"),
        }
    }

    #[test]
    fn test_update_multiple_actions() {
        let mut item = HashMap::new();
        item.insert("age".to_string(), Value::number(25));
        item.insert("temp".to_string(), Value::string("delete"));
        item.insert("score".to_string(), Value::number(100));

        let actions = UpdateExpressionParser::parse(
            "SET age = :new_age, active = :is_active REMOVE temp ADD score :bonus"
        ).unwrap();

        let context = ExpressionContext::new()
            .with_value(":new_age", Value::number(26))
            .with_value(":is_active", Value::Bool(true))
            .with_value(":bonus", Value::number(50));

        let executor = UpdateExecutor::new(&context);
        let result = executor.execute(&item, &actions).unwrap();

        match result.get("age").unwrap() {
            Value::N(n) => assert_eq!(n, "26"),
            _ => panic!("Expected number"),
        }
        assert_eq!(result.get("active").unwrap(), &Value::Bool(true));
        assert!(!result.contains_key("temp"));
        match result.get("score").unwrap() {
            Value::N(n) => assert_eq!(n, "150"),
            _ => panic!("Expected number"),
        }
    }
}

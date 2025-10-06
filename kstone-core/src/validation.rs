/// Schema validation for KeystoneDB
///
/// Provides attribute-level constraints and validation for items.

use crate::{Error, Result, Item, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Type constraint for an attribute
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttributeType {
    String,
    Number,
    Binary,
    Boolean,
    List,
    Map,
    Vector,      // VecF32
    Timestamp,   // Ts
}

impl AttributeType {
    /// Check if a value matches this type
    pub fn matches(&self, value: &Value) -> bool {
        match (self, value) {
            (AttributeType::String, Value::S(_)) => true,
            (AttributeType::Number, Value::N(_)) => true,
            (AttributeType::Binary, Value::B(_)) => true,
            (AttributeType::Boolean, Value::Bool(_)) => true,
            (AttributeType::List, Value::L(_)) => true,
            (AttributeType::Map, Value::M(_)) => true,
            (AttributeType::Vector, Value::VecF32(_)) => true,
            (AttributeType::Timestamp, Value::Ts(_)) => true,
            _ => false,
        }
    }
}

/// Value constraint for an attribute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValueConstraint {
    /// Minimum value (for numbers)
    MinValue(String),
    /// Maximum value (for numbers)
    MaxValue(String),
    /// Minimum length (for strings, lists)
    MinLength(usize),
    /// Maximum length (for strings, lists)
    MaxLength(usize),
    /// Must match regex pattern (for strings)
    Pattern(String),
    /// Must be one of these values
    Enum(Vec<Value>),
}

impl ValueConstraint {
    /// Validate a value against this constraint
    pub fn validate(&self, value: &Value) -> Result<()> {
        match self {
            ValueConstraint::MinValue(min) => {
                if let Value::N(n) = value {
                    let val: f64 = n.parse().map_err(|_| {
                        Error::InvalidArgument(format!("Invalid number: {}", n))
                    })?;
                    let min_val: f64 = min.parse().map_err(|_| {
                        Error::InvalidArgument(format!("Invalid min value: {}", min))
                    })?;
                    if val < min_val {
                        return Err(Error::InvalidArgument(
                            format!("Value {} is less than minimum {}", val, min_val)
                        ));
                    }
                }
                Ok(())
            }
            ValueConstraint::MaxValue(max) => {
                if let Value::N(n) = value {
                    let val: f64 = n.parse().map_err(|_| {
                        Error::InvalidArgument(format!("Invalid number: {}", n))
                    })?;
                    let max_val: f64 = max.parse().map_err(|_| {
                        Error::InvalidArgument(format!("Invalid max value: {}", max))
                    })?;
                    if val > max_val {
                        return Err(Error::InvalidArgument(
                            format!("Value {} exceeds maximum {}", val, max_val)
                        ));
                    }
                }
                Ok(())
            }
            ValueConstraint::MinLength(min) => {
                let len = match value {
                    Value::S(s) => s.len(),
                    Value::L(l) => l.len(),
                    _ => return Ok(()),
                };
                if len < *min {
                    return Err(Error::InvalidArgument(
                        format!("Length {} is less than minimum {}", len, min)
                    ));
                }
                Ok(())
            }
            ValueConstraint::MaxLength(max) => {
                let len = match value {
                    Value::S(s) => s.len(),
                    Value::L(l) => l.len(),
                    _ => return Ok(()),
                };
                if len > *max {
                    return Err(Error::InvalidArgument(
                        format!("Length {} exceeds maximum {}", len, max)
                    ));
                }
                Ok(())
            }
            ValueConstraint::Pattern(pattern) => {
                if let Value::S(s) = value {
                    let re = regex::Regex::new(pattern).map_err(|e| {
                        Error::InvalidArgument(format!("Invalid regex pattern: {}", e))
                    })?;
                    if !re.is_match(s) {
                        return Err(Error::InvalidArgument(
                            format!("Value '{}' does not match pattern '{}'", s, pattern)
                        ));
                    }
                }
                Ok(())
            }
            ValueConstraint::Enum(allowed_values) => {
                if !allowed_values.contains(value) {
                    return Err(Error::InvalidArgument(
                        format!("Value {:?} is not in allowed set", value)
                    ));
                }
                Ok(())
            }
        }
    }
}

/// Schema definition for an attribute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttributeSchema {
    /// Attribute name
    pub name: String,
    /// Type constraint
    pub attr_type: AttributeType,
    /// Whether this attribute is required
    pub required: bool,
    /// Value constraints
    pub constraints: Vec<ValueConstraint>,
    /// Description (for documentation)
    pub description: Option<String>,
}

impl AttributeSchema {
    /// Create a new attribute schema
    pub fn new(name: impl Into<String>, attr_type: AttributeType) -> Self {
        Self {
            name: name.into(),
            attr_type,
            required: false,
            constraints: Vec::new(),
            description: None,
        }
    }

    /// Mark this attribute as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Add a value constraint
    pub fn with_constraint(mut self, constraint: ValueConstraint) -> Self {
        self.constraints.push(constraint);
        self
    }

    /// Add a description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Validate a value against this schema
    pub fn validate(&self, value: Option<&Value>) -> Result<()> {
        match value {
            None => {
                if self.required {
                    return Err(Error::InvalidArgument(
                        format!("Required attribute '{}' is missing", self.name)
                    ));
                }
                Ok(())
            }
            Some(val) => {
                // Check type
                if !self.attr_type.matches(val) {
                    return Err(Error::InvalidArgument(
                        format!("Attribute '{}' has wrong type (expected {:?})", self.name, self.attr_type)
                    ));
                }

                // Check constraints
                for constraint in &self.constraints {
                    constraint.validate(val)?;
                }

                Ok(())
            }
        }
    }
}

/// Validator for items based on a schema
#[derive(Debug, Clone)]
pub struct Validator {
    schemas: HashMap<String, AttributeSchema>,
}

impl Validator {
    /// Create a new validator
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// Create validator from attribute schemas
    pub fn from_schemas(schemas: Vec<AttributeSchema>) -> Self {
        let mut validator = Self::new();
        for schema in schemas {
            validator.add_attribute(schema);
        }
        validator
    }

    /// Add an attribute schema
    pub fn add_attribute(&mut self, schema: AttributeSchema) {
        self.schemas.insert(schema.name.clone(), schema);
    }

    /// Validate an item against the schemas
    pub fn validate(&self, item: &Item) -> Result<()> {
        // Check all defined attributes
        for (attr_name, schema) in &self.schemas {
            schema.validate(item.get(attr_name))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attribute_type_matches() {
        assert!(AttributeType::String.matches(&Value::S("test".to_string())));
        assert!(AttributeType::Number.matches(&Value::N("42".to_string())));
        assert!(AttributeType::Boolean.matches(&Value::Bool(true)));
        assert!(!AttributeType::String.matches(&Value::N("42".to_string())));
    }

    #[test]
    fn test_required_attribute() {
        let schema = AttributeSchema::new("name", AttributeType::String).required();

        // Missing required attribute should fail
        assert!(schema.validate(None).is_err());

        // Present attribute should pass
        assert!(schema.validate(Some(&Value::S("Alice".to_string()))).is_ok());
    }

    #[test]
    fn test_type_constraint() {
        let schema = AttributeSchema::new("age", AttributeType::Number);

        // Correct type should pass
        assert!(schema.validate(Some(&Value::N("30".to_string()))).is_ok());

        // Wrong type should fail
        assert!(schema.validate(Some(&Value::S("thirty".to_string()))).is_err());
    }

    #[test]
    fn test_min_max_value() {
        let schema = AttributeSchema::new("age", AttributeType::Number)
            .with_constraint(ValueConstraint::MinValue("0".to_string()))
            .with_constraint(ValueConstraint::MaxValue("150".to_string()));

        assert!(schema.validate(Some(&Value::N("30".to_string()))).is_ok());
        assert!(schema.validate(Some(&Value::N("-1".to_string()))).is_err());
        assert!(schema.validate(Some(&Value::N("200".to_string()))).is_err());
    }

    #[test]
    fn test_length_constraints() {
        let schema = AttributeSchema::new("username", AttributeType::String)
            .with_constraint(ValueConstraint::MinLength(3))
            .with_constraint(ValueConstraint::MaxLength(20));

        assert!(schema.validate(Some(&Value::S("alice".to_string()))).is_ok());
        assert!(schema.validate(Some(&Value::S("ab".to_string()))).is_err());
        assert!(schema.validate(Some(&Value::S("a".repeat(25)))).is_err());
    }

    #[test]
    fn test_validator() {
        let mut validator = Validator::new();
        validator.add_attribute(
            AttributeSchema::new("name", AttributeType::String).required()
        );
        validator.add_attribute(
            AttributeSchema::new("age", AttributeType::Number)
                .with_constraint(ValueConstraint::MinValue("0".to_string()))
        );

        let mut valid_item = HashMap::new();
        valid_item.insert("name".to_string(), Value::S("Alice".to_string()));
        valid_item.insert("age".to_string(), Value::N("30".to_string()));

        assert!(validator.validate(&valid_item).is_ok());

        // Missing required field
        let mut invalid_item = HashMap::new();
        invalid_item.insert("age".to_string(), Value::N("30".to_string()));
        assert!(validator.validate(&invalid_item).is_err());
    }
}

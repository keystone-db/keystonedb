# Chapter 4: Data Model & Types

KeystoneDB follows Amazon DynamoDB's data model, providing a flexible yet structured approach to storing semi-structured data. This chapter explores the fundamental concepts of items, attributes, and values that form the foundation of KeystoneDB's storage system.

## Items and Attributes

In KeystoneDB, data is organized as **items**—the fundamental unit of storage. Each item is a collection of **attributes**, where an attribute is a name-value pair. Think of an item as analogous to a row in a relational database, but with much more flexibility.

An item in KeystoneDB is represented internally as a Rust `HashMap<String, Value>`:

```rust
pub type Item = HashMap<String, Value>;
```

This simple type definition belies the power of the model: each item can have a different set of attributes, and attributes can be added or removed without altering a schema. There is no fixed schema that all items must conform to—this is the essence of a schema-less database.

### Example Item

Here's what a typical user item might look like:

```rust
{
    "userId": "user#12345",
    "name": "Alice Johnson",
    "email": "alice@example.com",
    "age": 30,
    "active": true,
    "preferences": {
        "theme": "dark",
        "notifications": true
    },
    "tags": ["premium", "verified"]
}
```

Each attribute has a name (like `"userId"`, `"name"`, `"email"`) and a typed value. The type system is what gives KeystoneDB its flexibility while maintaining data integrity.

## The Value Type System

KeystoneDB supports nine distinct value types, each designed to handle different kinds of data efficiently. The `Value` enum in the core types module defines all supported types:

```rust
pub enum Value {
    N(String),              // Number (arbitrary precision)
    S(String),              // String
    B(Bytes),               // Binary
    Bool(bool),             // Boolean
    Null,                   // Null
    L(Vec<Value>),          // List
    M(HashMap<String, Value>), // Map (nested attributes)
    VecF32(Vec<f32>),       // Vector of f32 (for embeddings)
    Ts(i64),                // Timestamp (milliseconds since epoch)
}
```

Let's explore each type in detail.

### String (S)

The string type represents UTF-8 encoded text. Strings are one of the most common value types and are used for names, descriptions, identifiers, and any textual data.

```rust
let name = Value::string("Alice Johnson");
let email = Value::S("alice@example.com".to_string());
```

**Use cases:**
- User names, email addresses, phone numbers
- Product descriptions, titles
- JSON or XML documents stored as text
- Any UTF-8 encoded textual data

**Implementation notes:**
- Stored as Rust `String` (heap-allocated)
- No practical size limit beyond available memory
- Efficient for small to medium strings
- For very large text (MB+), consider compression or storing as binary

### Number (N)

Numbers in KeystoneDB are stored as strings, not native numeric types. This design choice provides arbitrary precision and avoids floating-point arithmetic issues.

```rust
let age = Value::number(30);
let price = Value::N("19.99".to_string());
let huge = Value::number(12345678901234567890i128);
```

**Why strings for numbers?**
1. **Arbitrary precision**: Can represent any integer or decimal without overflow
2. **Cross-platform consistency**: Avoids floating-point representation differences
3. **DynamoDB compatibility**: Matches DynamoDB's number representation
4. **Human readable**: Easy to inspect in logs and debugging

**Use cases:**
- Currency amounts (no floating-point rounding errors)
- Counters, quantities, scores
- Very large integers (beyond i64)
- Scientific calculations requiring precision

**Important considerations:**
- Arithmetic requires parsing to numeric types first
- Comparison is lexicographic unless parsed
- Consider `Ts` (timestamp) for time values

### Binary (B)

Binary data is stored as raw bytes, useful for non-textual data.

```rust
use bytes::Bytes;

let image_data = Value::binary(vec![0xFF, 0xD8, 0xFF, 0xE0]);
let encrypted = Value::B(Bytes::from(vec![0x01, 0x02, 0x03]));
```

**Use cases:**
- Images, audio, video thumbnails
- Encrypted data
- Cryptographic keys or hashes
- Serialized protocol buffers
- Compressed data

**Implementation notes:**
- Stored as `bytes::Bytes` (zero-copy, reference-counted)
- Efficient for cloning (reference count increment)
- No encoding/decoding overhead
- Consider external blob storage for very large binaries (>1MB)

### Boolean (Bool)

Simple true/false values for flags and states.

```rust
let is_active = Value::Bool(true);
let verified = Value::Bool(false);
```

**Use cases:**
- Feature flags
- User preferences (on/off settings)
- Status indicators
- Permissions and access control

### Null

Represents the explicit absence of a value, different from a missing attribute.

```rust
let middle_name = Value::Null;
```

**Important distinction:**
- `Null` value: Attribute exists with explicit null
- Missing attribute: Attribute key doesn't exist in the item
- Use `attribute_not_exists()` to check for missing attributes
- Use `= NULL` comparison for null values

**Use cases:**
- Optional fields that may be set later
- Clearing a value without removing the attribute
- Compatibility with JSON null values

### List (L)

Ordered collections of values. Lists can contain values of different types.

```rust
// Homogeneous list
let tags = Value::L(vec![
    Value::string("rust"),
    Value::string("database"),
    Value::string("lsm-tree"),
]);

// Heterogeneous list
let mixed = Value::L(vec![
    Value::number(42),
    Value::string("hello"),
    Value::Bool(true),
]);
```

**Use cases:**
- Tags, categories, labels
- Historical values or audit trails
- Ordered sequences
- JSON array compatibility

**Characteristics:**
- Ordered: Elements maintain insertion order
- Mutable: Can be modified in updates
- Indexed: Elements are accessed by position
- Nestable: Can contain other lists or maps

**Operations:**
```rust
// Accessing lists
if let Value::L(tags) = &item.get("tags").unwrap() {
    for tag in tags {
        println!("{:?}", tag);
    }
}
```

### Map (M)

Nested attribute collections that allow hierarchical data structures.

```rust
let address = Value::M(HashMap::from([
    ("street".to_string(), Value::string("123 Main St")),
    ("city".to_string(), Value::string("Springfield")),
    ("zip".to_string(), Value::string("12345")),
]));
```

**Use cases:**
- Nested objects (address, contact info)
- Configuration objects
- Metadata collections
- JSON object compatibility

**Characteristics:**
- Unordered: Key order is not guaranteed
- Nestable: Can contain other maps or lists
- Flexible: Different items can have different nested structures
- Type-safe: Values are still typed

**Complex nested structures:**
```rust
let user = Value::M(HashMap::from([
    ("name".to_string(), Value::string("Alice")),
    ("contact".to_string(), Value::M(HashMap::from([
        ("email".to_string(), Value::string("alice@example.com")),
        ("phones".to_string(), Value::L(vec![
            Value::string("+1-555-1234"),
            Value::string("+1-555-5678"),
        ])),
    ]))),
    ("preferences".to_string(), Value::M(HashMap::from([
        ("theme".to_string(), Value::string("dark")),
        ("language".to_string(), Value::string("en")),
    ]))),
]));
```

**Accessing nested values:**
```rust
// Navigate nested maps
if let Value::M(user_map) = &user {
    if let Some(Value::M(contact)) = user_map.get("contact") {
        if let Some(Value::S(email)) = contact.get("email") {
            println!("Email: {}", email);
        }
    }
}
```

### Vector of Float32 (VecF32)

A specialized type for machine learning embeddings and vector similarity search.

```rust
// Word embedding
let embedding = Value::vector(vec![0.1, -0.2, 0.3, 0.4, -0.1]);

// Image feature vector (512 dimensions)
let image_features = Value::VecF32(vec![0.0; 512]);
```

**Use cases:**
- Word embeddings (Word2Vec, GloVe)
- Sentence embeddings (BERT, Sentence-BERT)
- Image embeddings (ResNet, VGG)
- Audio fingerprints
- Recommendation system features

**Why VecF32?**
- **Compact**: f32 is 4 bytes vs f64's 8 bytes
- **Fast**: Hardware-optimized operations
- **Standard**: Most ML frameworks use f32
- **Sufficient precision**: For similarity metrics

**Future enhancements:**
- Vector similarity search (cosine, euclidean)
- Approximate nearest neighbor (ANN) indexing
- Integration with vector databases

### Timestamp (Ts)

Represents points in time as milliseconds since the Unix epoch (January 1, 1970, 00:00:00 UTC).

```rust
use std::time::{SystemTime, UNIX_EPOCH};

// Current time
let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_millis() as i64;

let created_at = Value::timestamp(now);
let expires_at = Value::Ts(now + 3600_000); // 1 hour from now
```

**Why milliseconds?**
- **Precision**: Sufficient for most applications
- **Compact**: i64 covers ~292 million years
- **Standard**: JavaScript Date.now() compatibility
- **Time zones**: UTC avoids timezone issues

**Use cases:**
- Creation timestamps (`created_at`)
- Modification timestamps (`updated_at`)
- TTL (Time To Live) values
- Event timestamps in audit logs
- Expiration times for sessions or caches

**TTL integration:**
When configured as the TTL attribute, timestamps enable automatic item expiration:

```rust
let schema = TableSchema::new().with_ttl("expiresAt");
let db = Database::create_with_schema(path, schema)?;

// Item expires in 1 hour
db.put(b"session#abc", ItemBuilder::new()
    .string("userId", "user#123")
    .timestamp("expiresAt", now + 3600_000)
    .build())?;
```

## Building Items: The ItemBuilder API

KeystoneDB provides a fluent builder API for constructing items easily:

```rust
use kstone_api::ItemBuilder;

let item = ItemBuilder::new()
    .string("name", "Alice Johnson")
    .number("age", 30)
    .bool("active", true)
    .build();
```

### Builder Methods

**String attributes:**
```rust
builder.string("key", "value")
builder.string("email", email_var)
```

**Number attributes:**
```rust
builder.number("age", 30)
builder.number("price", 19.99)
builder.number("count", count_var)
```

**Boolean attributes:**
```rust
builder.bool("active", true)
builder.bool("verified", is_verified)
```

**Complex example:**
```rust
let product = ItemBuilder::new()
    .string("productId", "prod#12345")
    .string("name", "Mechanical Keyboard")
    .string("category", "electronics")
    .number("price", 129.99)
    .number("stock", 45)
    .bool("available", true)
    .build();
```

### Adding Complex Types

For lists, maps, vectors, and timestamps, you need to insert them directly:

```rust
use std::collections::HashMap;

let mut item = ItemBuilder::new()
    .string("userId", "user#123")
    .string("name", "Alice")
    .build();

// Add a list
item.insert("tags".to_string(), Value::L(vec![
    Value::string("premium"),
    Value::string("verified"),
]));

// Add a nested map
item.insert("address".to_string(), Value::M(HashMap::from([
    ("city".to_string(), Value::string("New York")),
    ("zip".to_string(), Value::string("10001")),
])));

// Add a timestamp
let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_millis() as i64;
item.insert("createdAt".to_string(), Value::Ts(now));

// Add an embedding vector
item.insert("embedding".to_string(), Value::VecF32(vec![0.1, 0.2, 0.3]));
```

## Type Coercion and Conversion

### Working with Number Values

Numbers are stored as strings but need conversion for arithmetic:

```rust
// Reading a number
let age_value = item.get("age").unwrap();
if let Value::N(age_str) = age_value {
    let age: i32 = age_str.parse().unwrap();
    println!("Age: {}", age);
}

// Pattern matching for safety
match item.get("price") {
    Some(Value::N(price_str)) => {
        match price_str.parse::<f64>() {
            Ok(price) => println!("Price: ${:.2}", price),
            Err(_) => println!("Invalid price format"),
        }
    }
    _ => println!("Price not found or not a number"),
}
```

### Working with String Values

The `Value` type provides helper methods:

```rust
// as_string() for String values
if let Some(name) = item.get("name").and_then(|v| v.as_string()) {
    println!("Name: {}", name);
}

// For binary data
if let Some(data) = item.get("data").and_then(|v| {
    match v {
        Value::B(bytes) => Some(bytes),
        _ => None,
    }
}) {
    println!("Data length: {}", data.len());
}
```

### Type Safety Patterns

**Extracting typed values:**
```rust
fn get_user_age(item: &Item) -> Option<i32> {
    item.get("age")
        .and_then(|v| match v {
            Value::N(s) => s.parse().ok(),
            _ => None,
        })
}

fn get_tags(item: &Item) -> Vec<String> {
    item.get("tags")
        .and_then(|v| match v {
            Value::L(list) => Some(
                list.iter()
                    .filter_map(|v| v.as_string())
                    .map(String::from)
                    .collect()
            ),
            _ => None,
        })
        .unwrap_or_default()
}
```

## Nested Structures and Data Modeling

### Document-Style Data

KeystoneDB excels at storing document-style data with nested structures:

```rust
// E-commerce order
let order = ItemBuilder::new()
    .string("orderId", "order#789")
    .string("userId", "user#123")
    .number("total", 299.97)
    .build();

// Add nested line items
let line_items = Value::L(vec![
    Value::M(HashMap::from([
        ("productId".to_string(), Value::string("prod#1")),
        ("name".to_string(), Value::string("Widget")),
        ("quantity".to_string(), Value::number(2)),
        ("price".to_string(), Value::number(49.99)),
    ])),
    Value::M(HashMap::from([
        ("productId".to_string(), Value::string("prod#2")),
        ("name".to_string(), Value::string("Gadget")),
        ("quantity".to_string(), Value::number(1)),
        ("price".to_string(), Value::number(199.99)),
    ])),
]);

order.insert("lineItems".to_string(), line_items);

// Add shipping address
let shipping_address = Value::M(HashMap::from([
    ("name".to_string(), Value::string("Alice Johnson")),
    ("street".to_string(), Value::string("123 Main St")),
    ("city".to_string(), Value::string("Springfield")),
    ("state".to_string(), Value::string("IL")),
    ("zip".to_string(), Value::string("62701")),
    ("country".to_string(), Value::string("USA")),
]));

order.insert("shippingAddress".to_string(), shipping_address);
```

### Event Sourcing Pattern

Using lists to maintain event history:

```rust
let account = ItemBuilder::new()
    .string("accountId", "acct#456")
    .number("balance", 1000)
    .build();

// Add transaction history
let transactions = Value::L(vec![
    Value::M(HashMap::from([
        ("type".to_string(), Value::string("deposit")),
        ("amount".to_string(), Value::number(1000)),
        ("timestamp".to_string(), Value::Ts(now - 86400000)), // Yesterday
    ])),
    Value::M(HashMap::from([
        ("type".to_string(), Value::string("withdrawal")),
        ("amount".to_string(), Value::number(50)),
        ("timestamp".to_string(), Value::Ts(now - 3600000)), // 1 hour ago
    ])),
]);

account.insert("transactions".to_string(), transactions);
```

### Hierarchical Categories

Maps for representing tree structures:

```rust
let category_tree = Value::M(HashMap::from([
    ("electronics".to_string(), Value::M(HashMap::from([
        ("computers".to_string(), Value::L(vec![
            Value::string("laptops"),
            Value::string("desktops"),
            Value::string("tablets"),
        ])),
        ("phones".to_string(), Value::L(vec![
            Value::string("smartphones"),
            Value::string("feature-phones"),
        ])),
    ]))),
    ("books".to_string(), Value::M(HashMap::from([
        ("fiction".to_string(), Value::L(vec![
            Value::string("sci-fi"),
            Value::string("mystery"),
        ])),
        ("non-fiction".to_string(), Value::L(vec![
            Value::string("biography"),
            Value::string("history"),
        ])),
    ]))),
]));
```

## Best Practices for Data Modeling

### Choose Appropriate Types

1. **Use Number for numeric values**, even if they're identifiers
   ```rust
   // Good
   .number("userId", 12345)

   // Better for string IDs
   .string("userId", "user#12345")
   ```

2. **Use Timestamp for time values** instead of Number
   ```rust
   // Good
   .timestamp("createdAt", now_millis)

   // Avoid
   .number("createdAt", now_millis)
   ```

3. **Use Binary for non-textual data**
   ```rust
   // Good
   .binary(encrypted_data)

   // Avoid (base64 encoding overhead)
   .string(base64_encode(encrypted_data))
   ```

### Normalize vs. Denormalize

**Single-table design** (denormalized):
```rust
// User with embedded address
let user = ItemBuilder::new()
    .string("pk", "user#123")
    .string("name", "Alice")
    .string("city", "New York")
    .string("state", "NY")
    .build();
```

**Multi-item design** (normalized):
```rust
// User record
let user = ItemBuilder::new()
    .string("pk", "user#123")
    .string("name", "Alice")
    .build();

// Address record
let address = ItemBuilder::new()
    .string("pk", "user#123")
    .string("sk", "address")
    .string("city", "New York")
    .string("state", "NY")
    .build();
```

**Trade-offs:**
- Denormalized: Faster reads, larger items, potential duplication
- Normalized: Smaller items, more reads, consistency maintenance

### Size Considerations

While KeystoneDB has no hard item size limit, consider these practical guidelines:

1. **Keep items under 400KB** for optimal performance
2. **Use Binary for large blobs**, or store externally
3. **Limit list lengths** to hundreds, not thousands
4. **Deep nesting** (>5 levels) impacts deserialization performance

### Attribute Naming Conventions

Consistent naming improves maintainability:

```rust
// PascalCase for type prefixes
.string("userId", "user#123")      // User ID
.string("orderId", "order#456")    // Order ID

// camelCase for attributes
.string("firstName", "Alice")
.string("lastName", "Johnson")

// Timestamps with suffix
.timestamp("createdAt", now)
.timestamp("updatedAt", now)
.timestamp("expiresAt", future)

// Boolean with "is" or "has" prefix
.bool("isActive", true)
.bool("hasVerifiedEmail", true)
```

## Summary

KeystoneDB's type system provides the flexibility of schema-less storage with the safety of typed values. The nine value types—String, Number, Binary, Boolean, Null, List, Map, VecF32, and Timestamp—cover the full spectrum of application data needs, from simple scalars to complex nested documents.

Key takeaways:

1. **Items are flexible**: No fixed schema, attributes can vary between items
2. **Values are typed**: Type safety prevents data corruption
3. **Numbers as strings**: Arbitrary precision, DynamoDB compatibility
4. **Nesting is powerful**: Lists and Maps enable complex hierarchical data
5. **Modern types**: VecF32 for ML, Timestamp for precise time tracking
6. **ItemBuilder is convenient**: Fluent API for common value types

In the next chapter, we'll explore how keys work in KeystoneDB, including the critical concepts of partition keys, sort keys, and the 256-stripe architecture that enables horizontal scalability.

# Chapter 8: CRUD Operations

CRUD (Create, Read, Update, Delete) operations form the foundation of any database system. KeystoneDB provides a simple yet powerful API for these basic operations, modeled after DynamoDB's familiar interface. This chapter explores how to perform CRUD operations efficiently using both simple partition keys and composite keys with sort keys.

## Understanding Keys in KeystoneDB

Before diving into CRUD operations, it's essential to understand how KeystoneDB handles keys. Every item in KeystoneDB must have a **partition key (PK)**, and optionally a **sort key (SK)**. These keys determine:

1. **Data distribution**: The partition key determines which of the 256 internal stripes the item belongs to
2. **Item uniqueness**: The combination of PK and SK uniquely identifies an item
3. **Query capabilities**: Queries require a partition key and can filter by sort key

### Key Encoding

Keys in KeystoneDB are byte arrays (`&[u8]`), giving you complete flexibility in how you structure your keys:

```rust
// Simple string keys
b"user#123"

// Composite keys with delimiters
b"org#acme#user#alice"

// Date-based keys for time-series data
b"sensor#temp#2024-01-15"

// Binary keys (any byte sequence)
&[0x01, 0x02, 0x03, 0x04]
```

**Best Practice**: Use hierarchical key patterns with delimiters (like `#` or `::`) to create logical namespaces:
- `user#123` - User records
- `user#123#post#456` - User's posts
- `org#acme#dept#eng` - Organizational hierarchy

## Put Operation

The `put` operation writes an item to the database. If an item with the same key already exists, it will be completely replaced.

### Basic Put (Partition Key Only)

```rust
use kstone_api::{Database, ItemBuilder};

let db = Database::create("mydb.keystone")?;

// Create an item using ItemBuilder
let item = ItemBuilder::new()
    .string("name", "Alice Johnson")
    .number("age", 30)
    .bool("active", true)
    .string("email", "alice@example.com")
    .build();

// Put the item with a simple partition key
db.put(b"user#alice", item)?;
```

The `ItemBuilder` provides a fluent API for constructing items with different value types:

- `.string(key, value)` - String attributes
- `.number(key, value)` - Numeric attributes (stored as strings for precision)
- `.bool(key, value)` - Boolean attributes

### Put with Sort Key

For more complex data models, you can use composite keys with both partition and sort keys:

```rust
// Store multiple posts for a user
for i in 1..=5 {
    let post = ItemBuilder::new()
        .string("title", format!("Post #{}", i))
        .string("content", format!("Content for post {}", i))
        .number("timestamp", 1704067200 + i * 3600)
        .build();

    db.put_with_sk(
        b"user#alice",              // Partition key
        format!("post#{}", i).as_bytes(),  // Sort key
        post
    )?;
}
```

This creates 5 separate items:
- `user#alice / post#1`
- `user#alice / post#2`
- `user#alice / post#3`
- `user#alice / post#4`
- `user#alice / post#5`

All items share the same partition key (`user#alice`), so they're stored in the same LSM stripe and can be queried efficiently together.

### Working with Complex Data Types

KeystoneDB supports rich data types beyond simple strings and numbers:

```rust
use kstone_core::Value;
use std::collections::HashMap;

// Create a complex item with nested structures
let mut item = HashMap::new();

// Simple values
item.insert("name".to_string(), Value::string("Bob"));
item.insert("age".to_string(), Value::number(25));
item.insert("active".to_string(), Value::Bool(true));

// List (array) of values
let hobbies = Value::L(vec![
    Value::string("reading"),
    Value::string("coding"),
    Value::string("hiking"),
]);
item.insert("hobbies".to_string(), hobbies);

// Map (nested object)
let mut address = HashMap::new();
address.insert("street".to_string(), Value::string("123 Main St"));
address.insert("city".to_string(), Value::string("San Francisco"));
address.insert("zip".to_string(), Value::string("94102"));
item.insert("address".to_string(), Value::M(address));

// Binary data
let profile_image = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG header
item.insert("avatar".to_string(), Value::B(profile_image.into()));

// Timestamp (milliseconds since epoch)
item.insert("created_at".to_string(), Value::Ts(1704067200000));

db.put(b"user#bob", item)?;
```

### Put Semantics

Important characteristics of the `put` operation:

1. **Upsert behavior**: Put always succeeds, creating a new item or replacing an existing one
2. **Complete replacement**: The entire item is replaced; there's no merging of attributes
3. **Durability**: Data is written to the Write-Ahead Log (WAL) before returning
4. **Atomicity**: Each put operation is atomic; it either succeeds completely or fails

```rust
// First put
db.put(b"user#charlie", ItemBuilder::new()
    .string("name", "Charlie")
    .number("age", 35)
    .build())?;

// Second put - completely replaces the first item
db.put(b"user#charlie", ItemBuilder::new()
    .string("name", "Charlie Brown")
    .string("email", "charlie@example.com")
    .build())?;

// The age attribute is now gone - only name and email exist
```

## Get Operation

The `get` operation retrieves an item by its key. It returns `Option<Item>`, which is `None` if the item doesn't exist.

### Basic Get (Partition Key Only)

```rust
// Get an item
let result = db.get(b"user#alice")?;

match result {
    Some(item) => {
        // Item exists - access attributes
        if let Some(name) = item.get("name").and_then(|v| v.as_string()) {
            println!("Name: {}", name);
        }

        if let Some(Value::N(age)) = item.get("age") {
            println!("Age: {}", age);
        }

        if let Some(Value::Bool(active)) = item.get("active") {
            println!("Active: {}", active);
        }
    }
    None => {
        println!("User not found");
    }
}
```

### Get with Sort Key

When using composite keys, you must provide both the partition key and sort key:

```rust
// Get a specific post
let post = db.get_with_sk(b"user#alice", b"post#3")?;

if let Some(item) = post {
    let title = item.get("title").and_then(|v| v.as_string())
        .unwrap_or("Untitled");
    let content = item.get("content").and_then(|v| v.as_string())
        .unwrap_or("");

    println!("Title: {}", title);
    println!("Content: {}", content);
}
```

### Accessing Nested Data

When working with complex items containing maps and lists:

```rust
let user = db.get(b"user#bob")?.expect("User should exist");

// Access nested map (address)
if let Some(Value::M(address)) = user.get("address") {
    let city = address.get("city")
        .and_then(|v| v.as_string())
        .unwrap_or("Unknown");
    println!("City: {}", city);
}

// Access list (hobbies)
if let Some(Value::L(hobbies)) = user.get("hobbies") {
    println!("Hobbies:");
    for hobby in hobbies {
        if let Some(h) = hobby.as_string() {
            println!("  - {}", h);
        }
    }
}

// Access binary data
if let Some(Value::B(avatar_bytes)) = user.get("avatar") {
    println!("Avatar size: {} bytes", avatar_bytes.len());
}

// Access timestamp
if let Some(Value::Ts(created_ms)) = user.get("created_at") {
    let created_secs = created_ms / 1000;
    println!("Created at: {} seconds since epoch", created_secs);
}
```

### Get Performance Characteristics

Understanding get performance helps you design efficient applications:

1. **Memtable check** (fastest): If the item was recently written, it's in memory
2. **SST scan** (fast): Binary search through sorted files on disk
3. **Bloom filters**: Quickly skip SST files that definitely don't contain the key
4. **Stripe locality**: Items with the same partition key are in the same stripe

```rust
use std::time::Instant;

// Measure get performance
let start = Instant::now();
let result = db.get(b"user#alice")?;
let duration = start.elapsed();

println!("Get operation took: {:?}", duration);
// Typical: 1-10 microseconds from memtable, 10-100 microseconds from SST
```

## Delete Operation

The `delete` operation removes an item from the database. Like `put`, it uses tombstone markers internally to handle deletions in the LSM tree architecture.

### Basic Delete (Partition Key Only)

```rust
// Delete a user
db.delete(b"user#alice")?;

// Verify deletion
let result = db.get(b"user#alice")?;
assert!(result.is_none());
```

### Delete with Sort Key

When deleting items with composite keys:

```rust
// Delete a specific post
db.delete_with_sk(b"user#alice", b"post#3")?;

// Other posts remain
assert!(db.get_with_sk(b"user#alice", b"post#1")?.is_some());
assert!(db.get_with_sk(b"user#alice", b"post#2")?.is_some());
assert!(db.get_with_sk(b"user#alice", b"post#3")?.is_none()); // Deleted
assert!(db.get_with_sk(b"user#alice", b"post#4")?.is_some());
```

### Delete Semantics

Important characteristics of the `delete` operation:

1. **Idempotent**: Deleting a non-existent item succeeds without error
2. **Tombstone markers**: Deletes create tombstone records in the WAL and memtable
3. **Space reclamation**: Tombstones are removed during compaction
4. **No cascade**: Deleting a partition key doesn't delete related sort keys

```rust
// Delete is idempotent - safe to call multiple times
db.delete(b"user#nonexistent")?;  // Succeeds
db.delete(b"user#nonexistent")?;  // Also succeeds

// Deleting the partition key alone doesn't affect sort key items
db.delete(b"user#alice")?;

// Items with sort keys still exist!
assert!(db.get_with_sk(b"user#alice", b"post#1")?.is_some());
assert!(db.get_with_sk(b"user#alice", b"post#2")?.is_some());
```

### Deleting All Items for a Partition

To delete all items associated with a partition key, you need to query and delete individually:

```rust
use kstone_api::Query;

// Find all posts for a user
let query = Query::new(b"user#alice");
let response = db.query(query)?;

// Delete each post
for item in response.items {
    // Note: You need to extract the sort key from the query results
    // In practice, you'd track keys during iteration
    // For now, we'll delete known posts
}

// Delete posts explicitly
for i in 1..=5 {
    let sk = format!("post#{}", i);
    db.delete_with_sk(b"user#alice", sk.as_bytes())?;
}

// Finally delete the base item (if it exists)
db.delete(b"user#alice")?;
```

### Understanding Tombstones

Deletes in LSM-tree databases don't immediately remove data from disk. Instead:

1. A delete creates a **tombstone** marker with a higher sequence number
2. The tombstone is written to the WAL and memtable
3. During reads, newer tombstones shadow older data
4. During compaction, tombstones eliminate the actual data

```rust
// This sequence of operations demonstrates tombstone behavior:

// 1. Write data (sequence number 1)
db.put(b"key#1", ItemBuilder::new().string("data", "original").build())?;

// 2. Delete creates tombstone (sequence number 2)
db.delete(b"key#1")?;

// 3. Read returns None (tombstone shadows the data)
assert!(db.get(b"key#1")?.is_none());

// 4. Write again (sequence number 3, newer than tombstone)
db.put(b"key#1", ItemBuilder::new().string("data", "restored").build())?;

// 5. Read returns new data (newest record wins)
assert!(db.get(b"key#1")?.is_some());
```

## Working with Sort Keys

Sort keys enable powerful data modeling patterns by allowing multiple items under a single partition key.

### Time-Series Data

Store events in chronological order:

```rust
use std::time::SystemTime;

// Record temperature readings
let sensors = vec!["sensor#kitchen", "sensor#bedroom", "sensor#garage"];

for sensor in sensors {
    for hour in 0..24 {
        let timestamp = 1704067200 + (hour * 3600); // Unix timestamp
        let temp = 20.0 + (hour as f64) * 0.5; // Simulated temperature

        let reading = ItemBuilder::new()
            .number("temperature", temp)
            .number("humidity", 45 + hour)
            .build();

        // Sort key format: YYYY-MM-DD-HH for lexicographic ordering
        let sk = format!("2024-01-01-{:02}", hour);
        db.put_with_sk(sensor.as_bytes(), sk.as_bytes(), reading)?;
    }
}

// Query recent readings (covered in detail in Chapter 9)
let query = Query::new(b"sensor#kitchen")
    .sk_gte(b"2024-01-01-12")  // From noon onwards
    .limit(10);
let response = db.query(query)?;
```

### Hierarchical Data

Model parent-child relationships:

```rust
// Organization structure
db.put_with_sk(
    b"org#acme",
    b"meta",
    ItemBuilder::new()
        .string("name", "Acme Corporation")
        .string("industry", "Technology")
        .build()
)?;

// Departments
db.put_with_sk(
    b"org#acme",
    b"dept#engineering",
    ItemBuilder::new()
        .string("name", "Engineering")
        .number("headcount", 50)
        .build()
)?;

db.put_with_sk(
    b"org#acme",
    b"dept#sales",
    ItemBuilder::new()
        .string("name", "Sales")
        .number("headcount", 25)
        .build()
)?;

// Employees
db.put_with_sk(
    b"org#acme",
    b"dept#engineering#emp#alice",
    ItemBuilder::new()
        .string("name", "Alice Johnson")
        .string("role", "Senior Engineer")
        .build()
)?;
```

### Versioning

Track item history with version numbers:

```rust
// Store document versions
for version in 1..=5 {
    let doc = ItemBuilder::new()
        .string("title", "My Document")
        .string("content", format!("Version {} content", version))
        .number("version", version)
        .number("timestamp", 1704067200 + version * 3600)
        .build();

    let sk = format!("v{:05}", version); // v00001, v00002, etc.
    db.put_with_sk(b"doc#readme", sk.as_bytes(), doc)?;
}

// Get latest version
let latest = db.get_with_sk(b"doc#readme", b"v00005")?;

// Get specific historical version
let historical = db.get_with_sk(b"doc#readme", b"v00002")?;
```

## Error Handling

Proper error handling ensures your application degrades gracefully when issues occur.

### Common Errors

```rust
use kstone_core::Error;

match db.get(b"user#alice") {
    Ok(Some(item)) => {
        // Successfully retrieved item
        println!("Found user: {:?}", item);
    }
    Ok(None) => {
        // Item doesn't exist - this is NOT an error
        println!("User not found");
    }
    Err(Error::Io(e)) => {
        // I/O error (disk failure, permissions, etc.)
        eprintln!("I/O error: {}", e);
    }
    Err(Error::Corruption(msg)) => {
        // Data corruption detected
        eprintln!("Corruption detected: {}", msg);
    }
    Err(Error::Internal(msg)) => {
        // Internal database error
        eprintln!("Internal error: {}", msg);
    }
    Err(e) => {
        // Other errors
        eprintln!("Error: {}", e);
    }
}
```

### Handling Put Errors

```rust
// Check if put succeeded
if let Err(e) = db.put(b"user#dave", item) {
    match e {
        Error::Io(io_err) => {
            // Disk full, permissions, etc.
            eprintln!("Failed to write: {}", io_err);
            // Consider retrying or alerting
        }
        _ => {
            eprintln!("Put failed: {}", e);
        }
    }
}
```

### Validation Before Operations

```rust
fn validate_user_key(pk: &[u8]) -> Result<(), String> {
    if pk.is_empty() {
        return Err("Key cannot be empty".to_string());
    }
    if pk.len() > 2048 {
        return Err("Key too long (max 2048 bytes)".to_string());
    }
    Ok(())
}

fn put_user(db: &Database, pk: &[u8], item: Item) -> Result<(), Box<dyn std::error::Error>> {
    validate_user_key(pk)?;
    db.put(pk, item)?;
    Ok(())
}
```

## Best Practices

### 1. Use Meaningful Key Patterns

Design keys that are self-documenting and support your access patterns:

```rust
// Good: Clear hierarchy and purpose
b"tenant#abc#user#123"
b"order#2024#01#15#12345"
b"cache#session#xyz789"

// Bad: Opaque or inconsistent
b"t1u456"
b"20240115-order"
b"xyz789session"
```

### 2. Partition Key Design

Choose partition keys that:
- Distribute data evenly across stripes
- Group related data for efficient queries
- Avoid hot partitions

```rust
// Good: User-based partitioning (distributes well)
db.put(b"user#alice", user_data)?;
db.put_with_sk(b"user#alice", b"order#12345", order)?;

// Bad: All data in one partition (hot spot)
db.put_with_sk(b"all_users", b"alice", user_data)?;
db.put_with_sk(b"all_users", b"bob", user_data)?;
// Puts all users in one stripe, limiting parallelism
```

### 3. Sort Key Lexicographic Ordering

Design sort keys that sort naturally:

```rust
// Good: Zero-padded numbers sort correctly
b"order#00001"  // Comes before
b"order#00010"  // Correct ordering
b"order#00100"

// Bad: Non-padded numbers sort lexicographically
b"order#1"      // Comes after 10 lexicographically!
b"order#10"
b"order#100"

// Good: ISO date format sorts chronologically
b"2024-01-15"
b"2024-02-20"
b"2024-12-31"

// Bad: Non-sortable date format
b"01/15/2024"   // MM/DD/YYYY doesn't sort correctly
b"12/31/2024"   // Comes before 02/20/2024 lexicographically
```

### 4. Consistent Item Structure

Maintain consistent attribute names and types:

```rust
// Good: Consistent structure
let user1 = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .string("email", "alice@example.com")
    .build();

let user2 = ItemBuilder::new()
    .string("name", "Bob")
    .number("age", 25)
    .string("email", "bob@example.com")
    .build();

// Bad: Inconsistent attributes
let user3 = ItemBuilder::new()
    .string("full_name", "Charlie")  // Different attribute name
    .string("age", "35")              // Wrong type (string instead of number)
    .build();
```

### 5. Handle Optional Attributes

Not all attributes need to be present in every item:

```rust
let mut item = ItemBuilder::new()
    .string("name", "Eve")
    .number("age", 28)
    .build();

// Optional attributes
if let Some(email) = user_email {
    item.insert("email".to_string(), Value::string(email));
}

if let Some(phone) = user_phone {
    item.insert("phone".to_string(), Value::string(phone));
}

db.put(b"user#eve", item)?;

// When reading, safely handle missing attributes
let user = db.get(b"user#eve")?.unwrap();
let email = user.get("email")
    .and_then(|v| v.as_string())
    .unwrap_or("no-email@example.com");
```

### 6. Bulk Operations

For inserting many items, consider batching (covered in Chapter 11):

```rust
use kstone_api::BatchWriteRequest;

// Instead of individual puts
for i in 0..100 {
    let item = ItemBuilder::new().number("id", i).build();
    db.put(format!("item#{}", i).as_bytes(), item)?;
}

// Use batch operations (more efficient)
let mut batch = BatchWriteRequest::new();
for i in 0..100 {
    let item = ItemBuilder::new().number("id", i).build();
    batch = batch.put(format!("item#{}", i).as_bytes(), item);
}
db.batch_write(batch)?;
```

### 7. Durability vs Performance

Understand the trade-offs:

```rust
// Maximum durability: Flush after each write
db.put(b"critical#data", item)?;
db.flush()?;  // Ensures data is on disk

// Better performance: Let WAL group commits handle flushing
// Data is still durable, but flushing is batched
for i in 0..1000 {
    db.put(format!("item#{}", i).as_bytes(), item.clone())?;
}
// Implicit flush happens automatically after WAL batch timeout
```

### 8. Testing CRUD Operations

Write comprehensive tests for your data access patterns:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_user_crud() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Create
        let user = ItemBuilder::new()
            .string("name", "Alice")
            .number("age", 30)
            .build();
        db.put(b"user#alice", user.clone()).unwrap();

        // Read
        let retrieved = db.get(b"user#alice").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved.unwrap().get("name").unwrap().as_string(),
            Some("Alice")
        );

        // Update (via put)
        let updated = ItemBuilder::new()
            .string("name", "Alice Johnson")
            .number("age", 31)
            .build();
        db.put(b"user#alice", updated).unwrap();

        // Verify update
        let retrieved = db.get(b"user#alice").unwrap().unwrap();
        assert_eq!(
            retrieved.get("name").unwrap().as_string(),
            Some("Alice Johnson")
        );

        // Delete
        db.delete(b"user#alice").unwrap();
        assert!(db.get(b"user#alice").unwrap().is_none());
    }

    #[test]
    fn test_sort_key_operations() {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Create multiple items with same PK, different SKs
        for i in 1..=3 {
            let post = ItemBuilder::new()
                .string("title", format!("Post {}", i))
                .build();
            db.put_with_sk(
                b"user#bob",
                format!("post#{}", i).as_bytes(),
                post
            ).unwrap();
        }

        // Verify all exist
        assert!(db.get_with_sk(b"user#bob", b"post#1").unwrap().is_some());
        assert!(db.get_with_sk(b"user#bob", b"post#2").unwrap().is_some());
        assert!(db.get_with_sk(b"user#bob", b"post#3").unwrap().is_some());

        // Delete one
        db.delete_with_sk(b"user#bob", b"post#2").unwrap();

        // Verify correct deletion
        assert!(db.get_with_sk(b"user#bob", b"post#1").unwrap().is_some());
        assert!(db.get_with_sk(b"user#bob", b"post#2").unwrap().is_none());
        assert!(db.get_with_sk(b"user#bob", b"post#3").unwrap().is_some());
    }
}
```

## Summary

This chapter covered the fundamental CRUD operations in KeystoneDB:

- **Put**: Creating and updating items with `put()` and `put_with_sk()`
- **Get**: Retrieving items with `get()` and `get_with_sk()`
- **Delete**: Removing items with `delete()` and `delete_with_sk()`
- **Sort Keys**: Modeling complex data relationships with composite keys
- **Error Handling**: Properly handling database errors
- **Best Practices**: Key design, consistency, and testing strategies

In the next chapter, we'll explore querying data - how to efficiently retrieve multiple items using partition keys and sort key conditions.

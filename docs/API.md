# KeystoneDB API Reference

Complete API reference for KeystoneDB, covering all operations, types, and configurations.

## Table of Contents

- [Database Operations](#database-operations)
- [Item Builder](#item-builder)
- [Query API](#query-api)
- [Scan API](#scan-api)
- [Update API](#update-api)
- [Batch Operations](#batch-operations)
- [Transactions](#transactions)
- [Indexes](#indexes)
- [Streams](#streams)
- [Configuration](#configuration)
- [Error Types](#error-types)
- [Value Types](#value-types)

---

## Database Operations

### Creating and Opening

```rust
use kstone_api::{Database, TableSchema};

// Create a new database
let db = Database::create("path/to/db.keystone")?;

// Open existing database
let db = Database::open("path/to/db.keystone")?;

// Create with schema (for indexes, TTL, streams)
let schema = TableSchema::new()
    .with_ttl("expiresAt")
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"));
let db = Database::create_with_schema("path/to/db.keystone", schema)?;

// Create in-memory database (no persistence)
let db = Database::create_in_memory()?;
let db = Database::create_in_memory_with_schema(schema)?;
```

### Put Operations

```rust
use kstone_api::{Database, ItemBuilder};
use kstone_core::expression::ExpressionContext;

// Simple put
db.put(b"partition_key", item)?;

// Put with sort key
db.put_with_sk(b"pk", b"sk", item)?;

// Conditional put (only if key doesn't exist)
let context = ExpressionContext::new();
db.put_conditional(
    b"pk",
    item,
    "attribute_not_exists(pk)",
    context,
)?;

// Conditional put with sort key
db.put_conditional_with_sk(
    b"pk",
    b"sk",
    item,
    "attribute_not_exists(pk)",
    context,
)?;
```

### Get Operations

```rust
// Simple get (returns Option<Item>)
let item = db.get(b"pk")?;

// Get with sort key
let item = db.get_with_sk(b"pk", b"sk")?;

// Pattern: handle result
match db.get(b"user#123")? {
    Some(item) => println!("Found: {:?}", item),
    None => println!("Not found"),
}
```

### Delete Operations

```rust
// Simple delete
db.delete(b"pk")?;

// Delete with sort key
db.delete_with_sk(b"pk", b"sk")?;

// Conditional delete
let context = ExpressionContext::new()
    .with_value(":status", Value::S("inactive".to_string()));
db.delete_conditional(b"pk", "status = :status", context)?;

// Conditional delete with sort key
db.delete_conditional_with_sk(b"pk", b"sk", "status = :status", context)?;
```

### Flush and Stats

```rust
// Force flush memtable to disk
db.flush()?;

// Get database statistics
let stats = db.stats()?;
println!("Total SST files: {}", stats.total_sst_files);
println!("Total keys: {:?}", stats.total_keys);
println!("WAL size: {:?}", stats.wal_size_bytes);
```

---

## Item Builder

Build items with various value types:

```rust
use kstone_api::ItemBuilder;
use kstone_core::Value;

// Basic builder usage
let item = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .number("score", 95.5)        // Floats work too
    .bool("active", true)
    .build();

// All value types
let item = ItemBuilder::new()
    // String
    .string("name", "value")

    // Number (stored as string for precision)
    .number("count", 42)
    .number("price", 19.99)
    .number("big", 9999999999999i64)

    // Boolean
    .bool("enabled", true)

    // Binary
    .binary("data", vec![0x01, 0x02, 0x03])

    // Null (using raw Value)
    .build();

// Add null value directly
let mut item = ItemBuilder::new().string("name", "test").build();
item.insert("optional".to_string(), Value::Null);

// Nested structures using Value directly
use std::collections::HashMap;
let mut nested = HashMap::new();
nested.insert("city".to_string(), Value::S("NYC".to_string()));
nested.insert("zip".to_string(), Value::N("10001".to_string()));

let mut item = ItemBuilder::new()
    .string("name", "Alice")
    .build();
item.insert("address".to_string(), Value::M(nested));

// Lists
let list = Value::L(vec![
    Value::S("one".to_string()),
    Value::N("2".to_string()),
    Value::Bool(true),
]);
item.insert("items".to_string(), list);

// Vector embeddings
item.insert("embedding".to_string(), Value::VecF32(vec![0.1, 0.2, 0.3]));

// Timestamp
item.insert("created_at".to_string(), Value::Ts(1704067200000));
```

---

## Query API

Query items within a partition:

```rust
use kstone_api::{Query, Database};

// Basic query (all items in partition)
let query = Query::new(b"user#123");
let response = db.query(query)?;

// Access results
for (key, item) in response.items {
    println!("Key: {:?}, Item: {:?}", key, item);
}

// Check for more results
if let Some((last_pk, last_sk)) = response.last_key {
    println!("More results available, last key: {:?}", last_pk);
}
```

### Sort Key Conditions

```rust
// Equals
let query = Query::new(b"pk").sk_eq(b"sk_value");

// Less than
let query = Query::new(b"pk").sk_lt(b"2024-01-01");

// Less than or equal
let query = Query::new(b"pk").sk_lte(b"2024-01-31");

// Greater than
let query = Query::new(b"pk").sk_gt(b"2024-01-01");

// Greater than or equal
let query = Query::new(b"pk").sk_gte(b"2024-01-01");

// Between (inclusive)
let query = Query::new(b"pk").sk_between(b"2024-01-01", b"2024-12-31");

// Begins with (prefix match)
let query = Query::new(b"pk").sk_begins_with(b"post#");
```

### Query Options

```rust
// Limit results
let query = Query::new(b"pk").limit(10);

// Reverse order (descending sort key)
let query = Query::new(b"pk").forward(false);

// Pagination (start after last key from previous query)
let query = Query::new(b"pk")
    .start_after(b"pk", Some(b"last_sk"))
    .limit(10);

// Query using an index
let query = Query::new(b"pk")
    .index("email-index")
    .sk_begins_with(b"alice@");

// Combined
let query = Query::new(b"user#123")
    .sk_begins_with(b"post#")
    .limit(20)
    .forward(false);
```

---

## Scan API

Scan all items in the database:

```rust
use kstone_api::Scan;

// Basic scan
let scan = Scan::new();
let response = db.scan(scan)?;

// With limit
let scan = Scan::new().limit(100);

// Pagination
let scan = Scan::new()
    .start_after(b"last_pk", Some(b"last_sk"))
    .limit(100);

// Parallel scan (segment 0 of 4)
let scan = Scan::new().segment(0, 4);
```

### Parallel Scan Pattern

```rust
use std::thread;
use std::sync::Arc;

let db = Arc::new(db);
let total_segments = 4;

let handles: Vec<_> = (0..total_segments)
    .map(|segment| {
        let db = Arc::clone(&db);
        thread::spawn(move || {
            let scan = Scan::new().segment(segment, total_segments);
            db.scan(scan)
        })
    })
    .collect();

// Collect results
let mut all_items = Vec::new();
for handle in handles {
    if let Ok(Ok(response)) = handle.join() {
        all_items.extend(response.items);
    }
}
```

---

## Update API

Update items with expressions:

```rust
use kstone_api::Update;
use kstone_core::Value;

// SET expression
let update = Update::new(b"user#123")
    .expression("SET age = :new_age")
    .value(":new_age", Value::N("31".to_string()));
let response = db.update(update)?;

// Increment
let update = Update::new(b"user#123")
    .expression("SET visits = visits + :inc")
    .value(":inc", Value::N("1".to_string()));

// Multiple SET operations
let update = Update::new(b"user#123")
    .expression("SET name = :name, age = :age, updated = :ts")
    .value(":name", Value::S("Bob".to_string()))
    .value(":age", Value::N("32".to_string()))
    .value(":ts", Value::Ts(1704067200000));

// REMOVE attributes
let update = Update::new(b"user#123")
    .expression("REMOVE temp, verification_code");

// ADD (atomic addition)
let update = Update::new(b"counter#page")
    .expression("ADD views :count")
    .value(":count", Value::N("1".to_string()));

// Combined operations
let update = Update::new(b"user#123")
    .expression("SET status = :s REMOVE temp ADD login_count :one")
    .value(":s", Value::S("active".to_string()))
    .value(":one", Value::N("1".to_string()));

// Conditional update
let update = Update::new(b"user#123")
    .expression("SET age = :new_age")
    .condition("age = :old_age")
    .value(":new_age", Value::N("31".to_string()))
    .value(":old_age", Value::N("30".to_string()));
```

---

## Batch Operations

### Batch Get

```rust
use kstone_api::BatchGetRequest;

let request = BatchGetRequest::new()
    .add_key(b"user#1")
    .add_key(b"user#2")
    .add_key(b"user#3")
    .add_key_with_sk(b"user#4", b"profile");

let response = db.batch_get(request)?;

// Response contains only found items
for (key, item) in &response.items {
    println!("Found: {:?}", key.pk);
}
```

### Batch Write

```rust
use kstone_api::BatchWriteRequest;

let request = BatchWriteRequest::new()
    // Put operations
    .put(b"user#1", ItemBuilder::new().string("name", "Alice").build())
    .put(b"user#2", ItemBuilder::new().string("name", "Bob").build())
    .put_with_sk(b"user#3", b"profile", ItemBuilder::new().string("bio", "...").build())

    // Delete operations
    .delete(b"user#old")
    .delete_with_sk(b"user#temp", b"session");

let response = db.batch_write(request)?;
println!("Processed {} items", response.processed_count);
```

---

## Transactions

### Transact Get

```rust
use kstone_api::TransactGetRequest;

let request = TransactGetRequest::new()
    .get(b"account#source")
    .get(b"account#dest")
    .get_with_sk(b"user#123", b"profile");

let response = db.transact_get(request)?;

// Items in same order as request (None if not found)
for item_opt in response.items {
    match item_opt {
        Some(item) => println!("Found: {:?}", item),
        None => println!("Not found"),
    }
}
```

### Transact Write

```rust
use kstone_api::TransactWriteRequest;
use kstone_core::Value;

// Atomic transfer between accounts
let request = TransactWriteRequest::new()
    // Deduct from source (with condition)
    .update_with_condition(
        b"account#source",
        "SET balance = balance - :amount",
        "balance >= :amount",
    )
    // Add to destination
    .update(b"account#dest", "SET balance = balance + :amount")
    // Shared values
    .value(":amount", Value::N("100".to_string()));

match db.transact_write(request) {
    Ok(response) => println!("Committed {} operations", response.committed_count),
    Err(e) => println!("Transaction failed: {}", e),
}

// Mixed operations
let request = TransactWriteRequest::new()
    .put(b"order#new", order_item)
    .update(b"inventory#sku123", "SET quantity = quantity - :qty")
    .delete(b"cart#user123")
    .condition_check(b"product#sku123", "quantity > :min")
    .value(":qty", Value::N("1".to_string()))
    .value(":min", Value::N("0".to_string()));
```

---

## Indexes

### Local Secondary Index (LSI)

```rust
use kstone_api::{TableSchema, LocalSecondaryIndex, IndexProjection};

// Create schema with LSI
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"))
    .add_local_index(LocalSecondaryIndex::new("score-index", "score").keys_only())
    .add_local_index(
        LocalSecondaryIndex::new("name-index", "lastName")
            .include(vec!["firstName".to_string()])
    );

let db = Database::create_with_schema("path", schema)?;

// Query by LSI
let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice@");
```

### Global Secondary Index (GSI)

```rust
use kstone_api::{TableSchema, GlobalSecondaryIndex};

// GSI with partition key only
let schema = TableSchema::new()
    .add_global_index(GlobalSecondaryIndex::new("status-index", "status"));

// GSI with partition key AND sort key
let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::with_sort_key("category-price-index", "category", "price")
    );

// Query GSI
let query = Query::new(b"active")  // GSI partition key value
    .index("status-index");
```

---

## Streams

Change Data Capture (CDC):

```rust
use kstone_api::{TableSchema, StreamConfig, StreamViewType};

// Enable streams
let schema = TableSchema::new()
    .with_stream(StreamConfig::enabled());

// Configure view type
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_view_type(StreamViewType::NewAndOldImages)
            .with_buffer_size(1000)
    );

let db = Database::create_with_schema("path", schema)?;

// Read stream records
let records = db.read_stream(None)?;  // All records

for record in records {
    match record.event_type {
        StreamEventType::Insert => println!("INSERT: {:?}", record.new_image),
        StreamEventType::Modify => println!("MODIFY: {:?} -> {:?}", record.old_image, record.new_image),
        StreamEventType::Remove => println!("REMOVE: {:?}", record.old_image),
    }
}

// Poll for new records
let mut last_seq = None;
loop {
    let records = db.read_stream(last_seq)?;
    if !records.is_empty() {
        last_seq = records.last().map(|r| r.sequence_number);
        // Process records
    }
    std::thread::sleep(Duration::from_secs(1));
}
```

### Stream View Types

| Type | Description | Use Case |
|------|-------------|----------|
| `KeysOnly` | Only key information | Triggers, minimal data |
| `NewImage` | Item after change | Cache invalidation |
| `OldImage` | Item before change | Audit, undo |
| `NewAndOldImages` | Both states | Full audit trail |

---

## Configuration

### Database Configuration

```rust
use kstone_core::{DatabaseConfig, LsmEngine, TableSchema};

let config = DatabaseConfig::new()
    .with_max_memtable_size_bytes(8 * 1024 * 1024)  // 8MB
    .with_max_memtable_records(20_000)
    .with_compression()                              // Enable Zstd
    .with_compression_level(6);                      // Level 1-22

let engine = LsmEngine::create_with_config(path, config, TableSchema::new())?;
```

### Compaction Configuration

```rust
use kstone_core::CompactionConfig;

let config = CompactionConfig {
    enabled: true,
    sst_threshold: 10,           // Compact when ≥10 SSTs
    check_interval_secs: 10,     // Check every 10 seconds
    max_concurrent_compactions: 4,
};
```

### TTL Configuration

```rust
use kstone_api::TableSchema;

// Enable TTL on expiresAt attribute
let schema = TableSchema::new().with_ttl("expiresAt");

// Items with expiresAt < current_time are filtered automatically
let item = ItemBuilder::new()
    .string("data", "temporary")
    .number("expiresAt", now_secs + 3600)  // Expires in 1 hour
    .build();
```

---

## Error Types

```rust
use kstone_core::Error;

match db.get(b"key") {
    Ok(Some(item)) => { /* found */ }
    Ok(None) => { /* not found */ }
    Err(Error::NotFound(_)) => { /* explicit not found */ }
    Err(Error::Io(e)) => { /* I/O error */ }
    Err(Error::ConditionalCheckFailed(_)) => { /* condition failed */ }
    Err(Error::TransactionCanceled { reasons }) => { /* transaction failed */ }
    Err(Error::InvalidQuery(msg)) => { /* bad query */ }
    Err(Error::InvalidArgument(msg)) => { /* bad argument */ }
    Err(Error::Corruption(msg)) => { /* data corruption */ }
    Err(e) => { /* other error */ }
}
```

### Common Errors

| Error | Description | Recovery |
|-------|-------------|----------|
| `NotFound` | Key does not exist | Check key, create if needed |
| `ConditionalCheckFailed` | Condition expression failed | Retry with updated condition |
| `TransactionCanceled` | One or more operations failed | Check reasons, retry |
| `InvalidQuery` | Malformed query | Fix query syntax |
| `Io` | Filesystem error | Check disk space, permissions |
| `Corruption` | Data integrity error | Restore from backup |

---

## Value Types

All DynamoDB value types are supported:

```rust
use kstone_core::Value;
use bytes::Bytes;

// String (S)
Value::S("hello".to_string())

// Number (N) - stored as string for precision
Value::N("42".to_string())
Value::N("3.14159".to_string())
Value::N("-999".to_string())

// Binary (B)
Value::B(Bytes::from(vec![0x01, 0x02, 0x03]))

// Boolean (BOOL)
Value::Bool(true)
Value::Bool(false)

// Null (NULL)
Value::Null

// List (L)
Value::L(vec![
    Value::S("a".to_string()),
    Value::N("1".to_string()),
    Value::Bool(true),
])

// Map (M)
let mut map = std::collections::HashMap::new();
map.insert("nested".to_string(), Value::S("value".to_string()));
Value::M(map)

// Timestamp (Ts) - milliseconds since epoch
Value::Ts(1704067200000)

// Vector (VecF32) - for embeddings
Value::VecF32(vec![0.1, 0.2, 0.3, 0.4])
```

### Type Checking

```rust
use kstone_api::KeystoneValue;

match item.get("field") {
    Some(KeystoneValue::S(s)) => println!("String: {}", s),
    Some(KeystoneValue::N(n)) => println!("Number: {}", n),
    Some(KeystoneValue::Bool(b)) => println!("Bool: {}", b),
    Some(KeystoneValue::Null) => println!("Null"),
    Some(KeystoneValue::L(list)) => println!("List with {} items", list.len()),
    Some(KeystoneValue::M(map)) => println!("Map with {} keys", map.len()),
    Some(KeystoneValue::B(bytes)) => println!("Binary: {} bytes", bytes.len()),
    Some(KeystoneValue::Ts(ts)) => println!("Timestamp: {}", ts),
    Some(KeystoneValue::VecF32(vec)) => println!("Vector: {} dimensions", vec.len()),
    None => println!("Field not found"),
}
```

---

## See Also

- [README.md](../README.md) - Getting started guide
- [ARCHITECTURE.md](../ARCHITECTURE.md) - Internal design
- [PERFORMANCE.md](../PERFORMANCE.md) - Performance tuning
- [DEPLOYMENT.md](../DEPLOYMENT.md) - Production deployment

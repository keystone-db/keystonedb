# Chapter 36: API Reference

This chapter provides comprehensive reference documentation for KeystoneDB's public API, including all types, methods, and their usage patterns.

## 36.1 Database Handle

### 36.1.1 Creating Databases

#### `Database::create`

Creates a new database at the specified path.

```rust
pub fn create(path: impl AsRef<Path>) -> Result<Self>
```

**Parameters:**
- `path`: Directory path where database files will be stored

**Returns:**
- `Result<Database>`: Database handle on success

**Example:**
```rust
use kstone_api::Database;

let db = Database::create("myapp.keystone")?;
```

**Notes:**
- Creates directory if it doesn't exist
- Returns error if database already exists
- Initializes WAL, manifest, and stripe structures

---

#### `Database::create_with_schema`

Creates a database with a predefined schema including indexes, TTL, and streams.

```rust
pub fn create_with_schema(
    path: impl AsRef<Path>,
    schema: TableSchema
) -> Result<Self>
```

**Parameters:**
- `path`: Directory path for database files
- `schema`: Table schema defining indexes, TTL, streams

**Returns:**
- `Result<Database>`: Database handle on success

**Example:**
```rust
use kstone_api::{Database, TableSchema, GlobalSecondaryIndex};

let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::new("status-index", "status")
    )
    .with_ttl("expiresAt");

let db = Database::create_with_schema("myapp.keystone", schema)?;
```

---

#### `Database::create_with_config`

Creates a database with custom configuration.

```rust
pub fn create_with_config(
    path: impl AsRef<Path>,
    config: DatabaseConfig,
) -> Result<Self>
```

**Parameters:**
- `path`: Directory path for database files
- `config`: Database configuration (memtable size, compaction settings, etc.)

**Example:**
```rust
use kstone_api::{Database, DatabaseConfig};

let config = DatabaseConfig {
    memtable_threshold: 1000,
    compaction_enabled: true,
    compaction_threshold: 10,
    max_background_jobs: 2,
    block_size: 4096,
    bloom_bits_per_key: 10,
};

let db = Database::create_with_config("myapp.keystone", config)?;
```

---

#### `Database::open`

Opens an existing database.

```rust
pub fn open(path: impl AsRef<Path>) -> Result<Self>
```

**Parameters:**
- `path`: Path to existing database directory

**Returns:**
- `Result<Database>`: Database handle on success

**Example:**
```rust
let db = Database::open("myapp.keystone")?;
```

**Notes:**
- Replays WAL for crash recovery
- Loads manifest and stripe metadata
- Calculates next sequence number

---

#### `Database::create_in_memory`

Creates an in-memory database (no disk persistence).

```rust
pub fn create_in_memory() -> Result<Self>
```

**Returns:**
- `Result<Database>`: In-memory database handle

**Example:**
```rust
let db = Database::create_in_memory()?;
```

**Use Cases:**
- Testing
- Temporary caches
- Development

**Limitations:**
- All data lost when dropped
- Query/scan/transactions not yet supported

---

### 36.1.2 Basic CRUD Operations

#### `put`

Stores an item with a partition key.

```rust
pub fn put(&self, pk: &[u8], item: Item) -> Result<()>
```

**Parameters:**
- `pk`: Partition key (unique identifier)
- `item`: Item to store (HashMap of attributes)

**Example:**
```rust
use kstone_api::ItemBuilder;

let item = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .bool("active", true)
    .build();

db.put(b"user#123", item)?;
```

---

#### `put_with_sk`

Stores an item with partition key and sort key.

```rust
pub fn put_with_sk(
    &self,
    pk: &[u8],
    sk: &[u8],
    item: Item,
) -> Result<()>
```

**Parameters:**
- `pk`: Partition key
- `sk`: Sort key (for ordering within partition)
- `item`: Item to store

**Example:**
```rust
let post = ItemBuilder::new()
    .string("title", "My Post")
    .string("content", "Hello world")
    .build();

db.put_with_sk(b"author#alice", b"post#001", post)?;
```

---

#### `get`

Retrieves an item by partition key.

```rust
pub fn get(&self, pk: &[u8]) -> Result<Option<Item>>
```

**Parameters:**
- `pk`: Partition key to lookup

**Returns:**
- `Result<Option<Item>>`: Item if found, None if not found

**Example:**
```rust
match db.get(b"user#123")? {
    Some(item) => {
        println!("Found user: {:?}", item);
    }
    None => {
        println!("User not found");
    }
}
```

---

#### `get_with_sk`

Retrieves an item by partition key and sort key.

```rust
pub fn get_with_sk(
    &self,
    pk: &[u8],
    sk: &[u8],
) -> Result<Option<Item>>
```

**Parameters:**
- `pk`: Partition key
- `sk`: Sort key

**Example:**
```rust
let post = db.get_with_sk(b"author#alice", b"post#001")?;
```

---

#### `delete`

Deletes an item by partition key.

```rust
pub fn delete(&self, pk: &[u8]) -> Result<()>
```

**Parameters:**
- `pk`: Partition key of item to delete

**Example:**
```rust
db.delete(b"user#123")?;
```

---

#### `delete_with_sk`

Deletes an item by partition key and sort key.

```rust
pub fn delete_with_sk(
    &self,
    pk: &[u8],
    sk: &[u8],
) -> Result<()>
```

**Parameters:**
- `pk`: Partition key
- `sk`: Sort key

**Example:**
```rust
db.delete_with_sk(b"author#alice", b"post#001")?;
```

---

### 36.1.3 Conditional Operations

#### `put_conditional`

Puts an item only if condition is met.

```rust
pub fn put_conditional(
    &self,
    pk: &[u8],
    item: Item,
    condition: &str,
    context: ExpressionContext,
) -> Result<()>
```

**Parameters:**
- `pk`: Partition key
- `item`: Item to store
- `condition`: Condition expression
- `context`: Expression context with values/names

**Example:**
```rust
use kstone_core::expression::ExpressionContext;

let item = ItemBuilder::new()
    .string("name", "Alice")
    .build();

let context = ExpressionContext::new();

// Only put if item doesn't exist
db.put_conditional(
    b"user#123",
    item,
    "attribute_not_exists(name)",
    context,
)?;
```

**Common Conditions:**
- `attribute_not_exists(attr)`: Create if not exists
- `attribute_exists(attr)`: Update if exists
- `version = :old`: Optimistic locking

---

#### `delete_conditional`

Deletes an item only if condition is met.

```rust
pub fn delete_conditional(
    &self,
    pk: &[u8],
    condition: &str,
    context: ExpressionContext,
) -> Result<()>
```

**Example:**
```rust
use kstone_core::{expression::ExpressionContext, Value};

let context = ExpressionContext::new()
    .with_value(":status", Value::string("inactive"));

db.delete_conditional(
    b"user#123",
    "status = :status",
    context,
)?;
```

---

### 36.1.4 Query Operations

#### `query`

Queries items within a partition.

```rust
pub fn query(&self, query: Query) -> Result<QueryResponse>
```

**Parameters:**
- `query`: Query builder with partition key and optional conditions

**Returns:**
- `Result<QueryResponse>`: Response with items, count, and pagination info

**Example:**
```rust
use kstone_api::Query;

// Query all posts by author
let query = Query::new(b"author#alice");
let response = db.query(query)?;

for item in response.items {
    println!("Post: {:?}", item);
}

// Query with sort key condition
let query = Query::new(b"author#alice")
    .sk_begins_with(b"post#2024")
    .limit(10)
    .forward(false);  // Reverse order

let response = db.query(query)?;
```

---

#### `scan`

Scans all items in the database.

```rust
pub fn scan(&self, scan: Scan) -> Result<ScanResponse>
```

**Parameters:**
- `scan`: Scan builder with optional limits and filters

**Returns:**
- `Result<ScanResponse>`: Response with items and pagination info

**Example:**
```rust
use kstone_api::Scan;

// Scan all items
let scan = Scan::new();
let response = db.scan(scan)?;

// Scan with limit
let scan = Scan::new().limit(100);
let response = db.scan(scan)?;

// Parallel scan (4 segments)
for segment in 0..4 {
    let scan = Scan::new().segment(segment, 4);
    let response = db.scan(scan)?;
    // Process segment
}
```

---

### 36.1.5 Update Operations

#### `update`

Updates an item using update expressions.

```rust
pub fn update(&self, update: Update) -> Result<UpdateResponse>
```

**Parameters:**
- `update`: Update builder with expression and values

**Returns:**
- `Result<UpdateResponse>`: Response with updated item

**Example:**
```rust
use kstone_api::Update;
use kstone_core::Value;

// Simple SET
let update = Update::new(b"user#123")
    .expression("SET age = :new_age")
    .value(":new_age", Value::number(31));

let response = db.update(update)?;

// Increment counter
let update = Update::new(b"post#456")
    .expression("SET views = views + :inc")
    .value(":inc", Value::number(1));

db.update(update)?;

// Multiple actions
let update = Update::new(b"user#789")
    .expression("SET age = :age, verified = :v REMOVE temp ADD score :bonus")
    .value(":age", Value::number(25))
    .value(":v", Value::Bool(true))
    .value(":bonus", Value::number(100));

db.update(update)?;

// Conditional update
let update = Update::new(b"user#999")
    .expression("SET age = :new")
    .condition("age = :old")
    .value(":new", Value::number(26))
    .value(":old", Value::number(25));

db.update(update)?;
```

**Update Actions:**
- `SET attr = value`: Set attribute
- `SET attr = attr + value`: Arithmetic
- `REMOVE attr`: Delete attribute
- `ADD attr value`: Add to number

---

### 36.1.6 Batch Operations

#### `batch_get`

Retrieves multiple items in one call.

```rust
pub fn batch_get(&self, request: BatchGetRequest) -> Result<BatchGetResponse>
```

**Parameters:**
- `request`: Batch get request with keys to retrieve

**Returns:**
- `Result<BatchGetResponse>`: Response with found items

**Example:**
```rust
use kstone_api::BatchGetRequest;

let request = BatchGetRequest::new()
    .add_key(b"user#1")
    .add_key(b"user#2")
    .add_key(b"user#3")
    .add_key_with_sk(b"author#alice", b"post#1");

let response = db.batch_get(request)?;

println!("Retrieved {} items", response.items.len());

for (key, item) in response.items {
    // Process each item
}
```

**Limits:**
- Up to 100 items per request
- Only returns items that exist

---

#### `batch_write`

Writes or deletes multiple items atomically.

```rust
pub fn batch_write(&self, request: BatchWriteRequest) -> Result<BatchWriteResponse>
```

**Parameters:**
- `request`: Batch write request with put/delete operations

**Returns:**
- `Result<BatchWriteResponse>`: Response with operation count

**Example:**
```rust
use kstone_api::BatchWriteRequest;

let request = BatchWriteRequest::new()
    .put(b"user#1", ItemBuilder::new().string("name", "Alice").build())
    .put(b"user#2", ItemBuilder::new().string("name", "Bob").build())
    .delete(b"user#3")
    .put_with_sk(b"author#alice", b"post#1",
        ItemBuilder::new().string("title", "Hello").build());

let response = db.batch_write(request)?;
println!("Processed {} operations", response.processed_count);
```

**Limits:**
- Up to 25 operations per request
- Operations execute in order

---

### 36.1.7 Transactional Operations

#### `transact_get`

Reads multiple items atomically (consistent snapshot).

```rust
pub fn transact_get(&self, request: TransactGetRequest) -> Result<TransactGetResponse>
```

**Parameters:**
- `request`: Transaction get request with keys

**Returns:**
- `Result<TransactGetResponse>`: Response with items (Some or None for each)

**Example:**
```rust
use kstone_api::TransactGetRequest;

let request = TransactGetRequest::new()
    .get(b"user#1")
    .get(b"user#2")
    .get_with_sk(b"author#alice", b"post#1");

let response = db.transact_get(request)?;

for item_opt in response.items {
    match item_opt {
        Some(item) => println!("Item: {:?}", item),
        None => println!("Item not found"),
    }
}
```

---

#### `transact_write`

Writes multiple items atomically with conditions.

```rust
pub fn transact_write(&self, request: TransactWriteRequest) -> Result<TransactWriteResponse>
```

**Parameters:**
- `request`: Transaction write request with operations and conditions

**Returns:**
- `Result<TransactWriteResponse>`: Response with commit count

**Example:**
```rust
use kstone_api::TransactWriteRequest;
use kstone_core::Value;

// Transfer balance between accounts
let request = TransactWriteRequest::new()
    // Deduct from source (with condition)
    .update_with_condition(
        b"account#1",
        "SET balance = balance - :amount",
        "balance >= :amount"
    )
    // Add to destination
    .update(
        b"account#2",
        "SET balance = balance + :amount"
    )
    .value(":amount", Value::number(100));

let response = db.transact_write(request)?;
println!("Committed {} operations", response.committed_count);

// Complex transaction
let request = TransactWriteRequest::new()
    .put(b"user#1", ItemBuilder::new().string("name", "Alice").build())
    .update(b"counter#global", "ADD count :inc")
    .delete(b"temp#xyz")
    .condition_check(b"config#global", "attribute_exists(enabled)")
    .value(":inc", Value::number(1));

db.transact_write(request)?;
```

**ACID Guarantees:**
- All operations succeed or all fail
- Consistent snapshot for reads
- Condition failures cancel transaction

---

### 36.1.8 Streams

#### `read_stream`

Reads change data capture stream.

```rust
pub fn read_stream(
    &self,
    after_sequence_number: Option<u64>
) -> Result<Vec<StreamRecord>>
```

**Parameters:**
- `after_sequence_number`: Optional sequence to start after

**Returns:**
- `Result<Vec<StreamRecord>>`: Stream records

**Example:**
```rust
// Read all stream records
let records = db.read_stream(None)?;

for record in records {
    println!("Event: {:?}", record.event_type);
    println!("Key: {:?}", record.key);

    match record.event_type {
        StreamEventType::Insert => {
            println!("New item: {:?}", record.new_image);
        }
        StreamEventType::Modify => {
            println!("Old: {:?}", record.old_image);
            println!("New: {:?}", record.new_image);
        }
        StreamEventType::Remove => {
            println!("Deleted: {:?}", record.old_image);
        }
    }
}

// Poll for new records
let mut last_seq = None;

loop {
    let records = db.read_stream(last_seq)?;

    for record in &records {
        process_change(record);
    }

    if let Some(last) = records.last() {
        last_seq = Some(last.sequence_number);
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
}
```

---

### 36.1.9 Operational Methods

#### `flush`

Flushes pending writes to disk.

```rust
pub fn flush(&self) -> Result<()>
```

**Example:**
```rust
db.flush()?;
```

---

#### `stats`

Returns database statistics.

```rust
pub fn stats(&self) -> Result<DatabaseStats>
```

**Returns:**
- `DatabaseStats` with metrics

**Example:**
```rust
let stats = db.stats()?;

println!("Total SST files: {}", stats.total_sst_files);
println!("WAL size: {:?}", stats.wal_size_bytes);
println!("Compactions: {}", stats.compaction.total_compactions);
```

---

#### `health`

Checks database health.

```rust
pub fn health(&self) -> DatabaseHealth
```

**Returns:**
- `DatabaseHealth` with status and messages

**Example:**
```rust
let health = db.health();

println!("Status: {:?}", health.status);

for warning in health.warnings {
    println!("Warning: {}", warning);
}

for error in health.errors {
    println!("Error: {}", error);
}
```

---

## 36.2 Builder Types

### 36.2.1 ItemBuilder

Fluent builder for creating items.

```rust
pub struct ItemBuilder { /* ... */ }
```

**Methods:**

```rust
// Create new builder
pub fn new() -> Self

// Add string attribute
pub fn string(self, key: impl Into<String>, value: impl Into<String>) -> Self

// Add number attribute
pub fn number(self, key: impl Into<String>, value: impl ToString) -> Self

// Add boolean attribute
pub fn bool(self, key: impl Into<String>, value: bool) -> Self

// Build the item
pub fn build(self) -> Item
```

**Example:**
```rust
let item = ItemBuilder::new()
    .string("name", "Alice")
    .string("email", "alice@example.com")
    .number("age", 30)
    .number("balance", 100.50)
    .bool("active", true)
    .bool("verified", false)
    .build();
```

---

### 36.2.2 Query

Query builder for DynamoDB-style queries.

```rust
pub struct Query { /* ... */ }
```

**Methods:**

```rust
// Create query for partition key
pub fn new(pk: &[u8]) -> Self

// Sort key conditions
pub fn sk_eq(self, sk: &[u8]) -> Self
pub fn sk_lt(self, sk: &[u8]) -> Self
pub fn sk_lte(self, sk: &[u8]) -> Self
pub fn sk_gt(self, sk: &[u8]) -> Self
pub fn sk_gte(self, sk: &[u8]) -> Self
pub fn sk_between(self, sk1: &[u8], sk2: &[u8]) -> Self
pub fn sk_begins_with(self, prefix: &[u8]) -> Self

// Options
pub fn forward(self, forward: bool) -> Self
pub fn limit(self, limit: usize) -> Self
pub fn start_after(self, pk: &[u8], sk: Option<&[u8]>) -> Self

// Index query (Phase 3+)
pub fn index(self, index_name: impl Into<String>) -> Self
```

**Example:**
```rust
// All items in partition
let query = Query::new(b"author#alice");

// With sort key condition
let query = Query::new(b"author#alice")
    .sk_begins_with(b"post#2024");

// With limit and ordering
let query = Query::new(b"author#alice")
    .sk_gte(b"post#2024-01-01")
    .limit(10)
    .forward(false);

// Query index
let query = Query::new(b"active")
    .index("status-index")
    .limit(100);
```

---

### 36.2.3 Scan

Scan builder for table scans.

```rust
pub struct Scan { /* ... */ }
```

**Methods:**

```rust
// Create new scan
pub fn new() -> Self

// Options
pub fn limit(self, limit: usize) -> Self
pub fn start_after(self, pk: &[u8], sk: Option<&[u8]>) -> Self

// Parallel scan
pub fn segment(self, segment: usize, total_segments: usize) -> Self
```

**Example:**
```rust
// Scan all items
let scan = Scan::new();

// With limit
let scan = Scan::new().limit(100);

// Parallel scan (4 workers)
let scans: Vec<_> = (0..4)
    .map(|i| Scan::new().segment(i, 4))
    .collect();
```

---

### 36.2.4 Update

Update builder for update expressions.

```rust
pub struct Update { /* ... */ }
```

**Methods:**

```rust
// Create for partition key
pub fn new(pk: &[u8]) -> Self

// Create with sort key
pub fn with_sk(pk: &[u8], sk: &[u8]) -> Self

// Set expression and values
pub fn expression(self, expr: impl Into<String>) -> Self
pub fn condition(self, condition: impl Into<String>) -> Self
pub fn value(self, placeholder: impl Into<String>, value: Value) -> Self
pub fn name(self, placeholder: impl Into<String>, name: impl Into<String>) -> Self
```

**Example:**
```rust
let update = Update::new(b"user#123")
    .expression("SET age = :age, #n = :name REMOVE temp")
    .condition("age < :max")
    .value(":age", Value::number(30))
    .value(":name", Value::string("Alice"))
    .value(":max", Value::number(100))
    .name("#n", "name");
```

---

### 36.2.5 Batch Builders

#### BatchGetRequest

```rust
pub fn new() -> Self
pub fn add_key(self, pk: &[u8]) -> Self
pub fn add_key_with_sk(self, pk: &[u8], sk: &[u8]) -> Self
```

#### BatchWriteRequest

```rust
pub fn new() -> Self
pub fn put(self, pk: &[u8], item: Item) -> Self
pub fn put_with_sk(self, pk: &[u8], sk: &[u8], item: Item) -> Self
pub fn delete(self, pk: &[u8]) -> Self
pub fn delete_with_sk(self, pk: &[u8], sk: &[u8]) -> Self
```

---

### 36.2.6 Transaction Builders

#### TransactGetRequest

```rust
pub fn new() -> Self
pub fn get(self, pk: &[u8]) -> Self
pub fn get_with_sk(self, pk: &[u8], sk: &[u8]) -> Self
```

#### TransactWriteRequest

```rust
pub fn new() -> Self

pub fn put(self, pk: &[u8], item: Item) -> Self
pub fn put_with_condition(self, pk: &[u8], item: Item, condition: &str) -> Self

pub fn update(self, pk: &[u8], expression: &str) -> Self
pub fn update_with_condition(self, pk: &[u8], expression: &str, condition: &str) -> Self

pub fn delete(self, pk: &[u8]) -> Self
pub fn delete_with_condition(self, pk: &[u8], condition: &str) -> Self

pub fn condition_check(self, pk: &[u8], condition: &str) -> Self

pub fn value(self, placeholder: &str, value: Value) -> Self
pub fn name(self, placeholder: &str, name: &str) -> Self
```

---

## 36.3 Response Types

### 36.3.1 QueryResponse

```rust
pub struct QueryResponse {
    pub items: Vec<Item>,
    pub count: usize,
    pub last_key: Option<(Bytes, Option<Bytes>)>,
    pub scanned_count: usize,
}
```

---

### 36.3.2 ScanResponse

```rust
pub struct ScanResponse {
    pub items: Vec<Item>,
    pub count: usize,
    pub last_key: Option<(Bytes, Option<Bytes>)>,
    pub scanned_count: usize,
}
```

---

### 36.3.3 UpdateResponse

```rust
pub struct UpdateResponse {
    pub item: Item,
}
```

---

### 36.3.4 BatchGetResponse

```rust
pub struct BatchGetResponse {
    pub items: HashMap<Key, Item>,
}
```

---

### 36.3.5 BatchWriteResponse

```rust
pub struct BatchWriteResponse {
    pub processed_count: usize,
}
```

---

### 36.3.6 TransactGetResponse

```rust
pub struct TransactGetResponse {
    pub items: Vec<Option<Item>>,
}
```

---

### 36.3.7 TransactWriteResponse

```rust
pub struct TransactWriteResponse {
    pub committed_count: usize,
}
```

---

## 36.4 Configuration Types

### 36.4.1 TableSchema

Schema definition for indexes, TTL, and streams.

```rust
pub struct TableSchema { /* ... */ }

impl TableSchema {
    pub fn new() -> Self

    // Indexes
    pub fn add_local_index(self, index: LocalSecondaryIndex) -> Self
    pub fn add_global_index(self, index: GlobalSecondaryIndex) -> Self

    // TTL
    pub fn with_ttl(self, attribute_name: impl Into<String>) -> Self

    // Streams
    pub fn with_stream(self, config: StreamConfig) -> Self
}
```

---

### 36.4.2 DatabaseConfig

Runtime configuration.

```rust
pub struct DatabaseConfig {
    pub memtable_threshold: usize,
    pub compaction_enabled: bool,
    pub compaction_threshold: usize,
    pub max_background_jobs: usize,
    pub block_size: usize,
    pub bloom_bits_per_key: usize,
}
```

---

## 36.5 Error Handling

### 36.5.1 Error Type

```rust
pub enum Error {
    Io(io::Error),
    Corruption(String),
    NotFound(String),
    InvalidArgument(String),
    ConditionalCheckFailed(String),
    TransactionCanceled(String),
    InvalidExpression(String),
    InvalidQuery(String),
    // ... more variants
}

impl Error {
    pub fn code(&self) -> &'static str
    pub fn is_retryable(&self) -> bool
    pub fn with_context(self, context: &str) -> Error
}
```

**Error Codes:**
- `IO_ERROR`: I/O errors
- `NOT_FOUND`: Key not found
- `INVALID_ARGUMENT`: Invalid parameters
- `CONDITIONAL_CHECK_FAILED`: Condition not met
- `TRANSACTION_CANCELED`: Transaction failed
- `CORRUPTION`: Data corruption
- `RESOURCE_EXHAUSTED`: Rate limits

**Example:**
```rust
match db.get(b"key") {
    Ok(Some(item)) => { /* ... */ }
    Ok(None) => { /* not found */ }
    Err(e) => {
        eprintln!("Error {}: {}", e.code(), e);
        if e.is_retryable() {
            // Retry logic
        }
    }
}
```

This API reference provides complete coverage of KeystoneDB's public interface. Use it as a quick reference while building applications.

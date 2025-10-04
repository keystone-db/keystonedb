# Chapter 1: What is KeystoneDB?

## Introduction

KeystoneDB is a single-file, embedded, DynamoDB-style database written in Rust that brings the power and flexibility of Amazon DynamoDB to your local applications. It combines the simplicity of an embedded database like SQLite with the advanced features of a modern NoSQL system, offering developers a familiar DynamoDB API without the complexity of managing cloud infrastructure.

Whether you're building a desktop application that needs persistent storage, a mobile app requiring offline-first capabilities, or a backend service that wants to avoid cloud dependencies, KeystoneDB provides a robust, high-performance solution that runs entirely on your machine.

## What Makes KeystoneDB Different?

### The Best of Both Worlds

Traditional embedded databases like SQLite excel at relational data but lack modern NoSQL features like secondary indexes, TTL (Time To Live), and change streams. Cloud databases like DynamoDB offer these advanced capabilities but require network connectivity, incur costs, and add latency to every operation.

KeystoneDB bridges this gap by implementing the complete DynamoDB API locally. You get:

- **DynamoDB compatibility**: Familiar Put/Get/Delete operations, Query and Scan with secondary indexes
- **Local-first architecture**: Zero network latency, no cloud dependencies, complete data ownership
- **ACID transactions**: Full transactional support for complex operations
- **Advanced features**: LSI/GSI indexes, TTL, Streams, conditional operations, and PartiQL queries
- **Production-ready storage**: LSM tree engine with crash recovery, bloom filters, and background compaction

### Single-File Simplicity

Unlike traditional databases that scatter data across multiple files and directories, KeystoneDB uses a single database directory with a clean, predictable structure:

```
mydb.keystone/
├── wal.log              # Write-ahead log for durability
└── {stripe}-{sst}.sst   # Sorted string tables (e.g., 042-5.sst)
```

This makes backups trivial (copy one directory), deployment simple (no complex installation), and debugging easier (all data in one place). The `.keystone` extension clearly identifies database directories, and the internal structure is optimized for both write and read performance.

### Performance That Scales

At the heart of KeystoneDB is a sophisticated LSM (Log-Structured Merge-tree) storage engine with 256 independent stripes. This architecture provides:

- **Write performance**: 10,000-50,000 operations per second with group commit batching
- **Read performance**: 100,000+ operations per second from in-memory caches
- **Parallel operations**: 256-way parallelism for scans and compaction
- **Efficient queries**: Bloom filters reduce unnecessary disk I/O by ~99%
- **Background optimization**: Automatic compaction reclaims space and removes tombstones

The multi-stripe architecture means that write-heavy workloads can saturate multiple CPU cores, while the LSM design ensures predictable write performance even as your dataset grows.

## Core Features

### Data Model

KeystoneDB uses the DynamoDB data model, which centers around three key concepts:

**1. Items**: The fundamental unit of storage, similar to a row in SQL or a document in MongoDB. Each item is a collection of attributes stored as a JSON-like structure.

**2. Keys**: Each item has a partition key (required) and optionally a sort key (optional). Together, these form a composite key that uniquely identifies the item:
   - **Partition Key (PK)**: Determines which stripe stores the item (via CRC32 hashing)
   - **Sort Key (SK)**: Enables efficient range queries within a partition

**3. Attributes**: Flexible, schema-less fields within items. KeystoneDB supports all DynamoDB value types:
   - **Scalar types**: String (S), Number (N), Binary (B), Boolean (Bool), Null
   - **Document types**: List (L), Map (M) for nested structures
   - **Special types**: Vector of floats (VecF32) for embeddings, Timestamp (Ts) for time-series data

Example item structure:

```rust
{
    "pk": "user#alice",
    "sk": "profile",
    "name": "Alice Johnson",
    "age": 30,
    "email": "alice@example.com",
    "tags": ["developer", "rust", "databases"],
    "metadata": {
        "created": 1704067200,
        "updated": 1704153600
    }
}
```

### CRUD Operations

The foundation of any database is reliable Create, Read, Update, and Delete operations. KeystoneDB provides a clean, type-safe API:

**Put**: Insert or overwrite an item
```rust
use kstone_api::{Database, ItemBuilder};

let db = Database::open("mydb.keystone")?;
let item = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .bool("active", true)
    .build();

db.put(b"user#123", item)?;
```

**Get**: Retrieve an item by key
```rust
if let Some(item) = db.get(b"user#123")? {
    println!("Found user: {:?}", item);
}
```

**Delete**: Remove an item
```rust
db.delete(b"user#123")?;
```

**Update**: Modify specific attributes without replacing the entire item
```rust
use kstone_api::Update;
use kstone_core::Value;

let update = Update::new(b"user#123")
    .expression("SET age = age + 1, last_login = :now")
    .value(":now", Value::number(1704067200));

db.update(update)?;
```

### Query and Scan

Beyond simple key-value operations, KeystoneDB provides powerful querying capabilities:

**Query**: Efficiently retrieve items within a partition with sort key conditions
```rust
use kstone_api::Query;

// Get all posts for a user, sorted by timestamp
let query = Query::new(b"user#alice")
    .sk_begins_with(b"post#")
    .limit(20)
    .forward(false); // Reverse order (newest first)

let response = db.query(query)?;
for item in response.items {
    println!("Post: {:?}", item);
}
```

**Scan**: Traverse the entire table with optional filtering
```rust
use kstone_api::Scan;

// Find all active users
let scan = Scan::new()
    .filter_expression("active = :val")
    .expression_value(":val", Value::Bool(true))
    .limit(100);

let response = db.scan(scan)?;
```

For large tables, parallel scans distribute work across multiple threads:

```rust
use std::thread;

// Scan with 4 parallel segments
let handles: Vec<_> = (0..4).map(|segment| {
    let db = db.clone();
    thread::spawn(move || {
        let scan = Scan::new().segment(segment, 4);
        db.scan(scan)
    })
}).collect();

let mut all_items = Vec::new();
for handle in handles {
    if let Ok(Ok(response)) = handle.join() {
        all_items.extend(response.items);
    }
}
```

### Secondary Indexes

KeystoneDB supports both Local Secondary Indexes (LSI) and Global Secondary Indexes (GSI), matching DynamoDB's indexing capabilities:

**Local Secondary Indexes (LSI)**: Use the same partition key as the base table but provide an alternate sort key. Perfect for querying the same partition in different ways.

```rust
use kstone_api::{Database, TableSchema, LocalSecondaryIndex};

let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"))
    .add_local_index(LocalSecondaryIndex::new("score-index", "score"));

let db = Database::create_with_schema("mydb.keystone", schema)?;

// Later, query by email instead of sort key
let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice@");
```

**Global Secondary Indexes (GSI)**: Use different partition and sort keys, enabling queries across the entire table by non-key attributes.

```rust
use kstone_api::GlobalSecondaryIndex;

let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::with_sort_key("category-price-index", "category", "price")
    );

let db = Database::create_with_schema("mydb.keystone", schema)?;

// Query all electronics sorted by price
let query = Query::new(b"electronics")
    .index("category-price-index")
    .sk_between(b"100", b"1000");
```

### Time To Live (TTL)

Automatically expire items after a specified time, perfect for sessions, caches, and temporary data:

```rust
use std::time::SystemTime;

let schema = TableSchema::new().with_ttl("expiresAt");
let db = Database::create_with_schema("mydb.keystone", schema)?;

let now = SystemTime::now()
    .duration_since(SystemTime::UNIX_EPOCH)?
    .as_secs() as i64;

// Session expires in 1 hour
db.put(b"session#abc123", ItemBuilder::new()
    .string("userId", "user#456")
    .number("expiresAt", now + 3600)
    .build())?;

// After 1 hour, get() returns None (lazy deletion)
```

### Streams (Change Data Capture)

Track all changes to your data with built-in change streams:

```rust
use kstone_api::{StreamConfig, StreamViewType};

let schema = TableSchema::new()
    .with_stream(StreamConfig::enabled()
        .with_view_type(StreamViewType::NewAndOldImages)
        .with_buffer_size(1000));

let db = Database::create_with_schema("mydb.keystone", schema)?;

// Perform operations
db.put(b"user#123", item1)?;
db.put(b"user#123", item2)?;
db.delete(b"user#123")?;

// Read change stream
let records = db.read_stream(None)?;
for record in records {
    match record.event_type {
        StreamEventType::Insert => println!("New item created"),
        StreamEventType::Modify => println!("Item updated"),
        StreamEventType::Remove => println!("Item deleted"),
    }
}
```

### Transactions

ACID transactions ensure that multiple operations succeed or fail atomically:

```rust
use kstone_api::TransactWriteRequest;

// Transfer balance between accounts
let request = TransactWriteRequest::new()
    .update_with_condition(
        b"account#source",
        "SET balance = balance - :amount",
        "balance >= :amount"
    )
    .update(
        b"account#dest",
        "SET balance = balance + :amount"
    )
    .value(":amount", Value::number(100));

match db.transact_write(request) {
    Ok(_) => println!("Transfer succeeded"),
    Err(_) => println!("Transfer failed - insufficient funds"),
}
```

### PartiQL Support

Query your data using SQL-like syntax with PartiQL:

```bash
# SELECT with projection
kstone query mydb.keystone "SELECT name, email FROM users WHERE pk = 'user#123'"

# Scan with filtering
kstone query mydb.keystone "SELECT * FROM users WHERE age > 25 LIMIT 100"

# INSERT
kstone query mydb.keystone "INSERT INTO users VALUE {'pk': 'user#456', 'name': 'Bob', 'age': 35}"

# UPDATE with arithmetic
kstone query mydb.keystone "UPDATE users SET age = age + 1 WHERE pk = 'user#456'"
```

## Comparison with Other Databases

### KeystoneDB vs. DynamoDB

| Feature | KeystoneDB | DynamoDB |
|---------|-----------|----------|
| **Location** | Local filesystem | AWS Cloud |
| **Latency** | Microseconds | Milliseconds (network) |
| **Cost** | Free (compute only) | Pay per request/capacity |
| **Offline Support** | Full offline operation | Requires connectivity |
| **API Compatibility** | DynamoDB-compatible | Native DynamoDB API |
| **Indexes** | LSI + GSI | LSI + GSI |
| **Transactions** | Full ACID support | ACID with limitations |
| **PartiQL** | Full SQL support | Limited PartiQL |
| **Data Ownership** | Complete local control | AWS manages data |
| **Scalability** | Single machine limits | Unlimited horizontal scale |

**When to use KeystoneDB**: Desktop applications, mobile apps, offline-first systems, development/testing, data sovereignty requirements, cost-sensitive projects.

**When to use DynamoDB**: Cloud-native applications, massive scale requirements, global distribution, managed service preference, tight AWS integration.

### KeystoneDB vs. SQLite

| Feature | KeystoneDB | SQLite |
|---------|-----------|--------|
| **Data Model** | Document/NoSQL | Relational/SQL |
| **Schema** | Schema-optional | Schema-required |
| **Secondary Indexes** | LSI + GSI with projections | Standard B-tree indexes |
| **Transactions** | Item-level + multi-item | Full ACID transactions |
| **Query Language** | PartiQL (SQL-like) | SQL |
| **TTL** | Built-in automatic expiration | Manual with triggers |
| **Change Streams** | Built-in CDC | Requires triggers |
| **Concurrency** | Optimistic locking | WAL mode for writers |
| **Write Performance** | Optimized for writes (LSM) | Optimized for reads (B-tree) |

**When to use KeystoneDB**: NoSQL data models, high write throughput, DynamoDB migration, automatic expiration, change tracking, schema flexibility.

**When to use SQLite**: Relational data, complex joins, SQL expertise, maximum portability, embedded analytics, simple read-heavy workloads.

### KeystoneDB vs. RocksDB/LevelDB

| Feature | KeystoneDB | RocksDB/LevelDB |
|---------|-----------|-----------------|
| **API Level** | High-level (DynamoDB) | Low-level (key-value) |
| **Data Types** | Rich type system | Byte arrays only |
| **Queries** | Query/Scan with conditions | Iterator-based only |
| **Indexes** | Automatic LSI/GSI | Manual implementation |
| **Transactions** | Built-in ACID | Optional (RocksDB) |
| **Change Streams** | Built-in | Manual implementation |
| **Use Case** | Application database | Storage engine component |

**When to use KeystoneDB**: Application-level storage, DynamoDB-style queries, minimal boilerplate, rapid development.

**When to use RocksDB**: Building custom databases, maximum control, specialized storage engines, embedded in larger systems.

## Use Cases

### 1. Desktop Applications

KeystoneDB is ideal for desktop applications that need persistent storage without database setup:

```rust
// User preferences and settings
db.put(b"config#app", ItemBuilder::new()
    .string("theme", "dark")
    .number("fontSize", 14)
    .bool("notifications", true)
    .build())?;

// Document storage with metadata
db.put_with_sk(
    b"document#report-2024",
    b"metadata",
    ItemBuilder::new()
        .string("title", "Annual Report 2024")
        .string("author", "alice")
        .number("created", timestamp)
        .build()
)?;

// Recent files with TTL
db.put(b"recent#file1", ItemBuilder::new()
    .string("path", "/home/user/document.txt")
    .number("expiresAt", now + 86400 * 7) // 7 days
    .build())?;
```

### 2. Offline-First Mobile Apps

Build mobile applications that work seamlessly offline and sync when connected:

```rust
// Local cache with automatic expiration
let schema = TableSchema::new().with_ttl("expiresAt");
let db = Database::create_with_schema("cache.keystone", schema)?;

// Cache API responses
db.put(b"cache#user#123", ItemBuilder::new()
    .string("data", user_json)
    .number("expiresAt", now + 300) // 5 minutes
    .build())?;

// Queue operations for later sync
db.put_with_sk(
    b"queue#pending",
    format!("op#{}", timestamp).as_bytes(),
    ItemBuilder::new()
        .string("operation", "update_profile")
        .string("payload", payload_json)
        .build()
)?;
```

### 3. Development and Testing

Use KeystoneDB to develop and test DynamoDB applications locally:

```rust
// Same code works locally and in production with DynamoDB
#[cfg(test)]
fn test_user_registration() {
    let db = Database::create_in_memory()?; // Fast in-memory mode for tests

    let user = ItemBuilder::new()
        .string("email", "test@example.com")
        .number("created", timestamp)
        .build();

    db.put(b"user#test", user)?;

    let retrieved = db.get(b"user#test")?.unwrap();
    assert_eq!(retrieved.get("email"), Some(&Value::S("test@example.com".into())));
}
```

### 4. Edge Computing

Deploy databases at the edge without cloud dependencies:

```rust
// IoT sensor data with time-series support
db.put_with_sk(
    b"sensor#temperature",
    format!("{}", timestamp).as_bytes(),
    ItemBuilder::new()
        .number("value", temperature)
        .number("humidity", humidity)
        .build()
)?;

// Query recent readings
let query = Query::new(b"sensor#temperature")
    .sk_gte(format!("{}", now - 3600).as_bytes()) // Last hour
    .limit(100);

let readings = db.query(query)?;
```

### 5. Session Storage

High-performance session management with automatic cleanup:

```rust
let schema = TableSchema::new().with_ttl("expiresAt");
let db = Database::create_with_schema("sessions.keystone", schema)?;

// Create session with 30-minute expiration
db.put(b"session#abc123", ItemBuilder::new()
    .string("userId", "user#456")
    .string("ipAddress", "192.168.1.1")
    .number("expiresAt", now + 1800)
    .build())?;

// Sessions automatically expire - no manual cleanup needed
```

### 6. Change Data Capture

Track all changes for audit logs, replication, or event sourcing:

```rust
let schema = TableSchema::new()
    .with_stream(StreamConfig::enabled());
let db = Database::create_with_schema("audit.keystone", schema)?;

// All operations are automatically tracked
db.put(b"user#123", user)?;
db.update(Update::new(b"user#123").expression("SET status = :s"))?;
db.delete(b"user#123")?;

// Export audit trail
let records = db.read_stream(None)?;
for record in records {
    println!("Event: {:?} at {}", record.event_type, record.timestamp);
    if let Some(old) = record.old_image {
        println!("  Before: {:?}", old);
    }
    if let Some(new) = record.new_image {
        println!("  After: {:?}", new);
    }
}
```

## Architecture Overview

### Storage Engine

KeystoneDB uses a Log-Structured Merge (LSM) tree storage engine optimized for write-heavy workloads:

**Write Path**:
1. Record appended to Write-Ahead Log (WAL) for durability
2. Record added to in-memory memtable (sorted by key)
3. WAL flushed to disk with group commit batching
4. When memtable reaches 1000 records, flush to disk as SST file
5. Background compaction merges SST files and reclaims space

**Read Path**:
1. Check in-memory memtable first (most recent data)
2. If not found, scan SST files from newest to oldest
3. Bloom filters skip SSTs that definitely don't contain the key
4. First match wins (newer versions shadow older ones)

### 256-Stripe Architecture

KeystoneDB partitions data across 256 independent stripes for parallelism:

- Each stripe has its own memtable and SST files
- Partition key hash determines stripe: `stripe_id = crc32(pk) % 256`
- Items with same partition key always go to same stripe (enables efficient range queries)
- Independent flush and compaction per stripe
- Parallel scans distribute work across all stripes

```
Stripe 0:   [Memtable] → [000-1.sst, 000-2.sst, ...]
Stripe 1:   [Memtable] → [001-1.sst, 001-2.sst, ...]
...
Stripe 255: [Memtable] → [255-1.sst, 255-2.sst, ...]
```

### Crash Recovery

KeystoneDB ensures data durability through Write-Ahead Logging:

1. All writes are first recorded in the WAL
2. WAL is flushed to disk before acknowledging the write
3. On crash, replay WAL to reconstruct in-memory state
4. SST files are immutable - always in a consistent state
5. Checkpointing tracks which WAL records have been persisted to SST

### Bloom Filters

Each SST block has a bloom filter (10 bits per key, ~1% false positive rate):

- Test if key *might* be in block (no false negatives)
- Skip blocks that definitely don't contain the key
- Reduces disk I/O by ~99% for point lookups
- Small memory footprint (~1.25 bytes per key)

### Compaction

Background compaction keeps storage efficient:

- **Trigger**: When stripe reaches threshold (default: 10 SST files)
- **Process**: K-way merge of SST files, keeping newest version of each key
- **Benefits**: Removes deleted items (tombstones), reduces read amplification
- **Stats**: Tracks bytes read/written/reclaimed, tombstones removed

## When to Use KeystoneDB

KeystoneDB is an excellent choice when you need:

1. **DynamoDB-compatible API** without cloud dependencies
2. **Local-first architecture** with complete data ownership
3. **Offline operation** without network connectivity
4. **Low latency** (microseconds vs. milliseconds over network)
5. **Zero cost** for storage and operations (pay only for compute)
6. **Advanced features** like indexes, TTL, streams, and transactions
7. **Embedded database** that's easy to deploy and backup
8. **Development environment** for testing DynamoDB applications

KeystoneDB may not be suitable if you need:

1. **Distributed deployment** across multiple machines (single-machine only)
2. **Massive scale** beyond single-machine limits (use cloud DynamoDB)
3. **Built-in replication** or high availability (single point of failure)
4. **Multi-user concurrent access** over network (use gRPC server mode or cloud)

## What's Next?

Now that you understand what KeystoneDB is and what it can do, the next chapters will help you get started:

- **Chapter 2: Quick Start Guide** - Create your first database in 5 minutes
- **Chapter 3: Installation & Setup** - Detailed installation and configuration

By the end of Part I, you'll have a working KeystoneDB installation and hands-on experience with basic operations. You'll be ready to explore advanced features in Part II and build real applications in Part III.

# Chapter 25: Remote Clients

The KeystoneDB client library (`kstone-client`) provides a Rust-native interface for connecting to remote KeystoneDB servers via gRPC. This chapter explores the client architecture, connection management, operation builders, error handling, and practical usage patterns.

## Overview

The client library transforms gRPC protocol buffers into idiomatic Rust APIs that feel similar to the embedded `Database` API. Key features include:

- **Ergonomic Builders**: Fluent interfaces for queries, scans, batches, and transactions
- **Type Safety**: Compile-time guarantees for request construction
- **Async/Await**: Full async support with Tokio
- **Error Handling**: Rich error types with detailed context
- **Connection Pooling**: Efficient connection reuse (via gRPC's HTTP/2 multiplexing)

The client is implemented in the `kstone-client` crate and can be used by any Rust application.

## Client Architecture

### Component Structure

```
┌──────────────────────────────────────────────────┐
│         Your Application Code                    │
└──────────────────┬───────────────────────────────┘
                   │
                   ▼
┌──────────────────────────────────────────────────┐
│  Client API (kstone-client)                      │
│  ┌────────────────────────────────────────────┐  │
│  │  Client                                    │  │
│  │  - Connection management                   │  │
│  │  - CRUD operations                         │  │
│  ├────────────────────────────────────────────┤  │
│  │  Builders                                  │  │
│  │  - RemoteQuery                             │  │
│  │  - RemoteScan                              │  │
│  │  - RemoteBatchGetRequest                   │  │
│  │  - RemoteBatchWriteRequest                 │  │
│  │  - RemoteTransactGetRequest                │  │
│  │  - RemoteTransactWriteRequest              │  │
│  │  - RemoteUpdate                            │  │
│  ├────────────────────────────────────────────┤  │
│  │  Type Conversions (convert.rs)             │  │
│  │  - proto ↔ KeystoneDB types                │  │
│  ├────────────────────────────────────────────┤  │
│  │  Error Handling (error.rs)                 │  │
│  │  - ClientError enum                        │  │
│  │  - Status → Error mapping                  │  │
│  └────────────────────────────────────────────┘  │
└──────────────────┬───────────────────────────────┘
                   │ gRPC/HTTP2
                   ▼
┌──────────────────────────────────────────────────┐
│         Tonic gRPC Client                        │
│  - HTTP/2 connection pooling                     │
│  - Request/response serialization                │
│  - Automatic reconnection                        │
└──────────────────┬───────────────────────────────┘
                   │
                   ▼
           ┌───────────────┐
           │  gRPC Server  │
           │ (kstone-server)│
           └───────────────┘
```

### Client State

The `Client` struct wraps the generated gRPC client:

```rust
pub struct Client {
    inner: KeystoneDbClient<Channel>,
}
```

The `Channel` is a connection pool that:
- Reuses HTTP/2 connections efficiently
- Handles reconnection automatically
- Multiplexes multiple requests over a single connection
- Provides backpressure when the server is overloaded

## Connecting to a Server

### Basic Connection

The simplest way to connect:

```rust
use kstone_client::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = Client::connect("http://localhost:50051").await?;

    // Client is now ready for operations
    Ok(())
}
```

### Connection Options

The client uses Tonic's default connection settings, which are suitable for most use cases. For advanced scenarios, you might want to configure:

**Timeout**:
```rust
use tonic::transport::Channel;
use std::time::Duration;

let channel = Channel::from_static("http://localhost:50051")
    .timeout(Duration::from_secs(30))
    .connect()
    .await?;

let client = KeystoneDbClient::new(channel);
```

**Connection Limits**:
```rust
let channel = Channel::from_static("http://localhost:50051")
    .concurrency_limit(100)  // Max concurrent requests
    .connect()
    .await?;
```

**Keepalive**:
```rust
let channel = Channel::from_static("http://localhost:50051")
    .keep_alive_while_idle(true)
    .http2_keep_alive_interval(Duration::from_secs(30))
    .connect()
    .await?;
```

### Connection Pooling

Unlike traditional connection pools, gRPC uses a single HTTP/2 connection with multiplexing. The `Channel` automatically:

- Reuses connections across requests
- Handles connection failures with automatic retry
- Distributes load across multiple backend servers (when configured)

For most applications, a single `Client` instance per application is sufficient and recommended.

## CRUD Operations

The client provides async methods for basic database operations that mirror the embedded API.

### Put Operations

**Simple Put**:
```rust
use std::collections::HashMap;
use kstone_core::Value;

let mut item = HashMap::new();
item.insert("name".to_string(), Value::S("Alice".to_string()));
item.insert("age".to_string(), Value::N("30".to_string()));
item.insert("active".to_string(), Value::Bool(true));

client.put(b"user#123", item).await?;
```

**Put with Sort Key**:
```rust
let mut profile = HashMap::new();
profile.insert("bio".to_string(), Value::S("Software engineer".to_string()));
profile.insert("location".to_string(), Value::S("San Francisco".to_string()));

client.put_with_sk(b"user#123", b"profile", profile).await?;
```

**Conditional Put** (Put-if-not-exists):
```rust
let mut item = HashMap::new();
item.insert("username".to_string(), Value::S("alice".to_string()));

let mut values = HashMap::new();
// No additional values needed for attribute_not_exists

client.put_conditional(
    b"user#123",
    item,
    "attribute_not_exists(username)",
    values
).await?;
```

### Get Operations

**Simple Get**:
```rust
let item = client.get(b"user#123").await?;

match item {
    Some(data) => {
        if let Some(Value::S(name)) = data.get("name") {
            println!("Name: {}", name);
        }
    }
    None => println!("Item not found"),
}
```

**Get with Sort Key**:
```rust
let profile = client.get_with_sk(b"user#123", b"profile").await?;

if let Some(data) = profile {
    println!("Profile: {:?}", data);
}
```

### Delete Operations

**Simple Delete**:
```rust
client.delete(b"user#123").await?;
```

**Delete with Sort Key**:
```rust
client.delete_with_sk(b"user#123", b"profile").await?;
```

**Conditional Delete**:
```rust
let mut values = HashMap::new();
values.insert(":status".to_string(), Value::S("inactive".to_string()));

client.delete_conditional(
    b"user#123",
    "status = :status",
    values
).await?;
```

## Query Operations

The `RemoteQuery` builder provides a fluent interface for querying items.

### Basic Query

Query all items with a partition key:

```rust
use kstone_client::RemoteQuery;

let query = RemoteQuery::new(b"org#acme");
let response = client.query(query).await?;

println!("Found {} items", response.count);
for item in response.items {
    println!("{:?}", item);
}
```

### Sort Key Conditions

**Equals**:
```rust
let query = RemoteQuery::new(b"org#acme")
    .sk_eq(b"USER#alice");

let response = client.query(query).await?;
```

**Begins With**:
```rust
// Find all users in org
let query = RemoteQuery::new(b"org#acme")
    .sk_begins_with(b"USER#");

let response = client.query(query).await?;
```

**Between**:
```rust
// Date range query
let query = RemoteQuery::new(b"sensor#123")
    .sk_between(b"2024-01-01", b"2024-12-31");

let response = client.query(query).await?;
```

**Greater Than / Less Than**:
```rust
// Recent items
let query = RemoteQuery::new(b"user#123")
    .sk_gt(b"2024-06-01");

let response = client.query(query).await?;

// Older items
let query = RemoteQuery::new(b"user#123")
    .sk_lte(b"2024-01-01");

let response = client.query(query).await?;
```

### Pagination

Handle large result sets with pagination:

```rust
let mut all_items = Vec::new();
let mut query = RemoteQuery::new(b"org#acme")
    .limit(100);  // Page size

loop {
    let response = client.query(query.clone()).await?;
    all_items.extend(response.items);

    // Check if there are more results
    match response.last_key {
        Some((pk, sk)) => {
            // Continue from last key
            query = RemoteQuery::new(&pk)
                .limit(100)
                .start_after(&pk, sk.as_deref());
        }
        None => break,  // No more results
    }
}

println!("Retrieved {} total items", all_items.len());
```

### Reverse Iteration

Query in reverse order (most recent first):

```rust
let query = RemoteQuery::new(b"user#123")
    .forward(false)  // Reverse order
    .limit(10);

let response = client.query(query).await?;
```

### Querying Indexes

Query Local or Global Secondary Indexes:

```rust
// Query LSI by email
let query = RemoteQuery::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice@");

let response = client.query(query).await?;

// Query GSI by status
let query = RemoteQuery::new(b"active")  // GSI partition key
    .index("status-index")
    .limit(50);

let response = client.query(query).await?;
```

### Query Response

The `RemoteQueryResponse` contains:

```rust
pub struct RemoteQueryResponse {
    pub items: Vec<Item>,              // Items found
    pub count: usize,                  // Number of items returned
    pub scanned_count: usize,          // Number of items examined
    pub last_key: Option<(Bytes, Option<Bytes>)>,  // For pagination
}
```

## Scan Operations

The `RemoteScan` builder scans the entire table or index.

### Basic Scan

Scan all items in the table:

```rust
use kstone_client::RemoteScan;

let scan = RemoteScan::new();
let response = client.scan(scan).await?;

println!("Scanned {} items", response.count);
```

### Scan with Limit

Limit the number of items returned:

```rust
let scan = RemoteScan::new()
    .limit(100);

let response = client.scan(scan).await?;
```

### Paginated Scan

Scan large tables in pages:

```rust
let mut all_items = Vec::new();
let mut last_key = None;

loop {
    let mut scan = RemoteScan::new().limit(100);

    if let Some((pk, sk)) = last_key {
        scan = scan.start_after(&pk, sk.as_deref());
    }

    let response = client.scan(scan).await?;
    all_items.extend(response.items);

    last_key = response.last_key;
    if last_key.is_none() {
        break;
    }
}

println!("Total items: {}", all_items.len());
```

### Parallel Scan

Distribute scan across multiple segments for faster processing:

```rust
use tokio::task::JoinSet;

let total_segments = 4;
let mut tasks = JoinSet::new();

for segment in 0..total_segments {
    let mut client = client.clone();  // Clone the client for each task

    tasks.spawn(async move {
        let scan = RemoteScan::new()
            .segment(segment, total_segments);

        client.scan(scan).await
    });
}

let mut all_items = Vec::new();
while let Some(result) = tasks.join_next().await {
    let response = result??;
    all_items.extend(response.items);
}

println!("Scanned {} items with {} segments", all_items.len(), total_segments);
```

### Scan Response

The `RemoteScanResponse` has the same structure as `RemoteQueryResponse`:

```rust
pub struct RemoteScanResponse {
    pub items: Vec<Item>,
    pub count: usize,
    pub scanned_count: usize,
    pub last_key: Option<(Bytes, Option<Bytes>)>,
}
```

## Batch Operations

Batch operations allow you to read or write multiple items in a single RPC call.

### Batch Get

Retrieve multiple items efficiently:

```rust
use kstone_client::RemoteBatchGetRequest;

let batch = RemoteBatchGetRequest::new()
    .add_key(b"user#1")
    .add_key(b"user#2")
    .add_key(b"user#3")
    .add_key_with_sk(b"user#1", b"profile");

let response = client.batch_get(batch).await?;

println!("Retrieved {} items", response.count);
for item in response.items {
    println!("{:?}", item);
}
```

**Note**: The response only includes items that were found. Missing items are silently omitted.

### Batch Write

Write multiple items in one call:

```rust
use kstone_client::RemoteBatchWriteRequest;

let mut item1 = HashMap::new();
item1.insert("name".to_string(), Value::S("Alice".to_string()));

let mut item2 = HashMap::new();
item2.insert("name".to_string(), Value::S("Bob".to_string()));

let batch = RemoteBatchWriteRequest::new()
    .put(b"user#1", item1)
    .put(b"user#2", item2)
    .delete(b"user#old")  // Delete an item
    .put_with_sk(b"user#1", b"profile", HashMap::new());

let response = client.batch_write(batch).await?;
println!("Batch write success: {}", response.success);
```

### Bulk Data Loading

Use batch writes for efficient bulk loading:

```rust
use kstone_client::RemoteBatchWriteRequest;

const BATCH_SIZE: usize = 25;  // DynamoDB-compatible batch size

for chunk in items.chunks(BATCH_SIZE) {
    let mut batch = RemoteBatchWriteRequest::new();

    for (i, data) in chunk.iter().enumerate() {
        let pk = format!("item#{}", i);
        let mut item = HashMap::new();
        item.insert("data".to_string(), Value::S(data.clone()));

        batch = batch.put(pk.as_bytes(), item);
    }

    client.batch_write(batch).await?;
}

println!("Loaded {} items", items.len());
```

## Transactional Operations

The client supports atomic transactions across multiple items.

### Transactional Get

Read multiple items atomically (consistent snapshot):

```rust
use kstone_client::RemoteTransactGetRequest;

let request = RemoteTransactGetRequest::new()
    .get(b"account#source")
    .get(b"account#dest");

let response = client.transact_get(request).await?;

// Items returned in same order as request
for (i, item_opt) in response.items.iter().enumerate() {
    match item_opt {
        Some(item) => println!("Item {}: {:?}", i, item),
        None => println!("Item {} not found", i),
    }
}
```

### Transactional Write

Write multiple items atomically with conditions:

```rust
use kstone_client::RemoteTransactWriteRequest;

// Transfer balance between accounts
let request = RemoteTransactWriteRequest::new()
    // Deduct from source (only if sufficient balance)
    .update_with_condition(
        b"account#source",
        "SET balance = balance - :amount",
        "balance >= :amount"
    )
    // Add to destination
    .update(
        b"account#dest",
        "SET balance = balance + :amount"
    )
    .value(":amount", Value::N("100".to_string()));

match client.transact_write(request).await {
    Ok(_) => println!("Transaction committed"),
    Err(ClientError::TransactionAborted(msg)) => {
        println!("Transaction failed: {}", msg);
    }
    Err(e) => return Err(e.into()),
}
```

### Transaction Operations

The `RemoteTransactWriteRequest` supports four operation types:

**Put**: Insert or replace item
```rust
let mut item = HashMap::new();
item.insert("name".to_string(), Value::S("Alice".to_string()));

let request = RemoteTransactWriteRequest::new()
    .put(b"user#1", item);
```

**Update**: Modify item with update expression
```rust
let request = RemoteTransactWriteRequest::new()
    .update(b"user#1", "SET age = age + :inc")
    .value(":inc", Value::N("1".to_string()));
```

**Delete**: Remove item
```rust
let request = RemoteTransactWriteRequest::new()
    .delete(b"user#old");
```

**Condition Check**: Verify condition without modifying
```rust
let request = RemoteTransactWriteRequest::new()
    .condition_check(b"config#global", "attribute_exists(enabled)")
    .update(b"user#1", "SET status = :active")
    .value(":active", Value::S("online".to_string()));
```

### ACID Guarantees

Transactions provide full ACID guarantees:

- **Atomicity**: All operations succeed or all fail
- **Consistency**: Conditions are checked before any writes
- **Isolation**: Transactions execute serially on the server
- **Durability**: Committed transactions are persisted to disk

## Update Operations

The `RemoteUpdate` builder supports DynamoDB-style update expressions.

### Simple Update

Update attributes with SET:

```rust
use kstone_client::RemoteUpdate;

let update = RemoteUpdate::new(b"user#123")
    .expression("SET age = :new_age")
    .value(":new_age", Value::N("31".to_string()));

let response = client.update(update).await?;
println!("Updated item: {:?}", response.item);
```

### Arithmetic Operations

Increment or decrement numeric values:

```rust
// Increment counter
let update = RemoteUpdate::new(b"page#home")
    .expression("SET views = views + :inc")
    .value(":inc", Value::N("1".to_string()));

client.update(update).await?;

// Decrement lives
let update = RemoteUpdate::new(b"game#session1")
    .expression("SET lives = lives - :dec")
    .value(":dec", Value::N("1".to_string()));

client.update(update).await?;
```

### Remove Attributes

Remove attributes from an item:

```rust
let update = RemoteUpdate::new(b"user#123")
    .expression("REMOVE temp, verification_code");

client.update(update).await?;
```

### Add Operation

Add to a number (creates if doesn't exist):

```rust
let update = RemoteUpdate::new(b"counter#global")
    .expression("ADD total :count")
    .value(":count", Value::N("5".to_string()));

client.update(update).await?;
```

### Conditional Update

Only update if a condition is met:

```rust
let update = RemoteUpdate::new(b"user#123")
    .expression("SET age = :new_age")
    .condition("age = :old_age")  // Optimistic locking
    .value(":new_age", Value::N("31".to_string()))
    .value(":old_age", Value::N("30".to_string()));

match client.update(update).await {
    Ok(response) => println!("Updated: {:?}", response.item),
    Err(ClientError::ConditionCheckFailed(_)) => {
        println!("Conflict: age was modified by another process");
    }
    Err(e) => return Err(e.into()),
}
```

## PartiQL Queries

Execute SQL-style queries using PartiQL:

```rust
// SELECT query
let response = client.execute_statement(
    "SELECT * FROM items WHERE pk = 'user#123'"
).await?;

match response {
    RemoteExecuteStatementResponse::Select { items, count, .. } => {
        println!("Found {} items", count);
        for item in items {
            println!("{:?}", item);
        }
    }
    _ => println!("Unexpected response type"),
}

// INSERT
client.execute_statement(
    "INSERT INTO items VALUE {'pk': 'user#999', 'name': 'Charlie'}"
).await?;

// UPDATE
client.execute_statement(
    "UPDATE items SET age = 35 WHERE pk = 'user#999'"
).await?;

// DELETE
client.execute_statement(
    "DELETE FROM items WHERE pk = 'user#999'"
).await?;
```

## Error Handling

The client provides rich error types for comprehensive error handling.

### Error Types

```rust
pub enum ClientError {
    NotFound(String),                  // Item not found
    InvalidArgument(String),           // Bad request
    ConditionCheckFailed(String),      // Condition not met
    ConnectionError(String),           // Network error
    Unavailable(String),               // Server unavailable
    Timeout(String),                   // Request timeout
    InternalError(String),             // Server error
    DataCorruption(String),            // Data integrity issue
    TransactionAborted(String),        // Transaction failed
    AlreadyExists(String),             // Item already exists
    ResourceExhausted(String),         // Rate limited
    Unimplemented(String),             // Feature not implemented
    PermissionDenied(String),          // Authorization failed
    Unknown(String),                   // Other error
}
```

### Error Handling Patterns

**Match on Specific Errors**:
```rust
match client.get(b"user#123").await {
    Ok(Some(item)) => println!("Found: {:?}", item),
    Ok(None) => println!("Not found"),
    Err(ClientError::Unavailable(msg)) => {
        eprintln!("Server unavailable: {}", msg);
        // Retry with backoff
    }
    Err(ClientError::Timeout(msg)) => {
        eprintln!("Request timeout: {}", msg);
        // Retry
    }
    Err(e) => return Err(e.into()),
}
```

**Handle Rate Limiting**:
```rust
use tokio::time::{sleep, Duration};

let mut retries = 0;
const MAX_RETRIES: u32 = 3;

loop {
    match client.put(b"user#123", item.clone()).await {
        Ok(_) => break,
        Err(ClientError::ResourceExhausted(msg)) if retries < MAX_RETRIES => {
            retries += 1;
            let backoff = Duration::from_millis(100 * 2_u64.pow(retries));
            eprintln!("Rate limited, retrying in {:?}: {}", backoff, msg);
            sleep(backoff).await;
        }
        Err(e) => return Err(e.into()),
    }
}
```

**Conditional Operation Failure**:
```rust
match client.put_conditional(b"user#123", item, "attribute_not_exists(pk)", values).await {
    Ok(_) => println!("Item created"),
    Err(ClientError::ConditionCheckFailed(_)) => {
        println!("Item already exists, using existing");
        // Not an error in this case
    }
    Err(e) => return Err(e.into()),
}
```

## Connection Pooling and Reuse

### Single Client Pattern

For most applications, use a single `Client` instance:

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Client>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::connect("http://localhost:50051").await?;
    let state = AppState {
        db: Arc::new(Mutex::new(client)),
    };

    // Use in multiple tasks
    let state1 = state.clone();
    tokio::spawn(async move {
        let mut client = state1.db.lock().await;
        client.get(b"user#1").await.ok();
    });

    let state2 = state.clone();
    tokio::spawn(async move {
        let mut client = state2.db.lock().await;
        client.get(b"user#2").await.ok();
    });

    Ok(())
}
```

### Multiple Clients

For high-throughput applications, create multiple clients:

```rust
use std::sync::Arc;

const NUM_CLIENTS: usize = 10;

let clients: Vec<Arc<Mutex<Client>>> = futures::future::try_join_all(
    (0..NUM_CLIENTS).map(|_| async {
        Client::connect("http://localhost:50051")
            .await
            .map(|c| Arc::new(Mutex::new(c)))
    })
).await?;

// Round-robin distribution
for (i, key) in keys.iter().enumerate() {
    let client = &clients[i % NUM_CLIENTS];
    let mut client_guard = client.lock().await;
    client_guard.get(key).await?;
}
```

## Advanced Patterns

### Retry Logic

Implement exponential backoff for transient failures:

```rust
use tokio::time::{sleep, Duration};

async fn get_with_retry(
    client: &mut Client,
    key: &[u8],
    max_retries: u32,
) -> Result<Option<Item>, ClientError> {
    let mut retries = 0;

    loop {
        match client.get(key).await {
            Ok(result) => return Ok(result),
            Err(ClientError::Unavailable(_)) |
            Err(ClientError::Timeout(_))
                if retries < max_retries =>
            {
                retries += 1;
                let backoff = Duration::from_millis(100 * 2_u64.pow(retries));
                sleep(backoff).await;
            }
            Err(e) => return Err(e),
        }
    }
}
```

### Circuit Breaker

Prevent cascading failures with a circuit breaker:

```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

struct CircuitBreaker {
    failures: Arc<AtomicU32>,
    threshold: u32,
}

impl CircuitBreaker {
    fn new(threshold: u32) -> Self {
        Self {
            failures: Arc::new(AtomicU32::new(0)),
            threshold,
        }
    }

    fn is_open(&self) -> bool {
        self.failures.load(Ordering::Relaxed) >= self.threshold
    }

    fn record_success(&self) {
        self.failures.store(0, Ordering::Relaxed);
    }

    fn record_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
}

async fn get_with_circuit_breaker(
    client: &mut Client,
    key: &[u8],
    breaker: &CircuitBreaker,
) -> Result<Option<Item>, ClientError> {
    if breaker.is_open() {
        return Err(ClientError::Unavailable(
            "Circuit breaker is open".to_string()
        ));
    }

    match client.get(key).await {
        Ok(result) => {
            breaker.record_success();
            Ok(result)
        }
        Err(e) => {
            breaker.record_failure();
            Err(e)
        }
    }
}
```

### Caching Layer

Add client-side caching for read-heavy workloads:

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

struct CachedClient {
    client: Client,
    cache: Arc<RwLock<HashMap<Vec<u8>, Item>>>,
}

impl CachedClient {
    async fn get(&mut self, key: &[u8]) -> Result<Option<Item>, ClientError> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(item) = cache.get(key) {
                return Ok(Some(item.clone()));
            }
        }

        // Cache miss - fetch from server
        if let Some(item) = self.client.get(key).await? {
            let mut cache = self.cache.write().await;
            cache.insert(key.to_vec(), item.clone());
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    async fn put(&mut self, key: &[u8], item: Item) -> Result<(), ClientError> {
        // Write through to server
        self.client.put(key, item.clone()).await?;

        // Update cache
        let mut cache = self.cache.write().await;
        cache.insert(key.to_vec(), item);

        Ok(())
    }

    async fn delete(&mut self, key: &[u8]) -> Result<(), ClientError> {
        // Delete from server
        self.client.delete(key).await?;

        // Invalidate cache
        let mut cache = self.cache.write().await;
        cache.remove(key);

        Ok(())
    }
}
```

### Metrics and Instrumentation

Track client-side metrics:

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

struct InstrumentedClient {
    client: Client,
    requests: Arc<AtomicU64>,
    errors: Arc<AtomicU64>,
}

impl InstrumentedClient {
    async fn get(&mut self, key: &[u8]) -> Result<Option<Item>, ClientError> {
        self.requests.fetch_add(1, Ordering::Relaxed);
        let start = Instant::now();

        match self.client.get(key).await {
            Ok(result) => {
                let duration = start.elapsed();
                println!("GET latency: {:?}", duration);
                Ok(result)
            }
            Err(e) => {
                self.errors.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    fn stats(&self) -> (u64, u64) {
        (
            self.requests.load(Ordering::Relaxed),
            self.errors.load(Ordering::Relaxed),
        )
    }
}
```

## Testing with the Client

### Integration Tests

Test your application against a real server:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_client() -> Client {
        Client::connect("http://localhost:50051")
            .await
            .expect("Failed to connect to test server")
    }

    #[tokio::test]
    async fn test_put_get() {
        let mut client = setup_test_client().await;

        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::S("Test".to_string()));

        client.put(b"test#1", item).await.unwrap();

        let result = client.get(b"test#1").await.unwrap();
        assert!(result.is_some());

        client.delete(b"test#1").await.unwrap();
    }
}
```

### Mock Server

For unit tests, consider using a mock gRPC server or the embedded database directly.

## Performance Considerations

### Request Batching

Batch multiple operations to reduce network round trips:

```rust
// Instead of individual gets:
for key in keys {
    client.get(key).await?;  // N round trips
}

// Use batch get:
let batch = keys.iter().fold(
    RemoteBatchGetRequest::new(),
    |batch, key| batch.add_key(key)
);
client.batch_get(batch).await?;  // 1 round trip
```

### Parallel Requests

Use tokio to issue multiple requests concurrently:

```rust
use futures::future::try_join_all;

let futures: Vec<_> = keys.iter()
    .map(|key| client.get(key))
    .collect();

let results = try_join_all(futures).await?;
```

### Connection Warmup

Pre-warm connections during application startup:

```rust
async fn warmup_connection(client: &mut Client) -> Result<(), ClientError> {
    // Issue a dummy request to establish connection
    let _ = client.get(b"_warmup").await;
    Ok(())
}

let mut client = Client::connect("http://localhost:50051").await?;
warmup_connection(&mut client).await?;
```

## Summary

The KeystoneDB client library provides:

- **Ergonomic API**: Fluent builders for all operations
- **Type Safety**: Compile-time guarantees for request construction
- **Async Support**: Native async/await with Tokio
- **Rich Errors**: Detailed error types with context
- **Efficient**: Connection pooling via HTTP/2 multiplexing
- **Complete**: Full coverage of all server RPC methods

The client mirrors the embedded `Database` API while adding network-specific features like retry logic, connection management, and error handling. Combined with the gRPC server from Chapter 24, it enables building distributed KeystoneDB applications with ease.

In Chapter 26, we'll explore the network architecture in depth, covering communication flows, type conversions, async operations, TLS configuration, and deployment patterns.

# Chapter 11: Batch Operations

Batch operations allow you to read or write multiple items in a single request, significantly improving performance for bulk operations. KeystoneDB provides `BatchGet` for retrieving multiple items and `BatchWrite` for creating, updating, or deleting multiple items atomically. This chapter explores how to use batch operations effectively and when they provide the most benefit.

## Understanding Batch Operations

Batch operations group multiple individual operations into a single request, reducing overhead and improving throughput. KeystoneDB supports two batch operations:

1. **BatchGet**: Retrieve up to 25 items in a single request
2. **BatchWrite**: Put or delete up to 25 items in a single request

### Benefits of Batch Operations

1. **Reduced overhead**: One API call instead of many
2. **Better performance**: Operations can be optimized internally
3. **Network efficiency**: Fewer round trips to the database
4. **Consistent snapshots**: BatchGet reads from a consistent point in time

### Batch vs Individual Operations

```rust
use std::time::Instant;

// Individual operations
let start = Instant::now();
for i in 0..25 {
    let key = format!("user#{}", i);
    db.get(key.as_bytes())?;
}
let individual_duration = start.elapsed();

// Batch operation
let start = Instant::now();
let mut batch = BatchGetRequest::new();
for i in 0..25 {
    let key = format!("user#{}", i);
    batch = batch.add_key(key.as_bytes());
}
let response = db.batch_get(batch)?;
let batch_duration = start.elapsed();

println!("Individual gets: {:?}", individual_duration);
println!("Batch get: {:?}", batch_duration);
// Batch is typically 5-10x faster
```

## BatchGet Operation

`BatchGet` retrieves multiple items efficiently in a single request.

### Basic BatchGet

```rust
use kstone_api::{Database, BatchGetRequest, ItemBuilder};

let db = Database::open("mydb.keystone")?;

// Insert test data
for i in 1..=10 {
    let item = ItemBuilder::new()
        .string("name", format!("User {}", i))
        .number("id", i)
        .build();

    db.put(format!("user#{}", i).as_bytes(), item)?;
}

// Batch get multiple items
let request = BatchGetRequest::new()
    .add_key(b"user#1")
    .add_key(b"user#2")
    .add_key(b"user#3")
    .add_key(b"user#5")   // Notice we skip #4
    .add_key(b"user#100"); // Doesn't exist

let response = db.batch_get(request)?;

println!("Found {} items", response.items.len());

// Iterate over results
for (key, item) in &response.items {
    let name = item.get("name").and_then(|v| v.as_string()).unwrap();
    println!("Key: {:?}, Name: {}", key, name);
}
```

### BatchGet with Sort Keys

You can retrieve items with composite keys:

```rust
// Insert items with sort keys
for user in &["alice", "bob", "charlie"] {
    for i in 1..=3 {
        let post = ItemBuilder::new()
            .string("title", format!("{}'s post #{}", user, i))
            .build();

        db.put_with_sk(
            format!("user#{}", user).as_bytes(),
            format!("post#{}", i).as_bytes(),
            post
        )?;
    }
}

// Batch get specific posts
let request = BatchGetRequest::new()
    .add_key_with_sk(b"user#alice", b"post#1")
    .add_key_with_sk(b"user#alice", b"post#3")
    .add_key_with_sk(b"user#bob", b"post#2")
    .add_key_with_sk(b"user#charlie", b"post#1");

let response = db.batch_get(request)?;

println!("Retrieved {} posts", response.items.len());
```

### Handling Missing Items

BatchGet only returns items that exist. Missing items are simply omitted from the response:

```rust
let request = BatchGetRequest::new()
    .add_key(b"user#exists")
    .add_key(b"user#missing")
    .add_key(b"user#also-missing");

let response = db.batch_get(request)?;

// Only returns the item that exists
assert_eq!(response.items.len(), 1);

// Check if a specific key was found
use kstone_core::Key;

let key = Key::new(b"user#exists".to_vec());
if response.items.contains_key(&key) {
    println!("Item found!");
} else {
    println!("Item not found");
}
```

### BatchGet Response Structure

```rust
pub struct BatchGetResponse {
    /// Items retrieved (key -> item mapping)
    pub items: HashMap<Key, Item>,

    /// Keys that were not found (currently unused)
    pub unprocessed_keys: Vec<Key>,
}
```

The response uses a `HashMap` where keys map to their items. This allows efficient lookup:

```rust
let request = BatchGetRequest::new()
    .add_key(b"user#1")
    .add_key(b"user#2")
    .add_key(b"user#3");

let response = db.batch_get(request)?;

// Look up specific item
let key = Key::new(b"user#2".to_vec());
if let Some(item) = response.items.get(&key) {
    println!("User #2: {:?}", item);
}

// Iterate in any order
for (key, item) in response.items {
    println!("Key: {:?}, Item: {:?}", key, item);
}
```

### Practical Example: Loading User Profiles

```rust
fn load_user_profiles(
    db: &Database,
    user_ids: &[u64]
) -> Result<HashMap<u64, Item>, Error> {
    // Build batch get request
    let mut request = BatchGetRequest::new();

    for &user_id in user_ids {
        let key = format!("user#{}", user_id);
        request = request.add_key(key.as_bytes());
    }

    // Execute batch get
    let response = db.batch_get(request)?;

    // Convert to user_id -> item mapping
    let mut profiles = HashMap::new();

    for (key, item) in response.items {
        // Extract user ID from key
        let key_str = String::from_utf8_lossy(&key.pk);
        if let Some(id_str) = key_str.strip_prefix("user#") {
            if let Ok(user_id) = id_str.parse::<u64>() {
                profiles.insert(user_id, item);
            }
        }
    }

    Ok(profiles)
}

// Usage
let user_ids = vec![1, 2, 5, 7, 10];
let profiles = load_user_profiles(&db, &user_ids)?;

for (user_id, profile) in profiles {
    let name = profile.get("name").and_then(|v| v.as_string()).unwrap();
    println!("User {}: {}", user_id, name);
}
```

## BatchWrite Operation

`BatchWrite` performs multiple put or delete operations in a single request.

### Basic BatchWrite

```rust
use kstone_api::{Database, BatchWriteRequest, ItemBuilder};

let db = Database::open("mydb.keystone")?;

// Create a batch write request
let request = BatchWriteRequest::new()
    .put(b"user#1", ItemBuilder::new()
        .string("name", "Alice")
        .number("age", 30)
        .build())
    .put(b"user#2", ItemBuilder::new()
        .string("name", "Bob")
        .number("age", 25)
        .build())
    .put(b"user#3", ItemBuilder::new()
        .string("name", "Charlie")
        .number("age", 35)
        .build());

let response = db.batch_write(request)?;

println!("Processed {} operations", response.processed_count);
```

### Mixed Put and Delete Operations

BatchWrite can combine puts and deletes:

```rust
// Insert initial data
db.put(b"user#1", ItemBuilder::new().string("name", "Alice").build())?;
db.put(b"user#2", ItemBuilder::new().string("name", "Bob").build())?;
db.put(b"user#3", ItemBuilder::new().string("name", "Charlie").build())?;

// Batch write with mixed operations
let request = BatchWriteRequest::new()
    .put(b"user#4", ItemBuilder::new().string("name", "Dave").build())  // Create
    .put(b"user#1", ItemBuilder::new().string("name", "Alice Updated").build())  // Update
    .delete(b"user#2")  // Delete
    .put(b"user#5", ItemBuilder::new().string("name", "Eve").build());  // Create

let response = db.batch_write(request)?;

assert_eq!(response.processed_count, 4);

// Verify results
assert!(db.get(b"user#1")?.is_some());  // Updated
assert!(db.get(b"user#2")?.is_none());   // Deleted
assert!(db.get(b"user#3")?.is_some());  // Unchanged
assert!(db.get(b"user#4")?.is_some());  // Created
assert!(db.get(b"user#5")?.is_some());  // Created
```

### BatchWrite with Sort Keys

```rust
// Batch write with composite keys
let request = BatchWriteRequest::new()
    .put_with_sk(
        b"user#alice",
        b"post#1",
        ItemBuilder::new().string("title", "First post").build()
    )
    .put_with_sk(
        b"user#alice",
        b"post#2",
        ItemBuilder::new().string("title", "Second post").build()
    )
    .delete_with_sk(b"user#alice", b"post#old")
    .put_with_sk(
        b"user#bob",
        b"comment#1",
        ItemBuilder::new().string("text", "Great post!").build()
    );

let response = db.batch_write(request)?;

println!("Processed {} operations", response.processed_count);
```

### BatchWrite Response Structure

```rust
pub struct BatchWriteResponse {
    /// Number of items successfully written
    pub processed_count: usize,

    /// Items that failed to write (currently unused)
    pub unprocessed_items: Vec<BatchWriteItem>,
}
```

Currently, KeystoneDB processes all batch operations synchronously, so all items succeed or the entire batch fails.

### Practical Example: Bulk Data Import

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct User {
    id: u64,
    name: String,
    email: String,
    age: u32,
}

fn import_users_from_json(
    db: &Database,
    json_data: &str
) -> Result<usize, Box<dyn std::error::Error>> {
    // Parse JSON
    let users: Vec<User> = serde_json::from_str(json_data)?;

    // Process in batches of 25
    let mut total_imported = 0;

    for chunk in users.chunks(25) {
        let mut batch = BatchWriteRequest::new();

        for user in chunk {
            let item = ItemBuilder::new()
                .string("name", &user.name)
                .string("email", &user.email)
                .number("age", user.age)
                .build();

            let key = format!("user#{}", user.id);
            batch = batch.put(key.as_bytes(), item);
        }

        let response = db.batch_write(batch)?;
        total_imported += response.processed_count;

        println!("Imported batch of {} users", response.processed_count);
    }

    Ok(total_imported)
}

// Usage
let json = r#"[
    {"id": 1, "name": "Alice", "email": "alice@example.com", "age": 30},
    {"id": 2, "name": "Bob", "email": "bob@example.com", "age": 25},
    {"id": 3, "name": "Charlie", "email": "charlie@example.com", "age": 35}
]"#;

let count = import_users_from_json(&db, json)?;
println!("Total imported: {} users", count);
```

## Batch Size Limits

KeystoneDB follows DynamoDB conventions with a limit of **25 items per batch operation**.

### Respecting Batch Limits

```rust
// Good: Process in batches of 25
fn batch_write_items(db: &Database, items: Vec<(Vec<u8>, Item)>) -> Result<usize, Error> {
    let mut total_processed = 0;

    for chunk in items.chunks(25) {
        let mut batch = BatchWriteRequest::new();

        for (key, item) in chunk {
            batch = batch.put(key, item.clone());
        }

        let response = db.batch_write(batch)?;
        total_processed += response.processed_count;
    }

    Ok(total_processed)
}

// Bad: Exceeds batch limit (will fail)
fn batch_write_too_many(db: &Database) -> Result<(), Error> {
    let mut batch = BatchWriteRequest::new();

    for i in 0..50 {  // 50 items - exceeds limit!
        batch = batch.put(
            format!("item#{}", i).as_bytes(),
            ItemBuilder::new().number("id", i).build()
        );
    }

    db.batch_write(batch)?;  // This will fail
    Ok(())
}
```

### Automatic Batching

Create a helper function to automatically split large operations:

```rust
fn batch_write_auto(
    db: &Database,
    items: Vec<(Vec<u8>, Item)>
) -> Result<BatchWriteStats, Error> {
    const BATCH_SIZE: usize = 25;

    let mut stats = BatchWriteStats {
        total_batches: 0,
        total_items: 0,
        failed_batches: 0,
    };

    for chunk in items.chunks(BATCH_SIZE) {
        let mut batch = BatchWriteRequest::new();

        for (key, item) in chunk {
            batch = batch.put(key, item.clone());
        }

        match db.batch_write(batch) {
            Ok(response) => {
                stats.total_batches += 1;
                stats.total_items += response.processed_count;
            }
            Err(e) => {
                eprintln!("Batch failed: {}", e);
                stats.failed_batches += 1;
            }
        }
    }

    Ok(stats)
}

#[derive(Debug)]
struct BatchWriteStats {
    total_batches: usize,
    total_items: usize,
    failed_batches: usize,
}
```

## Error Handling in Batch Operations

Proper error handling ensures your batch operations are robust.

### BatchGet Error Handling

```rust
use kstone_core::Error;

match db.batch_get(request) {
    Ok(response) => {
        if response.items.is_empty() {
            println!("No items found");
        } else {
            println!("Found {} items", response.items.len());
            for (key, item) in response.items {
                process_item(key, item);
            }
        }
    }
    Err(Error::Io(e)) => {
        eprintln!("I/O error during batch get: {}", e);
        // Retry with exponential backoff
    }
    Err(Error::InvalidArgument(msg)) => {
        eprintln!("Invalid batch get request: {}", msg);
        // Fix the request parameters
    }
    Err(e) => {
        eprintln!("Batch get error: {}", e);
    }
}
```

### BatchWrite Error Handling

```rust
match db.batch_write(request) {
    Ok(response) => {
        println!("Successfully processed {} items", response.processed_count);
    }
    Err(Error::Io(e)) => {
        eprintln!("I/O error during batch write: {}", e);
        // Retry the entire batch
    }
    Err(Error::InvalidArgument(msg)) => {
        eprintln!("Invalid batch write request: {}", msg);
        // Check batch size and item structure
    }
    Err(e) => {
        eprintln!("Batch write error: {}", e);
    }
}
```

### Retry Logic

```rust
fn batch_write_with_retry(
    db: &Database,
    request: BatchWriteRequest,
    max_retries: u32
) -> Result<BatchWriteResponse, Error> {
    let mut retries = 0;

    loop {
        match db.batch_write(request.clone()) {
            Ok(response) => return Ok(response),
            Err(Error::Io(_)) if retries < max_retries => {
                retries += 1;
                eprintln!("Retry {}/{}", retries, max_retries);

                // Exponential backoff
                let delay = std::time::Duration::from_millis(100 * 2_u64.pow(retries));
                std::thread::sleep(delay);
            }
            Err(e) => return Err(e),
        }
    }
}
```

## Performance Benefits

Understanding when batch operations provide the most benefit.

### Benchmarking Batch vs Individual

```rust
use std::time::Instant;

fn benchmark_individual_vs_batch(db: &Database, count: usize) {
    // Setup: Create items
    let mut items = Vec::new();
    for i in 0..count {
        items.push((
            format!("item#{}", i),
            ItemBuilder::new().number("id", i).build()
        ));
    }

    // Benchmark individual puts
    let start = Instant::now();
    for (key, item) in &items {
        db.put(key.as_bytes(), item.clone()).unwrap();
    }
    let individual_duration = start.elapsed();

    // Delete for clean slate
    for (key, _) in &items {
        db.delete(key.as_bytes()).unwrap();
    }

    // Benchmark batch writes
    let start = Instant::now();
    for chunk in items.chunks(25) {
        let mut batch = BatchWriteRequest::new();
        for (key, item) in chunk {
            batch = batch.put(key.as_bytes(), item.clone());
        }
        db.batch_write(batch).unwrap();
    }
    let batch_duration = start.elapsed();

    println!("Writing {} items:", count);
    println!("  Individual: {:?} ({:.2} items/sec)",
        individual_duration,
        count as f64 / individual_duration.as_secs_f64());
    println!("  Batch:      {:?} ({:.2} items/sec)",
        batch_duration,
        count as f64 / batch_duration.as_secs_f64());
    println!("  Speedup:    {:.2}x",
        individual_duration.as_secs_f64() / batch_duration.as_secs_f64());
}

// Usage
benchmark_individual_vs_batch(&db, 100);
// Typical output:
// Writing 100 items:
//   Individual: 15.2ms (6578.95 items/sec)
//   Batch:      2.8ms (35714.29 items/sec)
//   Speedup:    5.43x
```

### When Batch Operations Excel

1. **Bulk data loading**:

```rust
// Import 10,000 records efficiently
let records = load_records_from_csv("data.csv")?;

for chunk in records.chunks(25) {
    let mut batch = BatchWriteRequest::new();
    for record in chunk {
        batch = batch.put(&record.key, record.item.clone());
    }
    db.batch_write(batch)?;
}
```

2. **Hydrating caches**:

```rust
// Load multiple user profiles into cache
let user_ids = get_active_user_ids()?;
let mut batch = BatchGetRequest::new();

for id in user_ids.iter().take(25) {
    batch = batch.add_key(format!("user#{}", id).as_bytes());
}

let response = db.batch_get(batch)?;

for (key, item) in response.items {
    cache.insert(key, item);
}
```

3. **Cleanup operations**:

```rust
// Delete old items in bulk
let old_keys = find_expired_items()?;

for chunk in old_keys.chunks(25) {
    let mut batch = BatchWriteRequest::new();
    for key in chunk {
        batch = batch.delete(key);
    }
    db.batch_write(batch)?;
}
```

## Use Cases for Batch Operations

### 1. Data Migration

```rust
fn migrate_users(source_db: &Database, dest_db: &Database) -> Result<usize, Error> {
    let mut migrated = 0;
    let mut last_key = None;

    loop {
        // Scan source in pages
        let mut scan = Scan::new().limit(25);
        if let Some((pk, sk)) = &last_key {
            scan = scan.start_after(pk, sk.as_deref());
        }

        let response = source_db.scan(scan)?;
        if response.items.is_empty() {
            break;
        }

        // Batch write to destination
        let mut batch = BatchWriteRequest::new();
        for item in response.items {
            // Extract key and add to batch
            batch = batch.put(b"migrated#key", item);
        }

        let write_response = dest_db.batch_write(batch)?;
        migrated += write_response.processed_count;

        last_key = response.last_key;
    }

    Ok(migrated)
}
```

### 2. Denormalization

```rust
// Copy user data to multiple access patterns
fn denormalize_user(db: &Database, user_id: u64, user: &Item) -> Result<(), Error> {
    let email = user.get("email").and_then(|v| v.as_string()).unwrap();
    let username = user.get("username").and_then(|v| v.as_string()).unwrap();

    // Write to multiple access patterns in one batch
    let batch = BatchWriteRequest::new()
        .put(
            format!("user#id#{}", user_id).as_bytes(),
            user.clone()
        )
        .put(
            format!("user#email#{}", email).as_bytes(),
            user.clone()
        )
        .put(
            format!("user#username#{}", username).as_bytes(),
            user.clone()
        );

    db.batch_write(batch)?;
    Ok(())
}
```

### 3. Relationship Management

```rust
// Create user and their initial posts in one batch
fn create_user_with_posts(
    db: &Database,
    user_id: u64,
    user_data: Item,
    posts: Vec<Item>
) -> Result<(), Error> {
    let mut batch = BatchWriteRequest::new()
        .put(format!("user#{}", user_id).as_bytes(), user_data);

    for (i, post) in posts.into_iter().enumerate() {
        batch = batch.put_with_sk(
            format!("user#{}", user_id).as_bytes(),
            format!("post#{}", i).as_bytes(),
            post
        );
    }

    db.batch_write(batch)?;
    Ok(())
}
```

### 4. Snapshot Creation

```rust
// Create point-in-time snapshot of related items
fn create_snapshot(db: &Database, entity_id: &str) -> Result<(), Error> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Get current state
    let current_keys = vec![
        format!("entity#{}", entity_id),
        format!("entity#{}#metadata", entity_id),
        format!("entity#{}#config", entity_id),
    ];

    let mut get_batch = BatchGetRequest::new();
    for key in &current_keys {
        get_batch = get_batch.add_key(key.as_bytes());
    }

    let response = db.batch_get(get_batch)?;

    // Write snapshot
    let mut write_batch = BatchWriteRequest::new();
    for (key, item) in response.items {
        let snapshot_key = format!("snapshot#{}#{}", timestamp, String::from_utf8_lossy(&key.pk));
        write_batch = write_batch.put(snapshot_key.as_bytes(), item);
    }

    db.batch_write(write_batch)?;
    Ok(())
}
```

## Best Practices

### 1. Always Process in Batches of 25 or Less

```rust
// Good: Respect batch size limit
for chunk in items.chunks(25) {
    let mut batch = BatchWriteRequest::new();
    for (key, item) in chunk {
        batch = batch.put(key, item.clone());
    }
    db.batch_write(batch)?;
}

// Bad: Attempt to exceed limit
let mut batch = BatchWriteRequest::new();
for i in 0..50 {
    batch = batch.put(format!("item#{}", i).as_bytes(), item.clone());
}
db.batch_write(batch)?;  // Will fail
```

### 2. Handle Missing Items in BatchGet

```rust
let request = BatchGetRequest::new()
    .add_key(b"user#1")
    .add_key(b"user#2")
    .add_key(b"user#3");

let response = db.batch_get(request)?;

// Check which items were found
let expected_keys = vec![
    Key::new(b"user#1".to_vec()),
    Key::new(b"user#2".to_vec()),
    Key::new(b"user#3".to_vec()),
];

for expected in expected_keys {
    if !response.items.contains_key(&expected) {
        println!("Missing: {:?}", expected);
    }
}
```

### 3. Use Batch Operations for Related Items

```rust
// Good: Batch related items together
let batch = BatchWriteRequest::new()
    .put(b"user#alice", user_profile)
    .put_with_sk(b"user#alice", b"settings", user_settings)
    .put_with_sk(b"user#alice", b"preferences", user_prefs);

db.batch_write(batch)?;

// Less efficient: Individual operations
db.put(b"user#alice", user_profile)?;
db.put_with_sk(b"user#alice", b"settings", user_settings)?;
db.put_with_sk(b"user#alice", b"preferences", user_prefs)?;
```

### 4. Implement Retry Logic

```rust
fn batch_write_with_exponential_backoff(
    db: &Database,
    request: BatchWriteRequest,
) -> Result<BatchWriteResponse, Error> {
    const MAX_RETRIES: u32 = 3;
    const BASE_DELAY_MS: u64 = 100;

    for attempt in 0..=MAX_RETRIES {
        match db.batch_write(request.clone()) {
            Ok(response) => return Ok(response),
            Err(Error::Io(e)) if attempt < MAX_RETRIES => {
                let delay = BASE_DELAY_MS * 2_u64.pow(attempt);
                eprintln!("Attempt {} failed: {}. Retrying in {}ms", attempt + 1, e, delay);
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
            Err(e) => return Err(e),
        }
    }

    unreachable!()
}
```

### 5. Monitor Batch Performance

```rust
use std::time::Instant;

fn batch_write_with_metrics(
    db: &Database,
    request: BatchWriteRequest,
) -> Result<BatchMetrics, Error> {
    let start = Instant::now();
    let item_count = request.items.len();

    let response = db.batch_write(request)?;

    let duration = start.elapsed();

    Ok(BatchMetrics {
        items_processed: response.processed_count,
        duration,
        items_per_second: response.processed_count as f64 / duration.as_secs_f64(),
    })
}

struct BatchMetrics {
    items_processed: usize,
    duration: std::time::Duration,
    items_per_second: f64,
}
```

## Summary

This chapter covered KeystoneDB's batch operations:

- **BatchGet**: Efficiently retrieve up to 25 items in a single request
- **BatchWrite**: Put or delete up to 25 items atomically
- **Batch limits**: 25 items per batch maximum
- **Error handling**: Retry logic and graceful degradation
- **Performance benefits**: 5-10x faster than individual operations
- **Use cases**: Data migration, bulk import, denormalization, cleanup
- **Best practices**: Respect limits, handle missing items, implement retries

Batch operations are essential tools for building high-performance applications with KeystoneDB. Use them whenever you need to work with multiple items to maximize throughput and minimize latency.

In the next part of the book, we'll explore advanced features like secondary indexes, conditional operations, and transactions that build upon these fundamental operations.

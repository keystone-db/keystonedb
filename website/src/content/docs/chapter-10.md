# Chapter 10: Scanning Tables

While queries efficiently retrieve items within a single partition, scans allow you to read items across all partitions in your database. Scans are essential for analytics, bulk operations, and administrative tasks. KeystoneDB's scan operation provides powerful features including parallel execution, pagination, and filtering. This chapter explores how to use scans effectively and when to choose scanning over querying.

## Understanding Scan Operations

A scan operation reads items across multiple partition keys, potentially examining the entire table. Unlike queries that target a single stripe, scans:

1. **Multi-stripe operation**: Read from all 256 LSM stripes
2. **Sequential within stripes**: Items are read in sort key order within each stripe
3. **Global ordering**: Results are sorted by key across all stripes
4. **Parallelizable**: Can distribute work across multiple segments

### Scan vs Query

Understanding the fundamental differences:

| Feature | Query | Scan |
|---------|-------|------|
| **Partition Key** | Required (exact match) | Not required |
| **Stripes Accessed** | Single stripe | All stripes (or subset with segments) |
| **Typical Use** | Retrieve related items | Analytics, bulk operations |
| **Performance** | Fast (targeted) | Slower (broad) |
| **Result Order** | Sort key order within partition | Global key order |

```rust
use kstone_api::{Database, Query, Scan};

// Query: Fast, targeted, single partition
let query = Query::new(b"user#alice");
let posts = db.query(query)?;

// Scan: Slower, comprehensive, all partitions
let scan = Scan::new();
let all_items = db.scan(scan)?;
```

## Basic Scan

The simplest scan retrieves all items in the database.

### Scan All Items

```rust
use kstone_api::{Database, Scan, ItemBuilder};

let db = Database::open("mydb.keystone")?;

// Insert test data across multiple partitions
for user in &["alice", "bob", "charlie"] {
    for i in 1..=5 {
        let item = ItemBuilder::new()
            .string("user", user.to_string())
            .number("post_id", i)
            .string("title", format!("{}'s post #{}", user, i))
            .build();

        db.put_with_sk(
            format!("user#{}", user).as_bytes(),
            format!("post#{}", i).as_bytes(),
            item
        )?;
    }
}

// Scan all items
let scan = Scan::new();
let response = db.scan(scan)?;

println!("Found {} items", response.count);
println!("Scanned {} items", response.scanned_count);

for item in response.items {
    let user = item.get("user").and_then(|v| v.as_string()).unwrap();
    let title = item.get("title").and_then(|v| v.as_string()).unwrap();
    println!("- [{}] {}", user, title);
}
```

### Scan Response Structure

The `ScanResponse` is similar to `QueryResponse`:

```rust
pub struct ScanResponse {
    /// Items found
    pub items: Vec<Item>,

    /// Number of items returned
    pub count: usize,

    /// Last evaluated key (for pagination)
    pub last_key: Option<(Bytes, Option<Bytes>)>,

    /// Number of items examined
    pub scanned_count: usize,
}
```

- `items`: The actual items retrieved
- `count`: Number of items in the response
- `last_key`: The last key evaluated; used for pagination
- `scanned_count`: Total items examined (before limit was applied)

## Scan with Limit

For large tables, use `limit()` to control the response size.

### Basic Limit

```rust
// Get first 100 items
let scan = Scan::new().limit(100);
let response = db.scan(scan)?;

println!("Retrieved {} items", response.count);

if response.last_key.is_some() {
    println!("More items available");
}
```

### Limit Behavior

The limit applies to the total number of items returned, not per stripe:

```rust
// Insert 500 items across multiple partitions
for i in 0..500 {
    let item = ItemBuilder::new()
        .number("id", i)
        .string("data", format!("Item {}", i))
        .build();

    db.put(format!("item#{:04}", i).as_bytes(), item)?;
}

// Scan with limit
let scan = Scan::new().limit(50);
let response = db.scan(scan)?;

assert_eq!(response.count, 50);  // Exactly 50 items
assert!(response.last_key.is_some());  // More items available
```

## Pagination

Like queries, scans support pagination for processing large datasets in chunks.

### Basic Pagination

```rust
// First page
let scan = Scan::new().limit(100);
let page1 = db.scan(scan)?;

println!("Page 1: {} items", page1.count);

// Second page
if let Some((last_pk, last_sk)) = page1.last_key {
    let scan2 = Scan::new()
        .limit(100)
        .start_after(&last_pk, last_sk.as_deref());

    let page2 = db.scan(scan2)?;
    println!("Page 2: {} items", page2.count);
}
```

### Complete Pagination Pattern

```rust
fn scan_all_items(db: &Database, page_size: usize) -> Result<Vec<Item>, Error> {
    let mut all_items = Vec::new();
    let mut last_key = None;

    loop {
        // Build scan request
        let mut scan = Scan::new().limit(page_size);

        // Add pagination marker if not first page
        if let Some((last_pk, last_sk)) = &last_key {
            scan = scan.start_after(last_pk, last_sk.as_deref());
        }

        // Execute scan
        let response = db.scan(scan)?;

        // Collect items
        all_items.extend(response.items);

        // Check for more pages
        if response.last_key.is_none() {
            break;  // No more pages
        }

        last_key = response.last_key;
    }

    Ok(all_items)
}

// Usage
let all_items = scan_all_items(&db, 100)?;
println!("Retrieved {} total items", all_items.len());
```

### Pagination with Progress Tracking

```rust
fn scan_with_progress<F>(
    db: &Database,
    page_size: usize,
    mut progress_fn: F
) -> Result<Vec<Item>, Error>
where
    F: FnMut(usize, usize),
{
    let mut all_items = Vec::new();
    let mut last_key = None;
    let mut page_num = 0;

    loop {
        let mut scan = Scan::new().limit(page_size);

        if let Some((last_pk, last_sk)) = &last_key {
            scan = scan.start_after(last_pk, last_sk.as_deref());
        }

        let response = db.scan(scan)?;
        page_num += 1;

        all_items.extend(response.items);

        // Report progress
        progress_fn(page_num, all_items.len());

        if response.last_key.is_none() {
            break;
        }

        last_key = response.last_key;
    }

    Ok(all_items)
}

// Usage
let items = scan_with_progress(&db, 100, |page, total| {
    println!("Page {}: {} items retrieved so far", page, total);
})?;
```

## Parallel Scan

KeystoneDB's most powerful scan feature is parallel execution across segments. This distributes the work across multiple workers for faster processing.

### Understanding Segments

KeystoneDB has 256 internal stripes. Segments allow you to distribute these stripes across workers:

- **Total segments**: How many workers you're using
- **Segment number**: Which worker this is (0-based)
- **Stripe distribution**: Each segment scans stripes where `stripe_id % total_segments == segment`

```rust
// 4 workers, each scans 64 stripes (256 / 4)
// Worker 0: stripes 0, 4, 8, 12, 16, ...
// Worker 1: stripes 1, 5, 9, 13, 17, ...
// Worker 2: stripes 2, 6, 10, 14, 18, ...
// Worker 3: stripes 3, 7, 11, 15, 19, ...
```

### Parallel Scan Example

```rust
// Scan with 4 parallel segments
let mut total_items = 0;

for segment in 0..4 {
    let scan = Scan::new().segment(segment, 4);
    let response = db.scan(scan)?;

    println!("Segment {} found {} items", segment, response.count);
    total_items += response.count;
}

println!("Total items across all segments: {}", total_items);
```

### Parallel Scan with Threads

```rust
use std::thread;
use std::sync::Arc;

fn parallel_scan(db: Arc<Database>, num_segments: usize) -> Result<Vec<Item>, Error> {
    let mut handles = Vec::new();

    // Spawn worker threads
    for segment in 0..num_segments {
        let db_clone = Arc::clone(&db);

        let handle = thread::spawn(move || {
            let scan = Scan::new().segment(segment, num_segments);
            db_clone.scan(scan)
        });

        handles.push(handle);
    }

    // Collect results from all threads
    let mut all_items = Vec::new();

    for (segment, handle) in handles.into_iter().enumerate() {
        match handle.join() {
            Ok(Ok(response)) => {
                println!("Segment {} retrieved {} items", segment, response.count);
                all_items.extend(response.items);
            }
            Ok(Err(e)) => {
                eprintln!("Segment {} error: {}", segment, e);
                return Err(e);
            }
            Err(_) => {
                return Err(Error::Internal(format!("Segment {} panicked", segment)));
            }
        }
    }

    Ok(all_items)
}

// Usage
let db = Arc::new(Database::open("mydb.keystone")?);
let items = parallel_scan(db, 4)?;
println!("Retrieved {} total items", items.len());
```

### Async Parallel Scan

```rust
use tokio::task;

async fn async_parallel_scan(
    db: Arc<Database>,
    num_segments: usize
) -> Result<Vec<Item>, Error> {
    let mut tasks = Vec::new();

    // Spawn async tasks
    for segment in 0..num_segments {
        let db_clone = Arc::clone(&db);

        let task = task::spawn_blocking(move || {
            let scan = Scan::new().segment(segment, num_segments);
            db_clone.scan(scan)
        });

        tasks.push(task);
    }

    // Await all tasks
    let mut all_items = Vec::new();

    for (segment, task) in tasks.into_iter().enumerate() {
        match task.await {
            Ok(Ok(response)) => {
                println!("Segment {} retrieved {} items", segment, response.count);
                all_items.extend(response.items);
            }
            Ok(Err(e)) => {
                eprintln!("Segment {} error: {}", segment, e);
                return Err(e);
            }
            Err(e) => {
                return Err(Error::Internal(format!("Segment {} join error: {}", segment, e)));
            }
        }
    }

    Ok(all_items)
}

// Usage
#[tokio::main]
async fn main() -> Result<(), Error> {
    let db = Arc::new(Database::open("mydb.keystone")?);
    let items = async_parallel_scan(db, 4).await?;
    println!("Retrieved {} total items", items.len());
    Ok(())
}
```

### Combining Parallel Scan with Pagination

```rust
fn parallel_scan_with_pagination(
    db: Arc<Database>,
    num_segments: usize,
    page_size: usize
) -> Result<Vec<Item>, Error> {
    let mut handles = Vec::new();

    for segment in 0..num_segments {
        let db_clone = Arc::clone(&db);

        let handle = thread::spawn(move || {
            let mut segment_items = Vec::new();
            let mut last_key = None;

            loop {
                let mut scan = Scan::new()
                    .segment(segment, num_segments)
                    .limit(page_size);

                if let Some((last_pk, last_sk)) = &last_key {
                    scan = scan.start_after(last_pk, last_sk.as_deref());
                }

                let response = db_clone.scan(scan)?;
                segment_items.extend(response.items);

                if response.last_key.is_none() {
                    break;
                }

                last_key = response.last_key;
            }

            Ok::<_, Error>(segment_items)
        });

        handles.push(handle);
    }

    // Collect all results
    let mut all_items = Vec::new();

    for handle in handles {
        match handle.join() {
            Ok(Ok(items)) => all_items.extend(items),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(Error::Internal("Thread panicked".to_string())),
        }
    }

    Ok(all_items)
}
```

## Performance Considerations

Understanding scan performance characteristics helps you optimize your applications.

### Scan Performance Factors

1. **Table size**: Larger tables take longer to scan
2. **Stripe distribution**: Items spread across all 256 stripes
3. **Page size**: Larger pages = fewer round trips
4. **Parallelism**: More segments = faster processing (up to a point)
5. **I/O patterns**: Sequential reads from SST files

### Benchmarking Scans

```rust
use std::time::Instant;

// Measure full table scan
let start = Instant::now();
let scan = Scan::new();
let response = db.scan(scan)?;
let duration = start.elapsed();

println!("Scanned {} items in {:?}", response.count, duration);
println!("Throughput: {:.2} items/sec",
    response.count as f64 / duration.as_secs_f64());

// Measure parallel scan
let start = Instant::now();
let items = parallel_scan(Arc::new(db), 4)?;
let duration = start.elapsed();

println!("Parallel scan: {} items in {:?}", items.len(), duration);
println!("Throughput: {:.2} items/sec",
    items.len() as f64 / duration.as_secs_f64());
```

### Optimizing Scan Performance

1. **Use parallel scans** for large tables:

```rust
// Good: 4-8 segments for most workloads
let items = parallel_scan(db, 4)?;

// Bad: Single-threaded scan on large table
let scan = Scan::new();
let items = db.scan(scan)?;
```

2. **Choose appropriate page sizes**:

```rust
// Good: Balanced page size (100-1000 items)
let scan = Scan::new().limit(500);

// Bad: Too small (many round trips)
let scan = Scan::new().limit(10);

// Bad: Too large (memory pressure)
let scan = Scan::new().limit(1_000_000);
```

3. **Process items incrementally**:

```rust
// Good: Stream processing
let mut last_key = None;
loop {
    let mut scan = Scan::new().limit(100);
    if let Some((pk, sk)) = &last_key {
        scan = scan.start_after(pk, sk.as_deref());
    }

    let response = db.scan(scan)?;

    // Process batch immediately
    for item in response.items {
        process_item(item)?;
    }

    if response.last_key.is_none() {
        break;
    }
    last_key = response.last_key;
}

// Bad: Load everything into memory
let scan = Scan::new();
let all_items = db.scan(scan)?;  // Could be millions of items!
```

## When to Use Scan vs Query

Choosing the right operation is critical for performance.

### Use Query When

1. **You know the partition key**:

```rust
// Query: Fast, targeted
let query = Query::new(b"user#alice");
let posts = db.query(query)?;
```

2. **Retrieving related items**:

```rust
// Query: All posts for a user
let query = Query::new(b"user#alice").sk_begins_with(b"post#");
let posts = db.query(query)?;
```

3. **Time-based filtering within a partition**:

```rust
// Query: Recent activity for a user
let query = Query::new(b"user#alice")
    .sk_gte(b"2024-01-01")
    .limit(100);
let recent = db.query(query)?;
```

### Use Scan When

1. **Analyzing all data**:

```rust
// Scan: Count all items
let scan = Scan::new();
let response = db.scan(scan)?;
println!("Total items: {}", response.count);
```

2. **Bulk operations across partitions**:

```rust
// Scan: Export all data
let scan = Scan::new();
let all_items = db.scan(scan)?;
export_to_csv(&all_items)?;
```

3. **Administrative tasks**:

```rust
// Scan: Find items with missing attributes
let scan = Scan::new();
let response = db.scan(scan)?;

for item in response.items {
    if !item.contains_key("email") {
        println!("Item missing email: {:?}", item);
    }
}
```

4. **Working with multiple partitions**:

```rust
// Scan: Find all active users (across partitions)
let scan = Scan::new();
let response = db.scan(scan)?;

let active_users: Vec<_> = response.items
    .into_iter()
    .filter(|item| {
        item.get("active")
            .and_then(|v| match v {
                Value::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false)
    })
    .collect();
```

### Hybrid Approach: Index + Query

For complex access patterns, use indexes instead of scans:

```rust
use kstone_api::{Database, TableSchema, GlobalSecondaryIndex};

// Create GSI on status attribute
let schema = TableSchema::new()
    .add_global_index(GlobalSecondaryIndex::new("status-index", "status"));

let db = Database::create_with_schema("mydb.keystone", schema)?;

// Instead of scanning all items and filtering by status...
let scan = Scan::new();
let all_items = db.scan(scan)?;
let active = all_items.into_iter()
    .filter(|item| item.get("status").and_then(|v| v.as_string()) == Some("active"))
    .collect::<Vec<_>>();

// Use GSI query (much faster!)
let query = Query::new(b"active").index("status-index");
let active = db.query(query)?;
```

## Common Scan Patterns

### 1. Count All Items

```rust
let scan = Scan::new();
let response = db.scan(scan)?;
println!("Total items: {}", response.count);
```

### 2. Export Data

```rust
fn export_to_json(db: &Database, output_path: &str) -> Result<(), Error> {
    let mut all_items = Vec::new();
    let mut last_key = None;

    loop {
        let mut scan = Scan::new().limit(1000);

        if let Some((pk, sk)) = &last_key {
            scan = scan.start_after(pk, sk.as_deref());
        }

        let response = db.scan(scan)?;
        all_items.extend(response.items);

        if response.last_key.is_none() {
            break;
        }
        last_key = response.last_key;
    }

    let json = serde_json::to_string_pretty(&all_items)?;
    std::fs::write(output_path, json)?;

    Ok(())
}
```

### 3. Data Migration

```rust
fn migrate_data(source: &Database, dest: &Database) -> Result<(), Error> {
    let mut migrated = 0;
    let mut last_key = None;

    loop {
        let mut scan = Scan::new().limit(100);

        if let Some((pk, sk)) = &last_key {
            scan = scan.start_after(pk, sk.as_deref());
        }

        let response = source.scan(scan)?;

        // Write to destination
        for item in response.items {
            // Extract key from item metadata (simplified)
            dest.put(b"migrated#item", item)?;
            migrated += 1;
        }

        if response.last_key.is_none() {
            break;
        }
        last_key = response.last_key;
    }

    println!("Migrated {} items", migrated);
    Ok(())
}
```

### 4. Data Validation

```rust
fn validate_all_items(db: &Database) -> Result<Vec<String>, Error> {
    let mut errors = Vec::new();
    let mut last_key = None;

    loop {
        let mut scan = Scan::new().limit(100);

        if let Some((pk, sk)) = &last_key {
            scan = scan.start_after(pk, sk.as_deref());
        }

        let response = db.scan(scan)?;

        for item in response.items {
            // Validate required fields
            if !item.contains_key("id") {
                errors.push("Missing id field".to_string());
            }

            if !item.contains_key("created_at") {
                errors.push("Missing created_at field".to_string());
            }

            // Validate data types
            if let Some(age) = item.get("age") {
                match age {
                    Value::N(_) => {}, // Valid
                    _ => errors.push("Age must be a number".to_string()),
                }
            }
        }

        if response.last_key.is_none() {
            break;
        }
        last_key = response.last_key;
    }

    Ok(errors)
}
```

### 5. Aggregation

```rust
fn compute_statistics(db: &Database) -> Result<Statistics, Error> {
    let mut total_items = 0;
    let mut total_age = 0.0;
    let mut last_key = None;

    loop {
        let mut scan = Scan::new().limit(1000);

        if let Some((pk, sk)) = &last_key {
            scan = scan.start_after(pk, sk.as_deref());
        }

        let response = db.scan(scan)?;

        for item in response.items {
            total_items += 1;

            if let Some(Value::N(age_str)) = item.get("age") {
                if let Ok(age) = age_str.parse::<f64>() {
                    total_age += age;
                }
            }
        }

        if response.last_key.is_none() {
            break;
        }
        last_key = response.last_key;
    }

    let avg_age = if total_items > 0 {
        total_age / total_items as f64
    } else {
        0.0
    };

    Ok(Statistics {
        total_items,
        average_age: avg_age,
    })
}

struct Statistics {
    total_items: usize,
    average_age: f64,
}
```

## Error Handling

Handle scan errors robustly:

```rust
use kstone_core::Error;

match db.scan(Scan::new()) {
    Ok(response) => {
        println!("Scanned {} items", response.count);
        process_items(response.items);
    }
    Err(Error::Io(e)) => {
        eprintln!("I/O error during scan: {}", e);
        // Retry or fail gracefully
    }
    Err(Error::Internal(msg)) => {
        eprintln!("Internal error: {}", msg);
    }
    Err(e) => {
        eprintln!("Scan error: {}", e);
    }
}
```

## Best Practices

### 1. Always Use Pagination

```rust
// Good: Paginated scan
let mut last_key = None;
loop {
    let mut scan = Scan::new().limit(100);
    if let Some((pk, sk)) = &last_key {
        scan = scan.start_after(pk, sk.as_deref());
    }
    let response = db.scan(scan)?;
    process_batch(response.items);
    if response.last_key.is_none() { break; }
    last_key = response.last_key;
}

// Bad: Load everything at once
let scan = Scan::new();
let all = db.scan(scan)?;  // Could exhaust memory
```

### 2. Use Parallel Scans for Large Tables

```rust
// Good: Parallel execution
let items = parallel_scan(Arc::new(db), 4)?;

// Bad: Single-threaded on large table
let scan = Scan::new();
let items = db.scan(scan)?;
```

### 3. Choose Appropriate Segment Counts

```rust
// Good: 4-8 segments for most workloads
let items = parallel_scan(db, 4)?;

// Bad: Too many segments (overhead)
let items = parallel_scan(db, 256)?;

// Bad: Single segment (defeats parallelism)
let items = parallel_scan(db, 1)?;
```

### 4. Avoid Scans for Frequent Operations

```rust
// Bad: Scanning for every request
fn get_user_by_email(db: &Database, email: &str) -> Result<Option<Item>, Error> {
    let scan = Scan::new();
    let response = db.scan(scan)?;

    Ok(response.items.into_iter()
        .find(|item| {
            item.get("email").and_then(|v| v.as_string()) == Some(email)
        }))
}

// Good: Use GSI for frequent lookups
let schema = TableSchema::new()
    .add_global_index(GlobalSecondaryIndex::new("email-index", "email"));

let db = Database::create_with_schema("mydb.keystone", schema)?;

let query = Query::new(email.as_bytes()).index("email-index");
let response = db.query(query)?;
```

### 5. Handle Large Result Sets Carefully

```rust
// Good: Stream processing
fn process_large_table(db: &Database) -> Result<(), Error> {
    let mut last_key = None;

    loop {
        let mut scan = Scan::new().limit(100);
        if let Some((pk, sk)) = &last_key {
            scan = scan.start_after(pk, sk.as_deref());
        }

        let response = db.scan(scan)?;

        // Process and discard items immediately
        for item in response.items {
            process_item(item)?;
        }

        if response.last_key.is_none() {
            break;
        }
        last_key = response.last_key;
    }

    Ok(())
}
```

## Summary

This chapter covered KeystoneDB's scan capabilities:

- **Scan basics**: Reading items across all partitions
- **Pagination**: Processing large tables in chunks
- **Parallel scans**: Distributing work across segments for faster processing
- **Performance**: Understanding scan costs and optimization strategies
- **Query vs Scan**: Choosing the right operation for your use case
- **Common patterns**: Export, migration, validation, aggregation
- **Best practices**: Pagination, parallelism, appropriate segment counts

In the next chapter, we'll explore batch operations - how to efficiently read and write multiple items in a single request.

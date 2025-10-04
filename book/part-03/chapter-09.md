# Chapter 9: Querying Data

While CRUD operations allow you to work with individual items, queries enable you to retrieve multiple related items efficiently. KeystoneDB's query API is modeled after DynamoDB's Query operation, providing powerful filtering and pagination capabilities. This chapter explores how to query data effectively using partition keys and sort key conditions.

## Understanding Query Operations

A query operation retrieves all items that share the same partition key, optionally filtered by sort key conditions. Unlike scans (which read the entire table), queries are highly efficient because:

1. **Stripe targeting**: KeystoneDB routes to a single stripe based on the partition key
2. **Sequential reads**: Items with the same partition key are stored together
3. **Sorted order**: Items are sorted by sort key, enabling range queries
4. **Early termination**: Queries stop as soon as the limit is reached

### Query Requirements

Every query must specify:
- **Partition key**: The exact partition key value to query (equality only)
- **Sort key condition** (optional): Filter items by sort key patterns

### Query vs Get vs Scan

Understanding when to use each operation:

```rust
// Get: Retrieve a single item by exact key
let user = db.get(b"user#alice")?;

// Query: Retrieve multiple items with same partition key
let query = Query::new(b"user#alice");
let posts = db.query(query)?;

// Scan: Retrieve items across all partition keys (covered in Chapter 10)
let scan = Scan::new();
let all_items = db.scan(scan)?;
```

## Basic Query

The simplest query retrieves all items for a given partition key.

### Query All Items in a Partition

```rust
use kstone_api::{Database, Query, ItemBuilder};

let db = Database::open("mydb.keystone")?;

// Insert test data: multiple posts for one user
for i in 1..=10 {
    let post = ItemBuilder::new()
        .string("title", format!("Post #{}", i))
        .string("content", format!("Content for post {}", i))
        .number("views", i * 100)
        .build();

    db.put_with_sk(
        b"user#alice",
        format!("post#{:03}", i).as_bytes(),  // post#001, post#002, etc.
        post
    )?;
}

// Query all posts for user#alice
let query = Query::new(b"user#alice");
let response = db.query(query)?;

println!("Found {} posts", response.count);
println!("Scanned {} items", response.scanned_count);

for item in response.items {
    let title = item.get("title").and_then(|v| v.as_string()).unwrap_or("Untitled");
    println!("- {}", title);
}
```

### Query Response Structure

The `QueryResponse` contains:

```rust
pub struct QueryResponse {
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
- `count`: Number of items in the response (same as `items.len()`)
- `last_key`: The last key evaluated; used for pagination
- `scanned_count`: Total items examined (before limit was applied)

## Sort Key Conditions

Sort key conditions allow you to filter which items are returned based on the sort key value.

### Equal Condition

Match an exact sort key value:

```rust
// Get a specific post
let query = Query::new(b"user#alice")
    .sk_eq(b"post#005");

let response = db.query(query)?;

// Returns at most 1 item (partition key + sort key uniquely identifies an item)
assert!(response.count <= 1);
```

**Note**: Using `sk_eq()` is functionally similar to `get_with_sk()`, but returns a collection.

### Less Than and Less Than or Equal

Retrieve items before a certain sort key:

```rust
// Get all posts before post#005 (exclusive)
let query = Query::new(b"user#alice")
    .sk_lt(b"post#005");

let response = db.query(query)?;
// Returns: post#001, post#002, post#003, post#004

// Get all posts up to and including post#005 (inclusive)
let query = Query::new(b"user#alice")
    .sk_lte(b"post#005");

let response = db.query(query)?;
// Returns: post#001, post#002, post#003, post#004, post#005
```

### Greater Than and Greater Than or Equal

Retrieve items after a certain sort key:

```rust
// Get all posts after post#005 (exclusive)
let query = Query::new(b"user#alice")
    .sk_gt(b"post#005");

let response = db.query(query)?;
// Returns: post#006, post#007, post#008, post#009, post#010

// Get all posts from post#005 onwards (inclusive)
let query = Query::new(b"user#alice")
    .sk_gte(b"post#005");

let response = db.query(query)?;
// Returns: post#005, post#006, post#007, post#008, post#009, post#010
```

### Between Condition

Retrieve items within a range (both bounds inclusive):

```rust
// Get posts from #003 to #007 (inclusive)
let query = Query::new(b"user#alice")
    .sk_between(b"post#003", b"post#007");

let response = db.query(query)?;
// Returns: post#003, post#004, post#005, post#006, post#007

assert_eq!(response.count, 5);
```

### Begins With Condition

Match items whose sort key starts with a specific prefix:

```rust
// Store different types of items for a user
db.put_with_sk(b"user#bob", b"post#001", post1)?;
db.put_with_sk(b"user#bob", b"post#002", post2)?;
db.put_with_sk(b"user#bob", b"comment#001", comment1)?;
db.put_with_sk(b"user#bob", b"comment#002", comment2)?;
db.put_with_sk(b"user#bob", b"like#001", like1)?;

// Query only posts
let query = Query::new(b"user#bob")
    .sk_begins_with(b"post#");

let response = db.query(query)?;
// Returns only: post#001, post#002

// Query only comments
let query = Query::new(b"user#bob")
    .sk_begins_with(b"comment#");

let response = db.query(query)?;
// Returns only: comment#001, comment#002
```

The `begins_with` condition is extremely useful for hierarchical data models and type prefixes.

### Practical Example: Time-Series Queries

```rust
// Store hourly temperature readings
for hour in 0..24 {
    let reading = ItemBuilder::new()
        .number("temperature", 20.0 + hour as f64 * 0.5)
        .number("humidity", 45 + hour)
        .build();

    // Sort key: YYYY-MM-DD-HH format for lexicographic ordering
    let timestamp = format!("2024-01-15-{:02}", hour);
    db.put_with_sk(b"sensor#kitchen", timestamp.as_bytes(), reading)?;
}

// Query: Get readings from noon onwards
let query = Query::new(b"sensor#kitchen")
    .sk_gte(b"2024-01-15-12");

let response = db.query(query)?;
assert_eq!(response.count, 12); // Hours 12-23

// Query: Get readings for business hours (9 AM to 5 PM)
let query = Query::new(b"sensor#kitchen")
    .sk_between(b"2024-01-15-09", b"2024-01-15-17");

let response = db.query(query)?;
assert_eq!(response.count, 9); // Hours 9-17 inclusive

// Query: Get all January 15th readings
let query = Query::new(b"sensor#kitchen")
    .sk_begins_with(b"2024-01-15");

let response = db.query(query)?;
assert_eq!(response.count, 24); // All 24 hours
```

## Forward and Reverse Iteration

By default, queries return items in ascending sort key order. You can reverse this to get items in descending order.

### Forward Iteration (Default)

```rust
// Forward iteration (ascending sort key order)
let query = Query::new(b"user#alice")
    .forward(true);  // Explicit, but this is the default

let response = db.query(query)?;
// Returns: post#001, post#002, post#003, ...
```

### Reverse Iteration

```rust
// Reverse iteration (descending sort key order)
let query = Query::new(b"user#alice")
    .forward(false);

let response = db.query(query)?;
// Returns: post#010, post#009, post#008, ...

// Useful for "most recent first" queries
for item in response.items {
    let title = item.get("title").and_then(|v| v.as_string()).unwrap();
    println!("{}", title); // Prints in reverse order
}
```

### Combining Reverse with Conditions

```rust
// Get the 5 most recent posts
let query = Query::new(b"user#alice")
    .forward(false)  // Reverse order
    .limit(5);       // Take first 5

let response = db.query(query)?;
// Returns: post#010, post#009, post#008, post#007, post#006

// Get recent posts after a specific point
let query = Query::new(b"user#alice")
    .sk_lt(b"post#008")   // Before post#008
    .forward(false)        // Reverse order
    .limit(3);

let response = db.query(query)?;
// Returns: post#007, post#006, post#005
```

### Practical Example: Leaderboard

```rust
// Store player scores (higher is better)
let players = vec![
    ("alice", 9500),
    ("bob", 8200),
    ("charlie", 7800),
    ("dave", 9200),
    ("eve", 8500),
];

for (player, score) in players {
    let player_data = ItemBuilder::new()
        .string("name", player)
        .number("score", score)
        .build();

    // Use zero-padded score as sort key (higher scores = higher sort key)
    let sk = format!("score#{:010}", score);
    db.put_with_sk(b"game#leaderboard", sk.as_bytes(), player_data)?;
}

// Get top 3 players (highest scores)
let query = Query::new(b"game#leaderboard")
    .sk_begins_with(b"score#")
    .forward(false)  // Descending order
    .limit(3);

let response = db.query(query)?;

for (rank, item) in response.items.iter().enumerate() {
    let name = item.get("name").and_then(|v| v.as_string()).unwrap();
    let score = item.get("score").and_then(|v| v.as_string()).unwrap();
    println!("{}. {} - {} points", rank + 1, name, score);
}
// Output:
// 1. alice - 9500 points
// 2. dave - 9200 points
// 3. eve - 8500 points
```

## Pagination with Limit and LastEvaluatedKey

For large result sets, pagination allows you to retrieve data in manageable chunks.

### Using Limit

The `limit()` method restricts how many items are returned:

```rust
// Get first 10 posts
let query = Query::new(b"user#alice")
    .limit(10);

let response = db.query(query)?;

if response.count == 10 && response.last_key.is_some() {
    println!("There may be more results");
}
```

### Pagination Pattern

Use `last_key` to fetch subsequent pages:

```rust
// First page
let query = Query::new(b"user#alice")
    .limit(5);

let page1 = db.query(query)?;
println!("Page 1: {} items", page1.count);

// Check if there are more results
if let Some((last_pk, last_sk)) = page1.last_key {
    // Second page
    let query2 = Query::new(&last_pk)
        .limit(5)
        .start_after(&last_pk, last_sk.as_deref());

    let page2 = db.query(query2)?;
    println!("Page 2: {} items", page2.count);

    // Third page
    if let Some((last_pk2, last_sk2)) = page2.last_key {
        let query3 = Query::new(&last_pk2)
            .limit(5)
            .start_after(&last_pk2, last_sk2.as_deref());

        let page3 = db.query(query3)?;
        println!("Page 3: {} items", page3.count);
    }
}
```

### Complete Pagination Example

```rust
fn paginate_query(db: &Database, pk: &[u8], page_size: usize) -> Result<Vec<Item>, Error> {
    let mut all_items = Vec::new();
    let mut last_key = None;

    loop {
        // Build query
        let mut query = Query::new(pk).limit(page_size);

        // Add pagination if not first page
        if let Some((last_pk, last_sk)) = &last_key {
            query = query.start_after(last_pk, last_sk.as_deref());
        }

        // Execute query
        let response = db.query(query)?;

        // Collect items
        all_items.extend(response.items);

        // Check for more pages
        if response.last_key.is_none() {
            break; // No more pages
        }

        last_key = response.last_key;
    }

    Ok(all_items)
}

// Usage
let all_posts = paginate_query(&db, b"user#alice", 10)?;
println!("Retrieved {} total posts", all_posts.len());
```

### Pagination with Conditions

Pagination works with all sort key conditions:

```rust
// Paginate through recent posts
let mut last_key = None;
let mut page_num = 1;

loop {
    let mut query = Query::new(b"user#alice")
        .sk_gte(b"post#050")  // Only posts >= 050
        .forward(false)        // Most recent first
        .limit(10);

    if let Some((last_pk, last_sk)) = &last_key {
        query = query.start_after(last_pk, last_sk.as_deref());
    }

    let response = db.query(query)?;

    println!("Page {}: {} items", page_num, response.count);
    for item in &response.items {
        let title = item.get("title").and_then(|v| v.as_string()).unwrap();
        println!("  - {}", title);
    }

    if response.last_key.is_none() {
        break;
    }

    last_key = response.last_key;
    page_num += 1;
}
```

## Query Performance Characteristics

Understanding query performance helps you design efficient applications.

### Stripe Targeting

Queries operate on a single stripe determined by the partition key:

```rust
use kstone_core::Key;

// These queries target different stripes
let query1 = Query::new(b"user#alice");   // Stripe: crc32("user#alice") % 256
let query2 = Query::new(b"user#bob");     // Stripe: crc32("user#bob") % 256
let query3 = Query::new(b"sensor#temp");  // Stripe: crc32("sensor#temp") % 256

// Queries are independent and can run in parallel
```

### Sequential Reads

Items with the same partition key are stored sequentially in SST files, making range queries efficient:

```rust
// Efficient: Sequential read within one stripe
let query = Query::new(b"user#alice")
    .sk_between(b"post#001", b"post#100");

let response = db.query(query)?;
// Fast: Items are stored together, minimal disk seeks
```

### Early Termination

Queries stop as soon as the limit is reached:

```rust
// Efficient: Stops after finding 10 items
let query = Query::new(b"user#alice")
    .limit(10);

let response = db.query(query)?;
// Even if there are 1000 items, only 10 are read and returned
```

### Bloom Filter Optimization

Bloom filters help skip SST files that don't contain matching keys:

```rust
// Query with specific condition
let query = Query::new(b"user#alice")
    .sk_eq(b"post#099");

// Bloom filters may skip SSTs that don't contain this key
let response = db.query(query)?;
```

### Performance Comparison

```rust
use std::time::Instant;

// Measure query performance
let start = Instant::now();
let query = Query::new(b"user#alice").limit(100);
let response = db.query(query)?;
let duration = start.elapsed();

println!("Query returned {} items in {:?}", response.count, duration);
// Typical: 10-100 microseconds for memtable, 100-1000 microseconds from SST

// Compare to individual gets
let start = Instant::now();
for i in 1..=100 {
    let sk = format!("post#{:03}", i);
    db.get_with_sk(b"user#alice", sk.as_bytes())?;
}
let duration = start.elapsed();

println!("100 individual gets took {:?}", duration);
// Much slower: Each get is a separate operation
```

## Advanced Query Patterns

### Multi-Entity Queries

Store different entity types under one partition key:

```rust
// User profile with multiple entity types
db.put_with_sk(b"user#alice", b"profile", profile_data)?;
db.put_with_sk(b"user#alice", b"settings", settings_data)?;

// Posts with timestamps
for i in 1..=5 {
    db.put_with_sk(
        b"user#alice",
        format!("post#2024-01-{:02}", i).as_bytes(),
        post_data.clone()
    )?;
}

// Comments with timestamps
for i in 1..=3 {
    db.put_with_sk(
        b"user#alice",
        format!("comment#2024-01-{:02}", i).as_bytes(),
        comment_data.clone()
    )?;
}

// Query all posts
let posts = db.query(Query::new(b"user#alice").sk_begins_with(b"post#"))?;

// Query all comments
let comments = db.query(Query::new(b"user#alice").sk_begins_with(b"comment#"))?;

// Query all activity in January
let activity = db.query(
    Query::new(b"user#alice")
        .sk_begins_with(b"post#2024-01")
)?;
```

### Composite Sort Keys

Combine multiple dimensions in the sort key:

```rust
// Format: TYPE#CATEGORY#TIMESTAMP#ID
db.put_with_sk(
    b"store#inventory",
    b"product#electronics#2024-01-15#laptop001",
    laptop_data
)?;

db.put_with_sk(
    b"store#inventory",
    b"product#electronics#2024-01-16#phone001",
    phone_data
)?;

db.put_with_sk(
    b"store#inventory",
    b"product#books#2024-01-15#novel001",
    book_data
)?;

// Query: All electronics
let electronics = db.query(
    Query::new(b"store#inventory")
        .sk_begins_with(b"product#electronics#")
)?;

// Query: Electronics added on 2024-01-15
let jan15_electronics = db.query(
    Query::new(b"store#inventory")
        .sk_begins_with(b"product#electronics#2024-01-15")
)?;

// Query: All products added between two dates
let date_range = db.query(
    Query::new(b"store#inventory")
        .sk_between(
            b"product#electronics#2024-01-15",
            b"product#electronics#2024-01-16"
        )
)?;
```

### Versioned Data

Query different versions of an entity:

```rust
// Store document versions
for version in 1..=10 {
    let doc = ItemBuilder::new()
        .string("content", format!("Version {} content", version))
        .number("version", version)
        .build();

    let sk = format!("v{:05}", version); // v00001, v00002, etc.
    db.put_with_sk(b"doc#readme", sk.as_bytes(), doc)?;
}

// Get latest 5 versions
let recent_versions = db.query(
    Query::new(b"doc#readme")
        .forward(false)  // Descending
        .limit(5)
)?;

// Get versions 3-7
let version_range = db.query(
    Query::new(b"doc#readme")
        .sk_between(b"v00003", b"v00007")
)?;

// Get all versions after v00005
let after_v5 = db.query(
    Query::new(b"doc#readme")
        .sk_gt(b"v00005")
)?;
```

### Querying with Indexes

Queries can also target Local Secondary Indexes (LSI) and Global Secondary Indexes (GSI):

```rust
use kstone_api::{Database, TableSchema, LocalSecondaryIndex};

// Create database with LSI on email
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"));

let db = Database::create_with_schema("mydb.keystone", schema)?;

// Put items
db.put(b"org#acme", ItemBuilder::new()
    .string("name", "Alice")
    .string("email", "alice@acme.com")
    .build())?;

db.put(b"org#acme", ItemBuilder::new()
    .string("name", "Bob")
    .string("email", "bob@acme.com")
    .build())?;

// Query by email using LSI
let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice");

let response = db.query(query)?;
// Returns items where email starts with "alice"
```

Indexes are covered in depth in Part IV: Advanced Features.

## Common Query Patterns

### 1. Latest N Items

```rust
// Get 10 most recent posts
let recent = db.query(
    Query::new(b"user#alice")
        .forward(false)
        .limit(10)
)?;
```

### 2. Time Range Queries

```rust
// Get activity between two timestamps
let range = db.query(
    Query::new(b"user#alice")
        .sk_between(b"2024-01-01", b"2024-01-31")
)?;
```

### 3. Type Filtering

```rust
// Get only comments
let comments = db.query(
    Query::new(b"user#alice")
        .sk_begins_with(b"comment#")
)?;
```

### 4. Pagination for Large Results

```rust
// Process all items in batches
let mut last_key = None;

loop {
    let mut query = Query::new(b"user#alice").limit(100);

    if let Some((pk, sk)) = &last_key {
        query = query.start_after(pk, sk.as_deref());
    }

    let response = db.query(query)?;

    // Process batch
    process_batch(&response.items);

    if response.last_key.is_none() {
        break;
    }
    last_key = response.last_key;
}
```

### 5. Count Items

```rust
// Count items (retrieve without processing)
let query = Query::new(b"user#alice");
let response = db.query(query)?;
println!("User has {} items", response.count);
```

## Error Handling

Handle query errors gracefully:

```rust
use kstone_core::Error;

match db.query(Query::new(b"user#alice")) {
    Ok(response) => {
        if response.count == 0 {
            println!("No items found");
        } else {
            println!("Found {} items", response.count);
            for item in response.items {
                process_item(item);
            }
        }
    }
    Err(Error::Io(e)) => {
        eprintln!("I/O error during query: {}", e);
    }
    Err(Error::InvalidQuery(msg)) => {
        eprintln!("Invalid query: {}", msg);
    }
    Err(e) => {
        eprintln!("Query error: {}", e);
    }
}
```

## Best Practices

### 1. Design Sort Keys for Queries

Choose sort key formats that support your query patterns:

```rust
// Good: Hierarchical sort keys
b"post#2024-01-15#12:30:00#abc123"  // Type, date, time, ID

// Good: Zero-padded numbers
b"order#00001234"

// Bad: Random UUIDs (can't do range queries)
b"550e8400-e29b-41d4-a716-446655440000"
```

### 2. Use Limit to Control Response Size

Always set reasonable limits:

```rust
// Good: Controlled batch size
let query = Query::new(b"user#alice").limit(100);

// Bad: No limit (could return millions of items)
let query = Query::new(b"user#alice");
```

### 3. Handle Empty Results

```rust
let response = db.query(Query::new(b"user#alice"))?;

if response.count == 0 {
    println!("No items found");
    return Ok(());
}

// Process items
for item in response.items {
    process_item(item);
}
```

### 4. Use Begins With for Prefixes

```rust
// Efficient: Prefix matching
let query = Query::new(b"user#alice")
    .sk_begins_with(b"post#2024");

// Less efficient: Multiple exact matches
for month in 1..=12 {
    let sk = format!("post#2024-{:02}", month);
    let q = Query::new(b"user#alice").sk_eq(sk.as_bytes());
    db.query(q)?;
}
```

### 5. Combine Forward/Reverse with Limit

```rust
// Get oldest 10 items
let oldest = db.query(
    Query::new(b"user#alice")
        .forward(true)
        .limit(10)
)?;

// Get newest 10 items
let newest = db.query(
    Query::new(b"user#alice")
        .forward(false)
        .limit(10)
)?;
```

## Summary

This chapter covered KeystoneDB's query capabilities:

- **Query basics**: Retrieving multiple items by partition key
- **Sort key conditions**: Equal, less than, greater than, between, begins with
- **Direction**: Forward and reverse iteration
- **Pagination**: Using limit and last_key for large result sets
- **Performance**: Understanding stripe targeting and sequential reads
- **Patterns**: Time-series, versioning, multi-entity queries
- **Best practices**: Sort key design, limits, error handling

In the next chapter, we'll explore table scans - how to retrieve data across all partition keys efficiently using parallel execution.

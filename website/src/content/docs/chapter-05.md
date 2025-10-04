# Chapter 5: Keys and Partitioning

Keys are the foundation of data organization in KeystoneDB. Unlike relational databases where primary keys are often simple integers, KeystoneDB uses a composite key model borrowed from Amazon DynamoDB. This chapter explores how keys work, how they're encoded, and how KeystoneDB's 256-stripe architecture uses keys to achieve horizontal scalability.

## The Key Model

KeystoneDB uses a **composite key** system consisting of two components:

1. **Partition Key (PK)**: Required for all items. Determines data distribution across stripes.
2. **Sort Key (SK)**: Optional. Enables multiple items under the same partition key to be stored in sorted order.

The `Key` struct in the core types module defines this model:

```rust
pub struct Key {
    pub pk: Bytes,              // Partition key (required)
    pub sk: Option<Bytes>,      // Sort key (optional)
}
```

### Partition Key (PK)

The partition key is the primary identifier for an item and serves two critical purposes:

1. **Uniqueness**: Items are uniquely identified by their key (PK alone, or PK+SK combination)
2. **Distribution**: The PK determines which stripe stores the item

**Creating keys:**
```rust
use kstone_core::Key;

// Simple key (PK only)
let user_key = Key::new(b"user#12345".to_vec());

// Application-level
db.put(b"user#12345", user_item)?;
```

**Partition key characteristics:**
- **Immutable**: Once set, cannot be changed (requires delete + re-insert)
- **Byte sequence**: Any valid byte sequence, typically UTF-8 strings
- **Arbitrary length**: No hard limit, but shorter keys are more efficient
- **Determines stripe**: Hash of PK determines storage location

### Sort Key (SK)

The sort key is optional but enables powerful data modeling patterns. When present, it allows multiple items under the same partition key, stored in sorted order.

**Creating composite keys:**
```rust
// Composite key (PK + SK)
let post_key = Key::with_sk(
    b"user#12345".to_vec(),      // Partition key
    b"post#2024-01-15#789".to_vec()  // Sort key
);

// Application-level
db.put_with_sk(
    b"user#12345",                   // PK
    b"post#2024-01-15#789",          // SK
    post_item
)?;
```

**Sort key characteristics:**
- **Optional**: Items can have just PK, or PK+SK
- **Sortable**: Items with same PK are stored sorted by SK
- **Byte-order sorted**: Lexicographic byte comparison
- **Enables range queries**: Query items within SK ranges
- **Part of uniqueness**: PK+SK combination must be unique

### Simple vs. Composite Keys

**Simple key (PK only):**
```rust
// User profiles - one item per user
db.put(b"user#12345", user_profile)?;

// Products - one item per product
db.put(b"product#SKU789", product_details)?;

// Configuration - one item per config key
db.put(b"config#max_connections", config_value)?;
```

**Composite key (PK + SK):**
```rust
// User posts - multiple posts per user
db.put_with_sk(b"user#12345", b"post#001", post_1)?;
db.put_with_sk(b"user#12345", b"post#002", post_2)?;
db.put_with_sk(b"user#12345", b"post#003", post_3)?;

// Order line items - multiple items per order
db.put_with_sk(b"order#789", b"item#001", line_item_1)?;
db.put_with_sk(b"order#789", b"item#002", line_item_2)?;

// Time-series data - multiple readings per sensor
db.put_with_sk(b"sensor#456", b"2024-01-15T10:00:00Z", reading_1)?;
db.put_with_sk(b"sensor#456", b"2024-01-15T10:01:00Z", reading_2)?;
```

## Key Encoding

Keys are stored in an efficient binary format that preserves their composite structure. Understanding key encoding is essential for understanding how keys are compared and sorted.

### Encoding Format

The key encoding uses length-prefixed components:

```
[pk_len(4 bytes) | pk_bytes | sk_len(4 bytes) | sk_bytes]
```

**For a simple key** (PK only):
```
[pk_len | pk_bytes | 0x00000000]
```

**For a composite key** (PK + SK):
```
[pk_len | pk_bytes | sk_len | sk_bytes]
```

**Implementation:**
```rust
impl Key {
    pub fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();
        buf.put_u32(self.pk.len() as u32);
        buf.put(self.pk.clone());
        if let Some(sk) = &self.sk {
            buf.put_u32(sk.len() as u32);
            buf.put(sk.clone());
        } else {
            buf.put_u32(0);
        }
        buf.freeze()
    }
}
```

### Why Length-Prefixed Encoding?

1. **Variable-length support**: Keys can be any length
2. **Efficient comparison**: Compare components separately
3. **Unambiguous parsing**: Know exactly where PK ends and SK begins
4. **Sortability**: Encoded keys sort correctly in BTreeMap

### Encoded Key Examples

**Example 1: Simple key**
```rust
let key = Key::new(b"user#123".to_vec());
// Encoded (hex): 00 00 00 08 75 73 65 72 23 31 32 33 00 00 00 00
//                |-- len --| |------- user#123 -------| |-- no SK-|
```

**Example 2: Composite key**
```rust
let key = Key::with_sk(b"user#123".to_vec(), b"post#001".to_vec());
// Encoded (hex):
// 00 00 00 08 75 73 65 72 23 31 32 33 00 00 00 08 70 6f 73 74 23 30 30 31
// |-- pk_len -| |------- user#123 -------| |-- sk_len -| |--- post#001 ---|
```

### Key Comparison and Sorting

Encoded keys are compared lexicographically (byte-by-byte):

```rust
// These keys sort in this order:
user#123 (no SK)
user#123:post#001
user#123:post#002
user#124 (no SK)
user#124:post#001
```

**Why this matters:**
1. **Query efficiency**: Range queries scan sorted keys
2. **Memtable organization**: BTreeMap keeps keys sorted
3. **SST file layout**: Records stored in key order for binary search

## Stripe Selection Algorithm

KeystoneDB uses a **256-stripe architecture** where each stripe is an independent LSM tree. The partition key determines which stripe stores an item using a simple but powerful algorithm.

### The Stripe Function

```rust
impl Key {
    pub fn stripe(&self) -> u8 {
        let hash = crc32fast::hash(&self.pk);
        (hash % 256) as u8
    }
}
```

**How it works:**
1. Compute CRC32 hash of partition key bytes
2. Take modulo 256 to get stripe ID (0-255)
3. Route all operations for that key to that stripe

### CRC32 Hash Function

**Why CRC32?**
- **Fast**: Hardware-accelerated on modern CPUs (SSE 4.2)
- **Good distribution**: Evenly distributes keys across stripes
- **Deterministic**: Same key always routes to same stripe
- **Simple**: No cryptographic overhead

**Hash distribution example:**
```rust
// Different PKs hash to different stripes
Key::new(b"user#1".to_vec()).stripe()   // → 147
Key::new(b"user#2".to_vec()).stripe()   // → 89
Key::new(b"user#3".to_vec()).stripe()   // → 201
```

### Same PK, Different SK → Same Stripe

This is a critical property: **all items with the same partition key route to the same stripe**, regardless of their sort keys.

```rust
// All these keys go to the same stripe
let key1 = Key::new(b"user#123".to_vec());
let key2 = Key::with_sk(b"user#123".to_vec(), b"post#001".to_vec());
let key3 = Key::with_sk(b"user#123".to_vec(), b"post#002".to_vec());

assert_eq!(key1.stripe(), key2.stripe());
assert_eq!(key2.stripe(), key3.stripe());
```

**Why this matters:**
1. **Efficient range queries**: All items in a partition are in the same stripe
2. **Data locality**: Related items stored together
3. **Consistent query performance**: No cross-stripe coordination needed

### Load Distribution

With 256 stripes and a good hash function, keys distribute evenly:

```rust
// Simulate 10,000 user keys
let mut stripe_counts = vec![0; 256];

for user_id in 0..10000 {
    let key = Key::new(format!("user#{}", user_id).into_bytes());
    stripe_counts[key.stripe() as usize] += 1;
}

// Each stripe gets approximately 10000 / 256 ≈ 39 keys
// Actual distribution: typically within 20% of average
```

**Distribution properties:**
- **Approximately uniform**: No stripe gets significantly more keys than others
- **Random**: Can't predict which stripe a key goes to
- **Stable**: Same key always goes to same stripe

## Key Design Patterns

Designing effective keys is crucial for performance and data modeling. Here are proven patterns for various use cases.

### Pattern 1: Hierarchical Keys

Use delimiters to create hierarchical namespaces:

```rust
// Tenant isolation
db.put(b"tenant#acme:user#123", user)?;
db.put(b"tenant#acme:order#789", order)?;
db.put(b"tenant#beta:user#456", user)?;

// Geographic hierarchy
db.put(b"country#US:state#CA:city#SF", data)?;

// Organization structure
db.put(b"org#engineering:team#backend:user#123", employee)?;
```

**Benefits:**
- Logical grouping by prefix
- Easy to filter by prefix in application code
- Clear ownership and relationships

### Pattern 2: Timestamp-Based Sort Keys

Sort keys with timestamps enable time-based queries:

```rust
// ISO 8601 timestamps sort chronologically
db.put_with_sk(
    b"sensor#temperature",
    b"2024-01-15T10:00:00Z",
    reading
)?;

// Reverse chronological (recent first)
// Use inverted timestamp or negative Unix time
let inverted_time = i64::MAX - timestamp;
db.put_with_sk(
    b"user#123:posts",
    format!("{:020}", inverted_time).as_bytes(),
    post
)?;
```

**Benefits:**
- Efficient time-range queries
- Natural chronological ordering
- Pagination through time windows

### Pattern 3: Composite Sort Keys

Combine multiple attributes in sort keys for complex queries:

```rust
// Status + Timestamp
db.put_with_sk(
    b"order#queue",
    b"pending#2024-01-15T10:00:00Z#order123",
    order
)?;

// Category + Price
db.put_with_sk(
    b"products",
    format!("{}#{:010}#{}",
        category,
        (price * 100.0) as i64,  // Fixed-point for sortability
        product_id
    ).as_bytes(),
    product
)?;

// Priority + CreatedAt
db.put_with_sk(
    b"tasks",
    format!("priority:{}#created:{}#{}",
        priority,
        created_at,
        task_id
    ).as_bytes(),
    task
)?;
```

**Benefits:**
- Multi-attribute sorting
- Enables complex range queries
- No secondary index needed for common patterns

### Pattern 4: Version Control

Use sort keys for versioning:

```rust
// Document versions
db.put_with_sk(b"doc#readme", b"v1", content_v1)?;
db.put_with_sk(b"doc#readme", b"v2", content_v2)?;
db.put_with_sk(b"doc#readme", b"v3", content_v3)?;

// Query latest version
let query = Query::new(b"doc#readme")
    .forward(false)  // Reverse order
    .limit(1);       // Just the latest
```

### Pattern 5: Aggregation Keys

Pre-compute aggregates with special keys:

```rust
// Individual page views
db.put_with_sk(b"stats#page:/home", b"2024-01-15#user#123", view)?;
db.put_with_sk(b"stats#page:/home", b"2024-01-15#user#456", view)?;

// Daily aggregate
db.put_with_sk(b"stats#page:/home", b"daily#2024-01-15", daily_summary)?;

// Monthly aggregate
db.put_with_sk(b"stats#page:/home", b"monthly#2024-01", monthly_summary)?;
```

## The 256-Stripe Architecture

KeystoneDB's 256-stripe architecture is central to its performance characteristics. Let's explore how it works and why 256 stripes.

### What is a Stripe?

A stripe is an **independent LSM tree** with its own:
- **Memtable**: In-memory BTreeMap for recent writes
- **SST files**: On-disk sorted string tables
- **Flush logic**: Independent flush threshold (1000 records per stripe)

```rust
struct Stripe {
    memtable: BTreeMap<Vec<u8>, Record>,  // Sorted by encoded key
    memtable_size_bytes: usize,           // Approximate size
    ssts: Vec<SstReader>,                  // Newest first
}
```

### Why 256 Stripes?

The number 256 is carefully chosen:

1. **One byte**: Stripe ID fits in a u8 (0-255)
2. **Good parallelism**: Enough stripes for multi-core systems
3. **Not too many**: Avoids excessive file descriptor usage
4. **Simple modulo**: 256 = 2^8, efficient modulo operation

**Alternative approaches:**
- **Fewer stripes (16, 32)**: Less parallelism, higher contention
- **More stripes (512, 1024)**: More file descriptors, higher overhead
- **Dynamic stripes**: Complex, requires redistribution

### Stripe Independence

Each stripe operates independently:

**Independent memtable flushing:**
```rust
// Stripe 42 flushes when its memtable hits threshold
if inner.stripes[42].memtable.len() >= MEMTABLE_THRESHOLD {
    self.flush_stripe(&mut inner, 42)?;
}

// Other stripes unaffected
```

**Independent SST files:**
```
mydb.keystone/
├── 000-1.sst    # Stripe 0, SST ID 1
├── 000-2.sst    # Stripe 0, SST ID 2
├── 042-1.sst    # Stripe 42, SST ID 1
├── 042-2.sst    # Stripe 42, SST ID 2
├── 137-1.sst    # Stripe 137, SST ID 1
└── wal.log      # Shared WAL (for crash recovery)
```

**Filename format:**
```
{stripe:03}-{sst_id}.sst
```

Examples:
- `000-1.sst` = Stripe 0, SST 1
- `042-15.sst` = Stripe 42, SST 15
- `255-3.sst` = Stripe 255, SST 3

### Read Path with Stripes

Reading an item involves a single stripe:

```rust
pub fn get(&self, key: &Key) -> Result<Option<Item>> {
    let inner = self.inner.read();

    // 1. Determine stripe
    let stripe_id = key.stripe() as usize;
    let stripe = &inner.stripes[stripe_id];

    // 2. Check stripe's memtable
    if let Some(record) = stripe.memtable.get(&key.encode()) {
        return Ok(record.value.clone());
    }

    // 3. Check stripe's SSTs (newest to oldest)
    for sst in &stripe.ssts {
        if let Some(record) = sst.get(key) {
            return Ok(record.value.clone());
        }
    }

    Ok(None)
}
```

**Performance characteristics:**
- **Single stripe lookup**: No cross-stripe communication
- **Lock-free after stripe selection**: Read lock on LSM, no stripe-specific locks
- **Cache-friendly**: Related keys in same stripe hit same memtable/SSTs

### Write Path with Stripes

Writing an item appends to WAL and updates one stripe:

```rust
pub fn put(&self, key: Key, item: Item) -> Result<()> {
    let mut inner = self.inner.write();

    // 1. Assign sequence number (global)
    let seq = inner.next_seq;
    inner.next_seq += 1;

    // 2. Create record
    let record = Record::put(key.clone(), item, seq);

    // 3. Append to WAL (shared)
    inner.wal.append(record.clone())?;
    inner.wal.flush()?;

    // 4. Determine stripe
    let stripe_id = key.stripe() as usize;

    // 5. Insert into stripe's memtable
    let key_enc = key.encode().to_vec();
    inner.stripes[stripe_id].memtable.insert(key_enc, record);

    // 6. Check if stripe needs flush
    if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
        self.flush_stripe(&mut inner, stripe_id)?;
    }

    Ok(())
}
```

**Performance characteristics:**
- **Global sequence number**: Ensures total ordering across all stripes
- **Shared WAL**: All writes logged (crash recovery)
- **Independent flush**: Hot stripes flush independently of cold stripes

### Query Path with Stripes

Queries are **single-stripe operations**:

```rust
pub fn query(&self, params: QueryParams) -> Result<QueryResult> {
    let inner = self.inner.read();

    // Route to stripe based on partition key
    let stripe_id = {
        let temp_key = Key::new(params.pk.clone());
        temp_key.stripe() as usize
    };
    let stripe = &inner.stripes[stripe_id];

    // Query only this stripe's memtable and SSTs
    // ...
}
```

**Why single-stripe:**
- Same partition key → same stripe (by design)
- All items in a partition are in one stripe
- No need to scan other stripes

**Scan path** (different):
Scans are **multi-stripe operations**:

```rust
pub fn scan(&self, params: ScanParams) -> Result<ScanResult> {
    let inner = self.inner.read();

    // Scan ALL stripes (or subset for parallel scans)
    for stripe_id in 0..NUM_STRIPES {
        if !params.should_scan_stripe(stripe_id) {
            continue;
        }

        let stripe = &inner.stripes[stripe_id];
        // Collect from this stripe
        // ...
    }

    // Merge and sort globally
    // ...
}
```

## Best Practices for Key Design

### 1. Choose Good Partition Keys

**Good partition keys:**
- High cardinality (many unique values)
- Even distribution (no hot keys)
- Logical grouping (related items together)

```rust
// Good: User ID (high cardinality)
db.put(b"user#12345", user)?;

// Good: Tenant + Resource
db.put(b"tenant#acme:resource#789", resource)?;

// Bad: Boolean value (low cardinality, only 2 stripes used)
db.put(b"active", user)?;  // Don't do this!

// Bad: Timestamp (hot key problem)
db.put(format!("events#{}", today).as_bytes(), event)?;  // Avoid!
```

### 2. Design Sort Keys for Access Patterns

**Know your queries:**
```rust
// If you query by status then timestamp:
db.put_with_sk(
    b"tasks",
    format!("status:{}#timestamp:{}", status, timestamp).as_bytes(),
    task
)?;

// If you query by timestamp then status:
db.put_with_sk(
    b"tasks",
    format!("timestamp:{}#status:{}", timestamp, status).as_bytes(),
    task
)?;
```

### 3. Avoid Hot Keys

**Problem:** One partition key gets all the writes

```rust
// Bad: All events go to one stripe
db.put_with_sk(b"events", timestamp_sk, event)?;

// Good: Distribute across hours or shards
db.put_with_sk(
    format!("events#{}", hour).as_bytes(),
    timestamp_sk,
    event
)?;
```

### 4. Use Prefixes for Hierarchies

```rust
// Enables prefix-based filtering
db.put(b"logs#2024#01#15#10#00#00", log_entry)?;

// Can query by year, month, day, etc.
let query = Query::new(b"logs#2024#01").sk_begins_with(b"15");
```

### 5. Keep Keys Reasonably Short

**Why:**
- Keys are stored in every record in SST files
- Keys are stored in memtable
- Shorter keys = less memory, less disk space

```rust
// Prefer
db.put(b"u#123", user)?;

// Over
db.put(b"user_with_very_long_prefix#123", user)?;
```

**Balance:**
- Readability vs. size
- `user#123` is a good middle ground
- `u#123` saves bytes but loses clarity

## Summary

Keys are the foundation of KeystoneDB's data model and determine how data is distributed, stored, and queried. Understanding keys is essential for building efficient applications.

Key takeaways:

1. **Composite keys**: Partition key (required) + Sort key (optional)
2. **Stripe routing**: CRC32 hash of PK determines stripe (0-255)
3. **Same PK → Same stripe**: All items with same PK are in one stripe
4. **256 stripes**: Independent LSM trees for parallelism
5. **Key encoding**: Length-prefixed binary format for efficient comparison
6. **Design patterns**: Hierarchical keys, timestamps, composite sort keys
7. **Avoid hot keys**: Choose high-cardinality partition keys
8. **Query-driven design**: Sort keys should match access patterns

In the next chapter, we'll explore the LSM tree architecture that powers KeystoneDB's storage engine, including how stripes, memtables, and SST files work together to provide fast reads and writes.

# Chapter 16: Time To Live (TTL)

Time To Live (TTL) is a powerful feature that automatically expires items after a specified timestamp, enabling efficient data lifecycle management without manual cleanup jobs. KeystoneDB's TTL implementation provides lazy deletion, ensuring expired items are automatically filtered from query results while minimizing storage overhead.

## Understanding TTL

TTL (Time To Live) allows you to specify an expiration time for database items. Once the expiration time passes, the item is considered expired and is automatically filtered out from read operations.

### Why Use TTL?

**Common use cases:**
- **Session management**: Auto-expire user sessions after inactivity
- **Temporary data**: Cache entries, verification codes, temporary tokens
- **Data retention**: Comply with data retention policies
- **Cleanup automation**: Remove old logs, expired offers, stale data

**Benefits:**
- ✅ Automatic expiration - no manual cleanup needed
- ✅ Storage efficiency - expired data eventually reclaimed
- ✅ Application simplicity - no background jobs required
- ✅ Compliance - enforce data retention policies

### TTL Concepts

**Key concepts:**
1. **TTL Attribute**: A designated attribute name containing the expiration timestamp
2. **Expiration Time**: Unix timestamp (seconds or milliseconds since epoch)
3. **Lazy Deletion**: Items are filtered on read, not immediately deleted
4. **Reclamation**: Expired items removed during compaction

## Enabling TTL

TTL is configured at the table level when creating the database:

```rust
use kstone_api::{Database, TableSchema};

// Enable TTL on "expiresAt" attribute
let schema = TableSchema::new()
    .with_ttl("expiresAt");

let db = Database::create_with_schema("mydb.keystone", schema)?;
```

**What this does:**
- Designates `expiresAt` as the TTL attribute
- Items with this attribute will expire when the timestamp passes
- Items without this attribute never expire

## Setting Expiration Times

### Using Unix Timestamp (Seconds)

The most common approach uses seconds since the Unix epoch:

```rust
use std::time::SystemTime;

let now = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

// Expire in 1 hour (3600 seconds)
let expires_at = now + 3600;

let item = ItemBuilder::new()
    .string("session_id", "abc123")
    .string("user_id", "user#456")
    .number("expiresAt", expires_at)
    .build();

db.put(b"session#abc123", item)?;
```

**After 1 hour:**
```rust
let result = db.get(b"session#abc123")?;
assert!(result.is_none()); // Automatically filtered out (expired)
```

### Using Timestamp Value Type (Milliseconds)

KeystoneDB also supports the `Timestamp` value type for millisecond precision:

```rust
use kstone_core::Value;

let now_millis = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as i64;

// Expire in 30 minutes (1,800,000 milliseconds)
let expires_at = now_millis + 1_800_000;

let mut item = ItemBuilder::new()
    .string("code", "VERIFY123")
    .build();

item.insert("expiresAt".to_string(), Value::Ts(expires_at));

db.put(b"verification#VERIFY123", item)?;
```

**Conversion:** TTL internally converts `Timestamp` (milliseconds) to seconds for comparison.

## Lazy Deletion Mechanism

KeystoneDB uses **lazy deletion** for TTL - items are not immediately removed when they expire. Instead, they're filtered during read operations.

### How Lazy Deletion Works

```rust
// 1. Create item that expires in past
let now = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

let item = ItemBuilder::new()
    .string("data", "temporary")
    .number("expiresAt", now - 100) // Expired 100 seconds ago
    .build();

db.put(b"temp#1", item)?;

// 2. Item is stored on disk (not immediately deleted)
// File system still contains the item

// 3. Get operation filters expired items
let result = db.get(b"temp#1")?;
assert!(result.is_none()); // Returns None (lazy deletion)

// 4. Item physically deleted during compaction
// Eventually removed from disk by background compaction
```

**Phases of deletion:**
1. **Expiration**: Item's TTL passes - it's now "expired"
2. **Filtering**: Read operations (get/query/scan) skip expired items
3. **Reclamation**: Compaction physically removes expired items from disk

### Benefits of Lazy Deletion

**Advantages:**
- ✅ **No write amplification**: Expiration doesn't trigger immediate writes
- ✅ **Batch efficiency**: Multiple expired items removed in one compaction
- ✅ **Simple implementation**: No background deletion threads needed
- ✅ **Predictable performance**: No surprise deletion storms

**Trade-offs:**
- ⚠️ **Storage lag**: Expired items occupy disk until compaction
- ⚠️ **Index entries**: GSI/LSI entries for expired items remain until compaction

## TTL with Get Operations

Get operations automatically filter expired items:

```rust
let schema = TableSchema::new().with_ttl("expiresAt");
let db = Database::create_with_schema("sessions.keystone", schema)?;

let now = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

// Create session expiring in 1 hour
db.put(b"session#active",
    ItemBuilder::new()
        .string("user_id", "user#123")
        .number("expiresAt", now + 3600)
        .build())?;

// Create expired session
db.put(b"session#expired",
    ItemBuilder::new()
        .string("user_id", "user#456")
        .number("expiresAt", now - 100)
        .build())?;

// Get returns active session
let active = db.get(b"session#active")?;
assert!(active.is_some());

// Get returns None for expired session
let expired = db.get(b"session#expired")?;
assert!(expired.is_none());
```

**Behavior:**
- Active items (TTL in future or no TTL): Returned normally
- Expired items (TTL in past): Treated as if they don't exist

## TTL with Query Operations

Query operations automatically filter out expired items from results:

```rust
let schema = TableSchema::new()
    .with_ttl("expiresAt")
    .add_local_index(LocalSecondaryIndex::new("status-index", "status"));

let db = Database::create_with_schema("tasks.keystone", schema)?;

let now = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

// Create mix of active and expired tasks
for i in 1..=5 {
    let expires = if i <= 2 {
        now - 100  // Tasks 1-2 expired
    } else {
        now + 1000  // Tasks 3-5 active
    };

    db.put_with_sk(b"project#alpha", format!("task#{}", i).as_bytes(),
        ItemBuilder::new()
            .string("name", format!("Task {}", i))
            .string("status", "pending")
            .number("expiresAt", expires)
            .build())?;
}

// Query returns only non-expired tasks (3, 4, 5)
let query = Query::new(b"project#alpha");
let response = db.query(query)?;

assert_eq!(response.items.len(), 3); // Only active tasks
```

**Key points:**
- Expired items are filtered **before** being added to results
- Pagination works correctly (LastEvaluatedKey skips expired items)
- Counts reflect only non-expired items

## TTL with Scan Operations

Scan operations also filter expired items across all partitions:

```rust
let schema = TableSchema::new().with_ttl("expiresAt");
let db = Database::create_with_schema("cache.keystone", schema)?;

let now = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

// Create items with various expiration times
db.put(b"cache#1",
    ItemBuilder::new()
        .string("value", "active1")
        .number("expiresAt", now + 1000)
        .build())?;

db.put(b"cache#2",
    ItemBuilder::new()
        .string("value", "expired1")
        .number("expiresAt", now - 100)
        .build())?;

db.put(b"cache#3",
    ItemBuilder::new()
        .string("value", "active2")
        .number("expiresAt", now + 2000)
        .build())?;

// Scan returns only non-expired items
let scan = Scan::new();
let response = db.scan(scan)?;

assert_eq!(response.items.len(), 2); // cache#1 and cache#3
```

## Items Without TTL Attribute

Items without the TTL attribute **never expire**:

```rust
let schema = TableSchema::new().with_ttl("expiresAt");
let db = Database::create_with_schema("mixed.keystone", schema)?;

// Item with TTL - will expire
db.put(b"temp#1",
    ItemBuilder::new()
        .string("data", "temporary")
        .number("expiresAt", now + 3600)
        .build())?;

// Item without TTL - never expires
db.put(b"permanent#1",
    ItemBuilder::new()
        .string("data", "permanent")
        .build())?;

// permanent#1 will always be retrievable
let result = db.get(b"permanent#1")?;
assert!(result.is_some());
```

**Use case:** Mix permanent and temporary data in the same table:
- User profiles: No TTL (permanent)
- User sessions: TTL (expire after inactivity)
- Verification codes: TTL (expire after 15 minutes)

## Practical Use Cases

### Use Case 1: Session Management

Auto-expire user sessions after inactivity:

```rust
use std::time::{SystemTime, Duration};

fn create_session(db: &Database, session_id: &str, user_id: &str) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Session expires in 30 minutes
    let expires_at = now + 1800;

    let session = ItemBuilder::new()
        .string("user_id", user_id)
        .string("session_id", session_id)
        .number("created_at", now)
        .number("expiresAt", expires_at)
        .build();

    db.put(format!("session#{}", session_id).as_bytes(), session)?;
    Ok(())
}

fn get_active_session(db: &Database, session_id: &str) -> Result<Option<Item>> {
    db.get(format!("session#{}", session_id).as_bytes())
    // Automatically returns None if session expired
}

fn refresh_session(db: &Database, session_id: &str) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Extend expiration by 30 minutes
    let update = Update::new(format!("session#{}", session_id).as_bytes())
        .expression("SET expiresAt = :new_expiry")
        .value(":new_expiry", Value::number(now + 1800));

    db.update(update)?;
    Ok(())
}
```

**Benefits:**
- Automatic cleanup of inactive sessions
- No manual session expiration job needed
- Storage automatically reclaimed

### Use Case 2: Verification Codes

Short-lived verification codes for email/SMS verification:

```rust
fn generate_verification_code(db: &Database, email: &str) -> Result<String> {
    let code = generate_random_code(); // e.g., "123456"

    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Code expires in 15 minutes
    let expires_at = now + 900;

    let item = ItemBuilder::new()
        .string("email", email)
        .string("code", &code)
        .number("expiresAt", expires_at)
        .number("attempts", 0)
        .build();

    db.put(format!("verification#{}", code).as_bytes(), item)?;

    Ok(code)
}

fn verify_code(db: &Database, code: &str, email: &str) -> Result<bool> {
    // Get returns None if code expired
    let item = db.get(format!("verification#{}", code).as_bytes())?;

    match item {
        Some(data) => {
            let stored_email = data.get("email")
                .and_then(|v| v.as_string())
                .unwrap_or("");

            Ok(stored_email == email)
        }
        None => Ok(false), // Expired or doesn't exist
    }
}
```

**Benefits:**
- Codes automatically expire after 15 minutes
- No cleanup job needed for old codes
- Security: Old codes can't be reused

### Use Case 3: Cache with Expiration

Implement a cache with automatic expiration:

```rust
struct CacheEntry {
    key: String,
    value: String,
    ttl_seconds: i64,
}

fn cache_set(db: &Database, entry: CacheEntry) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let expires_at = now + entry.ttl_seconds;

    let item = ItemBuilder::new()
        .string("value", &entry.value)
        .number("cached_at", now)
        .number("expiresAt", expires_at)
        .build();

    db.put(format!("cache#{}", entry.key).as_bytes(), item)?;
    Ok(())
}

fn cache_get(db: &Database, key: &str) -> Result<Option<String>> {
    let item = db.get(format!("cache#{}", key).as_bytes())?;

    Ok(item.and_then(|i| {
        i.get("value")
            .and_then(|v| v.as_string())
            .map(|s| s.to_string())
    }))
}

// Usage
cache_set(&db, CacheEntry {
    key: "user:123:profile".to_string(),
    value: "{\"name\":\"Alice\"}".to_string(),
    ttl_seconds: 300, // 5 minutes
})?;

// Automatically returns None after 5 minutes
let cached = cache_get(&db, "user:123:profile")?;
```

### Use Case 4: Temporary Access Tokens

Short-lived API tokens:

```rust
fn create_access_token(db: &Database, user_id: &str) -> Result<String> {
    let token = generate_secure_token(); // e.g., UUID

    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Token expires in 1 hour
    let expires_at = now + 3600;

    let item = ItemBuilder::new()
        .string("user_id", user_id)
        .string("token", &token)
        .number("created_at", now)
        .number("expiresAt", expires_at)
        .build();

    db.put(format!("token#{}", token).as_bytes(), item)?;

    Ok(token)
}

fn validate_token(db: &Database, token: &str) -> Result<Option<String>> {
    let item = db.get(format!("token#{}", token).as_bytes())?;

    Ok(item.and_then(|i| {
        i.get("user_id")
            .and_then(|v| v.as_string())
            .map(|s| s.to_string())
    }))
}

// Usage
let token = create_access_token(&db, "user#alice")?;
println!("Token: {}", token);

// Use token within 1 hour
if let Some(user_id) = validate_token(&db, &token)? {
    println!("Token valid for user: {}", user_id);
} else {
    println!("Token expired or invalid");
}
```

### Use Case 5: Rate Limiting

Track API usage with automatic reset:

```rust
fn record_api_call(db: &Database, api_key: &str) -> Result<bool> {
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Rate limit resets every hour
    let hour_start = (now / 3600) * 3600;
    let hour_end = hour_start + 3600;

    let rate_limit_key = format!("ratelimit#{}#{}", api_key, hour_start);

    // Try to increment count
    let update = Update::new(rate_limit_key.as_bytes())
        .expression("ADD call_count :one SET expiresAt = :expiry")
        .condition("call_count < :max_calls OR attribute_not_exists(call_count)")
        .value(":one", Value::number(1))
        .value(":expiry", Value::number(hour_end))
        .value(":max_calls", Value::number(1000)); // 1000 calls/hour

    match db.update(update) {
        Ok(_) => Ok(true), // Call allowed
        Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
            Ok(false) // Rate limit exceeded
        }
        Err(e) => Err(e),
    }
}

// Usage
if record_api_call(&db, "api_key_123")? {
    println!("API call allowed");
    // Process request
} else {
    println!("Rate limit exceeded - try again later");
    // Return 429 Too Many Requests
}
```

**Benefits:**
- Rate limit counters automatically expire each hour
- No manual cleanup needed
- Storage efficiency (old counters auto-removed)

## TTL Attribute Types

KeystoneDB supports two value types for TTL:

### Number (Seconds)

```rust
let now_seconds = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

let item = ItemBuilder::new()
    .number("expiresAt", now_seconds + 3600)
    .build();
```

**Precision:** 1 second
**Use case:** Most TTL scenarios

### Timestamp (Milliseconds)

```rust
use kstone_core::Value;

let now_millis = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as i64;

let mut item = ItemBuilder::new().build();
item.insert("expiresAt".to_string(), Value::Ts(now_millis + 60_000));
```

**Precision:** 1 millisecond
**Use case:** High-precision expiration (e.g., rate limiting)

**Note:** Internally, `Timestamp` values are converted to seconds for TTL comparison.

## TTL Disable/Enable Pattern

You can change TTL configuration by recreating the schema:

```rust
// Initially: No TTL
let schema = TableSchema::new();
let db = Database::create_with_schema("mydb.keystone", schema)?;

// Later: Enable TTL (requires schema update - not supported in Phase 3.3)
// Workaround: Create new database with TTL, migrate data
let schema_with_ttl = TableSchema::new().with_ttl("expiresAt");
let db_new = Database::create_with_schema("mydb_v2.keystone", schema_with_ttl)?;

// Migrate data
let scan = Scan::new();
let response = db.scan(scan)?;
for item in response.items {
    let key = extract_key(&item);
    db_new.put(key, item)?;
}
```

**Note:** Dynamic schema updates are not supported in Phase 3.3. You must create a new database with the desired schema.

## Monitoring Expired Items

Check if items are expired programmatically:

```rust
use kstone_core::index::TableSchema;

let schema = TableSchema::new().with_ttl("expiresAt");

let item = db.get(b"session#123")?;

match item {
    Some(data) => {
        if schema.is_expired(&data) {
            println!("Item exists but is expired");
        } else {
            println!("Item is active");
        }
    }
    None => {
        println!("Item does not exist or already deleted");
    }
}
```

**Note:** In normal operation, `get()` already filters expired items, so you won't see expired items. This is useful for debugging or custom expiration logic.

## Best Practices

### 1. Use Appropriate TTL Durations

```rust
// Good: Reasonable TTL for use case
let session_ttl = 1800;      // 30 minutes for sessions
let code_ttl = 900;          // 15 minutes for verification codes
let cache_ttl = 300;         // 5 minutes for cache entries

// Avoid: Extremely short or long TTLs
let too_short = 5;           // 5 seconds (too short, overhead)
let too_long = 31536000;     // 1 year (use permanent storage instead)
```

### 2. Set TTL on All Relevant Items

```rust
// Good: All temporary items have TTL
db.put(b"session#abc",
    ItemBuilder::new()
        .string("user_id", "user#123")
        .number("expiresAt", now + 1800)
        .build())?;

// Avoid: Forgetting TTL (item never expires)
db.put(b"session#xyz",
    ItemBuilder::new()
        .string("user_id", "user#456")
        // Missing expiresAt!
        .build())?;
```

### 3. Handle Missing Items Gracefully

```rust
// Good: Handle None case
match db.get(b"session#abc")? {
    Some(session) => {
        // Use session
        println!("Active session");
    }
    None => {
        // Session expired or doesn't exist
        println!("Session expired - please log in again");
    }
}
```

### 4. Use UTC Timestamps

```rust
// Good: Always use UTC
let now = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

// Avoid: Local time zones (can cause confusion)
// Don't use local time conversions for TTL
```

### 5. Consider Compaction Frequency

```rust
// Expired items occupy disk until compaction
// For high TTL churn, configure aggressive compaction:

let config = CompactionConfig::new()
    .with_enabled(true)
    .with_sst_threshold(5)  // Compact more frequently
    .with_check_interval(Duration::from_secs(60));

lsm.set_compaction_config(config);
```

## Summary

TTL in KeystoneDB provides:

✅ **Automatic expiration**: Items expire based on timestamp
✅ **Lazy deletion**: Expired items filtered on read, removed during compaction
✅ **Flexible configuration**: Per-table TTL attribute
✅ **Mixed data**: Items with and without TTL in same table
✅ **Two value types**: Number (seconds) and Timestamp (milliseconds)

**Key concepts:**
- **TTL attribute**: Designated attribute name (e.g., `expiresAt`)
- **Expiration time**: Unix timestamp when item expires
- **Lazy deletion**: Filtered on read, removed during compaction
- **No TTL**: Items without TTL attribute never expire

**Common use cases:**
- Session management (30-60 minutes)
- Verification codes (15 minutes)
- Cache entries (5-15 minutes)
- Access tokens (1-24 hours)
- Rate limiting (1 hour windows)

**Best practices:**
- Use appropriate TTL durations for your use case
- Set TTL on all temporary items
- Handle expired items gracefully (None result)
- Always use UTC timestamps
- Consider compaction frequency for high TTL churn

Master TTL to implement efficient data lifecycle management in KeystoneDB!

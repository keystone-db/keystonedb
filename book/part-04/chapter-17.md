# Chapter 17: Streams & Change Data Capture

Streams provide real-time access to all item-level modifications in your KeystoneDB database, enabling powerful Change Data Capture (CDC) use cases. From audit logging to cache invalidation to data replication, streams allow you to react to every INSERT, MODIFY, and REMOVE operation.

## What Are Streams?

Streams capture a time-ordered sequence of all changes made to items in your database. Every put, update, and delete operation generates a stream record containing:

- **Sequence number**: Globally unique, monotonically increasing identifier
- **Event type**: INSERT, MODIFY, or REMOVE
- **Item key**: The affected item's partition and sort keys
- **Old image**: Item state before modification (optional)
- **New image**: Item state after modification (optional)
- **Timestamp**: When the change occurred

### Stream Characteristics

**Key properties:**
- ✅ **Real-time**: Records available immediately after write
- ✅ **Ordered**: Globally ordered by sequence number
- ✅ **Complete**: Captures all modifications
- ✅ **Configurable**: Choose what data is included
- ✅ **Buffered**: Recent records kept in memory

## Enabling Streams

Streams are configured when creating the database:

```rust
use kstone_api::{Database, TableSchema};
use kstone_core::stream::{StreamConfig, StreamViewType};

// Enable streams with default settings
let schema = TableSchema::new()
    .with_stream(StreamConfig::enabled());

let db = Database::create_with_schema("mydb.keystone", schema)?;
```

**Default configuration:**
- View type: `NewAndOldImages` (both before and after state)
- Buffer size: 1000 records (most recent changes)
- Enabled: true

## Stream View Types

The view type controls what data is included in stream records:

### 1. KEYS_ONLY

Only the item's keys are captured (no attribute data):

```rust
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_view_type(StreamViewType::KeysOnly)
    );

let db = Database::create_with_schema("keys_only.keystone", schema)?;

db.put(b"user#123",
    ItemBuilder::new()
        .string("name", "Alice")
        .number("age", 30)
        .build())?;

let records = db.read_stream(None)?;
// Stream record contains:
// - key: user#123
// - old_image: None
// - new_image: None
```

**Use case:** Know which items changed without needing full data
- Cache invalidation (invalidate by key)
- Change notification (notify subscribers an item changed)
- Audit trail (track what was modified, not the content)

### 2. NEW_IMAGE

Only the new item state after modification:

```rust
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_view_type(StreamViewType::NewImage)
    );

let db = Database::create_with_schema("new_image.keystone", schema)?;

db.put(b"user#123",
    ItemBuilder::new()
        .string("name", "Alice")
        .number("age", 30)
        .build())?;

let records = db.read_stream(None)?;
// Stream record contains:
// - key: user#123
// - old_image: None
// - new_image: Some({"name": "Alice", "age": 30})
```

**Use case:** Replicate current state
- Database replication
- Search index updates
- Materialized view updates

### 3. OLD_IMAGE

Only the old item state before modification:

```rust
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_view_type(StreamViewType::OldImage)
    );

let db = Database::create_with_schema("old_image.keystone", schema)?;

// Initial insert
db.put(b"user#123",
    ItemBuilder::new()
        .string("name", "Alice")
        .number("age", 30)
        .build())?;

// Update
db.update(Update::new(b"user#123")
    .expression("SET age = :new_age")
    .value(":new_age", Value::number(31)))?;

let records = db.read_stream(None)?;
// Second record (MODIFY) contains:
// - key: user#123
// - old_image: Some({"name": "Alice", "age": 30})
// - new_image: None
```

**Use case:** Track what was changed
- Audit logging (what was the previous value?)
- Compliance (record original state)
- Change analysis (detect anomalies)

### 4. NEW_AND_OLD_IMAGES (Default)

Both old and new item states:

```rust
let schema = TableSchema::new()
    .with_stream(StreamConfig::enabled()); // Default view type

let db = Database::create_with_schema("full.keystone", schema)?;

// Initial insert
db.put(b"user#123",
    ItemBuilder::new()
        .string("name", "Alice")
        .number("age", 30)
        .build())?;

// Update
db.update(Update::new(b"user#123")
    .expression("SET age = :new_age")
    .value(":new_age", Value::number(31)))?;

let records = db.read_stream(None)?;
// Second record (MODIFY) contains:
// - key: user#123
// - old_image: Some({"name": "Alice", "age": 30})
// - new_image: Some({"name": "Alice", "age": 31})
```

**Use case:** Complete change tracking
- Full audit trail
- Change detection (compute diff)
- Event sourcing

## Stream Event Types

Streams capture three types of events:

### INSERT Event

Generated when a new item is created:

```rust
let schema = TableSchema::new()
    .with_stream(StreamConfig::enabled());
let db = Database::create_with_schema("mydb.keystone", schema)?;

db.put(b"user#123",
    ItemBuilder::new()
        .string("name", "Alice")
        .build())?;

let records = db.read_stream(None)?;

assert_eq!(records.len(), 1);
assert_eq!(records[0].event_type, StreamEventType::Insert);
assert!(records[0].old_image.is_none()); // No previous state
assert!(records[0].new_image.is_some()); // New item data
```

**Characteristics:**
- `old_image`: Always `None` (item didn't exist)
- `new_image`: Contains new item data (if view type includes it)

### MODIFY Event

Generated when an existing item is updated:

```rust
// Create item
db.put(b"user#123",
    ItemBuilder::new()
        .string("name", "Alice")
        .number("age", 30)
        .build())?;

// Update item
db.update(Update::new(b"user#123")
    .expression("SET age = :new_age")
    .value(":new_age", Value::number(31)))?;

let records = db.read_stream(None)?;

// Second record is MODIFY
assert_eq!(records[1].event_type, StreamEventType::Modify);
assert!(records[1].old_image.is_some()); // Previous state
assert!(records[1].new_image.is_some()); // New state
```

**Characteristics:**
- `old_image`: Previous item state (if view type includes it)
- `new_image`: Updated item state (if view type includes it)

**Note:** A put operation on an existing item generates a MODIFY event, not INSERT:

```rust
// First put: INSERT
db.put(b"user#123", item1)?;

// Second put (same key): MODIFY
db.put(b"user#123", item2)?;

let records = db.read_stream(None)?;
assert_eq!(records[0].event_type, StreamEventType::Insert);
assert_eq!(records[1].event_type, StreamEventType::Modify);
```

### REMOVE Event

Generated when an item is deleted:

```rust
// Create and delete item
db.put(b"user#123",
    ItemBuilder::new()
        .string("name", "Alice")
        .build())?;

db.delete(b"user#123")?;

let records = db.read_stream(None)?;

// Second record is REMOVE
assert_eq!(records[1].event_type, StreamEventType::Remove);
assert!(records[1].old_image.is_some()); // What was deleted
assert!(records[1].new_image.is_none()); // No new state
```

**Characteristics:**
- `old_image`: Final item state before deletion (if view type includes it)
- `new_image`: Always `None` (item no longer exists)

## Reading Stream Records

### Read All Records

Get all buffered stream records:

```rust
let records = db.read_stream(None)?;

for record in records {
    println!("Sequence: {}", record.sequence_number);
    println!("Event: {:?}", record.event_type);
    println!("Key: {:?}", record.key);
    println!("Timestamp: {}", record.timestamp);
}
```

### Read Records After Sequence Number

Poll for new changes since last read:

```rust
// Initial read
let records = db.read_stream(None)?;
let last_sequence = records.last().map(|r| r.sequence_number);

// ... time passes, more changes occur ...

// Read only new records
let new_records = db.read_stream(last_sequence)?;

for record in new_records {
    println!("New change: {:?}", record.event_type);
}
```

**Polling pattern:**
```rust
let mut last_sequence = None;

loop {
    let records = db.read_stream(last_sequence)?;

    if records.is_empty() {
        // No new changes
        thread::sleep(Duration::from_secs(1));
        continue;
    }

    for record in &records {
        process_change(record)?;
    }

    last_sequence = records.last().map(|r| r.sequence_number);
}
```

## Stream Buffer

Streams use an in-memory circular buffer to store recent records:

```rust
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_buffer_size(500) // Keep last 500 records
    );

let db = Database::create_with_schema("mydb.keystone", schema)?;
```

**Buffer behavior:**
- **Circular**: Old records dropped when buffer is full
- **In-memory**: Not persisted to disk
- **FIFO**: First in, first out
- **Default size**: 1000 records

**Important:** If you make more changes than the buffer size without reading, old records are lost.

```rust
// Buffer size = 1000
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_buffer_size(1000)
    );

// Make 1500 changes
for i in 0..1500 {
    db.put(format!("item#{}", i).as_bytes(), item.clone())?;
}

// Only last 1000 records available
let records = db.read_stream(None)?;
assert_eq!(records.len(), 1000);
```

**Best practice:** Poll frequently to avoid losing records.

## Practical Use Cases

### Use Case 1: Audit Logging

Track all database modifications for compliance:

```rust
use std::fs::OpenOptions;
use std::io::Write;

fn start_audit_logger(db: Database) -> Result<()> {
    let mut last_sequence = None;
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("audit.log")?;

    loop {
        let records = db.read_stream(last_sequence)?;

        for record in &records {
            let log_entry = format!(
                "{} | {:?} | {:?} | Old: {:?} | New: {:?}\n",
                record.timestamp,
                record.event_type,
                record.key,
                record.old_image,
                record.new_image
            );

            log_file.write_all(log_entry.as_bytes())?;
        }

        if !records.is_empty() {
            last_sequence = records.last().map(|r| r.sequence_number);
        }

        thread::sleep(Duration::from_secs(1));
    }
}
```

**Output:**
```
1704067200000 | Insert | user#123 | None | Some({"name": "Alice", "age": 30})
1704067201000 | Modify | user#123 | Some({"name": "Alice", "age": 30}) | Some({"name": "Alice", "age": 31})
1704067202000 | Remove | user#123 | Some({"name": "Alice", "age": 31}) | None
```

### Use Case 2: Cache Invalidation

Invalidate cache entries when database changes:

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

struct Cache {
    data: Arc<Mutex<HashMap<String, Item>>>,
}

impl Cache {
    fn invalidate(&self, key: &Key) {
        let key_str = format!("{:?}", key);
        self.data.lock().unwrap().remove(&key_str);
        println!("Invalidated cache for: {}", key_str);
    }
}

fn start_cache_invalidator(db: Database, cache: Cache) -> Result<()> {
    let mut last_sequence = None;

    loop {
        let records = db.read_stream(last_sequence)?;

        for record in &records {
            // Invalidate cache on any change
            cache.invalidate(&record.key);
        }

        if !records.is_empty() {
            last_sequence = records.last().map(|r| r.sequence_number);
        }

        thread::sleep(Duration::from_millis(100));
    }
}
```

**Benefits:**
- Cache always reflects latest database state
- No stale data served from cache
- Automatic invalidation (no manual cache management)

### Use Case 3: Search Index Synchronization

Keep a search index (like Elasticsearch) in sync:

```rust
fn start_search_indexer(db: Database, elasticsearch_client: ElasticsearchClient) -> Result<()> {
    let mut last_sequence = None;

    loop {
        let records = db.read_stream(last_sequence)?;

        for record in &records {
            match record.event_type {
                StreamEventType::Insert | StreamEventType::Modify => {
                    if let Some(new_item) = &record.new_image {
                        // Index new/updated item
                        elasticsearch_client.index(
                            extract_id(&record.key),
                            new_item
                        )?;
                    }
                }
                StreamEventType::Remove => {
                    // Remove from index
                    elasticsearch_client.delete(
                        extract_id(&record.key)
                    )?;
                }
            }
        }

        if !records.is_empty() {
            last_sequence = records.last().map(|r| r.sequence_number);
        }

        thread::sleep(Duration::from_millis(100));
    }
}
```

### Use Case 4: Database Replication

Replicate changes to another KeystoneDB instance:

```rust
fn start_replicator(
    source_db: Database,
    target_db: Database,
) -> Result<()> {
    let mut last_sequence = None;

    loop {
        let records = source_db.read_stream(last_sequence)?;

        for record in &records {
            match record.event_type {
                StreamEventType::Insert | StreamEventType::Modify => {
                    if let Some(new_item) = &record.new_image {
                        target_db.put(
                            record.key.pk(),
                            new_item.clone()
                        )?;
                    }
                }
                StreamEventType::Remove => {
                    target_db.delete(record.key.pk())?;
                }
            }
        }

        if !records.is_empty() {
            last_sequence = records.last().map(|r| r.sequence_number);
        }

        thread::sleep(Duration::from_millis(500));
    }
}
```

### Use Case 5: Change Notification

Send notifications when specific items change:

```rust
fn start_notification_service(db: Database) -> Result<()> {
    let mut last_sequence = None;

    loop {
        let records = db.read_stream(last_sequence)?;

        for record in &records {
            // Only notify on user changes
            if is_user_key(&record.key) {
                match record.event_type {
                    StreamEventType::Insert => {
                        send_notification("User created", &record.key)?;
                    }
                    StreamEventType::Modify => {
                        if password_changed(&record.old_image, &record.new_image) {
                            send_notification("Password changed", &record.key)?;
                        }
                    }
                    StreamEventType::Remove => {
                        send_notification("User deleted", &record.key)?;
                    }
                }
            }
        }

        if !records.is_empty() {
            last_sequence = records.last().map(|r| r.sequence_number);
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn password_changed(old: &Option<Item>, new: &Option<Item>) -> bool {
    if let (Some(old_item), Some(new_item)) = (old, new) {
        old_item.get("password_hash") != new_item.get("password_hash")
    } else {
        false
    }
}
```

### Use Case 6: Materialized View Updates

Maintain aggregated views:

```rust
struct OrderStats {
    total_orders: i64,
    total_revenue: f64,
}

fn update_order_stats(db: &Database, record: &StreamRecord) -> Result<()> {
    if !is_order_key(&record.key) {
        return Ok(());
    }

    match record.event_type {
        StreamEventType::Insert => {
            if let Some(order) = &record.new_image {
                let amount = extract_amount(order);

                db.update(Update::new(b"stats#orders")
                    .expression("ADD total_orders :one, total_revenue :amount")
                    .value(":one", Value::number(1))
                    .value(":amount", Value::number(amount)))?;
            }
        }
        StreamEventType::Remove => {
            if let Some(order) = &record.old_image {
                let amount = extract_amount(order);

                db.update(Update::new(b"stats#orders")
                    .expression("ADD total_orders :minus_one, total_revenue :minus_amount")
                    .value(":minus_one", Value::number(-1))
                    .value(":minus_amount", Value::number(-amount)))?;
            }
        }
        _ => {}
    }

    Ok(())
}
```

## Stream Processing Patterns

### Pattern 1: Simple Polling

Basic loop with sleep:

```rust
let mut last_sequence = None;

loop {
    let records = db.read_stream(last_sequence)?;

    for record in &records {
        process(record)?;
    }

    if !records.is_empty() {
        last_sequence = records.last().map(|r| r.sequence_number);
    }

    thread::sleep(Duration::from_secs(1));
}
```

### Pattern 2: Batch Processing

Process records in batches:

```rust
let mut last_sequence = None;
let batch_size = 100;

loop {
    let records = db.read_stream(last_sequence)?;

    if records.is_empty() {
        thread::sleep(Duration::from_millis(100));
        continue;
    }

    for batch in records.chunks(batch_size) {
        process_batch(batch)?;
    }

    last_sequence = records.last().map(|r| r.sequence_number);
}
```

### Pattern 3: Filtered Processing

Process only specific event types:

```rust
let mut last_sequence = None;

loop {
    let records = db.read_stream(last_sequence)?;

    for record in &records {
        // Only process MODIFY events
        if record.event_type == StreamEventType::Modify {
            process_modification(record)?;
        }
    }

    if !records.is_empty() {
        last_sequence = records.last().map(|r| r.sequence_number);
    }

    thread::sleep(Duration::from_millis(500));
}
```

### Pattern 4: Checkpointing

Persist last processed sequence for resumption:

```rust
fn load_checkpoint() -> Option<u64> {
    std::fs::read_to_string("checkpoint.txt")
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn save_checkpoint(seq: u64) -> Result<()> {
    std::fs::write("checkpoint.txt", seq.to_string())?;
    Ok(())
}

fn start_processor(db: Database) -> Result<()> {
    let mut last_sequence = load_checkpoint();

    loop {
        let records = db.read_stream(last_sequence)?;

        for record in &records {
            process(record)?;
        }

        if let Some(seq) = records.last().map(|r| r.sequence_number) {
            last_sequence = Some(seq);
            save_checkpoint(seq)?;
        }

        thread::sleep(Duration::from_secs(1));
    }
}
```

## Monitoring Stream Health

### Check Buffer Utilization

```rust
fn monitor_stream_buffer(db: &Database) -> Result<()> {
    let records = db.read_stream(None)?;
    let buffer_size = 1000; // From schema configuration

    let utilization = (records.len() as f64 / buffer_size as f64) * 100.0;

    if utilization > 90.0 {
        eprintln!("WARNING: Stream buffer {}% full - increase polling frequency", utilization);
    }

    Ok(())
}
```

### Detect Processing Lag

```rust
fn detect_lag(db: &Database, last_processed: u64) -> Result<()> {
    let records = db.read_stream(None)?;

    if let Some(latest) = records.last() {
        let lag = latest.sequence_number - last_processed;

        if lag > 1000 {
            eprintln!("WARNING: Processing lag of {} records", lag);
        }
    }

    Ok(())
}
```

## Best Practices

### 1. Choose Appropriate View Type

```rust
// For cache invalidation: KEYS_ONLY
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_view_type(StreamViewType::KeysOnly)
    );

// For replication: NEW_IMAGE
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_view_type(StreamViewType::NewImage)
    );

// For audit logging: NEW_AND_OLD_IMAGES
let schema = TableSchema::new()
    .with_stream(StreamConfig::enabled()); // Default is NEW_AND_OLD_IMAGES
```

### 2. Set Adequate Buffer Size

```rust
// High-volume database: Large buffer
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_buffer_size(10000)
    );

// Low-volume database: Small buffer
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_buffer_size(100)
    );
```

### 3. Poll Frequently

```rust
// Good: Frequent polling (100ms)
loop {
    let records = db.read_stream(last_sequence)?;
    // Process records
    thread::sleep(Duration::from_millis(100));
}

// Risky: Infrequent polling (60s)
// May lose records if buffer overflows
loop {
    let records = db.read_stream(last_sequence)?;
    // Process records
    thread::sleep(Duration::from_secs(60));
}
```

### 4. Handle Processing Failures

```rust
for record in &records {
    match process(record) {
        Ok(_) => {
            // Success - update checkpoint
            last_sequence = Some(record.sequence_number);
        }
        Err(e) => {
            eprintln!("Error processing record {}: {}", record.sequence_number, e);
            // Don't update checkpoint - retry on next poll
            break;
        }
    }
}
```

### 5. Use Checkpointing

```rust
// Save checkpoint after each batch
for batch in records.chunks(100) {
    process_batch(batch)?;
    let last_seq = batch.last().unwrap().sequence_number;
    save_checkpoint(last_seq)?;
}
```

## Limitations

### Current Limitations (Phase 3.4)

1. **In-memory buffer**: Stream records not persisted to disk
2. **Circular buffer**: Old records dropped when buffer is full
3. **No filtering**: All changes captured (can't filter by event type at source)
4. **Single consumer**: No built-in multi-consumer support

### Future Enhancements

Future versions may support:
- **Persistent streams**: Records stored on disk for longer retention
- **Multiple consumers**: Named stream consumers with independent checkpoints
- **Filtering**: Server-side filtering by event type or key pattern
- **Sharding**: Distribute stream processing across multiple consumers

## Summary

Streams in KeystoneDB provide:

✅ **Change Data Capture**: Real-time access to all database modifications
✅ **Three event types**: INSERT, MODIFY, REMOVE
✅ **Four view types**: KEYS_ONLY, NEW_IMAGE, OLD_IMAGE, NEW_AND_OLD_IMAGES
✅ **Sequential ordering**: Globally ordered by sequence number
✅ **Polling API**: Read all or read since last sequence number

**Event types:**
- **INSERT**: New item created (no old_image)
- **MODIFY**: Existing item updated (has old and new image)
- **REMOVE**: Item deleted (no new_image)

**View types:**
- **KEYS_ONLY**: Minimal data (keys only)
- **NEW_IMAGE**: Current state only
- **OLD_IMAGE**: Previous state only
- **NEW_AND_OLD_IMAGES**: Both states (default)

**Common use cases:**
- Audit logging (compliance)
- Cache invalidation (consistency)
- Search indexing (Elasticsearch sync)
- Database replication (disaster recovery)
- Change notifications (real-time alerts)
- Materialized views (aggregations)

**Best practices:**
- Choose appropriate view type for your use case
- Set adequate buffer size for your change volume
- Poll frequently to avoid losing records
- Implement checkpointing for resumption
- Handle processing failures gracefully

Master streams to build powerful event-driven applications with KeystoneDB!

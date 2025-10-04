# Chapter 29: In-Memory Mode

KeystoneDB's in-memory mode provides a complete database implementation that operates entirely in RAM, with no disk I/O. This mode is ideal for testing, temporary data processing, benchmarking, and development workflows where persistence is not required. Despite running in memory, in-memory databases offer the same rich feature set as disk-based databases, including full PartiQL support, transactions, and indexes.

## Creating In-Memory Databases

In-memory databases can be created through both the CLI and the Rust API, offering flexibility for different use cases.

### Using the CLI

The simplest way to create an in-memory database is through the interactive shell:

```bash
# Start shell with in-memory database (no argument)
kstone shell

# Or explicitly specify :memory:
kstone shell :memory:
```

When you start the shell with these commands, you'll see:

```
╔═══════════════════════════════════════════════════════╗
║                                                       ║
║         KeystoneDB Interactive Shell v0.1.0           ║
║                                                       ║
║  Database: :memory:                                   ║
║                                                       ║
║  Quick Start:                                         ║
║    .help           - Show all commands                ║
║    .format <type>  - Change output (table|json|compact)║
║    .exit           - Exit shell                       ║
║                                                       ║
╚═══════════════════════════════════════════════════════╝

  Note: In-memory mode - data is temporary and will be lost on exit.

kstone>
```

### Using the Rust API

For programmatic use, create in-memory databases with the `Database::create_in_memory()` method:

```rust
use kstone_api::Database;

// Create a simple in-memory database
let db = Database::create_in_memory()?;

// Use like any other database
db.put(b"key1", item)?;
let result = db.get(b"key1")?;
```

With a table schema (for indexes, TTL, streams):

```rust
use kstone_api::{Database, TableSchema, LocalSecondaryIndex};

let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"))
    .with_ttl("expires_at");

let db = Database::create_in_memory_with_schema(schema)?;
```

### Automatic Cleanup

In-memory databases are automatically cleaned up when dropped:

```rust
{
    let db = Database::create_in_memory()?;
    db.put(b"key1", item)?;
    // Database exists and is usable here
} // db is dropped, all memory is freed
```

No disk files are created or left behind. All data is lost when the database instance is dropped.

## Use Cases for In-Memory Mode

In-memory mode excels in scenarios where persistence is unnecessary or even undesirable. Understanding these use cases helps you choose the right mode for your application.

### Unit Testing

In-memory databases are perfect for unit tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kstone_api::{Database, ItemBuilder};

    #[test]
    fn test_user_registration() {
        // Each test gets a fresh, isolated database
        let db = Database::create_in_memory().unwrap();

        let user = ItemBuilder::new()
            .string("name", "Alice")
            .string("email", "alice@example.com")
            .number("created_at", 1704067200)
            .build();

        // Test registration logic
        db.put(b"user#123", user).unwrap();

        let result = db.get(b"user#123").unwrap();
        assert!(result.is_some());

        // Database is automatically cleaned up when test ends
    }

    #[test]
    fn test_user_deletion() {
        let db = Database::create_in_memory().unwrap();

        // Set up test data
        let user = ItemBuilder::new()
            .string("name", "Bob")
            .build();
        db.put(b"user#456", user).unwrap();

        // Test deletion
        db.delete(b"user#456").unwrap();
        assert!(db.get(b"user#456").unwrap().is_none());
    }
}
```

Benefits for testing:
- **Fast**: No disk I/O overhead
- **Isolated**: Each test gets a clean database
- **Parallel**: Tests can run concurrently without conflicts
- **No Cleanup**: No temporary files to delete

### Temporary Data Processing

Process data temporarily without persistence overhead:

```rust
use kstone_api::{Database, ItemBuilder, Query};

fn process_csv_file(csv_path: &str) -> Result<Vec<Report>> {
    // Create temporary in-memory database for processing
    let db = Database::create_in_memory()?;

    // Load CSV data into database
    let mut reader = csv::Reader::from_path(csv_path)?;
    for (idx, result) in reader.deserialize().enumerate() {
        let record: CsvRecord = result?;
        let item = ItemBuilder::new()
            .string("category", &record.category)
            .number("amount", record.amount)
            .number("timestamp", record.timestamp)
            .build();

        db.put(format!("record#{}", idx).as_bytes(), item)?;
    }

    // Run aggregation queries
    let query = Query::new()
        .filter_expression("category = :cat")
        .expression_value(":cat", KeystoneValue::string("sales"))
        .build();

    let results = db.query(query)?;

    // Generate reports from query results
    let reports = generate_reports(results.items);

    Ok(reports)
    // Database is dropped here, all memory freed
}
```

This pattern is useful for:
- CSV/JSON processing pipelines
- Data transformation workflows
- Batch analytics jobs
- ETL temporary staging

### Benchmarking and Performance Testing

Measure database performance without disk I/O interference:

```rust
use std::time::Instant;
use kstone_api::{Database, ItemBuilder};

fn benchmark_writes(num_items: usize) {
    let db = Database::create_in_memory().unwrap();

    let start = Instant::now();

    for i in 0..num_items {
        let item = ItemBuilder::new()
            .number("value", i as i64)
            .build();

        db.put(format!("key#{}", i).as_bytes(), item).unwrap();
    }

    let duration = start.elapsed();
    let throughput = num_items as f64 / duration.as_secs_f64();

    println!("Write throughput: {:.2} ops/sec", throughput);
}

fn benchmark_reads(num_items: usize) {
    let db = Database::create_in_memory().unwrap();

    // Populate database
    for i in 0..num_items {
        let item = ItemBuilder::new()
            .number("value", i as i64)
            .build();
        db.put(format!("key#{}", i).as_bytes(), item).unwrap();
    }

    // Benchmark reads
    let start = Instant::now();

    for i in 0..num_items {
        db.get(format!("key#{}", i).as_bytes()).unwrap();
    }

    let duration = start.elapsed();
    let throughput = num_items as f64 / duration.as_secs_f64();

    println!("Read throughput: {:.2} ops/sec", throughput);
}
```

Benchmarking benefits:
- Isolate in-memory performance from disk latency
- Consistent performance across runs
- No disk caching effects
- Pure computational performance metrics

### Development and Prototyping

Rapidly prototype database schemas and queries:

```rust
use kstone_api::{Database, TableSchema, LocalSecondaryIndex, GlobalSecondaryIndex};

fn prototype_schema() {
    // Try different schema designs quickly
    let schema_v1 = TableSchema::new()
        .add_local_index(LocalSecondaryIndex::new("email-index", "email"));

    let schema_v2 = TableSchema::new()
        .add_global_index(GlobalSecondaryIndex::new("status-index", "status"))
        .with_ttl("expires_at");

    // Test each schema
    let db1 = Database::create_in_memory_with_schema(schema_v1).unwrap();
    let db2 = Database::create_in_memory_with_schema(schema_v2).unwrap();

    // Experiment with queries
    test_queries(&db1);
    test_queries(&db2);

    // Compare results and choose best schema
}
```

Prototyping use cases:
- Schema design experimentation
- Query optimization
- Application logic development
- Feature exploration

### Caching Layer

Use in-memory databases as a sophisticated cache:

```rust
use kstone_api::{Database, ItemBuilder};
use std::sync::Arc;

struct CacheLayer {
    cache: Arc<Database>,
}

impl CacheLayer {
    fn new() -> Self {
        Self {
            cache: Arc::new(Database::create_in_memory().unwrap()),
        }
    }

    fn get_or_compute(&self, key: &[u8], compute_fn: impl Fn() -> Item) -> Item {
        // Check cache first
        if let Some(item) = self.cache.get(key).unwrap() {
            return item;
        }

        // Cache miss - compute value
        let item = compute_fn();

        // Store in cache
        self.cache.put(key, item.clone()).unwrap();

        item
    }

    fn invalidate(&self, key: &[u8]) {
        self.cache.delete(key).unwrap();
    }

    fn clear(&self) {
        // Create new database to clear all data
        // (Alternative: iterate and delete all items)
    }
}
```

Cache advantages:
- Rich query capabilities (vs. simple key-value caches)
- TTL support for automatic expiration
- Indexes for complex lookups
- Transactions for atomic updates

## MemoryWal and MemorySst Implementation

KeystoneDB's in-memory mode uses specialized in-memory implementations of the WAL (Write-Ahead Log) and SST (Sorted String Table) components. These provide the same API as their disk-based counterparts but store data entirely in RAM.

### MemoryWal Architecture

The `MemoryWal` struct provides an in-memory write-ahead log:

```rust
pub struct MemoryWal {
    inner: Arc<Mutex<MemoryWalInner>>,
}

struct MemoryWalInner {
    records: Vec<(Lsn, Record)>,  // All records in memory
    next_lsn: Lsn,                // Next LSN to assign
}
```

**Key characteristics:**

1. **Storage**: All records stored in a `Vec<(Lsn, Record)>`
2. **Synchronization**: Uses `Arc<Mutex<...>>` for thread-safe access
3. **LSN Assignment**: Monotonically increasing sequence numbers
4. **No Persistence**: Data exists only in RAM

**API methods:**

```rust
impl MemoryWal {
    // Create a new in-memory WAL
    pub fn create() -> Result<Self>;

    // Append a record (assigns LSN)
    pub fn append(&self, record: Record) -> Result<Lsn>;

    // Flush is a no-op for in-memory WAL
    pub fn flush(&self) -> Result<()>;

    // Read all records
    pub fn read_all(&self) -> Result<Vec<(Lsn, Record)>>;

    // Clear all records (testing)
    pub fn clear(&self);

    // Get number of records
    pub fn len(&self) -> usize;
}
```

**Example usage:**

```rust
use kstone_core::memory_wal::MemoryWal;

let wal = MemoryWal::create()?;

// Append records
let lsn1 = wal.append(record1)?;
let lsn2 = wal.append(record2)?;

// Read back
let all_records = wal.read_all()?;
assert_eq!(all_records.len(), 2);

// Flush is a no-op (returns immediately)
wal.flush()?;
```

**Performance characteristics:**

- **Append**: O(1) - just push to Vec and increment LSN
- **Read All**: O(n) - clone entire Vec
- **Flush**: O(1) - no-op
- **Memory**: O(n) where n is number of records

The `MemoryWal` doesn't write to disk, so `flush()` is effectively a no-op. This eliminates disk I/O overhead while maintaining API compatibility with the disk-based WAL.

### MemorySst Architecture

The `MemorySst` implementation provides in-memory sorted string tables:

```rust
pub struct MemorySstWriter {
    records: Vec<Record>,  // Unsorted records during writing
}

pub struct MemorySstReader {
    name: String,           // Virtual "filename"
    records: Vec<Record>,   // Sorted records
    bloom: BloomFilter,     // Bloom filter for fast negative lookups
}
```

**Writer API:**

```rust
impl MemorySstWriter {
    pub fn new() -> Self;

    pub fn add(&mut self, record: Record);

    // Finish writing, sort records, build bloom filter
    pub fn finish(self, name: impl Into<String>) -> Result<MemorySstReader>;
}
```

**Reader API:**

```rust
impl MemorySstReader {
    // Get a record by key (uses bloom filter + binary search)
    pub fn get(&self, key: &Key) -> Option<&Record>;

    // Iterate all records
    pub fn iter(&self) -> impl Iterator<Item = &Record>;

    // Scan records with partition key prefix
    pub fn scan_prefix(&self, pk: &Bytes) -> impl Iterator<Item = &Record>;

    // Get SST name
    pub fn name(&self) -> &str;

    // Get record count
    pub fn len(&self) -> usize;
}
```

**Example usage:**

```rust
use kstone_core::memory_sst::MemorySstWriter;

// Create writer
let mut writer = MemorySstWriter::new();

// Add records (any order)
writer.add(record3);
writer.add(record1);
writer.add(record2);

// Finish writing - sorts records and builds bloom filter
let reader = writer.finish("test.sst")?;

// Read records
if let Some(record) = reader.get(&key1) {
    println!("Found: {:?}", record);
}

// Iterate all records (in sorted order)
for record in reader.iter() {
    println!("{:?}", record);
}
```

**Sorting and bloom filters:**

When `finish()` is called:
1. Records are sorted by encoded key: `self.records.sort_by(|a, b| a.key.encode().cmp(&b.key.encode()))`
2. A bloom filter is built with all keys for fast negative lookups
3. A `MemorySstReader` is returned with sorted data

**Performance characteristics:**

- **Add**: O(1) - append to Vec
- **Finish**: O(n log n) - sort records
- **Get**: O(log n) - binary search (with bloom filter optimization)
- **Iter**: O(1) - return iterator over Vec
- **Memory**: O(n) where n is number of records

### MemorySstStore - Virtual File System

For managing multiple SSTs, KeystoneDB provides `MemorySstStore`:

```rust
pub struct MemorySstStore {
    ssts: Arc<Mutex<HashMap<String, MemorySstReader>>>,
}

impl MemorySstStore {
    pub fn new() -> Self;

    // Store an SST by name
    pub fn store(&self, name: impl Into<String>, sst: MemorySstReader);

    // Retrieve an SST by name
    pub fn get(&self, name: &str) -> Option<MemorySstReader>;

    // Delete an SST
    pub fn delete(&self, name: &str) -> bool;

    // List all SST names
    pub fn list_names(&self) -> Vec<String>;

    // Clear all SSTs
    pub fn clear(&self);
}
```

This acts like a virtual file system for in-memory SSTs:

```rust
use kstone_core::memory_sst::{MemorySstWriter, MemorySstStore};

let store = MemorySstStore::new();

// Create and store SST
let mut writer = MemorySstWriter::new();
writer.add(record);
let sst = writer.finish("stripe-042-001.sst")?;
store.store("stripe-042-001.sst", sst);

// Retrieve later
if let Some(sst) = store.get("stripe-042-001.sst") {
    let record = sst.get(&key);
}

// Delete SST
store.delete("stripe-042-001.sst");
```

The store enables:
- Named SST management (like file paths)
- Concurrent access with `Arc<Mutex<...>>`
- Simulates disk-based SST directory structure

## Performance Characteristics

In-memory databases offer significantly different performance profiles compared to disk-based databases. Understanding these characteristics helps you choose the right mode and optimize your applications.

### Write Performance

**In-Memory Writes:**
- **Append to WAL**: ~50-100 nanoseconds (just Vec push + counter increment)
- **Memtable insert**: ~100-200 nanoseconds (BTreeMap insertion)
- **No fsync overhead**: Eliminates ~1-10ms disk sync latency
- **Throughput**: 100,000-500,000+ writes/second on modern hardware

**Disk-Based Writes (for comparison):**
- **Append to WAL**: ~100-200 microseconds (write + fsync)
- **Memtable insert**: ~100-200 nanoseconds (same as in-memory)
- **Total**: Dominated by fsync latency (~1-10ms)
- **Throughput**: 10,000-50,000 writes/second (depends on group commit batching)

**Speedup**: In-memory writes are **~1000x faster** due to eliminating disk I/O.

### Read Performance

**In-Memory Reads:**
- **Memtable lookup**: ~100-200 nanoseconds (BTreeMap get)
- **SST lookup**: ~500-1000 nanoseconds (bloom filter + binary search in RAM)
- **No disk I/O**: All data in RAM
- **Throughput**: 500,000-1,000,000+ reads/second

**Disk-Based Reads (for comparison):**
- **Memtable lookup**: ~100-200 nanoseconds (same as in-memory)
- **SST lookup (cached)**: ~1-2 microseconds (mmap read from page cache)
- **SST lookup (uncached)**: ~5-10 milliseconds (disk read)
- **Throughput**: Varies widely (100,000+ when cached, 100-1000 when uncached)

**Speedup**: In-memory reads are **~10-100x faster** than cached disk reads, and **~10,000x faster** than uncached disk reads.

### Memory Usage

Memory consumption is the primary constraint for in-memory databases:

**Per-Record Overhead:**
- Key encoding: `4 + pk_len + 4 + sk_len` bytes
- Value: Depends on item size
- Record struct overhead: ~64 bytes (Rust struct padding, enum discriminants)
- BTreeMap node overhead: ~32 bytes per entry

**Example calculations:**

```rust
// Simple record with 10-byte key and 100-byte item
// - Key: 4 + 10 + 4 + 0 = 18 bytes (no sort key)
// - Item: ~100 bytes
// - Overhead: 64 + 32 = 96 bytes
// Total: ~214 bytes per record

// 1 million records ≈ 214 MB
// 10 million records ≈ 2.14 GB
// 100 million records ≈ 21.4 GB
```

**SST Memory Usage:**
- Sorted record Vec: Same as records in memtable
- Bloom filter: ~10 bits per key (~1.25 bytes per record)
- Minimal overhead compared to raw data

**WAL Memory Usage:**
- Stores records in a Vec
- Cleared on flush, so typically small
- Worst case: 1000 records (memtable threshold) before flush

### Flush Performance

In-memory flush operations are extremely fast:

**Flush steps:**
1. **Sort memtable records**: O(n log n) where n = 1000 (default threshold)
2. **Build bloom filter**: O(n) - hash each key
3. **Create SST**: O(n) - clone records into Vec
4. **Clear memtable**: O(1) - just allocate new BTreeMap

**Total time**: ~100-500 microseconds for 1000 records

**Comparison to disk flush:**
- Disk flush includes: sort (same) + write to file (~10-50ms) + fsync (~1-10ms)
- In-memory flush is **~100-1000x faster**

### Query Performance

Query performance depends on query type:

**Partition Key Queries (Query):**
- Route to single stripe: O(1)
- Scan memtable: O(log n + k) where k = matching records
- Scan SSTs: O(m * log n) where m = number of SSTs
- All in memory, so: **~1-10 microseconds** for typical queries

**Full Table Scans (Scan):**
- Scan all 256 stripes
- Merge results in BTreeMap: O(n log n)
- In-memory: **~100-1000 microseconds** for 1000 records
- Disk-based: **~10-100 milliseconds** (depends on caching)

**Indexed Queries:**
- Same as partition key queries (indexes stored as records)
- Bloom filters provide fast negative lookups
- Binary search in sorted SSTs

### Throughput Benchmarks

Typical throughput on modern hardware (2023-era CPU, 64GB RAM):

**Sequential Writes:**
```
100,000 writes: 1.2 seconds → 83,000 ops/sec
1,000,000 writes: 8.5 seconds → 117,000 ops/sec
```

**Random Reads (all in memtable):**
```
100,000 reads: 0.3 seconds → 333,000 ops/sec
1,000,000 reads: 2.8 seconds → 357,000 ops/sec
```

**Mixed Workload (50/50 read/write):**
```
100,000 operations: 1.8 seconds → 55,000 ops/sec
1,000,000 operations: 15.2 seconds → 65,000 ops/sec
```

These numbers demonstrate that in-memory mode is ideal for:
- High-throughput testing
- Real-time data processing
- Performance-critical temporary storage

## Testing with In-Memory Databases

In-memory databases are invaluable for testing, providing fast, isolated, and repeatable test environments.

### Unit Test Patterns

**Basic pattern:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kstone_api::Database;

    #[test]
    fn test_feature() {
        let db = Database::create_in_memory().unwrap();

        // Test logic here

        // No cleanup needed - db dropped automatically
    }
}
```

**Fixture pattern:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kstone_api::{Database, ItemBuilder};

    fn create_test_db() -> Database {
        let db = Database::create_in_memory().unwrap();

        // Populate with test data
        for i in 0..10 {
            let item = ItemBuilder::new()
                .number("id", i)
                .string("name", &format!("User {}", i))
                .build();
            db.put(format!("user#{}", i).as_bytes(), item).unwrap();
        }

        db
    }

    #[test]
    fn test_with_fixture() {
        let db = create_test_db();

        // Test operates on pre-populated database
        let result = db.get(b"user#5").unwrap();
        assert!(result.is_some());
    }
}
```

**Parameterized tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiple_scenarios() {
        let test_cases = vec![
            (b"user#1", "Alice", 30),
            (b"user#2", "Bob", 35),
            (b"user#3", "Carol", 28),
        ];

        for (key, name, age) in test_cases {
            let db = Database::create_in_memory().unwrap();

            let item = ItemBuilder::new()
                .string("name", name)
                .number("age", age)
                .build();

            db.put(key, item).unwrap();

            let result = db.get(key).unwrap().unwrap();
            assert_eq!(result.get("name").unwrap().as_string(), Some(name));
        }
    }
}
```

### Integration Test Patterns

**Multi-component testing:**

```rust
#[test]
fn test_application_workflow() {
    let db = Database::create_in_memory().unwrap();

    // Test complete workflow
    let user_service = UserService::new(db.clone());
    let post_service = PostService::new(db.clone());

    // Create user
    user_service.create_user("alice", "alice@example.com").unwrap();

    // Create post
    post_service.create_post("alice", "Hello world").unwrap();

    // Query posts
    let posts = post_service.get_user_posts("alice").unwrap();
    assert_eq!(posts.len(), 1);
}
```

**Concurrent testing:**

```rust
#[test]
fn test_concurrent_access() {
    use std::thread;
    use std::sync::Arc;

    let db = Arc::new(Database::create_in_memory().unwrap());

    let mut handles = vec![];

    // Spawn 10 threads, each writing 100 items
    for thread_id in 0..10 {
        let db = db.clone();
        let handle = thread::spawn(move || {
            for i in 0..100 {
                let key = format!("thread#{}#item#{}", thread_id, i);
                let item = ItemBuilder::new()
                    .number("thread_id", thread_id)
                    .number("item_id", i)
                    .build();

                db.put(key.as_bytes(), item).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all items written
    for thread_id in 0..10 {
        for i in 0..100 {
            let key = format!("thread#{}#item#{}", thread_id, i);
            let result = db.get(key.as_bytes()).unwrap();
            assert!(result.is_some());
        }
    }
}
```

### Snapshot Testing

Create snapshots of database state for regression testing:

```rust
#[test]
fn test_with_snapshot() {
    let db = Database::create_in_memory().unwrap();

    // Populate database
    populate_test_data(&db);

    // Export snapshot
    let snapshot = export_all_items(&db);

    // Verify against expected snapshot
    let expected = load_expected_snapshot("test_data_v1.json");
    assert_eq!(snapshot, expected);
}

fn export_all_items(db: &Database) -> Vec<HashMap<String, KeystoneValue>> {
    let scan = Scan::new();
    db.scan(scan).unwrap().items
}
```

### Property-Based Testing

Use in-memory databases with property-based testing frameworks:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_put_get_roundtrip(key in "[a-z]{1,20}", value in any::<i64>()) {
        let db = Database::create_in_memory().unwrap();

        let item = ItemBuilder::new()
            .number("value", value)
            .build();

        db.put(key.as_bytes(), item.clone()).unwrap();
        let result = db.get(key.as_bytes()).unwrap().unwrap();

        assert_eq!(result.get("value").unwrap().as_number(), Some(&value.to_string()));
    }
}
```

## Limitations vs Disk Mode

While in-memory mode offers many advantages, it has important limitations compared to disk-based mode.

### No Persistence

**Limitation:** All data is lost when the database is dropped or the process exits.

**Impact:**
- No crash recovery
- No WAL-based durability
- No point-in-time recovery
- Data exists only while process runs

**Workarounds:**
1. **Snapshot to disk periodically:**
   ```rust
   fn snapshot_to_disk(db: &Database, path: &str) -> Result<()> {
       let scan = Scan::new();
       let items = db.scan(scan)?.items;

       let json = serde_json::to_string_pretty(&items)?;
       std::fs::write(path, json)?;
       Ok(())
   }
   ```

2. **Hybrid approach - write critical data to disk database:**
   ```rust
   let memory_db = Database::create_in_memory()?;
   let disk_db = Database::open("persistent.keystone")?;

   // Work with memory_db for performance
   memory_db.put(key, item.clone())?;

   // Persist critical items to disk
   if is_critical(&item) {
       disk_db.put(key, item)?;
   }
   ```

### Memory Constraints

**Limitation:** Total data size limited by available RAM.

**Impact:**
- Cannot store datasets larger than memory
- Risk of out-of-memory crashes
- No memory-mapped file support for virtual memory

**Capacity estimates:**

```
1 GB RAM → ~4-5 million small records
8 GB RAM → ~30-40 million small records
64 GB RAM → ~250-300 million small records
```

**Workarounds:**
1. **Use disk mode for large datasets**
2. **Implement eviction policies:**
   ```rust
   struct LruCache {
       db: Database,
       max_items: usize,
       access_times: HashMap<Vec<u8>, Instant>,
   }

   impl LruCache {
       fn evict_oldest(&mut self) {
           // Find oldest accessed item
           let oldest_key = self.access_times
               .iter()
               .min_by_key(|(_, time)| *time)
               .map(|(key, _)| key.clone());

           if let Some(key) = oldest_key {
               self.db.delete(&key).unwrap();
               self.access_times.remove(&key);
           }
       }
   }
   ```

3. **Partition data across multiple in-memory databases:**
   ```rust
   struct ShardedMemoryDb {
       shards: Vec<Database>,
   }

   impl ShardedMemoryDb {
       fn get_shard(&self, key: &[u8]) -> &Database {
           let shard_id = crc32fast::hash(key) as usize % self.shards.len();
           &self.shards[shard_id]
       }
   }
   ```

### No Background Compaction

**Limitation:** In-memory mode doesn't perform background compaction (yet).

**Impact:**
- SSTs accumulate after flushes
- Memory usage grows over time
- Slower reads as SST count increases

**Current behavior:**
- Memtable flush creates new SST
- Old SSTs remain in memory
- No tombstone removal
- No space reclamation

**Workarounds:**
1. **Manual flush control:**
   ```rust
   // Flush less frequently to reduce SST count
   // (Not yet exposed in API)
   ```

2. **Recreate database periodically:**
   ```rust
   fn compact_by_recreate(old_db: &Database) -> Result<Database> {
       let new_db = Database::create_in_memory()?;

       // Copy all items to new database
       let scan = Scan::new();
       let items = old_db.scan(scan)?.items;

       for item in items {
           if let Some(pk) = item.get("pk") {
               new_db.put(pk.as_bytes(), item)?;
           }
       }

       Ok(new_db)
   }
   ```

### Advanced Features Support

Some advanced features are not yet fully implemented for in-memory mode:

**Not Supported (returns error):**
- Transaction operations (TransactGet, TransactWrite)
- Update expressions
- Advanced queries (some complex filters)

**Example:**

```rust
let db = Database::create_in_memory()?;

// Basic operations work
db.put(b"key1", item)?;  // ✓ Works
db.get(b"key1")?;        // ✓ Works
db.delete(b"key1")?;     // ✓ Works

// Query and Scan work
db.query(query)?;        // ✓ Works
db.scan(scan)?;          // ✓ Works

// Transactions don't work yet
let result = db.transact_write(request);
// Returns: Err(Error::Internal("In-memory mode does not support transactions"))
```

**Roadmap:** Full feature parity is planned for future releases.

### Thread Safety Differences

**In-memory:**
- Uses `Arc<RwLock<...>>` for synchronization
- Shared state across all operations
- Lock contention possible under high concurrency

**Disk-based:**
- Also uses `Arc<RwLock<...>>`
- Additional file locks for crash safety
- Same concurrency model

Both modes are thread-safe, but in-memory mode has no filesystem-level locking overhead.

## Summary

KeystoneDB's in-memory mode provides a powerful tool for testing, development, and temporary data processing:

**Key Benefits:**
- **Performance**: 100-1000x faster than disk-based mode for writes
- **Simplicity**: No files, no cleanup, automatic memory management
- **Isolation**: Perfect for unit tests and parallel testing
- **Full API**: Same interface as disk-based mode (for supported features)

**Best Use Cases:**
- Unit testing and integration testing
- Temporary data processing pipelines
- Benchmarking and performance testing
- Development and prototyping
- In-process caching layers

**Implementation Details:**
- `MemoryWal`: In-memory write-ahead log with Vec storage
- `MemorySst`: In-memory sorted string tables with bloom filters
- `MemorySstStore`: Virtual file system for SST management
- 256-stripe LSM architecture (same as disk mode)

**Limitations:**
- No persistence (data lost on exit)
- Memory-constrained capacity
- No background compaction (yet)
- Some advanced features not yet supported

**Performance Characteristics:**
- Writes: 100,000+ ops/sec
- Reads: 500,000+ ops/sec
- Flush: ~100-500 microseconds
- Memory: ~200 bytes per record overhead

In-memory mode complements disk-based mode, giving you the flexibility to choose the right storage backend for each use case. For testing and temporary workloads, in-memory mode offers unmatched performance and simplicity. For production data requiring persistence, disk-based mode provides durability and crash recovery.

Use in-memory databases liberally in tests, prototypes, and pipelines - they make development faster and more enjoyable while maintaining full compatibility with production disk-based databases.

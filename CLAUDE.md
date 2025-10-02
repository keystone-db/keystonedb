# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

KeystoneDB is a single-file, embedded, DynamoDB-style database written in Rust. **Phase 6 (Network Layer & gRPC Server) is COMPLETE** - the database now supports remote access via gRPC in addition to embedded usage. Previous phases (0-3) are complete: storage engine, DynamoDB API, and full index support (LSI, GSI, TTL, Streams).

**Target:** Eventually a full Dynamo-model database with cloud sync, FTS/vector indexes, and attachment to DynamoDB or remote KeystoneDB instances.

## Commands

### Build & Run
```bash
# Build (debug)
cargo build

# Build (release)
cargo build --release

# Run CLI
cargo run --bin kstone -- <command>
# Or after building:
./target/release/kstone <command>
```

### Testing
```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p kstone-core
cargo test -p kstone-api
cargo test -p kstone-tests

# Run specific test
cargo test -p kstone-core --lib lsm::tests::test_lsm_put_get

# Run integration tests only
cargo test -p kstone-tests
```

### CLI Usage
```bash
# Create database
kstone create <path>

# Put item
kstone put <path> <key> '<json-item>'

# Get item
kstone get <path> <key>

# Delete item
kstone delete <path> <key>
```

### Server Usage
```bash
# Start gRPC server
cargo run --bin kstone-server -- --db-path <path> --port 50051

# Or after building:
./target/release/kstone-server --db-path <path> --port 50051

# Server options:
#   --db-path, -d <PATH>    Path to database directory (required)
#   --port, -p <PORT>       Port to listen on (default: 50051)
#   --host <HOST>           Host to bind to (default: 127.0.0.1)
```

## Architecture

### Workspace Structure
This is a Cargo workspace with 6 crates:
- **kstone-core**: Storage engine internals (WAL, SST, LSM)
- **kstone-api**: Public API wrapping the core engine
- **kstone-proto**: Protocol Buffers definitions for gRPC
- **kstone-server**: gRPC server implementation
- **kstone-cli**: Command-line binary for local database access
- **kstone-tests**: Integration tests

### Core Modules (kstone-core)

**Phase 0 modules:**
- `types.rs` - Core types (Value, Key, Record, Item)
- `error.rs` - Error types and Result
- `wal.rs` - Write-ahead log (Phase 0: separate files)
- `sst.rs` - Sorted string tables (Phase 0: separate files)
- `lsm.rs` - LSM engine orchestration (Phase 1.6+: 256 stripes)

**Phase 1.2+ modules:**
- `layout.rs` - Single-file layout with regions (Header, WAL Ring, Manifest Ring, SST Heap)
- `block.rs` - 4KB block I/O with optional AES-256-GCM encryption
- `extent.rs` - Extent allocator for SST heap (bump allocation)
- `mmap.rs` - Memory-mapped file reader pool
- `wal_ring.rs` - Ring buffer WAL with group commit (Phase 1.3+)
- `bloom.rs` - Bloom filter implementation (Phase 1.4+)
- `sst_block.rs` - Block-based SST with compression & bloom filters (Phase 1.4+)
- `manifest.rs` - Metadata catalog with ring buffer format (Phase 1.5+)

### Data Flow (Write Path - Phase 1.6)
1. `Database::put()` (kstone-api) → `LsmEngine::put()` (kstone-core)
2. Record created with auto-incrementing global `SeqNo`
3. Record appended to WAL (write-ahead log) and flushed to disk
4. Key stripe calculated: `stripe_id = crc32(pk) % 256`
5. Record added to stripe's in-memory memtable (BTreeMap)
6. If stripe memtable ≥ 1000 records → flush stripe to SST file (`{stripe:03}-{sst_id}.sst`)

### Data Flow (Read Path - Phase 1.6)
1. `Database::get()` → `LsmEngine::get()`
2. Key stripe calculated: `stripe_id = crc32(pk) % 256`
3. Check stripe's memtable first (newest data)
4. If not found, scan stripe's SSTs from newest to oldest
5. Return first match (newer versions shadow older)

### Storage Format

**Database directory structure (Phase 1.6):**
```
mydb.keystone/
├── wal.log         # Write-ahead log (crash recovery)
└── {stripe:03}-{sst_id}.sst  # Striped SST files (e.g., 042-5.sst for stripe 42, SST ID 5)
```

**Key encoding:**
- Composite keys: `[pk_len(4) | pk_bytes | sk_len(4) | sk_bytes]`
- Stripe selection: `stripe_id = crc32(pk) % 256`
- Keys with same PK always route to same stripe (allows efficient range queries)

**Value types (DynamoDB-style):**
- `N` - Number (stored as string for precision)
- `S` - String
- `B` - Binary (Bytes)
- `Bool` - Boolean
- `Null` - Null
- `L` - List
- `M` - Map (nested attributes)
- `VecF32` - Vector of f32 (for embeddings/vector search) [Phase 1+]
- `Ts` - Timestamp (i64 milliseconds since epoch) [Phase 1+]

### File Formats

All multi-byte integers use **little-endian** encoding (except magic numbers which are big-endian).

**WAL format:**
```
[Header: magic(4) version(4) reserved(8)]
[Record: lsn(8) len(4) data(bincode) crc(4)]*
```

**SST format:**
```
[Header: magic(4) version(4) count(4) reserved(4)]
[Records: (len(4) bincode_record)*]
[CRC: crc(4)]
```

Records in SST are sorted by encoded key.

### Concurrency Model
- **LSM engine**: Uses `RwLock` - multiple readers OR single writer
- **WAL**: Uses `Mutex` - serialized writes for group commit
- **Reads**: Lock-free after acquiring read lock (memtable + SST scan)
- **Writes**: Hold write lock during WAL append, memtable insert, potential flush

### Key Type Constraints

When working with keys in tests:
- Use `.to_vec()` for byte string literals: `Key::new(b"mykey".to_vec())`
- `Key::new()` and `Key::with_sk()` require `Into<Bytes>`, not `&[u8; N]`

### Value Type Testing

When asserting on `Value::N` (numbers):
```rust
// ✅ Correct
match value {
    Value::N(n) => assert_eq!(n, "42"),
    _ => panic!("Expected number"),
}

// ❌ Wrong - as_string() only works for Value::S
assert_eq!(value.as_string(), Some("42"));
```

### CRC32C Checksums

Phase 1.1+ uses CRC32C for hardware-accelerated checksums:
```rust
use kstone_core::types::checksum;

// Compute checksum
let data = b"hello world";
let crc = checksum::compute(data);

// Verify checksum
if checksum::verify(data, crc) {
    // Valid checksum
}
```

The crc32c crate uses hardware acceleration (SSE 4.2) when available, falling back to software implementation.

### Block I/O and Encryption (Phase 1.2+)

#### Writing blocks:
```rust
use kstone_core::{block::{BlockWriter, Block}, layout::BLOCK_SIZE};
use bytes::Bytes;

let file = File::create("data.db")?;
let mut writer = BlockWriter::new(file);

// Write plain block
let data = Bytes::from("hello");
let block = Block::new(1, data);
writer.write(&block, 0)?;

// Write encrypted block
let key = [42u8; 32]; // AES-256 key
let mut enc_writer = BlockWriter::with_encryption(file, key);
let enc_block = Block::with_encryption(2, Bytes::from("secret"));
enc_writer.write(&enc_block, BLOCK_SIZE as u64)?;
```

#### Reading blocks:
```rust
use kstone_core::block::BlockReader;

let file = File::open("data.db")?;
let mut reader = BlockReader::new(file);

// Read plain block
let block = reader.read(1, 0)?;

// Read encrypted block
let key = [42u8; 32];
let mut enc_reader = BlockReader::with_encryption(file, key);
let enc_block = enc_reader.read(2, BLOCK_SIZE as u64)?;
```

#### Memory-mapped reading:
```rust
use kstone_core::mmap::{MmapReader, MmapPool};

// Direct reader
let reader = MmapReader::open("data.db")?;
let data = reader.read(0, 1024)?;

// Using pool (cached)
let pool = MmapPool::new();
let reader = pool.get_or_open("data.db")?;
let block = reader.read_block(0)?;
```

### Ring Buffer WAL (Phase 1.3+)

#### Creating and using WAL ring:
```rust
use kstone_core::{wal_ring::WalRing, layout::Region, Record, Key};
use std::collections::HashMap;

// Define WAL region (64MB)
let wal_region = Region::new(4096, 64 * 1024 * 1024);

// Create new WAL
let wal = WalRing::create("db.wal", wal_region)?;

// Append records (buffered)
let key = Key::new(b"user#123".to_vec());
let item = HashMap::new();
let record = Record::put(key, item, 1);
let lsn = wal.append(record)?;

// Flush to disk (group commit)
wal.flush()?;

// Configure batch timeout for auto-flush
wal.set_batch_timeout(Duration::from_millis(5));
```

#### Recovery:
```rust
// Open existing WAL and recover
let wal = WalRing::open("db.wal", wal_region)?;

// Read all records (sorted by LSN)
let records = wal.read_all()?;
for (lsn, record) in records {
    // Replay record
}
```

#### Checkpointing:
```rust
// Set checkpoint LSN (records before this can be compacted)
wal.set_checkpoint(100)?;

// Compact (implicit in ring buffer - just marks for overwrite)
wal.compact()?;
```

**Key features:**
- Circular wrap-around when ring is full
- LSN 0 indicates empty/overwritten space
- Group commit batching with configurable timeout
- CRC32C validation per record
- Handles partial writes during recovery

### Block-Based SST (Phase 1.4+)

#### Writing SST:
```rust
use kstone_core::{
    sst_block::SstBlockWriter,
    extent::ExtentAllocator,
    Record, Key
};

let allocator = ExtentAllocator::new(sst_heap_offset);
let mut writer = SstBlockWriter::new();

// Add records (will be sorted automatically)
for record in records {
    writer.add(record);
}

// Write to file
let handle = writer.finish(&mut file, &allocator)?;
// handle contains: extent, num_data_blocks, index_offset, bloom_offset
```

#### Reading SST:
```rust
use kstone_core::sst_block::SstBlockReader;

// Open with handle
let reader = SstBlockReader::open(file, handle)?;

// Get with bloom filter optimization
let key = Key::new(b"user#123".to_vec());
if let Some(record) = reader.get(&key)? {
    // Found!
}
```

#### Bloom Filters:
```rust
use kstone_core::bloom::BloomFilter;

// Create filter (100 items, 10 bits per key)
let mut bloom = BloomFilter::new(100, 10);

// Add keys
bloom.add(b"key1");
bloom.add(b"key2");

// Test membership
assert!(bloom.contains(b"key1")); // Might be present
assert!(!bloom.contains(b"key3")); // Definitely not present

// Serialize/deserialize
let data = bloom.encode();
let bloom2 = BloomFilter::decode(&data).unwrap();
```

**Block-based SST features:**
- 4KB aligned blocks for optimal I/O
- Prefix compression (shared prefix length stored)
- ~1% false positive rate with 10 bits/key bloom filters
- Index block maps first key of each block to offset
- Footer: `[num_data_blocks(4) | index_offset(8) | bloom_offset(8) | crc32c(4)]`

### Manifest (Phase 1.5+)

#### Creating and using manifest:
```rust
use kstone_core::{
    manifest::{Manifest, ManifestRecord, SstMetadata},
    layout::Region,
    extent::Extent,
    sst_block::SstBlockHandle
};

// Define manifest region
let manifest_region = Region::new(64 * 1024 * 1024, 16 * 1024 * 1024);

// Create new manifest
let manifest = Manifest::create("db.manifest", manifest_region)?;

// Add SST record
let extent = Extent::new(1, offset, size);
let handle = SstBlockHandle { extent, num_data_blocks: 10, ... };
manifest.append(ManifestRecord::AddSst {
    sst_id: 1,
    stripe: 0,
    extent,
    handle,
    first_key: Bytes::from("a"),
    last_key: Bytes::from("z"),
})?;

// Flush to disk
manifest.flush()?;

// Get SST metadata
if let Some(meta) = manifest.get_sst(1) {
    // Use metadata
}
```

#### Recovery and state:
```rust
// Open existing manifest (auto-recovers)
let manifest = Manifest::open("db.manifest", manifest_region)?;

// Get current state
let state = manifest.state();
for (sst_id, meta) in &state.ssts {
    println!("SST {}: {:?}", sst_id, meta);
}

// Get checkpoint info
println!("Checkpoint LSN: {}", state.checkpoint_lsn);
println!("Checkpoint SeqNo: {}", state.checkpoint_seq);
```

#### Record types:
```rust
// Add new SST
ManifestRecord::AddSst { sst_id, stripe, extent, handle, first_key, last_key }

// Remove SST (for compaction)
ManifestRecord::RemoveSst { sst_id }

// Update checkpoint
ManifestRecord::Checkpoint { lsn, seq }

// Stripe assignment (Phase 1.6)
ManifestRecord::AssignStripe { stripe, sst_id }
```

#### Compaction:
```rust
// Compact manifest (rewrite only active records)
manifest.compact()?;
```

**Manifest features:**
- Ring buffer format with copy-on-write updates
- Tracks SST metadata (extent, handle, key range, stripe)
- Checkpoint LSN and SeqNo for recovery coordination
- Stripe assignments for multi-stripe LSM (Phase 1.6)
- Record format: `[seq(8) | len(4) | bincode_data | crc32c(4)]`
- Recovery via sequential scan with CRC validation
- Compaction removes obsolete AddSst/RemoveSst pairs

### 256-Stripe LSM (Phase 1.6+)

#### Architecture:
The LSM engine uses 256 independent stripes for horizontal scalability:
- Each stripe has its own memtable (BTreeMap) and SST list
- Keys automatically route to stripes based on `crc32(pk) % 256`
- Keys with same partition key (PK) always go to same stripe
- Independent flush per stripe (1000 record threshold)
- Global SeqNo counter maintains total ordering across stripes

#### Stripe routing:
```rust
use kstone_core::Key;

let key = Key::new(b"user#123".to_vec());
let stripe_id = key.stripe(); // Returns 0-255

// Composite keys with same PK go to same stripe
let key1 = Key::with_sk(b"user#123".to_vec(), b"profile".to_vec());
let key2 = Key::with_sk(b"user#123".to_vec(), b"settings".to_vec());
assert_eq!(key1.stripe(), key2.stripe()); // Same stripe!
```

#### SST file naming:
- Format: `{stripe:03}-{sst_id}.sst`
- Example: `042-15.sst` = Stripe 42, SST ID 15
- Backward compatible: Legacy `{sst_id}.sst` files load into stripe 0

#### Benefits:
- Horizontal scalability: 256 independent flush operations
- Better concurrency: Stripes can be flushed independently
- Efficient range queries: Same PK → same stripe → sequential scan
- Load distribution: CRC-based hashing distributes keys evenly

## Common Patterns

### Creating a test database
```rust
use tempfile::TempDir;
use kstone_api::Database;

let dir = TempDir::new().unwrap();
let db = Database::create(dir.path()).unwrap();
```

### Building items
```rust
use kstone_api::ItemBuilder;

let item = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .bool("active", true)
    .build();

db.put(b"user#123", item).unwrap();
```

### Querying items (Phase 2.1+)

#### Basic query - all items in partition
```rust
use kstone_api::Query;

let query = Query::new(b"user#123");
let response = db.query(query)?;

for item in response.items {
    println!("{:?}", item);
}
```

#### Query with sort key condition
```rust
// BeginsWith - find all posts for a user
let query = Query::new(b"user#123")
    .sk_begins_with(b"post#");
let response = db.query(query)?;

// Between - date range query
let query = Query::new(b"sensor#456")
    .sk_between(b"2024-01-01", b"2024-12-31");
let response = db.query(query)?;

// Greater than - recent items
let query = Query::new(b"user#789")
    .sk_gt(b"2024-06-01");
let response = db.query(query)?;
```

#### Query with limit and pagination
```rust
// First page
let query = Query::new(b"user#999").limit(10);
let response = db.query(query)?;

println!("Found {} items", response.items.len());

// Second page (if more results exist)
if let Some((last_pk, last_sk)) = response.last_key {
    let query2 = Query::new(&last_pk)
        .limit(10)
        .start_after(&last_pk, last_sk.as_deref());
    let response2 = db.query(query2)?;
}
```

#### Reverse iteration
```rust
// Get most recent items first
let query = Query::new(b"user#123")
    .forward(false)  // Reverse order
    .limit(5);
let response = db.query(query)?;
```

### Scanning table (Phase 2.2+)

#### Basic scan - all items in table
```rust
use kstone_api::Scan;

let scan = Scan::new();
let response = db.scan(scan)?;

println!("Found {} items", response.items.len());
```

#### Scan with limit and pagination
```rust
// First page
let scan = Scan::new().limit(100);
let response = db.scan(scan)?;

// Second page (if more results exist)
if let Some((last_pk, last_sk)) = response.last_key {
    let scan2 = Scan::new()
        .limit(100)
        .start_after(&last_pk, last_sk.as_deref());
    let response2 = db.scan(scan2)?;
}
```

#### Parallel scan - distribute across segments
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

// Collect results from all segments
let mut all_items = Vec::new();
for handle in handles {
    if let Ok(Ok(response)) = handle.join() {
        all_items.extend(response.items);
    }
}

println!("Total items: {}", all_items.len());
```

### Updating items (Phase 2.4+)

#### Simple SET operation
```rust
use kstone_api::Update;
use kstone_core::Value;

let update = Update::new(b"user#123")
    .expression("SET age = :new_age")
    .value(":new_age", Value::number(30));

let response = db.update(update)?;
println!("Updated item: {:?}", response.item);
```

#### Increment/decrement with arithmetic
```rust
// Increment score
let update = Update::new(b"game#456")
    .expression("SET score = score + :inc")
    .value(":inc", Value::number(50));

let response = db.update(update)?;

// Decrement lives
let update = Update::new(b"game#456")
    .expression("SET lives = lives - :dec")
    .value(":dec", Value::number(1));

let response = db.update(update)?;
```

#### Remove attributes
```rust
// Remove temporary or sensitive attributes
let update = Update::new(b"user#789")
    .expression("REMOVE temp, verification_code");

let response = db.update(update)?;
```

#### ADD operation (atomic addition)
```rust
// Add to existing number (creates if doesn't exist)
let update = Update::new(b"counter#global")
    .expression("ADD views :count")
    .value(":count", Value::number(1));

let response = db.update(update)?;
```

#### Multiple actions
```rust
// Combine SET, REMOVE, and ADD in one update
let update = Update::new(b"user#999")
    .expression("SET last_login = :now, status = :active REMOVE temp ADD login_count :inc")
    .value(":now", Value::number(1704067200))
    .value(":active", Value::string("online"))
    .value(":inc", Value::number(1));

let response = db.update(update)?;
```

### Batch operations (Phase 2.6+)

#### Batch get - retrieve multiple items
```rust
use kstone_api::BatchGetRequest;

// Get multiple items in one call
let request = BatchGetRequest::new()
    .add_key(b"user#1")
    .add_key(b"user#2")
    .add_key_with_sk(b"user#3", b"profile");

let response = db.batch_get(request)?;

// Response only contains items that were found
for (key, item) in &response.items {
    println!("Found: {:?} -> {:?}", key, item);
}
```

#### Batch write - put/delete multiple items
```rust
use kstone_api::BatchWriteRequest;

// Put and delete multiple items in one call
let request = BatchWriteRequest::new()
    .put(b"user#1", ItemBuilder::new().string("name", "Alice").build())
    .put(b"user#2", ItemBuilder::new().string("name", "Bob").build())
    .delete(b"user#3") // Delete this item
    .put_with_sk(b"user#4", b"profile", ItemBuilder::new().string("bio", "test").build());

let response = db.batch_write(request)?;
println!("Processed {} items", response.processed_count);
```

#### Batch operations for bulk data loading
```rust
// Efficient bulk insert
let mut request = BatchWriteRequest::new();

for i in 0..100 {
    let pk = format!("item#{}", i);
    let item = ItemBuilder::new()
        .number("id", i)
        .string("data", format!("Item {}", i))
        .build();

    request = request.put(pk.as_bytes(), item);
}

let response = db.batch_write(request)?;
println!("Loaded {} items", response.processed_count);
```

### Local Secondary Indexes (Phase 3.1+)

#### Creating a table with LSI
```rust
use kstone_api::{Database, TableSchema, LocalSecondaryIndex};

// Define schema with LSI on email attribute
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"))
    .add_local_index(LocalSecondaryIndex::new("score-index", "score"));

let db = Database::create_with_schema(path, schema)?;
```

#### LSI with projection types
```rust
use kstone_api::{LocalSecondaryIndex, IndexProjection};

// Project all attributes (default)
let lsi_all = LocalSecondaryIndex::new("email-index", "email");

// Project only keys
let lsi_keys = LocalSecondaryIndex::new("status-index", "status").keys_only();

// Project specific attributes
let lsi_include = LocalSecondaryIndex::new("name-index", "lastName")
    .include(vec!["firstName".to_string(), "email".to_string()]);

let schema = TableSchema::new()
    .add_local_index(lsi_all)
    .add_local_index(lsi_keys)
    .add_local_index(lsi_include);
```

#### Querying by LSI
```rust
use kstone_api::Query;

// LSI entries are automatically created when you put items
db.put(b"org#acme", ItemBuilder::new()
    .string("name", "Alice")
    .string("email", "alice@example.com")
    .number("score", 950)
    .build())?;

// Query by email using LSI (instead of base table sort key)
let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice");

let response = db.query(query)?;

// All query features work with indexes
let query = Query::new(b"org#acme")
    .index("score-index")
    .sk_gte(b"500")  // Scores >= 500
    .limit(10)
    .forward(false); // Descending order

let response = db.query(query)?;
```

#### LSI query with conditions
```rust
// Find all users in org with high scores
let query = Query::new(b"org#acme")
    .index("score-index")
    .sk_between(b"800", b"999")
    .limit(20);

let response = db.query(query)?;
println!("Found {} high scorers", response.items.len());
```

### Global Secondary Indexes (Phase 3.2+)

#### Creating a table with GSI
```rust
use kstone_api::{Database, TableSchema, GlobalSecondaryIndex};

// GSI with partition key only
let schema = TableSchema::new()
    .add_global_index(GlobalSecondaryIndex::new("status-index", "status"));

// GSI with partition key AND sort key
let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::with_sort_key("category-price-index", "category", "price")
    );

let db = Database::create_with_schema(path, schema)?;
```

#### Key difference between LSI and GSI
- **LSI**: Uses same partition key as base table, different sort key
  - LSI entries stored in SAME stripe as base record
  - Query by base table PK, filter by LSI sort key
- **GSI**: Uses DIFFERENT partition key (and optionally sort key)
  - GSI entries route to stripe based on GSI partition key value
  - Enables queries across different base table partitions

#### Querying by GSI
```rust
use kstone_api::Query;

// Put items with different base PKs but same GSI partition key
db.put(b"user#alice", ItemBuilder::new()
    .string("name", "Alice")
    .string("status", "active")  // GSI partition key
    .number("timestamp", 1000)
    .build())?;

db.put(b"user#bob", ItemBuilder::new()
    .string("name", "Bob")
    .string("status", "active")  // Same GSI PK - different base PK
    .number("timestamp", 2000)
    .build())?;

// Query by status="active" - finds items across different base partitions
let query = Query::new(b"active")
    .index("status-index");

let response = db.query(query)?;
println!("Found {} active users", response.items.len()); // Finds both Alice and Bob
```

#### GSI with sort key conditions
```rust
// Query GSI with sort key range
let query = Query::new(b"electronics")
    .index("category-price-index")
    .sk_between(b"100", b"1000")  // Price range
    .limit(20);

let response = db.query(query)?;

// GSI supports all query features
let query = Query::new(b"books")
    .index("category-price-index")
    .sk_gte(b"50")
    .forward(false)  // Descending price
    .limit(10);

let response = db.query(query)?;
```

#### GSI projection types
```rust
use kstone_api::{GlobalSecondaryIndex, IndexProjection};

// Project all attributes (default)
let gsi_all = GlobalSecondaryIndex::new("status-index", "status");

// Project only keys
let gsi_keys = GlobalSecondaryIndex::with_sort_key("category-index", "category", "price")
    .keys_only();

// Project specific attributes
let gsi_include = GlobalSecondaryIndex::new("region-index", "region")
    .include(vec!["country".to_string(), "city".to_string()]);

let schema = TableSchema::new()
    .add_global_index(gsi_all)
    .add_global_index(gsi_keys)
    .add_global_index(gsi_include);
```

### Time To Live (TTL) (Phase 3.3+)

#### Creating a table with TTL
```rust
use kstone_api::{Database, TableSchema};

// Enable TTL on "expiresAt" attribute
let schema = TableSchema::new().with_ttl("expiresAt");
let db = Database::create_with_schema(path, schema)?;
```

#### Using TTL - automatic expiration
```rust
use kstone_core::Value;

// Get current time in seconds since epoch
let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

// Put item that expires in 1 hour
db.put(b"session#abc123", ItemBuilder::new()
    .string("userId", "user#456")
    .string("token", "xyz...")
    .number("expiresAt", now + 3600)  // 1 hour from now
    .build())?;

// Put item that expires in 24 hours
db.put(b"cache#data1", ItemBuilder::new()
    .string("value", "cached data")
    .number("expiresAt", now + 86400)  // 24 hours from now
    .build())?;
```

#### TTL lazy deletion
```rust
// Items past their expiration time are automatically filtered out
// during read operations (get, query, scan)

let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

// Put item that expired 100 seconds ago
db.put(b"expired#1", ItemBuilder::new()
    .string("data", "old data")
    .number("expiresAt", now - 100)
    .build())?;

// Get returns None for expired items (lazy deletion)
let result = db.get(b"expired#1")?;
assert!(result.is_none());
```

#### TTL with queries and scans
```rust
// Expired items are automatically filtered from query/scan results

let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

// Put mix of expired and valid items
for i in 1..=5 {
    let expires = if i <= 2 {
        now - 100  // Expired
    } else {
        now + 1000  // Valid
    };

    db.put_with_sk(b"user#123", format!("item#{}", i).as_bytes(),
        ItemBuilder::new()
            .string("name", format!("Item {}", i))
            .number("expiresAt", expires)
            .build())?;
}

// Query only returns non-expired items (items 3, 4, 5)
let query = Query::new(b"user#123");
let response = db.query(query)?;
assert_eq!(response.items.len(), 3);

// Scan also filters expired items
let scan = Scan::new();
let response = db.scan(scan)?;
// Only non-expired items returned
```

#### TTL with Timestamp value type
```rust
use kstone_core::Value;

// TTL supports both Number (seconds) and Timestamp (milliseconds) types
let now_millis = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as i64;

let mut item = ItemBuilder::new()
    .string("data", "test")
    .build();
item.insert("expiresAt".to_string(), Value::Ts(now_millis + 3600_000));  // +1 hour

db.put(b"item#1", item)?;
```

#### Items without TTL attribute
```rust
// Items without the TTL attribute never expire
let schema = TableSchema::new().with_ttl("expiresAt");
let db = Database::create_with_schema(path, schema)?;

// This item has no expiresAt attribute - will never expire
db.put(b"permanent#1", ItemBuilder::new()
    .string("data", "permanent data")
    .build())?;

// Always retrievable
let result = db.get(b"permanent#1")?;
assert!(result.is_some());
```

### Streams (Change Data Capture) (Phase 3.4+)

#### Creating a table with streams
```rust
use kstone_api::{Database, TableSchema, StreamConfig, StreamViewType};

// Enable streams with default settings (NEW_AND_OLD_IMAGES)
let schema = TableSchema::new()
    .with_stream(StreamConfig::enabled());
let db = Database::create_with_schema(path, schema)?;

// Configure stream view type
let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_view_type(StreamViewType::NewImage)
            .with_buffer_size(500)
    );
let db = Database::create_with_schema(path, schema)?;
```

#### Stream view types
```rust
// KeysOnly - Only key information (no item data)
StreamViewType::KeysOnly

// NewImage - Only the new state of the item
StreamViewType::NewImage

// OldImage - Only the previous state of the item
StreamViewType::OldImage

// NewAndOldImages - Both states (default)
StreamViewType::NewAndOldImages
```

#### Reading stream records
```rust
// Get all stream records
let records = db.read_stream(None)?;

for record in records {
    match record.event_type {
        StreamEventType::Insert => {
            println!("INSERT: {:?}", record.new_image);
        }
        StreamEventType::Modify => {
            println!("MODIFY: {:?} -> {:?}", record.old_image, record.new_image);
        }
        StreamEventType::Remove => {
            println!("REMOVE: {:?}", record.old_image);
        }
    }
}
```

#### Polling for new changes
```rust
let mut last_sequence = None;

loop {
    // Get only new records since last poll
    let records = db.read_stream(last_sequence)?;

    if records.is_empty() {
        // No new changes
        std::thread::sleep(std::time::Duration::from_secs(1));
        continue;
    }

    for record in &records {
        // Process record
        process_change(record);
    }

    // Update last sequence for next poll
    last_sequence = records.last().map(|r| r.sequence_number);
}
```

#### Stream record structure
```rust
pub struct StreamRecord {
    pub sequence_number: u64,        // Globally unique, monotonic
    pub event_type: StreamEventType,  // Insert, Modify, or Remove
    pub key: Key,                     // Item key
    pub old_image: Option<Item>,      // Before state (if applicable)
    pub new_image: Option<Item>,      // After state (if applicable)
    pub timestamp: i64,               // Milliseconds since epoch
}
```

#### Stream buffer and retention
```rust
// Streams use an in-memory circular buffer
// Old records are dropped when buffer is full

let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_buffer_size(1000)  // Keep last 1000 records
    );
```

#### Example: Audit log
```rust
let schema = TableSchema::new()
    .with_stream(StreamConfig::enabled());
let db = Database::create_with_schema(path, schema)?;

// Perform operations
db.put(b"user#123", ItemBuilder::new().string("name", "Alice").build())?;
db.put(b"user#123", ItemBuilder::new().string("name", "Bob").build())?;
db.delete(b"user#123")?;

// Read audit trail
let records = db.read_stream(None)?;
assert_eq!(records.len(), 3);
assert_eq!(records[0].event_type, StreamEventType::Insert);
assert_eq!(records[1].event_type, StreamEventType::Modify);
assert_eq!(records[2].event_type, StreamEventType::Remove);
```

### Transactions (Phase 2.7+)

#### Transact get - atomic reads
```rust
use kstone_api::TransactGetRequest;

// Read multiple items atomically (consistent snapshot)
let request = TransactGetRequest::new()
    .get(b"user#1")
    .get(b"user#2")
    .get_with_sk(b"user#3", b"profile");

let response = db.transact_get(request)?;

// Items in same order as request, None if not found
for item_opt in response.items {
    if let Some(item) = item_opt {
        println!("Item: {:?}", item);
    } else {
        println!("Not found");
    }
}
```

#### Transact write - atomic writes with conditions
```rust
use kstone_api::TransactWriteRequest;
use kstone_core::Value;

// Transfer balance between accounts atomically
let request = TransactWriteRequest::new()
    // Deduct from source account (only if balance sufficient)
    .update_with_condition(
        b"account#source",
        "SET balance = balance - :amount",
        "balance >= :amount"
    )
    // Add to destination account
    .update(
        b"account#dest",
        "SET balance = balance + :amount"
    )
    .value(":amount", Value::number(100));

match db.transact_write(request) {
    Ok(response) => println!("Transaction committed: {} operations", response.committed_count),
    Err(e) => println!("Transaction failed: {}", e),
}
```

#### Mixed transaction operations
```rust
// Put, update, delete, and condition check in single transaction
let request = TransactWriteRequest::new()
    // Create new user
    .put(b"user#new", ItemBuilder::new()
        .string("name", "Alice")
        .number("balance", 0)
        .build())
    // Update existing user status
    .update(b"user#existing", "SET status = :status")
    // Delete old user
    .delete(b"user#old")
    // Verify prerequisite condition (doesn't write anything)
    .condition_check(b"config#global", "attribute_exists(enabled)")
    .value(":status", Value::string("active"));

let response = db.transact_write(request)?;
// All operations succeed or all fail (ACID)
```

#### Transaction atomicity guarantee
```rust
// If ANY condition fails, NOTHING is committed
let request = TransactWriteRequest::new()
    .put(b"item#1", ItemBuilder::new().number("value", 1).build())
    .put_with_condition(
        b"item#2",
        ItemBuilder::new().number("value", 2).build(),
        "attribute_exists(nonexistent)" // This will fail
    );

// Result: TransactionCanceled error, item#1 NOT created (rolled back)
let result = db.transact_write(request);
assert!(result.is_err());
```

### Conditional operations (Phase 2.5+)

#### Put if not exists (optimistic locking)
```rust
use kstone_core::expression::ExpressionContext;

// Only put if item doesn't exist
let item = ItemBuilder::new().string("name", "Alice").build();
let context = ExpressionContext::new();

match db.put_conditional(
    b"user#123",
    item,
    "attribute_not_exists(name)",  // Condition: name attribute doesn't exist
    context,
) {
    Ok(_) => println!("Item created"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("Item already exists");
    }
    Err(e) => println!("Error: {}", e),
}
```

#### Conditional update (optimistic locking)
```rust
// Only update if age hasn't changed (optimistic locking)
let update = Update::new(b"user#456")
    .expression("SET age = :new_age")
    .condition("age = :old_age")  // Condition: age must equal expected value
    .value(":new_age", Value::number(26))
    .value(":old_age", Value::number(25));

match db.update(update) {
    Ok(response) => println!("Updated: {:?}", response.item),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("Update conflict - age was modified by another process");
    }
    Err(e) => println!("Error: {}", e),
}
```

#### Conditional delete
```rust
// Only delete if status is inactive
let context = ExpressionContext::new()
    .with_value(":status", Value::string("inactive"));

db.delete_conditional(
    b"user#789",
    "status = :status",
    context,
)?;
```

#### Update with attribute_exists check
```rust
// Only update if email attribute exists
let update = Update::new(b"user#999")
    .expression("SET verified = :val")
    .condition("attribute_exists(email)")
    .value(":val", Value::Bool(true));

let response = db.update(update)?;
```

#### Complex conditional expressions
```rust
// Update only if age >= 18 AND account is active
let update = Update::new(b"user#111")
    .expression("SET can_vote = :val")
    .condition("age >= :min_age AND active = :is_active")
    .value(":val", Value::Bool(true))
    .value(":min_age", Value::number(18))
    .value(":is_active", Value::Bool(true));

let response = db.update(update)?;
```

### Using condition expressions (Phase 2.3+)

```rust
use kstone_core::expression::{ExpressionParser, ExpressionContext, ExpressionEvaluator};
use kstone_core::Value;

// Parse condition expression
let expr = ExpressionParser::parse(
    "age >= :min_age AND (active = :is_active OR attribute_exists(verified))"
)?;

// Create context with attribute values
let context = ExpressionContext::new()
    .with_value(":min_age", Value::number(18))
    .with_value(":is_active", Value::Bool(true));

// Evaluate expression against item
let evaluator = ExpressionEvaluator::new(&item, &context);
let result = evaluator.evaluate(&expr)?;

if result {
    println!("Condition passed!");
}
```

#### Expression syntax examples:
```rust
// Comparison operators
"age > :min"
"name = :target_name"
"score >= :passing_grade"

// Logical operators
"age > :min AND active = :is_active"
"status = :pending OR status = :processing"
"NOT archived"

// Functions
"attribute_exists(email)"
"attribute_not_exists(deleted_at)"
"begins_with(email, :domain)"

// Name placeholders (for reserved words)
"#name = :value"  // Use #name if 'name' is reserved

// Complex expressions with parentheses
"(age >= :min AND age <= :max) OR verified = :true"
```

### Working with core types directly
```rust
use kstone_core::{Key, Record, Value};
use std::collections::HashMap;

let key = Key::new(b"mykey".to_vec());
let mut item = HashMap::new();
item.insert("field".to_string(), Value::string("value"));
let record = Record::put(key, item, seq_no);
```

## Important Implementation Details

### Endianness Bug (Fixed)
Earlier versions had a critical bug: WAL/SST wrote in big-endian but read in little-endian. Now **all** multi-byte integers (except magic numbers) use little-endian for consistency.

### File Position After Clone
When cloning file handles (e.g., in `Wal::read_all()`), the clone inherits the current position. Always `seek()` after cloning if you need to read from a specific offset.

### Memtable Flush Threshold
Currently hardcoded at 1000 records (`MEMTABLE_THRESHOLD` in lsm.rs). When memtable reaches this size:
1. SST created with sorted records
2. WAL rotated (old deleted, new created)
3. Memtable cleared

### Recovery on Open
`Database::open()` automatically:
1. Scans directory for SST files
2. Opens WAL and replays all records into memtable
3. Determines next SeqNo from max in WAL
4. Ready for operations

## gRPC Server (Phase 6)

### Protocol Definition (kstone-proto)

The server uses Protocol Buffers (proto3) to define the gRPC service interface:

- **Service**: `KeystoneDb` with 11 RPC methods
- **Methods Implemented**:
  - `Put`, `Get`, `Delete` - Basic CRUD operations
  - `Query` - Query items by partition key with sort key conditions
  - `Scan` - Server-side streaming scan of all items
  - `BatchGet`, `BatchWrite` - Batch operations
- **Methods Stubbed** (return `UNIMPLEMENTED`):
  - `TransactGet`, `TransactWrite` - Transactional operations
  - `Update` - Update expressions
  - `ExecuteStatement` - PartiQL queries

### Server Implementation (kstone-server)

**Architecture**:
- `service.rs`: Implements the `KeystoneDb` gRPC trait
- `convert.rs`: Bidirectional type conversions between protobuf and KeystoneDB types
- `bin/kstone-server.rs`: Server binary with CLI

**Key Patterns**:
- `Arc<Database>` for thread-safe sharing across async tasks
- `tokio::task::spawn_blocking` to bridge async gRPC handlers with synchronous Database API
- Comprehensive error mapping from `kstone_core::Error` to gRPC `Status` codes

**Error Mapping**:
```rust
NotFound → NOT_FOUND
InvalidQuery/InvalidArgument → INVALID_ARGUMENT
ConditionalCheckFailed → FAILED_PRECONDITION
Io/Corruption → INTERNAL/DATA_LOSS
TransactionCanceled → ABORTED
```

**Type Conversions**:
Due to Rust's orphan rules, we use conversion functions instead of trait implementations:
- `proto_value_to_ks()` / `ks_value_to_proto()`
- `proto_item_to_ks()` / `ks_item_to_proto()`
- `proto_key_to_ks()` / `ks_key_to_proto()`

## Development Status

**Phase 0 (Walking Skeleton) - COMPLETE ✅**
- Single-stripe LSM engine
- WAL with crash recovery
- Put/Get/Delete operations
- Memtable flush to SST
- CLI with JSON support
- All tests passing (30 tests)

**Phase 1: Core Storage - IN PROGRESS 🚧**

*Phase 1.1 Foundation - COMPLETE ✅*
- Enhanced error types (EncryptionError, CompressionError, ManifestCorruption, CompactionError, StripeError)
- Added VecF32 and Ts value types for vector/time-series support
- Added crc32c dependency for hardware-accelerated checksums
- CRC32C checksum helpers in types::checksum module
- All tests passing (19 core tests)

*Phase 1.2 File Layout & Block I/O - COMPLETE ✅*
- Single-file layout design: `[Header(4KB) | WAL Ring | Manifest Ring | SST Heap]`
- Block-based I/O (4KB blocks) with CRC32C checksums
- Optional AES-256-GCM encryption per block
- Extent allocator for SST heap (bump allocation)
- Memory-mapped reader pool for efficient reads
- New modules: layout.rs, block.rs, extent.rs, mmap.rs
- All tests passing (44 core tests)

*Phase 1.3 WAL Enhancements - COMPLETE ✅*
- Ring buffer WAL implementation with circular wrap-around
- Group commit batching with configurable timeout (default 10ms)
- Enhanced recovery: handles partial writes, LSN-based validation
- Checkpoint support for compaction (implicit in ring buffer)
- Zero-initialized ring buffer regions
- Record format: `[lsn(8) | len(4) | data | crc32c(4)]`
- New module: wal_ring.rs
- All tests passing (49 core tests)

*Phase 1.4 SST Improvements - COMPLETE ✅*
- Block-based SST format (4KB blocks)
- Prefix compression for keys within blocks
- Bloom filters per data block (~1% false positive rate, 10 bits/key)
- Index block for fast key-to-block mapping
- Metadata blocks: index + bloom filters + footer
- Compression support (stub for future Zstd integration)
- New modules: bloom.rs, sst_block.rs
- All tests passing (57 core tests)

*Phase 1.5 Manifest - COMPLETE ✅*
- Ring buffer manifest implementation with copy-on-write updates
- Tracks SST metadata (extent, handle, key range, stripe)
- Checkpoint LSN and SeqNo tracking for recovery coordination
- Stripe assignment records (for Phase 1.6)
- Record types: AddSst, RemoveSst, Checkpoint, AssignStripe
- Recovery via sequential scan with CRC32C validation
- Compaction to rewrite only active records
- New module: manifest.rs
- All tests passing (62 core tests)

*Phase 1.6 LSM with 256 Stripes - COMPLETE ✅*
- 256-way striped LSM tree architecture
- Stripe struct with independent memtable and SST list per stripe
- Key routing based on `crc32(pk) % 256` for automatic load distribution
- Independent flush per stripe (1000 record threshold per stripe)
- SST filename format: `{stripe:03}-{sst_id}.sst`
- Global SeqNo counter maintains total ordering
- Backward compatible SST recovery (legacy format assigned to stripe 0)
- New tests: test_lsm_striping, test_lsm_stripe_independent_flush
- All tests passing (64 core tests)

**Phase 2: Complete Dynamo API - IN PROGRESS 🚧**

*Phase 2.1 Query Operation - COMPLETE ✅*
- Iterator module with QueryParams and SortKeyCondition support
- Sort key conditions: Equal, LessThan, LessThanOrEqual, GreaterThan, GreaterThanOrEqual, Between, BeginsWith
- Query within partition (same PK, filter by SK)
- Forward and reverse iteration (ScanDirection)
- Pagination support with LastEvaluatedKey
- Limit parameter (max items per query)
- Query builder API: `Query::new(pk).sk_begins_with(prefix).limit(10)`
- Stripe-aware routing (queries single stripe only)
- New module: iterator.rs (core), query.rs (API)
- New tests: test_lsm_query_*, test_database_query_*
- All tests passing (74 core + 9 API = 83 tests + 6 integration = 89 total)

*Phase 2.2 Scan Operation - COMPLETE ✅*
- ScanParams added to iterator.rs with limit, start_key, segment, and total_segments
- Multi-stripe scanning across all 256 stripes
- Global sorting via BTreeMap for consistent ordering
- Pagination support with LastEvaluatedKey
- Parallel scan with segment distribution (stripe_id % total_segments == segment)
- Scan builder API: `Scan::new().limit(100).segment(0, 4)`
- Stripe filtering for parallel scans (each segment scans subset of stripes)
- New module: scan.rs (API)
- New tests: test_lsm_scan_*, test_database_scan_*
- All tests passing (78 core + 16 API = 94 tests + 6 integration = 100 total)

*Phase 2.3 Expression System - COMPLETE ✅*
- Expression AST with operators, functions, and operands
- Comparison operators: =, <>, <, <=, >, >=
- Logical operators: AND, OR, NOT
- Condition functions: attribute_exists(), attribute_not_exists(), begins_with()
- Expression parser (text → AST) with lexer and recursive descent parser
- Expression evaluator (AST + Item + context → bool)
- ExpressionContext for attribute value/name substitution (:value, #name)
- Attribute path resolution with placeholder support
- Parser handles parentheses, operator precedence, and complex expressions
- New module: expression.rs
- New tests: test_parse_*, test_attribute_*, test_and_operator, test_begins_with
- All tests passing (94 core + 16 API = 110 tests + 6 integration = 116 total)

*Phase 2.4 Update Operations - COMPLETE ✅*
- Update expression AST with actions: SET, REMOVE, ADD, DELETE
- UpdateValue enum supporting paths, placeholders, and arithmetic (path + value, path - value)
- Update expression parser (text → actions)
- UpdateExecutor applies actions to items
- LSM update() method: get → apply → put
- Update builder API: `Update::new(pk).expression("SET age = :val").value(":val", ...)`
- SET action for setting attributes or incrementing (SET x = x + :inc)
- REMOVE action for deleting attributes
- ADD action for adding to numbers
- Extended expression.rs with update functionality
- New module: update.rs (API)
- New tests: test_update_*, test_database_update_*
- All tests passing (99 core + 22 API = 121 tests + 6 integration = 127 total)

*Phase 2.5 Conditional Operations - COMPLETE ✅*
- ConditionalCheckFailed error type
- Conditional put: put_conditional(), put_conditional_with_sk()
- Conditional update: update_conditional() in LSM, Update.condition() in API
- Conditional delete: delete_conditional(), delete_conditional_with_sk()
- Condition evaluation before write operations (get → evaluate → write)
- Common patterns: attribute_not_exists() for put-if-not-exists, attribute_exists() for update-if-exists
- Update builder condition support: `.condition("age = :old_age")`
- Failed conditions return ConditionalCheckFailed error
- New tests: test_database_put_if_not_exists, test_database_update_with_condition, test_database_delete_with_condition, test_database_conditional_attribute_exists
- All tests passing (99 core + 27 API = 126 tests + 6 integration = 132 total)

*Phase 2.6 Batch Operations - COMPLETE ✅*
- BatchGetRequest/Response for retrieving multiple items
- BatchWriteRequest/Response for putting/deleting multiple items
- batch_get() in LSM: get multiple keys in one call
- batch_write() in LSM: put/delete multiple items in one call
- Builder API: `.add_key()`, `.put()`, `.delete()` for fluent construction
- Returns only found items (missing keys excluded from response)
- Processed count tracking for write operations
- New module: batch.rs (API)
- New tests: test_batch_get_builder, test_batch_write_builder, test_database_batch_get, test_database_batch_write, test_database_batch_write_mixed
- All tests passing (99 core + 32 API = 131 tests + 6 integration = 137 total)

*Phase 2.7 Transactions - COMPLETE ✅*
- TransactGetRequest/Response for atomic reads (consistent snapshot)
- TransactWriteRequest/Response for atomic writes with conditions
- TransactWriteOperation enum (Put, Update, Delete, ConditionCheck)
- transact_get() in LSM: read multiple items under read lock
- transact_write() in LSM: two-phase commit (check all conditions → execute all writes)
- TransactionCanceled error type for failed transactions
- ACID guarantees: all operations succeed or all fail atomically
- Condition checks without writes (ConditionCheck operation)
- Mixed operations: put, update, delete, condition_check in single transaction
- Builder API: `.get()`, `.put()`, `.update()`, `.delete()`, `.condition_check()`
- Expression context shared across all transaction operations
- New module: transaction.rs (API)
- New tests: test_transact_get_builder, test_transact_write_builder, test_transact_write_with_conditions, test_database_transact_get_basic, test_database_transact_get_missing_items, test_database_transact_write_puts, test_database_transact_write_with_condition_success, test_database_transact_write_condition_failure, test_database_transact_write_mixed_operations, test_database_transact_write_condition_check_only, test_database_transact_write_atomicity
- All tests passing (99 core + 43 API = 142 tests + 6 integration = 148 total)

**Phase 2 (Complete Dynamo API) - COMPLETE ✅**
All sub-phases 2.1-2.7 implemented with full DynamoDB-compatible API.

*Phase 3.1 Local Secondary Indexes (LSI) - COMPLETE ✅*
- Index infrastructure: LocalSecondaryIndex, IndexProjection, TableSchema
- Index key encoding with 0xFF marker to distinguish from base records
- Manifest UpdateSchema record type for storing table schema
- Automatic LSI materialization during writes (put creates index entries)
- LSI query support: Query::new(pk).index("index-name").sk_condition()
- Index key format: [0xFF | index_name_len | index_name | pk | index_sk]
- Supports String, Number, Binary, Bool, Timestamp as index sort keys
- Full projection support (All attributes stored in index by default)
- Stripe routing: index records stored in same stripe as base record
- Database::create_with_schema() to create table with indexes
- Query builder .index() method to query by LSI instead of base table
- All query features work with indexes (pagination, limits, conditions)
- New module: index.rs (core)
- New tests: test_lsi_*, test_database_create_with_lsi, test_database_query_by_lsi, test_database_query_lsi_with_condition
- All tests passing (105 core + 46 API = 151 tests + 6 integration = 157 total)

*Phase 3.2 Global Secondary Indexes (GSI) - COMPLETE ✅*
- GlobalSecondaryIndex struct with partition_key_attribute and optional sort_key_attribute
- Builder methods: GlobalSecondaryIndex::new(), with_sort_key(), keys_only(), include()
- TableSchema methods: add_global_index(), get_global_index()
- Automatic GSI materialization during writes (put creates GSI index entries)
- GSI index key format: [0xFF | index_name | gsi_pk | gsi_sk + base_pk] for uniqueness
- GSI stripe routing: based on GSI partition key value (not base table PK)
- Enables cross-partition queries (different base PKs, same GSI PK)
- Query support: Query::new(gsi_pk).index("gsi-name").sk_condition()
- Same query path handles both LSI and GSI (unified index query logic)
- All projection types supported (All, KeysOnly, Include)
- Extended index.rs with GSI builders and tests
- New tests: test_gsi_*, test_database_create_with_gsi, test_database_query_by_gsi, test_database_query_gsi_with_sort_key_condition, test_database_gsi_different_stripes
- All tests passing (109 core + 50 API = 159 tests + 6 integration = 165 total)

*Phase 3.3 Time To Live (TTL) - COMPLETE ✅*
- TableSchema.ttl_attribute_name field for configuring TTL attribute
- TableSchema.with_ttl(attribute_name) builder method
- TableSchema.is_expired(item) method checks if item is expired
- Lazy deletion: expired items filtered during get(), query(), scan()
- Supports both Number (seconds since epoch) and Timestamp (milliseconds) value types
- Items without TTL attribute never expire
- Automatic deletion on read: get() deletes expired items and returns None
- Query and scan automatically filter out expired items
- Extended index.rs with TTL schema methods and 6 unit tests
- New tests: test_ttl_*, test_database_ttl_lazy_deletion, test_database_ttl_query_filter, test_database_ttl_scan_filter, test_database_ttl_no_ttl_attribute, test_database_ttl_timestamp_value_type
- All tests passing (115 core + 55 API = 170 tests + 6 integration = 176 total)

*Phase 3.4 Streams (Change Data Capture) - COMPLETE ✅*
- StreamRecord struct captures item-level changes (INSERT, MODIFY, REMOVE)
- StreamViewType enum controls what data is included (KeysOnly, NewImage, OldImage, NewAndOldImages)
- StreamConfig for enabling/configuring streams with buffer size
- TableSchema.stream_config field and with_stream() builder method
- In-memory circular buffer (VecDeque) stores recent changes
- Automatic stream record emission during put() and delete() operations
- LsmEngine.read_stream(after_sequence_number) for reading records
- Database.read_stream() API method
- Supports polling for new changes via sequence number filtering
- New module: stream.rs with 7 unit tests
- New tests: test_stream_*, test_database_stream_insert, test_database_stream_modify, test_database_stream_remove, test_database_stream_view_type_keys_only, test_database_stream_after_sequence, test_database_stream_buffer_limit
- All tests passing (122 core + 61 API = 183 tests + 6 integration = 189 total)

**Phase 3 (Indexes) - COMPLETE ✅**
All sub-phases 3.1-3.4 implemented with complete DynamoDB-style secondary indexes, TTL, and streams.

**Future phases** (not yet implemented):

**Phase 4: PartiQL Compatibility**
Add SQL-compatible query language support for DynamoDB-style operations.

*Phase 4.1 PartiQL Parser*
- Implement SQL parser supporting SELECT, INSERT, UPDATE, DELETE
- Lexer for SQL tokens (keywords, identifiers, operators, literals)
- Recursive descent parser for PartiQL grammar
- AST representation for queries and DML statements

*Phase 4.2 Query Translation*
- Convert PartiQL SELECT to KeystoneDB Query/Scan operations
- WHERE clause parsing for partition key and sort key conditions
- Support for index hints (query LSI/GSI)
- Pagination support (LIMIT, continuation tokens)

*Phase 4.3 DML Translation*
- Map INSERT to put operations
- Map UPDATE to update operations with SET/REMOVE/ADD
- Map DELETE to delete operations
- Single-item constraints (PartiQL doesn't support bulk DELETE WHERE)

*Phase 4.4 Expression Mapping*
- Translate WHERE clauses to existing expression system
- Convert SQL comparison operators to expression AST
- Handle attribute names and value placeholders
- Support for AND/OR/NOT logical operators

*Phase 4.5 CLI Integration*
- Add `kstone query <path> '<partiql>'` command
- ExecuteStatement API (single query)
- BatchExecuteStatement API (batch queries)
- Result formatting (table/JSON output)

**PartiQL Example Usage:**
```bash
# SELECT with WHERE clause
kstone query mydb.keystone "SELECT * FROM items WHERE pk = 'user#123'"

# SELECT with index
kstone query mydb.keystone "SELECT * FROM items.email-index WHERE pk = 'org#acme' AND email = 'alice@example.com'"

# INSERT
kstone query mydb.keystone "INSERT INTO items VALUE {'pk': 'user#999', 'name': 'Alice', 'age': 30}"

# UPDATE
kstone query mydb.keystone "UPDATE items SET age = 31 WHERE pk = 'user#999'"

# DELETE
kstone query mydb.keystone "DELETE FROM items WHERE pk = 'user#999'"
```

**Phase 5: In-Memory Database**
Provide memory-only database mode for testing and temporary data.

*Phase 5.1 Storage Abstraction*
- Create StorageBackend trait (abstract disk vs memory)
- Separate WAL backend (disk file vs memory buffer)
- Separate SST backend (disk file vs memory map)
- Refactor LsmEngine to use storage abstraction

*Phase 5.2 In-Memory Implementation*
- MemoryWal: WAL stored in Vec<Record> (no disk writes)
- MemorySst: SST stored in Vec<Record> (no disk files)
- All LSM operations work in-memory
- No persistence, data lost when database closed

*Phase 5.3 Database Mode Selection*
- `Database::create_in_memory()` API
- `Database::create_in_memory_with_schema(schema)` API
- Optional: `Database::snapshot_to_disk(path)` for exporting
- Optional: `Database::restore_from_disk(path)` for importing

*Phase 5.4 Test Utilities*
- Helper functions for creating test databases
- Performance benchmarks comparing disk vs memory
- Migration tools for disk → memory (load entire DB into RAM)

**In-Memory Database Example Usage:**
```rust
use kstone_api::Database;

// Create in-memory database (no disk I/O)
let db = Database::create_in_memory()?;

// Same API as disk-based database
db.put(b"user#123", item)?;
let result = db.get(b"user#123")?;

// Optional: snapshot to disk
db.snapshot_to_disk("backup.keystone")?;
```

**Phase 6: Network Layer & gRPC Server - COMPLETE ✅**

*Phase 6.1 Protocol Definition - COMPLETE ✅*
- Protocol Buffers (proto3) service definition
- 11 RPC methods: Put, Get, Delete, Query, Scan, BatchGet, BatchWrite, TransactGet, TransactWrite, Update, ExecuteStatement
- Bidirectional type conversions (kstone-proto crate)
- All value types supported (S, N, B, Bool, Null, L, M, VecF32, Ts)

*Phase 6.2 Server Implementation - COMPLETE ✅*
- gRPC service implementation (kstone-server crate)
- Put/Get/Delete fully implemented
- Query with sort key conditions (eq, lt, lte, gt, gte, between, begins_with)
- Scan with server-side streaming and parallel segments
- BatchGet/BatchWrite operations
- Error mapping from KeystoneDB to gRPC Status codes
- Server binary with CLI (`kstone-server --db-path <path> --port 50051`)

*Phase 6.3 Stubbed Methods*
- TransactGet/TransactWrite return UNIMPLEMENTED (requires transaction coordinator)
- Update returns UNIMPLEMENTED (requires update expression parsing)
- ExecuteStatement returns UNIMPLEMENTED (requires PartiQL parser)

**Phase 7: Interactive CLI (Not Yet Implemented)**
REPL-style interactive query editor with autocomplete.

*Phase 7.1 REPL Infrastructure*
- Add rustyline or reedline dependency for line editing
- `kstone shell <path>` command to enter interactive mode
- Prompt with database info (path, table schema)
- Graceful exit with Ctrl+D or `.exit` command

*Phase 7.2 Autocomplete Engine*
- Context-aware tab completion
- PartiQL keyword completion (SELECT, FROM, WHERE, INSERT, UPDATE, DELETE)
- Table attribute name completion (from schema inspection)
- Index name completion (LSI/GSI names)
- Function name completion (attribute_exists, begins_with, etc.)

*Phase 7.3 Query History*
- Persistent command history across sessions
- Up/down arrow navigation through history
- Ctrl+R reverse search in history
- `.history` command to show recent queries

*Phase 7.4 Result Formatting*
- Pretty-print query results in table format
- JSON output mode (`.format json`)
- Compact mode for large result sets
- Color-coded output (optional, with termcolor)
- Item count and timing statistics

*Phase 7.5 Multi-line Support*
- Detect incomplete PartiQL queries
- Continue prompt for multi-line input
- Semicolon terminates query
- Backslash for line continuation

**Interactive CLI Example Usage:**
```bash
$ kstone shell mydb.keystone

KeystoneDB Interactive Shell v0.4.0
Database: mydb.keystone
Type .help for commands, .exit to quit

kstone> SELECT * FROM items WHERE pk = 'user#123'
┌─────────────┬───────┬─────┬────────┐
│ pk          │ name  │ age │ active │
├─────────────┼───────┼─────┼────────┤
│ user#123    │ Alice │ 30  │ true   │
└─────────────┴───────┴─────┴────────┘
1 row (12.3ms)

kstone> .indexes
Local Secondary Indexes:
  - email-index (sort_key: email)
  - score-index (sort_key: score)

Global Secondary Indexes:
  - status-index (partition_key: status)

kstone> SELECT * FROM items.status-index WHERE pk = 'active'<TAB>
                                                              ^
                                                              [autocomplete: LIMIT, AND, OR]

kstone> .exit
Goodbye!
```

**Meta-commands for Interactive CLI:**
- `.help` - Show available commands
- `.schema` - Display table schema (indexes, TTL, streams)
- `.indexes` - List all indexes (LSI/GSI)
- `.format <table|json|compact>` - Set output format
- `.history` - Show command history
- `.timer <on|off>` - Show/hide query timing
- `.exit` or `.quit` - Exit shell

**Phase 8+: Attachment Framework**
- DynamoDB sync (bidirectional replication)
- Remote KeystoneDB sync (peer-to-peer replication)
- Cloud integration and sync strategies

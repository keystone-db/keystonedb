# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

KeystoneDB is a single-file, embedded, DynamoDB-style database written in Rust. Currently in **Phase 0 (Walking Skeleton)** - a minimal but complete end-to-end implementation with Put/Get/Delete operations, persistent storage, and crash recovery.

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

## Architecture

### Workspace Structure
This is a Cargo workspace with 4 crates:
- **kstone-core**: Storage engine internals (WAL, SST, LSM)
- **kstone-api**: Public API wrapping the core engine
- **kstone-cli**: Command-line binary
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

*Phase 2.7 Transactions - TODO*

**Future phases** (not yet implemented):
- Phase 3: Indexes (GSI, LSI, TTL, Streams)
- Phase 4+: Attachment framework (DynamoDB sync, remote KeystoneDB sync)

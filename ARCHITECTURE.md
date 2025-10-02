# KeystoneDB Architecture

This document describes the internal design and implementation details of KeystoneDB.

## Table of Contents

1. [Overview](#overview)
2. [Storage Engine](#storage-engine)
3. [Data Flow](#data-flow)
4. [Compaction](#compaction)
5. [Indexes](#indexes)
6. [PartiQL Processing](#partiql-processing)
7. [Concurrency Model](#concurrency-model)
8. [File Formats](#file-formats)

## Overview

KeystoneDB is a single-file embedded database that implements a DynamoDB-compatible API using an LSM (Log-Structured Merge-tree) storage engine. The architecture prioritizes write performance, crash recovery, and efficient point queries.

```
┌─────────────────────────────────────────────────────────────┐
│                        kstone-api                           │
│  (Public API: Put/Get/Query/Scan/Batch/Transactions)       │
└─────────────────────────────────────────────────────────────┘
                            │
┌─────────────────────────────────────────────────────────────┐
│                       kstone-core                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │  LSM Engine  │  │  PartiQL     │  │  Compaction  │     │
│  │  (256 stripes)  │  │  Parser      │  │  Manager     │     │
│  └──────────────┘  └──────────────┘  └──────────────┘     │
│         │                                     │             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐     │
│  │  Memtable    │  │     WAL      │  │     SST      │     │
│  │  (BTreeMap)  │  │  (Append-only│  │  (Immutable) │     │
│  └──────────────┘  └──────────────┘  └──────────────┘     │
└─────────────────────────────────────────────────────────────┘
                            │
                     ┌──────────────┐
                     │  Filesystem  │
                     │  (.keystone/ │
                     │   directory) │
                     └──────────────┘
```

## Storage Engine

### LSM Tree Architecture

KeystoneDB uses a **256-stripe LSM tree** design for parallelism and performance:

- **Stripe Selection**: `stripe = crc32(partition_key) % 256`
- **Independent Operation**: Each stripe has its own:
  - Memtable (in-memory BTreeMap)
  - WAL file (`wal_<stripe>.log`)
  - SST files (`<stripe>_<timestamp>.sst`)
  - Compaction schedule

**Benefits:**
- Parallel writes to different stripes
- Independent flush and compaction
- Better cache locality
- Reduced lock contention

### Components

#### 1. Memtable

**Purpose**: In-memory buffer for recent writes

**Implementation**: `BTreeMap<EncodedKey, Record>`
- Keys are encoded with partition key + sort key lengths
- Records contain operation type (Put/Delete), item data, and sequence number
- Sorted by key for efficient range queries

**Lifecycle:**
```
Write → Memtable (in-memory) → Flush (when threshold reached) → SST file
```

**Flush Threshold**: 1000 records per stripe (configurable via `MEMTABLE_THRESHOLD`)

#### 2. Write-Ahead Log (WAL)

**Purpose**: Durability and crash recovery

**Design:**
- One WAL file per stripe: `<dbdir>/wal_<stripe>.log`
- Append-only, sequential writes
- Group commit for efficiency (multiple writes batched per fsync)
- Rotated after memtable flush (old WAL deleted, new created)

**Format:**
```
[Header: magic(4) version(4) reserved(8)]
[Record: lsn(8) len(4) bincode_data crc(4)]*
```

**Recovery Process:**
1. On database open, scan all WAL files
2. Replay records into memtables
3. Determine next sequence number from max LSN
4. Resume normal operation

#### 3. Sorted String Table (SST)

**Purpose**: Immutable on-disk storage

**Characteristics:**
- Sorted by key for binary search
- Named `<stripe>_<timestamp>.sst`
- Contains bloom filter for fast negative lookups
- Checksum for integrity verification

**Format:**
```
[Header: magic(4) version(4) count(4) reserved(4)]
[Records: (len(4) bincode_record)*]
[CRC: crc(4)]
```

**Read Path:**
```
Query → Check memtable → Check SSTs (newest to oldest)
                        → Binary search within SST
                        → Bloom filter for early exit
```

#### 4. Bloom Filters

**Purpose**: Avoid disk reads for non-existent keys

**Design:**
- Per-SST bloom filter stored in memory
- Configurable false positive rate (default: ~1%)
- Hash count optimized for filter size
- Built during SST creation

**Usage:**
```rust
// Check bloom filter before reading SST
if !sst.bloom.may_contain(&key) {
    // Key definitely not in SST, skip
    continue;
}
// Key might be in SST, perform binary search
```

### Key Encoding

Keys are encoded to support composite keys and efficient comparison:

```rust
// Composite key: [pk_len(4) | pk_bytes | sk_len(4) | sk_bytes]
struct Key {
    pk: Bytes,      // Partition key
    sk: Option<Bytes>,  // Optional sort key
}
```

**Encoding:**
1. Write partition key length (little-endian u32)
2. Write partition key bytes
3. If sort key exists:
   - Write sort key length (little-endian u32)
   - Write sort key bytes

**Benefits:**
- Efficient lexicographic ordering
- Supports range queries on sort key
- Compatible with binary search

## Data Flow

### Write Path

```
┌──────────────┐
│ API: put()   │
└──────┬───────┘
       │
       ▼
┌──────────────────────┐
│ 1. Acquire write lock│
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 2. Assign SeqNo      │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 3. Append to WAL     │
│    (group commit)    │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 4. Insert to Memtable│
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 5. Check threshold   │
│    (1000 records?)   │
└──────┬───────────────┘
       │
       ▼ (if threshold exceeded)
┌──────────────────────┐
│ 6. Flush to SST      │
│ 7. Rotate WAL        │
│ 8. Clear Memtable    │
└──────────────────────┘
```

**Key Points:**
- Writes are serialized per stripe (write lock)
- WAL ensures durability before acknowledging write
- Memtable flush is automatic and transparent
- No write amplification until compaction

### Read Path

```
┌──────────────┐
│ API: get()   │
└──────┬───────┘
       │
       ▼
┌──────────────────────┐
│ 1. Acquire read lock │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 2. Check Memtable    │
│    (newest data)     │
└──────┬───────────────┘
       │
       │ (if not found)
       ▼
┌──────────────────────┐
│ 3. Check SSTs        │
│    (newest to oldest)│
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ For each SST:        │
│ - Check bloom filter │
│ - Binary search      │
│ - Return if found    │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ Return result        │
└──────────────────────┘
```

**Optimization:**
- Memtable check is O(log n) in-memory
- Bloom filters eliminate most SST reads
- SSTs scanned in reverse chronological order (newest first)
- Newer versions shadow older ones

### Query Path

```
┌─────────────────┐
│ API: query()    │
│ (with pk + sk   │
│  condition)     │
└────────┬────────┘
         │
         ▼
┌────────────────────────┐
│ 1. Determine key range │
│    from conditions     │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. Scan memtable       │
│    (range query)       │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 3. Scan SSTs           │
│    (binary search for  │
│     range start)       │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 4. Merge results       │
│    (deduplicate by     │
│     sequence number)   │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 5. Apply sort key      │
│    filter & limit      │
└────────────────────────┘
```

### Scan Path

```
┌─────────────────┐
│ API: scan()     │
│ (full table or  │
│  filtered)      │
└────────┬────────┘
         │
         ▼
┌────────────────────────┐
│ 1. Determine segments  │
│    (parallel scan:     │
│     segment N of M)    │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 2. For each stripe in  │
│    segment:            │
│    - Scan memtable     │
│    - Scan SSTs         │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 3. Merge and dedupe    │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 4. Apply filter expr   │
│    (if provided)       │
└────────┬───────────────┘
         │
         ▼
┌────────────────────────┐
│ 5. Apply limit         │
└────────────────────────┘
```

**Parallel Scan:**
- Segments divide stripes: segment N processes stripes where `stripe % total_segments == segment_id`
- Each segment can run in parallel
- Total parallelism: up to 256 segments

## Compaction

### Purpose

Compaction reclaims space and improves read performance by:
- Removing tombstones (deleted records)
- Deduplicating multiple versions of same key
- Reducing number of SST files per stripe

### Compaction Manager

**Architecture:**
- Runs in background thread spawned on database open
- Checks compaction needs every 5 seconds
- Operates on one stripe at a time

**Trigger Conditions:**
```rust
// Stripe needs compaction if:
sst_count >= 4  // At least 4 SST files
```

### Compaction Process

```
┌──────────────────────┐
│ 1. Select stripe     │
│    needing compaction│
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 2. Acquire write lock│
│    on stripe         │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 3. Collect all SSTs  │
│    for stripe        │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 4. Merge records     │
│    (keep latest      │
│     version by seqno)│
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 5. Filter tombstones │
│    (remove deletes)  │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 6. Write new SST     │
│    (compacted)       │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 7. Update manifest   │
│    (atomic swap)     │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│ 8. Delete old SSTs   │
└──────────────────────┘
```

**Benefits:**
- Space reclamation from deleted records
- Fewer SST files = faster queries
- Better bloom filter effectiveness
- Improved read performance

**Trade-offs:**
- Write amplification (records rewritten)
- Temporary disk space usage (old + new SSTs)
- CPU and I/O overhead during compaction

## Indexes

### Local Secondary Index (LSI)

**Purpose:** Query same partition key with different sort key

**Design:**
- Index records stored in same stripes as base table
- Key encoding: `[pk | lsi_name | lsi_sk | base_sk]`
- Shares partition key with base table
- Up to 5 LSIs per table

**Example:**
```rust
// Base table: pk="user#123", sk="profile"
// LSI "by-email": pk="user#123", lsi_sk="alice@example.com"
// Encoded key: "user#123|by-email|alice@example.com|profile"
```

**Query Flow:**
```
Query LSI → Translate to index key range
         → Scan for index records
         → Extract base table keys
         → Fetch base items
         → Return results
```

### Global Secondary Index (GSI)

**Purpose:** Query by non-key attributes

**Design:**
- Index records in independent stripe (not tied to base table stripe)
- Stripe selection: `crc32(gsi_pk) % 256` (different from base table)
- Key encoding: `[gsi_pk | gsi_sk | base_pk | base_sk]`
- Can span multiple partitions of base table

**Example:**
```rust
// Base table: pk="user#123", sk="profile"
// GSI "by-status": gsi_pk="active", gsi_sk="2024-01-01"
// Encoded key: "active|2024-01-01|user#123|profile"
```

**Query Flow:**
```
Query GSI → Stripe from gsi_pk (may be different)
         → Scan for index records
         → Extract base table keys
         → Fetch from base table (different stripes)
         → Return results
```

### Index Projections

Both LSI and GSI support projection types:
- **KEYS_ONLY**: Index contains only key attributes
- **INCLUDE**: Index contains keys + specified attributes
- **ALL**: Index contains full copy of item

**Trade-offs:**
- KEYS_ONLY: Smallest index, requires base table lookup
- ALL: Largest index, no base table lookup needed
- INCLUDE: Balance between size and fetch performance

## PartiQL Processing

### Pipeline

```
SQL String
    │
    ▼
┌────────────────┐
│  sqlparser-rs  │  (SQL parsing)
└────┬───────────┘
     │
     ▼
┌────────────────┐
│   Validator    │  (DynamoDB constraints)
│                │  - Partition key required for Query
│                │  - Full key for Update/Delete
└────┬───────────┘
     │
     ▼
┌────────────────┐
│   Translator   │  (AST → KeystoneDB ops)
│                │  - SELECT → Query/Scan
│                │  - INSERT → Put
│                │  - UPDATE → Update
│                │  - DELETE → Delete
└────┬───────────┘
     │
     ▼
┌────────────────┐
│   Executor     │  (Execute operation)
└────────────────┘
```

### Query Optimization

**Query Type Determination:**
```rust
// SELECT with pk = value → Query operation
SELECT * FROM users WHERE pk = 'user#123'

// SELECT with pk IN (values) → MultiGet operation
SELECT * FROM users WHERE pk IN ('user#123', 'user#456')

// SELECT without pk or pk with non-equality → Scan operation
SELECT * FROM users WHERE age > 25
```

**Processing Order for SELECT:**
1. Execute Query/Scan
2. Apply filter conditions (for Scan)
3. Apply OFFSET (skip N items)
4. Apply LIMIT (take N items)
5. Apply projection (filter attributes)

### PartiQL Extensions

KeystoneDB supports DynamoDB-specific PartiQL syntax:

```sql
-- INSERT with object literal
INSERT INTO users VALUE {'pk': 'user#123', 'name': 'Alice'}

-- UPDATE with arithmetic
UPDATE users SET age = age + 1 WHERE pk = 'user#123'

-- UPDATE with REMOVE
UPDATE users SET visits = visits + 1 REMOVE temp_field WHERE pk = 'user#123'
```

## Concurrency Model

### Locking Strategy

```
LsmEngine
    │
    ├── RwLock<Engine>  (read/write separation)
    │       │
    │       ├── Multiple readers (get/query/scan)
    │       │
    │       └── Single writer (put/delete/update)
    │
    └── Wal
            │
            └── Mutex<File>  (serialized writes)
```

**Key Points:**
- **Read Lock**: Multiple concurrent readers allowed
- **Write Lock**: Exclusive access for mutations
- **WAL Mutex**: Serializes appends for group commit
- **Per-Stripe**: Each stripe has independent locks

### Read Concurrency

```rust
// Multiple readers can proceed in parallel
thread 1: db.get(key1)?  ─┐
thread 2: db.get(key2)?  ─┼─ All acquire read lock
thread 3: db.query(...)?  ─┘
```

### Write Serialization

```rust
// Writes to same stripe are serialized
thread 1: db.put(key1, item1)?  ─┐
thread 2: db.put(key2, item2)?  ─┼─ Sequential (write lock)
thread 3: db.delete(key3)?      ─┘

// Writes to different stripes can be parallel
thread 1: db.put(key_stripe_0, ...)?  ─┐
thread 2: db.put(key_stripe_1, ...)?  ─┼─ Parallel (different stripes)
thread 3: db.put(key_stripe_2, ...)?  ─┘
```

### Group Commit

**WAL writes are batched for efficiency:**

```rust
// Multiple put operations can share single fsync
db.put(key1, item1)?  ─┐
db.put(key2, item2)?  ─┼─ Single WAL append + fsync
db.put(key3, item3)?  ─┘
```

**Implementation:**
- Writer thread acquires WAL mutex
- Appends records to buffer
- Flushes to disk (fsync)
- Releases mutex
- Other writers waiting on mutex benefit from fsync

## File Formats

### Endianness

**All integers use little-endian encoding** (except magic numbers which are big-endian for human readability).

### WAL Format

```
┌─────────────────────────────────────────────────┐
│ Header                                          │
│  - magic: 0x57414C00 (4 bytes, big-endian)     │
│  - version: 1 (4 bytes, little-endian)         │
│  - reserved: 0 (8 bytes)                        │
├─────────────────────────────────────────────────┤
│ Record 1                                        │
│  - lsn: sequence number (8 bytes, LE)          │
│  - len: data length (4 bytes, LE)              │
│  - data: bincode-encoded record                 │
│  - crc: CRC32C checksum (4 bytes, LE)          │
├─────────────────────────────────────────────────┤
│ Record 2                                        │
│  ...                                            │
└─────────────────────────────────────────────────┘
```

**Record Data** (bincode-encoded):
```rust
struct Record {
    key: EncodedKey,
    record_type: RecordType,  // Put or Delete
    seqno: u64,
    item: Option<Item>,  // None for deletes
}
```

### SST Format

```
┌─────────────────────────────────────────────────┐
│ Header                                          │
│  - magic: 0x53535400 (4 bytes, big-endian)     │
│  - version: 1 (4 bytes, little-endian)         │
│  - record_count: N (4 bytes, LE)               │
│  - reserved: 0 (4 bytes)                        │
├─────────────────────────────────────────────────┤
│ Record 1                                        │
│  - len: record length (4 bytes, LE)            │
│  - data: bincode-encoded record                 │
├─────────────────────────────────────────────────┤
│ Record 2                                        │
│  ...                                            │
├─────────────────────────────────────────────────┤
│ Record N                                        │
├─────────────────────────────────────────────────┤
│ CRC: CRC32C of all records (4 bytes, LE)       │
└─────────────────────────────────────────────────┘
```

**Properties:**
- Records sorted by encoded key
- Binary searchable
- Bloom filter stored separately in memory
- Immutable once written

### Manifest Format

The manifest tracks active SST files per stripe:

```rust
struct Manifest {
    stripes: HashMap<StripeId, Vec<SstFile>>,
}

struct SstFile {
    name: String,
    record_count: usize,
    min_seqno: u64,
    max_seqno: u64,
}
```

**Operations:**
- **Flush**: Add new SST to stripe
- **Compact**: Replace multiple SSTs with one compacted SST
- **Checkpoint**: Persist manifest to disk (every N operations)

### Database Directory Structure

```
mydb.keystone/
├── wal_000.log                  # WAL for stripe 0
├── wal_001.log                  # WAL for stripe 1
├── ...
├── wal_255.log                  # WAL for stripe 255
├── 000_1704067200000.sst       # SST for stripe 0, timestamp
├── 000_1704070800000.sst       # Another SST for stripe 0
├── 001_1704067200000.sst       # SST for stripe 1
├── ...
└── manifest.json                # Manifest (if persisted)
```

**Naming Conventions:**
- WAL: `wal_<stripe_id>.log` (stripe ID is 0-padded to 3 digits)
- SST: `<stripe_id>_<timestamp>.sst` (timestamp in milliseconds since epoch)

## Summary

KeystoneDB's architecture emphasizes:

1. **Write Performance**: LSM tree with WAL and group commit
2. **Read Performance**: Memtable + bloom filters + binary search
3. **Parallelism**: 256 stripes for concurrent operations
4. **Durability**: WAL ensures crash recovery
5. **Space Efficiency**: Background compaction removes obsolete data
6. **Flexibility**: DynamoDB-compatible API + PartiQL SQL interface

The design trade-offs favor write-heavy workloads while maintaining good read performance through caching (memtable) and optimization (bloom filters, binary search).

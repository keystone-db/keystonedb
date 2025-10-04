# Chapter 6: LSM Tree Architecture

The Log-Structured Merge tree (LSM tree) is the storage engine architecture that powers KeystoneDB. Understanding how the LSM tree works is fundamental to understanding KeystoneDB's performance characteristics, trade-offs, and operational behavior. This chapter provides a comprehensive exploration of LSM trees in general and KeystoneDB's 256-stripe implementation specifically.

## What is an LSM Tree?

A Log-Structured Merge tree is a write-optimized data structure designed for high-throughput write workloads while maintaining reasonable read performance. Unlike B-trees (used in traditional databases like MySQL and PostgreSQL), LSM trees prioritize write performance by deferring expensive disk updates.

### The Core Insight

The fundamental insight behind LSM trees is simple: **writes to disk are expensive, so minimize them by batching**.

Traditional B-tree approach:
```
Write → Find page on disk → Read page → Modify → Write page back
```
Every write requires at least one disk read and one disk write, plus potential tree rebalancing.

LSM tree approach:
```
Write → Append to log → Insert into memory → Periodically flush to disk
```
Writes are fast (in-memory operations), and disk writes are batched for efficiency.

### Historical Context

LSM trees were invented by Patrick O'Neil et al. in 1996 in the paper "The Log-Structured Merge-Tree (LSM-Tree)." The design was motivated by the growing gap between sequential and random I/O performance on hard drives.

**Key innovations:**
1. **Write-ahead log (WAL)**: Durability without random writes
2. **In-memory buffer**: Fast writes to memory
3. **Batched flushes**: Amortize disk write cost across many records
4. **Compaction**: Merge sorted files to bound read amplification

**Modern LSM tree databases:**
- Google Bigtable / LevelDB / RocksDB
- Apache Cassandra / HBase
- Amazon DynamoDB
- ScyllaDB
- KeystoneDB

## LSM Tree Components

An LSM tree has three main components:

1. **Memtable**: In-memory sorted data structure (recent writes)
2. **Write-Ahead Log (WAL)**: Durable log of all writes (crash recovery)
3. **Sorted String Tables (SSTs)**: Immutable on-disk sorted files

```
┌─────────────────────────────────────────┐
│              Memtable                   │
│   (In-memory BTreeMap, sorted)         │
│                                         │
│   [key1 → value1]                       │
│   [key2 → value2]                       │
│   [key3 → value3]                       │
└─────────────────────────────────────────┘
                 ↓
         (Flush when full)
                 ↓
┌─────────────────────────────────────────┐
│          SST Files (on disk)            │
│                                         │
│  SST-1: [key1, key3, key5, ...]        │
│  SST-2: [key2, key4, key6, ...]        │
│  SST-3: [key1, key7, key9, ...]        │
└─────────────────────────────────────────┘
                 ↓
         (Periodic compaction)
                 ↓
┌─────────────────────────────────────────┐
│       Compacted SST Files               │
│                                         │
│  SST-4: [key1, key2, key3, ...]        │
│  (Deduplicated, tombstones removed)     │
└─────────────────────────────────────────┘
```

Additionally, there's a **Write-Ahead Log (WAL)** that runs in parallel:

```
┌─────────────────────────────────────────┐
│         Write-Ahead Log (WAL)           │
│                                         │
│  [LSN:1, Record1]                       │
│  [LSN:2, Record2]                       │
│  [LSN:3, Record3]                       │
│  ...                                    │
└─────────────────────────────────────────┘
```

## KeystoneDB's 256-Stripe LSM Implementation

KeystoneDB extends the classic LSM tree with a **256-stripe architecture**. Instead of one global LSM tree, KeystoneDB maintains 256 independent LSM trees (stripes), each with its own memtable and SST files.

### Stripe Structure

Each stripe is a complete mini LSM tree:

```rust
struct Stripe {
    memtable: BTreeMap<Vec<u8>, Record>,  // In-memory sorted records
    memtable_size_bytes: usize,           // Approximate size tracking
    ssts: Vec<SstReader>,                 // SST files (newest first)
}
```

**256 stripes means:**
- 256 independent memtables
- 256 independent SST file collections
- 256 independent flush thresholds
- 1 shared WAL (for crash recovery)
- 1 global sequence number counter

### LsmEngine Structure

The top-level `LsmEngine` coordinates all stripes:

```rust
pub struct LsmEngine {
    inner: Arc<RwLock<LsmInner>>,
}

struct LsmInner {
    dir: PathBuf,                 // Database directory
    wal: Wal,                     // Shared write-ahead log
    stripes: Vec<Stripe>,         // 256 stripes
    next_seq: SeqNo,              // Global sequence number
    next_sst_id: u64,             // Global SST ID counter
    schema: TableSchema,          // Table schema (indexes, TTL, streams)
    // ... other fields
}
```

### File Layout

On disk, a KeystoneDB database looks like this:

```
mydb.keystone/
├── wal.log          # Shared write-ahead log
├── 000-1.sst        # Stripe 0, SST 1
├── 000-2.sst        # Stripe 0, SST 2
├── 042-1.sst        # Stripe 42, SST 1
├── 042-2.sst        # Stripe 42, SST 2
├── 137-1.sst        # Stripe 137, SST 1
└── ...              # More SST files
```

Each SST file belongs to exactly one stripe and is named `{stripe:03}-{sst_id}.sst`.

## Memtable: The In-Memory Buffer

The memtable is where all writes land initially. It's an in-memory sorted data structure that provides fast lookups and ordered iteration.

### Data Structure: BTreeMap

KeystoneDB uses Rust's `BTreeMap` for memtables:

```rust
memtable: BTreeMap<Vec<u8>, Record>
```

**Why BTreeMap?**

1. **Sorted order**: Keys are always kept sorted (for SST generation)
2. **Logarithmic operations**: Insert, lookup, delete are O(log n)
3. **Range iteration**: Can iterate over key ranges efficiently
4. **Standard library**: Well-tested, optimized implementation

**Alternative considered:**
- **HashMap**: Faster lookups (O(1)), but no ordering
- **SkipList**: Similar performance, more complex
- **Red-Black Tree**: Similar to BTreeMap

### Memtable Operations

**Insert:**
```rust
let key_enc = record.key.encode().to_vec();
stripe.memtable.insert(key_enc, record);
```

**Lookup:**
```rust
let key_enc = key.encode().to_vec();
if let Some(record) = stripe.memtable.get(&key_enc) {
    return Ok(record.value.clone());
}
```

**Iteration:**
```rust
for (key_enc, record) in &stripe.memtable {
    // Process records in sorted order
}
```

### Memtable Size Tracking

KeystoneDB tracks both record count and estimated byte size:

```rust
struct Stripe {
    memtable: BTreeMap<Vec<u8>, Record>,
    memtable_size_bytes: usize,  // Estimated size in bytes
}
```

**Size estimation:**
```rust
fn estimate_record_size(key_enc: &[u8], record: &Record) -> usize {
    let mut size = key_enc.len();              // Key size
    size += std::mem::size_of::<SeqNo>();      // Sequence number

    if let Some(item) = &record.value {
        for (attr_name, value) in item {
            size += attr_name.len();
            size += match value {
                Value::S(s) => s.len(),
                Value::N(n) => n.len(),
                Value::B(b) => b.len(),
                Value::VecF32(v) => v.len() * 4,  // 4 bytes per f32
                Value::Ts(_) => 8,                 // i64
                // ... other types
            };
        }
    }

    size
}
```

### Flush Threshold

KeystoneDB flushes a stripe's memtable when it reaches a threshold:

```rust
const MEMTABLE_THRESHOLD: usize = 1000;  // Records per stripe

fn should_flush_stripe(&self, stripe_id: usize) -> bool {
    let stripe = &self.stripes[stripe_id];

    // Check record count
    if stripe.memtable.len() >= MEMTABLE_THRESHOLD {
        return true;
    }

    // Optionally check byte size
    if let Some(max_bytes) = self.config.max_memtable_size_bytes {
        if stripe.memtable_size_bytes >= max_bytes {
            return true;
        }
    }

    false
}
```

**Why 1000 records?**
- **Balance**: Not too small (excessive flushes), not too large (high memory usage)
- **Predictable**: Each stripe flushes independently at consistent size
- **Configurable**: Future enhancement will make this user-configurable

## Write-Ahead Log (WAL): Durability

The WAL ensures durability: if KeystoneDB crashes, no committed writes are lost.

### WAL Purpose

Without a WAL, memtable data would be lost on crash:

```
1. Write to memtable (in memory)
2. [CRASH]
3. Restart → memtable is gone → data lost!
```

With a WAL:

```
1. Append to WAL (durable on disk)
2. Write to memtable (in memory)
3. [CRASH]
4. Restart → replay WAL → memtable reconstructed → no data lost!
```

### WAL Structure

KeystoneDB's WAL is a simple append-only log:

```
[Header: magic(4) | version(4) | reserved(8)]
[Record: lsn(8) | len(4) | data(bincode) | crc32c(4)]
[Record: lsn(8) | len(4) | data(bincode) | crc32c(4)]
[Record: lsn(8) | len(4) | data(bincode) | crc32c(4)]
...
```

**Record format:**
- `lsn`: Log Sequence Number (monotonically increasing)
- `len`: Length of serialized record data
- `data`: Bincode-serialized `Record` (key + value + sequence number)
- `crc32c`: Checksum for corruption detection

### Write Path with WAL

Every write follows this pattern:

```rust
pub fn put(&self, key: Key, item: Item) -> Result<()> {
    let mut inner = self.inner.write();

    // 1. Assign sequence number
    let seq = inner.next_seq;
    inner.next_seq += 1;

    // 2. Create record
    let record = Record::put(key.clone(), item, seq);

    // 3. Append to WAL (durability point)
    inner.wal.append(record.clone())?;
    inner.wal.flush()?;  // fsync to disk

    // 4. Update memtable (fast, in-memory)
    let stripe_id = key.stripe() as usize;
    let key_enc = key.encode().to_vec();
    inner.stripes[stripe_id].memtable.insert(key_enc, record);

    // 5. Check flush threshold
    if should_flush_stripe(stripe_id) {
        flush_stripe(stripe_id)?;
    }

    Ok(())
}
```

**Critical ordering:**
1. WAL write BEFORE memtable update
2. WAL flush (fsync) BEFORE returning success
3. Memtable update after WAL is durable

### WAL Recovery

On database open, the WAL is replayed:

```rust
pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
    // 1. Open WAL file
    let wal = Wal::open(&wal_path)?;

    // 2. Read all records from WAL
    let records = wal.read_all()?;

    // 3. Replay records into memtables
    let mut stripes = vec![Stripe::new(); 256];
    let mut max_seq = 0;

    for (_lsn, record) in records {
        max_seq = max_seq.max(record.seq);
        let stripe_id = record.key.stripe() as usize;
        let key_enc = record.key.encode().to_vec();
        stripes[stripe_id].memtable.insert(key_enc, record);
    }

    // 4. Resume with next sequence number
    let next_seq = max_seq + 1;

    Ok(Self { ... })
}
```

### WAL Rotation

After a memtable flush, the corresponding WAL entries are no longer needed:

```rust
fn flush_stripe(&mut self, stripe_id: usize) -> Result<()> {
    // 1. Write memtable to SST file
    let sst_path = self.dir.join(format!("{:03}-{}.sst", stripe_id, sst_id));
    let mut writer = SstWriter::new();
    for record in self.stripes[stripe_id].memtable.values() {
        writer.add(record.clone());
    }
    writer.finish(&sst_path)?;

    // 2. Clear memtable
    self.stripes[stripe_id].memtable.clear();

    // 3. WAL can be truncated/rotated (future enhancement)
    // For now, WAL grows indefinitely and is replayed on restart
}
```

**Future enhancement:** WAL compaction to remove entries for records that have been flushed.

## Sorted String Tables (SSTs): On-Disk Storage

SSTs are immutable on-disk files containing sorted records. They are the persistent storage layer of the LSM tree.

### SST File Format

KeystoneDB SST files have a simple structure:

```
[Header: magic(4) | version(4) | count(4) | reserved(4)]
[Records: (len(4) | bincode_record)*]
[CRC: crc32c(4)]
```

**Key properties:**
- **Sorted**: Records sorted by encoded key
- **Immutable**: Once written, never modified
- **Versioned**: Can evolve format while maintaining compatibility
- **Checksummed**: CRC32C for corruption detection

### Creating an SST

SSTs are created by flushing memtables:

```rust
fn flush_stripe(&mut self, stripe_id: usize) -> Result<()> {
    let sst_id = self.next_sst_id;
    self.next_sst_id += 1;

    let sst_path = self.dir.join(format!("{:03}-{}.sst", stripe_id, sst_id));

    // Create SST writer
    let mut writer = SstWriter::new();

    // Add all records from memtable (already sorted)
    for record in self.stripes[stripe_id].memtable.values() {
        writer.add(record.clone());
    }

    // Write to disk
    writer.finish(&sst_path)?;

    // Load the new SST for reading
    let reader = SstReader::open(&sst_path)?;
    self.stripes[stripe_id].ssts.insert(0, reader);  // Newest first

    // Clear memtable
    self.stripes[stripe_id].memtable.clear();

    Ok(())
}
```

### Reading from SSTs

SSTs are read during lookups when the memtable doesn't contain the key:

```rust
pub fn get(&self, key: &Key) -> Result<Option<Item>> {
    let inner = self.inner.read();
    let stripe_id = key.stripe() as usize;
    let stripe = &inner.stripes[stripe_id];

    // 1. Check memtable first
    let key_enc = key.encode().to_vec();
    if let Some(record) = stripe.memtable.get(&key_enc) {
        return Ok(record.value.clone());
    }

    // 2. Check SSTs (newest to oldest)
    for sst in &stripe.ssts {
        if let Some(record) = sst.get(key) {
            return Ok(record.value.clone());  // First match wins
        }
    }

    // 3. Not found
    Ok(None)
}
```

**Why newest to oldest?**
- Newer SSTs contain more recent versions of keys
- If a key exists in multiple SSTs, the newest version is the current one
- Deletions (tombstones) in newer SSTs override values in older SSTs

### SST Bloom Filters (Phase 1.4+)

To avoid reading SSTs that definitely don't contain a key, KeystoneDB uses **Bloom filters**:

```rust
// Check bloom filter before reading SST
if !sst.bloom.contains(&key.encode()) {
    // Key definitely not in this SST, skip
    continue;
}

// Bloom filter says "maybe", read SST
if let Some(record) = sst.read_from_disk(key) {
    return Ok(record.value.clone());
}
```

**Bloom filter properties:**
- ~1% false positive rate (10 bits per key)
- Saves disk reads for keys that don't exist
- Critical for read performance with many SSTs

## Relationship Between Memtable, WAL, and SSTs

These three components work together to provide the LSM tree's properties:

### Write Path

```
User write (put/delete)
    ↓
[1] Append to WAL (durability)
    ↓
[2] Insert into memtable (fast in-memory)
    ↓
[3] Check flush threshold
    ↓
[4] If threshold reached:
        → Write memtable to SST (batched disk write)
        → Clear memtable
        → (Future: Truncate WAL)
```

### Read Path

```
User read (get)
    ↓
[1] Check memtable (newest data)
    ↓
[2] If found → return
    ↓
[3] If not found, check SSTs (oldest to newest)
    ↓
[4] For each SST:
        → Check bloom filter (fast negative lookup)
        → If maybe present, read from disk
        → If found → return (first match wins)
    ↓
[5] Not found in any SST → return None
```

### Recovery Path

```
Database crash
    ↓
Restart and open database
    ↓
[1] Read WAL from disk
    ↓
[2] Replay all records into memtables
    ↓
[3] Load existing SST files
    ↓
[4] Resume operations with recovered state
```

## The Write Path in Detail

Let's trace a write operation through KeystoneDB's LSM tree:

### Example: Writing a User Record

```rust
let user = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .build();

db.put(b"user#12345", user)?;
```

**Step-by-step execution:**

**1. API Layer** (`kstone-api`):
```rust
pub fn put(&self, pk: &[u8], item: Item) -> Result<()> {
    let key = Key::new(Bytes::copy_from_slice(pk));
    self.engine.put(key, item)
}
```

**2. LSM Engine** (`kstone-core`):
```rust
pub fn put(&self, key: Key, item: Item) -> Result<()> {
    let mut inner = self.inner.write();  // Acquire write lock

    // Assign global sequence number
    let seq = inner.next_seq;
    inner.next_seq += 1;  // seq = 12345

    // Create record
    let record = Record::put(key.clone(), item, seq);

    // WAL append (durability)
    inner.wal.append(record.clone())?;
    inner.wal.flush()?;  // fsync!

    // Determine stripe
    let stripe_id = key.stripe() as usize;  // CRC32(b"user#12345") % 256 = 147

    // Insert into memtable
    let key_enc = key.encode().to_vec();
    inner.stripes[147].memtable.insert(key_enc, record);

    // Check flush threshold
    if inner.stripes[147].memtable.len() >= 1000 {
        self.flush_stripe(&mut inner, 147)?;
    }

    Ok(())
}  // Release write lock
```

**3. If flush triggered:**
```rust
fn flush_stripe(&self, inner: &mut LsmInner, stripe_id: usize) -> Result<()> {
    let sst_id = inner.next_sst_id;
    inner.next_sst_id += 1;

    // File: 147-23.sst (stripe 147, SST ID 23)
    let sst_path = inner.dir.join(format!("{:03}-{}.sst", stripe_id, sst_id));

    // Write SST
    let mut writer = SstWriter::new();
    for record in inner.stripes[stripe_id].memtable.values() {
        writer.add(record.clone());
    }
    writer.finish(&sst_path)?;

    // Load SST reader
    let reader = SstReader::open(&sst_path)?;
    inner.stripes[stripe_id].ssts.insert(0, reader);

    // Clear memtable
    inner.stripes[stripe_id].memtable.clear();

    Ok(())
}
```

**Timeline:**
```
Time  | Operation                           | Durability
------|-------------------------------------|------------
t0    | Acquire write lock                  | -
t1    | Assign seq = 12345                  | -
t2    | Append to WAL                       | -
t3    | fsync WAL                           | ✓ Durable
t4    | Insert into stripe 147 memtable     | -
t5    | Check threshold (999 < 1000)        | -
t6    | Release write lock                  | -
```

**Key observations:**
- Write lock held during entire operation (serializes writes)
- Durability achieved at WAL fsync (t3)
- Memtable update is fast (in-memory)
- Flush happens asynchronously when threshold reached

## The Read Path in Detail

Let's trace a read operation:

### Example: Reading a User Record

```rust
let user = db.get(b"user#12345")?;
```

**Step-by-step execution:**

**1. API Layer:**
```rust
pub fn get(&self, pk: &[u8]) -> Result<Option<Item>> {
    let key = Key::new(Bytes::copy_from_slice(pk));
    self.engine.get(&key)
}
```

**2. LSM Engine:**
```rust
pub fn get(&self, key: &Key) -> Result<Option<Item>> {
    let inner = self.inner.read();  // Acquire read lock

    // Determine stripe
    let stripe_id = key.stripe() as usize;  // 147
    let stripe = &inner.stripes[stripe_id];

    // 1. Check memtable (newest data)
    let key_enc = key.encode().to_vec();
    if let Some(record) = stripe.memtable.get(&key_enc) {
        return Ok(record.value.clone());  // Cache hit!
    }

    // 2. Check SSTs (newest to oldest)
    for sst in &stripe.ssts {
        // Check bloom filter first
        if !sst.bloom.contains(&key_enc) {
            continue;  // Definitely not in this SST
        }

        // Bloom says "maybe", read from disk
        if let Some(record) = sst.get(key) {
            return Ok(record.value.clone());  // Found!
        }
    }

    // 3. Not found anywhere
    Ok(None)
}  // Release read lock
```

**Timeline (cache miss, 3 SSTs):**
```
Time  | Operation                           | I/O
------|-------------------------------------|-----
t0    | Acquire read lock                   | -
t1    | Calculate stripe (147)              | -
t2    | Check memtable (miss)               | -
t3    | Check SST-1 bloom (negative)        | -
t4    | Check SST-2 bloom (false positive)  | -
t5    | Read SST-2 from disk (miss)         | Disk
t6    | Check SST-3 bloom (positive)        | -
t7    | Read SST-3 from disk (hit!)         | Disk
t8    | Release read lock                   | -
```

**Key observations:**
- Read lock allows concurrent reads (no serialization)
- Memtable check is very fast (BTreeMap lookup)
- Bloom filters save unnecessary disk reads
- False positives cause wasted disk reads (but rare: ~1%)
- Read cost increases with number of SSTs (compaction mitigates)

## Benefits and Trade-offs of LSM Trees

Understanding the trade-offs helps you use KeystoneDB effectively.

### Benefits

**1. Write Performance**
- Writes are sequential (append to WAL, append to memtable)
- No random disk writes during normal operation
- Batched flushes amortize disk I/O cost
- Write throughput scales with stripe count

**2. Compressibility**
- SSTs are immutable, ideal for compression
- Block-based compression achieves high ratios
- Reduces storage cost and I/O bandwidth

**3. Crash Recovery**
- WAL provides simple, robust crash recovery
- No complex undo/redo logs
- Deterministic recovery process

**4. Horizontal Scalability (256 stripes)**
- Independent stripes reduce contention
- Parallel flushes improve throughput
- Natural sharding for multi-core systems

### Trade-offs

**1. Write Amplification**
- Each write appears in: WAL, memtable, SST, compacted SST
- Amplification factor typically 10-30x
- More writes = more disk I/O and wear

**2. Read Amplification**
- Must check memtable + N SST files
- Worst case: read N SST files for non-existent key
- Bloom filters help but don't eliminate all reads
- Compaction reduces SST count but adds overhead

**3. Space Amplification**
- Old versions of data remain until compaction
- Deleted records (tombstones) consume space
- SST overhead (bloom filters, indexes, metadata)
- Typical overhead: 20-50% of logical data size

**4. Compaction Overhead**
- Background compaction consumes CPU and I/O
- Can interfere with foreground operations
- Complex scheduling and tuning required
- Trade-off between read performance and write overhead

### Performance Characteristics

**Writes:**
- **Throughput**: Very high (10,000+ writes/sec per stripe)
- **Latency**: Low, consistent (WAL fsync dominates)
- **Scaling**: Linear with stripe count (to a point)

**Reads (hot data):**
- **Throughput**: High (memtable lookups)
- **Latency**: Very low (<1ms)
- **Cache hit rate**: Critical for performance

**Reads (cold data):**
- **Throughput**: Moderate (limited by SST reads)
- **Latency**: Variable (depends on SST count and bloom filters)
- **Compaction**: Improves read performance

**Scans:**
- **Sequential**: Good (SSTs are sorted)
- **Random**: Poor (requires seeking across SSTs)

## Summary

The LSM tree is the heart of KeystoneDB's storage engine. Its design prioritizes write performance while maintaining reasonable read performance through careful component design.

Key concepts:

1. **Three components**: Memtable (in-memory), WAL (durability), SSTs (persistent storage)
2. **256 stripes**: Independent LSM trees for parallelism and scalability
3. **Write-optimized**: Sequential writes, batched flushes, no in-place updates
4. **Read path**: Memtable → SSTs (newest to oldest), bloom filters optimize
5. **Trade-offs**: Write amplification, read amplification, space amplification
6. **Compaction**: Merges SSTs to reduce read cost (covered in next chapter)

Understanding the LSM tree architecture is essential for:
- Choosing appropriate data models
- Tuning performance for your workload
- Debugging performance issues
- Operating KeystoneDB in production

In the next chapter, we'll dive deep into the storage engine internals, exploring the WAL, SST files, and compaction process in implementation detail.

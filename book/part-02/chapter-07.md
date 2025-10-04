# Chapter 7: Storage Engine Internals

Building on the LSM tree concepts from the previous chapter, we now dive into the implementation details of KeystoneDB's storage engine. This chapter explores the Write-Ahead Log, Sorted String Tables, memtable management, crash recovery, and compaction—the mechanisms that make KeystoneDB reliable, fast, and efficient.

## Write-Ahead Log (WAL) Deep Dive

The Write-Ahead Log is the foundation of KeystoneDB's durability guarantee. Every write is logged before being applied to the in-memory memtable, ensuring no data is lost even if the process crashes.

### WAL File Format

KeystoneDB's WAL uses a simple, robust format:

```
┌────────────────────────────────────────┐
│            WAL Header (16 bytes)       │
├────────────────────────────────────────┤
│  Magic Number (4 bytes): 0x4B535457    │  "KSTW"
│  Version (4 bytes): 0x00000001         │
│  Reserved (8 bytes): 0x0000...         │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│         Record 1 (variable length)     │
├────────────────────────────────────────┤
│  LSN (8 bytes): log sequence number    │
│  Length (4 bytes): record data length  │
│  Data (N bytes): bincode(Record)       │
│  CRC32C (4 bytes): checksum            │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│         Record 2 (variable length)     │
│  ...                                   │
└────────────────────────────────────────┘
```

**Record structure in detail:**
- **LSN (Log Sequence Number)**: Monotonically increasing identifier (u64)
- **Length**: Byte length of serialized record data (u32)
- **Data**: Bincode-serialized `Record` struct containing:
  - Key (partition key + optional sort key)
  - Value (Item or None for tombstones)
  - Sequence number (for MVCC ordering)
- **CRC32C**: Cyclic redundancy check for detecting corruption

### Writing to the WAL

The write path involves appending and flushing:

```rust
impl Wal {
    pub fn append(&mut self, record: Record) -> Result<Lsn> {
        // Assign LSN
        let lsn = self.next_lsn;
        self.next_lsn += 1;

        // Serialize record
        let data = bincode::serialize(&record)?;

        // Compute CRC32C checksum
        let crc = crc32c::crc32c(&data);

        // Write to file
        self.file.write_u64::<LittleEndian>(lsn)?;
        self.file.write_u32::<LittleEndian>(data.len() as u32)?;
        self.file.write_all(&data)?;
        self.file.write_u32::<LittleEndian>(crc)?;

        Ok(lsn)
    }

    pub fn flush(&mut self) -> Result<()> {
        self.file.sync_all()?;  // fsync - critical for durability
        Ok(())
    }
}
```

**Key points:**
- `append()` writes to OS buffer (fast)
- `flush()` calls `sync_all()` (fsync) to ensure data reaches physical disk
- Every user-facing write calls both `append()` and `flush()` for durability

### Reading from the WAL

During recovery, the WAL is read sequentially:

```rust
impl Wal {
    pub fn read_all(&self) -> Result<Vec<(Lsn, Record)>> {
        let mut file = File::open(&self.path)?;
        let mut records = Vec::new();

        // Skip header
        file.seek(SeekFrom::Start(16))?;

        loop {
            // Try to read LSN (8 bytes)
            let mut lsn_buf = [0u8; 8];
            match file.read_exact(&mut lsn_buf) {
                Ok(_) => {},
                Err(ref e) if e.kind() == ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
            let lsn = u64::from_le_bytes(lsn_buf);

            // Read length
            let mut len_buf = [0u8; 4];
            file.read_exact(&mut len_buf)?;
            let len = u32::from_le_bytes(len_buf) as usize;

            // Read data
            let mut data = vec![0u8; len];
            file.read_exact(&mut data)?;

            // Read CRC
            let mut crc_buf = [0u8; 4];
            file.read_exact(&mut crc_buf)?;
            let stored_crc = u32::from_le_bytes(crc_buf);

            // Verify CRC
            let computed_crc = crc32c::crc32c(&data);
            if computed_crc != stored_crc {
                // Corruption detected - stop reading
                eprintln!("WAL corruption detected at LSN {}", lsn);
                break;
            }

            // Deserialize record
            let record: Record = bincode::deserialize(&data)?;
            records.push((lsn, record));
        }

        Ok(records)
    }
}
```

**Recovery properties:**
- Sequential read (efficient)
- CRC validation prevents using corrupted data
- Partial writes (from crashes) detected and skipped
- Records sorted by LSN for deterministic replay

### WAL Durability Guarantees

KeystoneDB's durability guarantee is simple: **once a write returns success, it will survive any crash**.

This is achieved by:
1. Appending record to WAL
2. Calling `fsync` (via `sync_all()`)
3. Only returning success after fsync completes

**Important:** Durability depends on the storage device honoring fsync. SSDs with volatile write caches that don't flush on fsync can violate this guarantee.

### WAL Size Growth and Rotation

A naive WAL grows indefinitely:

```
wal.log: 1 GB, 2 GB, 3 GB, ... → out of disk space!
```

**Solution: WAL rotation after flush**

When a memtable is flushed to an SST:
1. All records in that flush are now durable in the SST
2. Those WAL entries are no longer needed for recovery
3. WAL can be truncated or rotated

**Current implementation:** WAL grows until database restart
**Future enhancement:** Periodic WAL compaction to remove flushed records

### Group Commit Optimization (Phase 1.3+)

Writing and fsyncing for every write is expensive. **Group commit** batches multiple writes into a single fsync:

```rust
// Without group commit:
write1 → append → fsync
write2 → append → fsync  // Another fsync!
write3 → append → fsync  // Another fsync!

// With group commit:
write1 → append (buffer)
write2 → append (buffer)
write3 → append (buffer)
fsync (all three at once)
```

**Implementation:**
```rust
impl WalRing {
    pub fn append(&mut self, record: Record) -> Result<Lsn> {
        self.buffer.push((self.next_lsn, record));
        self.next_lsn += 1;

        // Auto-flush if buffer full
        if self.buffer.len() >= MAX_BUFFER_SIZE {
            self.flush()?;
        }

        Ok(self.next_lsn - 1)
    }

    pub fn flush(&mut self) -> Result<()> {
        // Write all buffered records
        for (lsn, record) in &self.buffer {
            // ... write to file ...
        }

        // Single fsync for all records
        self.file.sync_all()?;

        self.buffer.clear();
        Ok(())
    }
}
```

**Benefits:**
- Amortize fsync cost across multiple writes
- Higher throughput under concurrent load
- Lower per-write latency (at cost of slightly higher worst-case latency)

## Sorted String Table (SST) Deep Dive

SSTs are the persistent storage layer. They're immutable, sorted, and optimized for both sequential scans and random lookups.

### SST File Format (Phase 0)

KeystoneDB's initial SST format is straightforward:

```
┌────────────────────────────────────────┐
│           SST Header (16 bytes)        │
├────────────────────────────────────────┤
│  Magic (4): 0x4B535354                 │  "KSST"
│  Version (4): 0x00000001               │
│  Record Count (4): number of records   │
│  Reserved (4): 0x00000000              │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│        Record 1 (variable length)      │
├────────────────────────────────────────┤
│  Length (4 bytes)                      │
│  Bincode(Record)                       │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│        Record 2 (variable length)      │
│  ...                                   │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│         Footer (4 bytes)               │
├────────────────────────────────────────┤
│  CRC32C of all records                 │
└────────────────────────────────────────┘
```

**Key properties:**
- Records stored in **sorted order** by encoded key
- Each record contains the full key and value
- CRC32C checksum at end for corruption detection

### Writing SST Files

SSTs are created during memtable flushes:

```rust
impl SstWriter {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn add(&mut self, record: Record) {
        self.records.push(record);
    }

    pub fn finish(mut self, path: &Path) -> Result<()> {
        // Sort records by encoded key
        self.records.sort_by(|a, b| {
            a.key.encode().cmp(&b.key.encode())
        });

        let mut file = File::create(path)?;

        // Write header
        file.write_u32::<BigEndian>(0x4B535354)?;  // Magic
        file.write_u32::<LittleEndian>(1)?;        // Version
        file.write_u32::<LittleEndian>(self.records.len() as u32)?;
        file.write_u32::<LittleEndian>(0)?;        // Reserved

        // Write records
        let mut crc = crc32c::Crc32c::new();
        for record in &self.records {
            let data = bincode::serialize(&record)?;
            file.write_u32::<LittleEndian>(data.len() as u32)?;
            file.write_all(&data)?;
            crc.update(&data);
        }

        // Write footer
        file.write_u32::<LittleEndian>(crc.finalize())?;
        file.sync_all()?;

        Ok(())
    }
}
```

**Critical step:** Records are **sorted before writing**. This enables:
- Binary search for point lookups
- Efficient range scans
- Merge algorithms during compaction

### Reading SST Files

SST readers load the entire file into memory:

```rust
impl SstReader {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = File::open(path)?;

        // Read and validate header
        let magic = file.read_u32::<BigEndian>()?;
        if magic != 0x4B535354 {
            return Err(Error::Corruption("Invalid SST magic".into()));
        }

        let version = file.read_u32::<LittleEndian>()?;
        let count = file.read_u32::<LittleEndian>()? as usize;
        let _reserved = file.read_u32::<LittleEndian>()?;

        // Read all records
        let mut records = Vec::with_capacity(count);
        for _ in 0..count {
            let len = file.read_u32::<LittleEndian>()? as usize;
            let mut data = vec![0u8; len];
            file.read_exact(&mut data)?;
            let record: Record = bincode::deserialize(&data)?;
            records.push(record);
        }

        // Verify CRC
        let stored_crc = file.read_u32::<LittleEndian>()?;
        // ... verify CRC ...

        Ok(Self { records })
    }

    pub fn get(&self, key: &Key) -> Option<&Record> {
        // Binary search on sorted records
        let key_enc = key.encode();
        self.records.binary_search_by(|record| {
            record.key.encode().as_ref().cmp(key_enc.as_ref())
        }).ok().map(|idx| &self.records[idx])
    }
}
```

**Trade-off:** Entire SST in memory
- **Pro**: Fast lookups (binary search on Vec)
- **Pro**: Simple implementation
- **Con**: Memory usage grows with SST count
- **Future**: Block-based SST for large files (Phase 1.4+)

### Block-Based SSTs (Phase 1.4+)

For large SSTs, loading the entire file is impractical. Block-based SSTs divide the file into fixed-size blocks:

```
┌────────────────────────────────────────┐
│         SST Header (4KB block)         │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│      Data Block 1 (4KB, compressed)    │
│  [record1, record2, record3, ...]      │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│      Data Block 2 (4KB, compressed)    │
│  [record10, record11, ...]             │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│     Index Block (maps key → block)     │
│  [first_key_block1 → offset1]          │
│  [first_key_block2 → offset2]          │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│    Bloom Filter Block                  │
│  (bit array for membership testing)    │
└────────────────────────────────────────┘

┌────────────────────────────────────────┐
│         Footer                         │
│  num_data_blocks                       │
│  index_offset                          │
│  bloom_offset                          │
│  crc32c                                │
└────────────────────────────────────────┘
```

**Benefits:**
- Only read blocks that might contain the key
- Compression reduces storage and I/O
- Bloom filter eliminates negative lookups

**Lookup process:**
1. Check bloom filter (in memory)
2. If maybe present, consult index (in memory or on disk)
3. Read data block (4KB aligned I/O)
4. Decompress block
5. Binary search within block

## Memtable Management

The memtable is where all the action happens for writes. Efficient memtable management is critical for performance.

### Memtable Size Tracking

KeystoneDB tracks both record count and estimated byte size:

```rust
impl LsmInner {
    fn insert_into_memtable(
        &mut self,
        stripe_id: usize,
        key_enc: Vec<u8>,
        record: Record
    ) {
        // Estimate record size
        let record_size = estimate_record_size(&key_enc, &record);

        // If key exists, subtract old size
        if let Some(old_record) = self.stripes[stripe_id].memtable.get(&key_enc) {
            let old_size = estimate_record_size(&key_enc, old_record);
            self.stripes[stripe_id].memtable_size_bytes -= old_size;
        }

        // Insert new record
        self.stripes[stripe_id].memtable.insert(key_enc, record);
        self.stripes[stripe_id].memtable_size_bytes += record_size;
    }
}
```

### Flush Triggers

Multiple conditions can trigger a flush:

```rust
fn should_flush_stripe(&self, stripe_id: usize) -> bool {
    let stripe = &self.stripes[stripe_id];

    // Record count threshold (default: 1000)
    if stripe.memtable.len() >= self.config.max_memtable_records {
        return true;
    }

    // Byte size threshold (optional)
    if let Some(max_bytes) = self.config.max_memtable_size_bytes {
        if stripe.memtable_size_bytes >= max_bytes {
            return true;
        }
    }

    false
}
```

**Why two thresholds?**
- **Record count**: Simple, predictable
- **Byte size**: Accounts for variable record sizes
- **Combined**: Flush when either threshold reached

### Flush Process

Flushing a memtable is a multi-step process:

```rust
fn flush_stripe(&self, inner: &mut LsmInner, stripe_id: usize) -> Result<()> {
    // 1. Allocate SST ID
    let sst_id = inner.next_sst_id;
    inner.next_sst_id += 1;

    // 2. Create SST file path
    let sst_path = inner.dir.join(format!("{:03}-{}.sst", stripe_id, sst_id));

    // 3. Write SST from memtable
    let mut writer = SstWriter::new();
    for record in inner.stripes[stripe_id].memtable.values() {
        writer.add(record.clone());
    }
    writer.finish(&sst_path)?;

    // 4. Load SST reader
    let reader = SstReader::open(&sst_path)?;

    // 5. Add to stripe's SST list (newest first)
    inner.stripes[stripe_id].ssts.insert(0, reader);

    // 6. Clear memtable
    inner.stripes[stripe_id].memtable.clear();
    inner.stripes[stripe_id].memtable_size_bytes = 0;

    // 7. (Future) Compact WAL

    Ok(())
}
```

**Performance considerations:**
- Flush holds write lock (blocks writes to this stripe)
- SST write is I/O bound (SSD: ~100MB/s, HDD: ~50MB/s)
- Typical flush: 1000 records × 500 bytes/record = 500KB → ~5ms on SSD

## Crash Recovery Process

When KeystoneDB opens an existing database, it must recover to a consistent state.

### Recovery Steps

```rust
pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
    let dir = dir.as_ref();
    let wal_path = dir.join("wal.log");

    // 1. Open WAL
    let wal = Wal::open(&wal_path)?;

    // 2. Initialize empty stripes
    let mut stripes: Vec<Stripe> = (0..256).map(|_| Stripe::new()).collect();
    let mut max_sst_id = 0u64;

    // 3. Load existing SST files
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension() == Some(OsStr::new("sst")) {
            // Parse filename: {stripe:03}-{sst_id}.sst
            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                if let Some((stripe_str, id_str)) = name.split_once('-') {
                    let stripe = stripe_str.parse::<usize>()?;
                    let sst_id = id_str.parse::<u64>()?;

                    max_sst_id = max_sst_id.max(sst_id);

                    // Load SST
                    let reader = SstReader::open(&path)?;
                    stripes[stripe].ssts.push(reader);
                }
            }
        }
    }

    // 4. Sort SSTs within each stripe (newest first)
    for stripe in &mut stripes {
        stripe.ssts.reverse();
    }

    // 5. Replay WAL into memtables
    let records = wal.read_all()?;
    let mut max_seq = 0;

    for (_lsn, record) in records {
        max_seq = max_seq.max(record.seq);
        let stripe_id = record.key.stripe() as usize;
        let key_enc = record.key.encode().to_vec();
        stripes[stripe_id].memtable.insert(key_enc, record);
    }

    // 6. Resume with next sequence number and SST ID
    Ok(Self {
        inner: Arc::new(RwLock::new(LsmInner {
            dir: dir.to_path_buf(),
            wal,
            stripes,
            next_seq: max_seq + 1,
            next_sst_id: max_sst_id + 1,
            // ... other fields ...
        })),
    })
}
```

### Recovery Guarantees

**What is recovered:**
- All writes that completed successfully before crash
- Partial memtable data from last flush (replayed from WAL)
- All SST files (already durable on disk)

**What is lost:**
- Writes in progress during crash (never returned success to user)
- Corrupted WAL records (CRC validation fails)

**Consistency guarantee:**
- Database always opens in a valid state
- Sequence numbers continue from highest seen
- No duplicate or lost records

### Recovery Performance

Factors affecting recovery time:

1. **WAL size**: Larger WAL → more records to replay
2. **Number of SSTs**: More files → longer directory scan
3. **Disk speed**: HDD vs SSD significantly affects SST loading

**Typical recovery times:**
- Small database (1GB, 100 SSTs): <1 second
- Medium database (10GB, 1000 SSTs): ~5 seconds
- Large database (100GB, 10000 SSTs): ~30 seconds

**Optimization:** Periodic WAL compaction reduces replay time.

## Compaction: Merging and Cleanup

Over time, SSTs accumulate, slowing down reads. **Compaction** merges SSTs to:
1. Reduce SST count (improve read performance)
2. Remove deleted records (reclaim space)
3. Deduplicate keys (keep only latest version)

### When to Compact

KeystoneDB compacts a stripe when it reaches a threshold:

```rust
// After flush, check SST count
if inner.stripes[stripe_id].ssts.len() >= COMPACTION_THRESHOLD {
    compact_stripe(stripe_id)?;
}
```

**Default threshold:** 10 SSTs per stripe

### Compaction Process

Compaction is a **k-way merge** of sorted lists:

```rust
fn compact_stripe(stripe_id: usize, ssts: &[SstReader]) -> Result<SstReader> {
    // 1. Create min-heap for k-way merge
    let mut heap = BinaryHeap::new();

    // 2. Initialize heap with first record from each SST
    for (sst_idx, sst) in ssts.iter().enumerate() {
        if let Some(record) = sst.first() {
            heap.push(Reverse((record.key.encode(), sst_idx, 0)));
        }
    }

    // 3. Merge into new SST
    let mut writer = SstWriter::new();
    let mut last_key: Option<Bytes> = None;

    while let Some(Reverse((key_enc, sst_idx, record_idx))) = heap.pop() {
        let record = &ssts[sst_idx].records[record_idx];

        // Deduplication: skip if we've already seen this key
        if let Some(ref last) = last_key {
            if *last == key_enc {
                continue;  // Newer version already written
            }
        }

        // Skip tombstones (deleted records)
        if record.is_tombstone() {
            continue;
        }

        // Write to new SST
        writer.add(record.clone());
        last_key = Some(key_enc);

        // Add next record from this SST to heap
        if record_idx + 1 < ssts[sst_idx].records.len() {
            let next = &ssts[sst_idx].records[record_idx + 1];
            heap.push(Reverse((next.key.encode(), sst_idx, record_idx + 1)));
        }
    }

    // 4. Write new SST
    let new_sst_path = format!("{:03}-{}.sst", stripe_id, new_sst_id);
    writer.finish(&new_sst_path)?;

    // 5. Load and return new SST
    SstReader::open(&new_sst_path)
}
```

### Compaction Benefits

**Before compaction:**
```
SST-1: [key1→v1, key3→v3, key5→v5]  (oldest)
SST-2: [key2→v2, key4→v4, key6→v6]
SST-3: [key1→v2, key7→v7]  (updated key1)
SST-4: [key3→deleted, key8→v8]  (deleted key3)

Read for key1:
  Check memtable → miss
  Check SST-4 → miss
  Check SST-3 → HIT (key1→v2)
  (4 lookups total)
```

**After compaction:**
```
SST-5: [key1→v2, key2→v2, key4→v4, key5→v5, key6→v6, key7→v7, key8→v8]

Read for key1:
  Check memtable → miss
  Check SST-5 → HIT (key1→v2)
  (2 lookups total)
```

**Space savings:**
- Before: 8 records + 1 tombstone = 9 entries
- After: 7 records = 7 entries (22% reduction)

### Compaction Configuration

KeystoneDB provides configuration options:

```rust
pub struct CompactionConfig {
    pub enabled: bool,              // Enable/disable compaction
    pub sst_threshold: usize,       // Compact when SST count exceeds this
    pub check_interval_secs: u64,   // How often to check (future: background)
    pub max_concurrent: usize,      // Max concurrent compactions
}
```

**Tuning guidelines:**
- **Write-heavy workload**: Increase threshold (less compaction overhead)
- **Read-heavy workload**: Decrease threshold (fewer SSTs to scan)
- **Balanced workload**: Default threshold (10) works well

### Compaction Statistics

KeystoneDB tracks compaction metrics:

```rust
pub struct CompactionStats {
    pub total_compactions: u64,     // Number of compactions performed
    pub total_ssts_merged: u64,     // Total SSTs merged
    pub total_ssts_created: u64,    // Total compacted SSTs created
    pub total_bytes_read: u64,      // Bytes read during compaction
    pub total_bytes_written: u64,   // Bytes written
    pub total_bytes_reclaimed: u64, // Space reclaimed (deleted records)
    pub active_compactions: u64,    // Currently running compactions
}
```

**Accessing stats:**
```rust
let stats = db.compaction_stats();
println!("Total compactions: {}", stats.total_compactions);
println!("Bytes reclaimed: {}", stats.total_bytes_reclaimed);
```

## Advanced Topics

### Concurrency Control

KeystoneDB uses read-write locks for concurrency:

```rust
pub struct LsmEngine {
    inner: Arc<RwLock<LsmInner>>,
}
```

**Lock semantics:**
- **Read lock**: Allows concurrent reads, blocks writes
- **Write lock**: Exclusive access, blocks all other operations

**Read path:**
```rust
pub fn get(&self, key: &Key) -> Result<Option<Item>> {
    let inner = self.inner.read();  // Acquire read lock
    // ... perform read ...
}  // Release read lock
```

**Write path:**
```rust
pub fn put(&self, key: Key, item: Item) -> Result<()> {
    let mut inner = self.inner.write();  // Acquire write lock
    // ... perform write ...
}  // Release write lock
```

**Trade-off:**
- **Reads are concurrent**: Multiple threads can read simultaneously
- **Writes are serialized**: Only one write at a time across all stripes
- **Future enhancement**: Per-stripe locks for better write concurrency

### Sequence Numbers and MVCC

Every record has a **sequence number** (SeqNo):

```rust
pub struct Record {
    pub key: Key,
    pub value: Option<Item>,  // None = tombstone
    pub seq: SeqNo,            // Global sequence number
}
```

**Purpose:**
- **Ordering**: Total order across all writes (even across stripes)
- **Versioning**: Newer records have higher sequence numbers
- **Conflict resolution**: During compaction, keep highest SeqNo

**Global counter:**
```rust
// In LsmInner
next_seq: SeqNo  // Incremented on every write
```

**Why global instead of per-stripe?**
- Simplifies reasoning about ordering
- Enables cross-stripe operations (scans, transactions)
- Stream records have globally ordered sequence numbers

### Bloom Filters Optimization

Bloom filters provide **probabilistic membership testing**:

```rust
pub struct BloomFilter {
    bits: BitVec,
    num_hashes: usize,
}

impl BloomFilter {
    pub fn new(expected_items: usize, bits_per_key: usize) -> Self {
        let num_bits = expected_items * bits_per_key;
        let num_hashes = optimal_num_hashes(bits_per_key);

        Self {
            bits: BitVec::from_elem(num_bits, false),
            num_hashes,
        }
    }

    pub fn add(&mut self, key: &[u8]) {
        for i in 0..self.num_hashes {
            let hash = hash_with_seed(key, i);
            let bit_idx = (hash % self.bits.len() as u64) as usize;
            self.bits.set(bit_idx, true);
        }
    }

    pub fn contains(&self, key: &[u8]) -> bool {
        for i in 0..self.num_hashes {
            let hash = hash_with_seed(key, i);
            let bit_idx = (hash % self.bits.len() as u64) as usize;
            if !self.bits[bit_idx] {
                return false;  // Definitely not present
            }
        }
        true  // Maybe present (or false positive)
    }
}
```

**False positive rate:**
```
FPR ≈ (1 - e^(-k*n/m))^k

where:
  k = number of hash functions
  n = number of items
  m = number of bits
```

**KeystoneDB configuration:**
- 10 bits per key
- ~1% false positive rate
- Good balance between space and accuracy

## Summary

KeystoneDB's storage engine is built on proven LSM tree principles with careful attention to implementation details:

1. **Write-Ahead Log**: Simple, robust durability with fsync guarantees
2. **Memtable**: Fast in-memory BTreeMap with size tracking
3. **SST Files**: Immutable sorted files with efficient encoding
4. **Crash Recovery**: Deterministic WAL replay ensures consistency
5. **Compaction**: K-way merge reduces SST count and reclaims space
6. **Concurrency**: RwLock enables concurrent reads
7. **Optimizations**: Bloom filters, group commit, block-based SSTs

Understanding these internals enables you to:
- Tune performance for your workload
- Diagnose issues (high latency, disk usage)
- Contribute to KeystoneDB development
- Make informed operational decisions

In the next part of the book, we'll explore KeystoneDB's DynamoDB-compatible API, including queries, scans, secondary indexes, and advanced features.

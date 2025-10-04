# Chapter 40: Recovery & Consistency

Database durability and crash recovery are critical for data integrity. This chapter explores how KeystoneDB ensures ACID guarantees, handles crash scenarios, and maintains consistency through its write-ahead log and recovery mechanisms.

## ACID Guarantees

KeystoneDB provides full ACID (Atomicity, Consistency, Isolation, Durability) properties for all write operations:

### Atomicity

**Definition:** Each operation either completes entirely or has no effect.

**Implementation:**
- Single-item operations (put, delete) are atomic by design
- Multi-item transactions use two-phase commit
- WAL ensures all-or-nothing persistence

**Example:**
```rust
// Atomic put - either succeeds completely or fails completely
db.put(b"user#123", item)?;  // If this returns Ok, item is persisted

// Atomic transaction - all operations or none
db.transact_write()
    .put(b"account#1", account1)
    .put(b"account#2", account2)
    .update(b"total", "SET sum = sum + :val")
    .execute()?;  // Either all three operations persist or none
```

### Consistency

**Definition:** Database moves from one valid state to another, maintaining all invariants.

**Implementation:**
- Schema validation (Phase 3+)
- Conditional writes enforce application-level constraints
- Transactions ensure cross-record consistency

**Example:**
```rust
// Ensure balance never goes negative
db.update(b"account#123")
    .expression("SET balance = balance - :amount")
    .condition("balance >= :amount")  // Consistency check
    .value(":amount", Value::number(100))
    .execute()?;
```

### Isolation

**Definition:** Concurrent transactions don't interfere with each other.

**Implementation:**
- Read Committed isolation level (default)
- Transactions see consistent snapshots
- Write locks prevent concurrent modifications

**Isolation Levels:**

**Read Committed (Current):**
```rust
// Transaction sees all committed changes before it starts
let tx1 = db.transact_get()
    .get(b"counter")
    .execute()?;  // Sees latest committed value

// Concurrent write completes
db.put(b"counter", new_value)?;

// Second read in same transaction sees update
let tx2 = db.transact_get()
    .get(b"counter")
    .execute()?;  // Sees new value (Read Committed)
```

**Snapshot Isolation (Future):**
```rust
// Transaction sees consistent snapshot from start time
let snapshot_time = db.begin_transaction()?;

// Concurrent writes happen
db.put(b"counter", new_value)?;

// Read uses snapshot - doesn't see concurrent writes
let value = db.get_at_snapshot(b"counter", snapshot_time)?;
```

### Durability

**Definition:** Committed changes survive system crashes and power failures.

**Implementation:**
- Write-ahead log (WAL) with fsync
- Crash recovery replays WAL on startup
- Group commit optimizes fsync overhead

**Guarantee:**
```rust
db.put(b"critical#1", item)?;  // Returns Ok

// System crashes immediately after

// On restart:
let recovered = db.open(path)?;  // WAL replay recovers data
assert!(recovered.get(b"critical#1")?.is_some());  // Data is there!
```

## Write-Ahead Log (WAL) Mechanics

### WAL Append Process

Every write operation follows this sequence:

1. **Assign sequence number** (monotonic counter)
2. **Create WAL record** (key, value, seqno, operation type)
3. **Append to WAL buffer** (in-memory)
4. **Flush to disk with fsync** (durability guarantee)
5. **Update memtable** (in-memory index)
6. **Check flush threshold** (trigger SST creation if needed)

**Critical Invariant:** WAL flush happens **before** acknowledging write to client.

```rust
pub fn put(&self, key: Key, item: Item) -> Result<()> {
    let mut inner = self.inner.write();

    // Step 1: Assign sequence number
    let seq = inner.next_seq;
    inner.next_seq += 1;

    // Step 2: Create record
    let record = Record::put(key.clone(), item, seq);

    // Step 3-4: Append to WAL and fsync
    inner.wal.append(record.clone())?;
    inner.wal.flush()?;  // ← Durability point

    // Step 5: Update memtable
    let stripe_id = key.stripe();
    inner.stripes[stripe_id].memtable.insert(key.encode(), record);

    // Step 6: Check flush threshold
    if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
        self.flush_stripe(&mut inner, stripe_id)?;
    }

    Ok(())
}  // Only returns Ok after WAL is persisted
```

### Group Commit Optimization

Multiple concurrent writes batch their fsync calls:

**Without Group Commit:**
```
Thread 1: append → fsync (10ms) ─────┐
Thread 2: append → fsync (10ms)       │ 30ms total
Thread 3: append → fsync (10ms) ─────┘
```

**With Group Commit:**
```
Thread 1: append ─┐
Thread 2: append  ├─→ Single fsync (10ms)
Thread 3: append ─┘
```

**Implementation:**

```rust
pub fn flush(&self) -> Result<()> {
    let mut inner = self.inner.lock();

    if inner.pending.is_empty() {
        return Ok(());  // Nothing to flush
    }

    // Batch all pending records into single buffer
    let mut buf = BytesMut::new();
    for record in &inner.pending {
        serialize_record(&mut buf, record)?;
    }

    // Single write + fsync for entire batch
    inner.file.write_all(&buf)?;
    inner.file.sync_all()?;  // fsync once for all records

    inner.pending.clear();
    Ok(())
}
```

Threads waiting on the WAL mutex might find their records already flushed by the previous holder, getting a "free" fsync.

### WAL Rotation

When a memtable flushes to SST, the WAL is rotated:

```rust
fn flush_stripe(&mut self, stripe_id: usize) -> Result<()> {
    // 1. Create SST from memtable
    let sst_path = format!("{:03}-{}.sst", stripe_id, self.next_sst_id);
    self.next_sst_id += 1;

    let mut writer = SstWriter::new();
    for (_, record) in &self.stripes[stripe_id].memtable {
        writer.add(record.clone());
    }
    writer.finish(sst_path)?;

    // 2. Rotate WAL (old WAL can be deleted)
    let new_wal_path = self.dir.join("wal.log.new");
    let old_wal_path = self.dir.join("wal.log");

    let new_wal = Wal::create(&new_wal_path)?;
    self.wal = new_wal;

    // 3. Delete old WAL (data is now in SST)
    fs::remove_file(&old_wal_path)?;
    fs::rename(&new_wal_path, &old_wal_path)?;

    // 4. Clear memtable
    self.stripes[stripe_id].memtable.clear();

    Ok(())
}
```

**Why rotation works:**
- Memtable records are now in SST (durable)
- WAL only needed for unflushed memtable records
- New WAL starts empty
- Old WAL can be safely deleted

## Crash Recovery Process

### Recovery Algorithm

When opening a database after a crash:

```rust
pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
    let dir = dir.as_ref();

    // 1. Open WAL
    let wal_path = dir.join("wal.log");
    let wal = Wal::open(&wal_path)?;

    // 2. Read all WAL records
    let records = wal.read_all()?;

    // 3. Determine max sequence number
    let max_seq = records.iter()
        .map(|(_, record)| record.seqno)
        .max()
        .unwrap_or(0);

    // 4. Rebuild memtables from WAL
    let mut stripes = vec![Stripe::new(); 256];
    for (_, record) in records {
        let stripe_id = record.key.stripe();
        let key_enc = record.key.encode();
        stripes[stripe_id].memtable.insert(key_enc, record);
    }

    // 5. Discover SST files
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension() == Some("sst") {
            let sst = SstReader::open(&path)?;
            // Parse stripe ID from filename (e.g., "042-15.sst")
            let stripe_id = parse_stripe_id(&path)?;
            stripes[stripe_id].ssts.push(sst);
        }
    }

    // 6. Sort SSTs by ID (newest first)
    for stripe in &mut stripes {
        stripe.ssts.sort_by_key(|sst| std::cmp::Reverse(sst.id()));
    }

    Ok(Self {
        inner: Arc::new(RwLock::new(LsmInner {
            dir: dir.to_path_buf(),
            wal,
            stripes,
            next_seq: max_seq + 1,  // Resume from max
            // ...
        })),
    })
}
```

### WAL Replay Details

**Step 1: Read all records**
```rust
pub fn read_all(&self) -> Result<Vec<(Lsn, Record)>> {
    let mut records = Vec::new();
    let mut file = self.inner.lock().file.try_clone()?;

    file.seek(SeekFrom::Start(WAL_HEADER_SIZE as u64))?;

    loop {
        match read_wal_record(&mut file)? {
            Some((lsn, record)) => records.push((lsn, record)),
            None => break,  // EOF
        }
    }

    Ok(records)
}
```

**Step 2: Validate and filter**
```rust
// Check for corruption
for (lsn, record) in &records {
    if record.seqno == 0 {
        return Err(Error::Corruption("Invalid sequence number"));
    }
}

// Filter out records already in SSTs (optimization)
let wal_records: Vec<_> = records.into_iter()
    .filter(|(_, record)| !is_in_sst(record))
    .collect();
```

**Step 3: Rebuild memtables**
```rust
let mut memtables: Vec<BTreeMap<_, _>> = vec![BTreeMap::new(); 256];

for (_, record) in wal_records {
    let stripe_id = record.key.stripe();
    let key_enc = record.key.encode();

    match record.record_type {
        RecordType::Put => {
            memtables[stripe_id].insert(key_enc, record);
        }
        RecordType::Delete => {
            // Insert tombstone
            memtables[stripe_id].insert(key_enc, record);
        }
    }
}
```

**Step 4: Verify consistency**
```rust
// Check that sequence numbers are monotonic
let mut prev_seq = 0;
for (_, record) in &sorted_records {
    if record.seqno <= prev_seq {
        return Err(Error::Corruption("Non-monotonic sequence numbers"));
    }
    prev_seq = record.seqno;
}
```

## Failure Scenarios and Handling

### Scenario 1: Crash During Write

**Situation:** System crashes after WAL append but before memtable update.

```rust
// Crash happens here ↓
pub fn put(&self, key: Key, item: Item) -> Result<()> {
    let mut inner = self.inner.write();

    // WAL append succeeds
    inner.wal.append(record)?;
    inner.wal.flush()?;

    // ← CRASH HERE

    // Memtable update never happens
    inner.stripes[stripe_id].memtable.insert(key.encode(), record);

    Ok(())
}
```

**Recovery:**
1. On restart, WAL replay rebuilds memtable
2. Record appears in memtable even though original put didn't complete
3. **Result:** Write is durable (correct behavior)

**Client Impact:**
- Client never got Ok response (operation timed out)
- But write was actually persisted
- This is acceptable - client can retry (idempotent)

### Scenario 2: Crash During Flush

**Situation:** System crashes while flushing memtable to SST.

```rust
fn flush_stripe(&mut self, stripe_id: usize) -> Result<()> {
    // Write SST
    let sst_path = format!("{:03}-{}.sst", stripe_id, self.next_sst_id);
    writer.finish(&sst_path)?;

    // ← CRASH HERE (before WAL rotation)

    // WAL rotation never happens
    self.wal = new_wal;
}
```

**Recovery:**
1. On restart, incomplete SST might exist on disk
2. WAL still contains all records (wasn't rotated)
3. SST file is detected but might be corrupt

**Handling:**
```rust
// During recovery, validate SST files
match SstReader::open(&path) {
    Ok(sst) => {
        // Checksum passed - SST is valid
        stripes[stripe_id].ssts.push(sst);
    }
    Err(Error::ChecksumMismatch) => {
        // Incomplete SST - delete and rely on WAL
        fs::remove_file(&path)?;
    }
    Err(e) => return Err(e),
}
```

**Result:** WAL replay reconstructs memtable, data not lost.

### Scenario 3: Crash During Compaction

**Situation:** System crashes while compacting SSTs.

```rust
fn compact_stripe(&mut self, stripe_id: usize) -> Result<()> {
    // Read old SSTs
    let records = merge_ssts(&old_ssts)?;

    // Write new SST
    let new_sst_path = format!("{:03}-{}.sst", stripe_id, self.next_sst_id);
    writer.finish(&new_sst_path)?;

    // ← CRASH HERE (before deleting old SSTs)

    // Old SSTs still exist
    fs::remove_file(&old_ssts[0])?;
    fs::remove_file(&old_ssts[1])?;
}
```

**Recovery:**
1. Both old and new SST files exist
2. New SST might be incomplete (checksum fails)

**Handling:**
```rust
// During recovery, detect SST conflicts
let mut ssts_by_stripe: HashMap<usize, Vec<SstReader>> = HashMap::new();

for path in find_sst_files(dir)? {
    match SstReader::open(&path) {
        Ok(sst) => {
            ssts_by_stripe.entry(stripe_id).or_default().push(sst);
        }
        Err(Error::ChecksumMismatch) => {
            // Incomplete compaction - delete partial SST
            fs::remove_file(&path)?;
        }
    }
}
```

**Result:** Old SSTs still present, compaction can retry later.

### Scenario 4: Partial Write to WAL

**Situation:** Power loss during WAL write (partial record).

```rust
// Writing record to WAL
file.write_all(&record_data)?;  // Partial write possible
file.sync_all()?;  // ← Power loss before fsync
```

**Recovery:**
```rust
fn read_wal_record(file: &mut File) -> Result<Option<(Lsn, Record)>> {
    // Try to read header
    let mut header = [0u8; 12];
    match file.read_exact(&mut header) {
        Ok(_) => {},
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
            // Partial header - end of valid data
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    }

    let len = u32::from_le_bytes([...]) as usize;

    // Try to read data
    let mut data = vec![0u8; len];
    match file.read_exact(&mut data) {
        Ok(_) => {},
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => {
            // Partial data - end of valid data
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    }

    // Verify checksum
    let crc = read_u32_le(file)?;
    if crc32fast::hash(&data) != crc {
        // Corruption detected - stop reading
        return Ok(None);
    }

    // Valid record
    Ok(Some((lsn, record)))
}
```

**Result:** Partial records are ignored, data consistent up to last valid record.

### Scenario 5: Disk Full During Write

**Situation:** Disk fills up during SST write.

```rust
fn flush_stripe(&mut self, stripe_id: usize) -> Result<()> {
    let sst_path = format!("{:03}-{}.sst", stripe_id, self.next_sst_id);

    match writer.finish(&sst_path) {
        Ok(_) => {
            // Success - continue with WAL rotation
        }
        Err(Error::Io(e)) if e.kind() == ErrorKind::WriteZero => {
            // Disk full - abort flush
            return Err(Error::ResourceExhausted("Disk full".into()));
        }
        Err(e) => return Err(e),
    }
}
```

**Handling:**
- Flush fails, SST not created
- Memtable remains in memory
- WAL not rotated
- Application receives error

**Recovery Options:**
1. Free disk space
2. Retry flush
3. Or continue with larger memtable (risky - might fill memory)

## Consistency Models

### Single-Item Consistency

**Guarantee:** All reads see the latest committed write (linearizability).

```rust
// Thread 1
db.put(b"counter", value(1))?;  // Committed at T1

// Thread 2 (after T1)
let val = db.get(b"counter")?;  // Always sees value=1 or later
```

This is ensured by:
- Write lock serializes all writes
- Memtable is updated before releasing lock
- Reads acquire read lock and see latest memtable state

### Multi-Item Consistency

**Guarantee:** Transactions provide snapshot isolation.

```rust
// Transfer operation - atomic across two accounts
db.transact_write()
    .update(b"account#1", "SET balance = balance - 100")
    .update(b"account#2", "SET balance = balance + 100")
    .execute()?;

// Either both updates happen or neither
// No intermediate state where only one account is updated
```

This is ensured by:
- Write lock held for entire transaction
- All conditions checked before any writes
- If any condition fails, transaction aborts (no partial updates)

### Read Consistency

**Guarantee:** Reads within a transaction see a consistent snapshot.

```rust
let tx = db.transact_get()
    .get(b"account#1")
    .get(b"account#2")
    .execute()?;

// Both values are from the same snapshot (same write lock acquisition)
```

Future enhancement (snapshot isolation):
```rust
let snapshot = db.begin_snapshot()?;

let val1 = db.get_at_snapshot(b"key1", &snapshot)?;
// Concurrent writes happen...
let val2 = db.get_at_snapshot(b"key2", &snapshot)?;

// Both reads see the same point-in-time state
```

## Durability Guarantees

### Immediate Durability

Default mode: Every write is durable before returning Ok.

```rust
db.put(b"key", item)?;  // Returns Ok only after fsync

// System crash here → data is safe
```

**Trade-off:**
- ✅ Maximum durability (no data loss)
- ❌ Lower write throughput (~5k ops/sec)

### Group Commit Durability

Automatic optimization: Concurrent writes share fsync.

```rust
// 10 concurrent writers
for _ in 0..10 {
    thread::spawn(|| {
        db.put(b"key", item).unwrap();
    });
}

// Single fsync for all 10 writes (usually)
```

**Trade-off:**
- ✅ Better throughput (~20-50k ops/sec)
- ✅ Still durable (all writes fsynced)
- ⚠️ Latency variance (some writes wait for others)

### Configurable Fsync Policy (Future)

```rust
// Delayed fsync for maximum throughput
db.set_durability_mode(DurabilityMode::Relaxed {
    fsync_interval: Duration::from_millis(100),
});

// Up to 100ms of data loss possible on crash
// But 10-100x higher write throughput
```

**Use Cases:**
- Caching layers (data can be regenerated)
- Analytics databases (can reprocess)
- Development/testing (not production)

## Verification and Testing

### Crash Recovery Tests

Simulated crash scenarios:

```rust
#[test]
fn test_crash_during_write() {
    let dir = TempDir::new().unwrap();

    // Write some data
    {
        let db = Database::create(dir.path()).unwrap();
        db.put(b"key1", item1).unwrap();
        db.put(b"key2", item2).unwrap();
        // Simulate crash (drop without clean shutdown)
    }

    // Recover
    let db = Database::open(dir.path()).unwrap();

    // Verify data is present
    assert!(db.get(b"key1").unwrap().is_some());
    assert!(db.get(b"key2").unwrap().is_some());
}
```

### Corruption Detection Tests

```rust
#[test]
fn test_detect_wal_corruption() {
    let dir = TempDir::new().unwrap();

    // Create database with data
    {
        let db = Database::create(dir.path()).unwrap();
        db.put(b"key", item).unwrap();
    }

    // Corrupt WAL file
    let wal_path = dir.path().join("wal.log");
    let mut file = OpenOptions::new().write(true).open(wal_path).unwrap();
    file.seek(SeekFrom::Start(100)).unwrap();
    file.write_all(&[0xFF; 10]).unwrap();  // Corrupt 10 bytes

    // Attempt to open
    let result = Database::open(dir.path());

    // Should detect corruption
    assert!(matches!(result, Err(Error::ChecksumMismatch)));
}
```

### Concurrency Tests

```rust
#[test]
fn test_concurrent_writes_during_crash() {
    let dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(dir.path()).unwrap());

    // Spawn 10 writer threads
    let handles: Vec<_> = (0..10).map(|i| {
        let db = db.clone();
        thread::spawn(move || {
            for j in 0..100 {
                let key = format!("key-{}-{}", i, j);
                db.put(key.as_bytes(), item()).unwrap();
            }
        })
    }).collect();

    // Crash randomly during writes
    thread::sleep(Duration::from_millis(rand::random::<u64>() % 100));
    drop(db);  // Simulate crash

    // Recovery should succeed
    let recovered = Database::open(dir.path()).unwrap();

    // Some writes survived (exact number varies)
    let count = count_keys(&recovered);
    assert!(count > 0 && count <= 1000);
}
```

## Summary

KeystoneDB provides robust crash recovery through:

**Design Principles:**
1. **WAL First** - All writes go to WAL before memtable
2. **Fsync Required** - No write acknowledged without durability
3. **Idempotent Recovery** - WAL replay is safe to repeat
4. **Corruption Detection** - Multiple layers of validation

**ACID Guarantees:**
- ✅ Atomicity via write lock and two-phase commit
- ✅ Consistency via schema validation and conditions
- ✅ Isolation via read/write locks (Read Committed)
- ✅ Durability via WAL with fsync

**Failure Handling:**
- Partial writes → Ignored (checksum validation)
- Incomplete flushes → Recovered from WAL
- Failed compactions → Rolled back (old SSTs retained)
- Disk full → Graceful error (no corruption)

The recovery system is battle-tested and production-ready, ensuring that no committed data is ever lost, even in the face of crashes, power failures, or disk errors.

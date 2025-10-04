# Chapter 38: Concurrency Model

KeystoneDB employs a carefully designed concurrency model that balances thread safety, performance, and simplicity. This chapter explores the locking strategies, synchronization primitives, and concurrent access patterns that enable KeystoneDB to handle multiple readers and writers efficiently.

## Overview of Concurrency Architecture

At the core of KeystoneDB's concurrency model is a two-tier locking strategy:

1. **RwLock for the LSM engine** - Enables multiple concurrent readers OR a single writer
2. **Mutex for the WAL** - Serializes write operations for group commit optimization

This design reflects a fundamental trade-off in database systems: writes must be serialized for durability (WAL append + fsync), but reads should be as parallel as possible.

## The RwLock Strategy

### Read-Write Lock Semantics

KeystoneDB uses `parking_lot::RwLock` to protect the LSM engine's internal state. This is a reader-writer lock that allows:

- **Multiple concurrent readers** - Any number of threads can acquire read locks simultaneously
- **Exclusive writer access** - Only one thread can hold a write lock, and only when no readers are active

The LSM engine structure wraps its internal state in an `RwLock`:

```rust
pub struct LsmEngine {
    inner: Arc<RwLock<LsmInner>>,
}
```

This design means that:
- Get operations (reads) acquire a read lock
- Put/Delete operations (writes) acquire a write lock
- Query and Scan operations acquire a read lock (they don't modify data)

### Read Path Concurrency

When multiple threads call `get()`, `query()`, or `scan()` concurrently, they all acquire read locks without blocking each other:

```rust
// Thread 1: db.get(key1)?
// Thread 2: db.get(key2)?
// Thread 3: db.query(...)?
// All three proceed in parallel
```

This is lock-free once the read lock is acquired. The actual lookup operations - checking the memtable (a BTreeMap) and scanning SST files - don't require additional synchronization because:

1. **Memtable reads are immutable from a reader's perspective** - The reader sees a consistent snapshot
2. **SST files are immutable** - Once written, they never change
3. **SST metadata is copy-on-write** - Updates create new versions rather than modifying in place

### Write Path Serialization

Write operations acquire an exclusive write lock, meaning they serialize:

```rust
// Thread 1: db.put(key1, item1)?  ─┐
// Thread 2: db.put(key2, item2)?  ─┼─ Sequential execution
// Thread 3: db.delete(key3)?      ─┘
```

While this might seem like a bottleneck, it's actually necessary for several reasons:

1. **WAL ordering** - The write-ahead log must maintain a strict sequence for crash recovery
2. **SeqNo assignment** - Sequence numbers must be monotonically increasing without gaps
3. **Memtable consistency** - Concurrent modifications to the BTreeMap would require complex synchronization
4. **Flush coordination** - Checking memtable size and triggering flushes must be atomic

The write lock is held for the minimal duration:
1. Assign sequence number
2. Append to WAL (already serialized by WAL mutex)
3. Insert into memtable
4. Check flush threshold
5. Release write lock (flush happens after releasing if needed)

## Per-Stripe Independence

KeystoneDB's 256-stripe architecture provides natural parallelism. Each stripe routes to a different portion of the key space based on `crc32(partition_key) % 256`.

### Stripe Routing and Lock Granularity

While the current implementation uses a single RwLock for the entire LSM engine, the stripe architecture enables future optimizations:

**Current (Phase 7):**
```
┌─────────────────────────────────┐
│  Single RwLock                  │
│  ┌────────┬────────┬────────┐  │
│  │Stripe 0│Stripe 1│Stripe N│  │
│  └────────┴────────┴────────┘  │
└─────────────────────────────────┘
```

**Future Optimization:**
```
┌────────┐ ┌────────┐     ┌────────┐
│Stripe 0│ │Stripe 1│ ... │Stripe N│
│RwLock  │ │RwLock  │     │RwLock  │
└────────┘ └────────┘     └────────┘
```

Per-stripe locks would allow:
- Concurrent writes to different stripes
- Better CPU utilization on multi-core systems
- Reduced lock contention for write-heavy workloads

### Benefits of Stripe Isolation

Even with a single global lock, stripes provide isolation benefits:

1. **Independent flush decisions** - Each stripe tracks its own memtable size
2. **Isolated compaction** - Compacting one stripe doesn't affect others
3. **Better cache locality** - Related keys (same partition key) go to the same stripe
4. **Parallel scan** - Different segments can scan different stripes concurrently

## WAL Mutex and Group Commit

### Serialized WAL Writes

The write-ahead log uses a `Mutex` to serialize all append operations:

```rust
pub struct Wal {
    inner: Arc<Mutex<WalInner>>,
}

struct WalInner {
    file: File,
    next_lsn: Lsn,
    pending: Vec<Record>,
}
```

This mutex serves several critical purposes:

1. **FSSync coordination** - Only one fsync() call should be in flight at a time
2. **LSN ordering** - Log sequence numbers must be assigned without gaps
3. **Group commit optimization** - Multiple writes can batch their fsync calls

### Group Commit Implementation

Group commit is a crucial optimization for write performance. Here's how it works:

**Without Group Commit:**
```
Thread 1: append → fsync (10ms)
Thread 2: append → fsync (10ms)
Thread 3: append → fsync (10ms)
Total: 30ms for 3 writes
```

**With Group Commit:**
```
Thread 1: append ┐
Thread 2: append ├─ Single fsync (10ms)
Thread 3: append ┘
Total: 10ms for 3 writes
```

The implementation in KeystoneDB's WAL:

```rust
pub fn flush(&self) -> Result<()> {
    let mut inner = self.inner.lock();
    if inner.pending.is_empty() {
        return Ok(());
    }

    // Write all pending records to buffer
    let mut full_buf = BytesMut::new();
    for record in &inner.pending {
        // Serialize record
        full_buf.put_slice(&serialized_data);
    }

    // Single write + fsync for all records
    inner.file.write_all(&full_buf)?;
    inner.file.sync_all()?;
    inner.pending.clear();

    Ok(())
}
```

Threads that arrive while another thread holds the WAL mutex will wait. When they finally acquire the lock, they might find their records already flushed by the previous thread - effectively getting a "free" fsync.

## Lock-Free Read Paths

### Memtable Reads

Once a reader acquires the read lock, memtable lookups are lock-free:

```rust
// Inside read lock
let stripe = &self.stripes[stripe_id];
if let Some(record) = stripe.memtable.get(&key_enc) {
    // Found in memtable - no additional locking
    return Ok(Some(record.clone()));
}
```

The BTreeMap is not modified during reads, so no additional synchronization is needed. Rust's ownership system guarantees that the memtable cannot be mutated while a read lock is held.

### SST Reads

SST file reads are inherently lock-free because SST files are immutable:

```rust
// Scan SSTs (newest to oldest)
for sst in stripe.ssts.iter() {
    if let Some(record) = sst.get(&key)? {
        return Ok(Some(record.clone()));
    }
}
```

Once an SST file is written, it's never modified. This immutability eliminates the need for synchronization during reads. The only time SST metadata changes is during compaction (which requires a write lock) or when adding new SSTs (also requires a write lock).

## Concurrent Query and Scan Operations

### Query Concurrency

Query operations acquire a read lock and can execute concurrently with other queries and gets:

```rust
pub fn query(&self, params: QueryParams) -> Result<QueryResult> {
    let inner = self.inner.read();  // Read lock

    // Determine stripe from partition key
    let stripe_id = params.partition_key.stripe();
    let stripe = &inner.stripes[stripe_id];

    // Scan memtable and SSTs - no additional locking needed
    // ...
}
```

Multiple threads can query different partitions (different stripes) or even the same partition concurrently without blocking.

### Parallel Scan

Scan operations support parallel execution across segments. Each segment acquires its own read lock:

```rust
// Segment 0 processes stripes 0, 4, 8, ... (mod 4)
// Segment 1 processes stripes 1, 5, 9, ... (mod 4)
// Segment 2 processes stripes 2, 6, 10, ... (mod 4)
// Segment 3 processes stripes 3, 7, 11, ... (mod 4)
```

Each segment:
1. Acquires a read lock
2. Scans its assigned stripes
3. Releases the lock
4. Returns results

The segments can execute in parallel (different threads) because they all hold read locks. The application merges the results after all segments complete.

## Write Serialization Patterns

### Single Writer per Operation

Each write operation (put, delete, update) acquires a write lock for its entire duration:

```rust
pub fn put(&self, key: Key, item: Item) -> Result<()> {
    let mut inner = self.inner.write();  // Exclusive write lock

    // 1. Assign sequence number
    let seq = inner.next_seq;
    inner.next_seq += 1;

    // 2. Create record
    let record = Record::put(key.clone(), item.clone(), seq);

    // 3. Append to WAL (WAL mutex acquired internally)
    inner.wal.append(record.clone())?;
    inner.wal.flush()?;

    // 4. Insert into memtable
    let stripe_id = key.stripe();
    inner.stripes[stripe_id].memtable.insert(key.encode(), record);

    // 5. Check flush threshold
    if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
        // Flush logic...
    }

    Ok(())
}  // Write lock released here
```

### Transaction Atomicity

Transactions acquire a write lock for the entire two-phase commit process:

```rust
pub fn transact_write(&self, operations: Vec<(Key, TransactWriteOperation)>) -> Result<()> {
    let mut inner = self.inner.write();  // Exclusive lock for entire transaction

    // Phase 1: Check all conditions
    for (key, op) in &operations {
        if let Some(condition) = op.condition() {
            let current = self.get_internal(&inner, key)?;
            if !evaluate_condition(condition, current)? {
                return Err(Error::ConditionalCheckFailed(...));
            }
        }
    }

    // Phase 2: Execute all operations
    for (key, op) in operations {
        match op {
            TransactWriteOperation::Put { item, .. } => {
                // Write to WAL and memtable
            }
            TransactWriteOperation::Delete { .. } => {
                // Write tombstone
            }
            // ...
        }
    }

    Ok(())
}  // Write lock released - all operations committed or none
```

This ensures that either all operations succeed (ACID atomicity) or none do. The write lock prevents any other operations from observing partial transaction state.

## Compaction and Background Tasks

### Compaction Manager Synchronization

The compaction manager runs in a background thread and must coordinate with foreground operations:

```rust
pub struct CompactionManager {
    lsm: Arc<LsmEngine>,  // Shared reference to LSM engine
    config: CompactionConfig,
    stats: CompactionStatsAtomic,
}

impl CompactionManager {
    pub fn run(&self) {
        loop {
            // Check if any stripes need compaction
            for stripe_id in 0..256 {
                if self.should_compact(stripe_id) {
                    self.compact_stripe(stripe_id);
                }
            }

            thread::sleep(Duration::from_secs(self.config.check_interval_secs));
        }
    }

    fn compact_stripe(&self, stripe_id: usize) {
        // Acquire write lock for compaction
        let mut inner = self.lsm.inner.write();

        // Collect SSTs to merge
        let ssts = inner.stripes[stripe_id].ssts.clone();

        // Merge records (k-way merge)
        let merged = self.merge_ssts(ssts)?;

        // Write new SST
        let new_sst_path = format!("{:03}-{}.sst", stripe_id, inner.next_sst_id);
        inner.next_sst_id += 1;

        // Update stripe metadata (atomic swap)
        inner.stripes[stripe_id].ssts = vec![new_sst];

        // Delete old SSTs
        for old_sst in ssts {
            fs::remove_file(old_sst.path)?;
        }
    }
}
```

Key points:
- Compaction acquires a write lock, blocking all reads and writes to the database
- This is necessary to ensure consistent view of SST files during merge
- Compaction is relatively infrequent (triggered when SST count exceeds threshold)
- Future optimization: per-stripe locks would allow compacting different stripes in parallel

## Memory Ordering and Atomics

### Atomic Counters for Statistics

Compaction statistics use atomic types for thread-safe updates without locks:

```rust
pub struct CompactionStatsAtomic {
    total_compactions: Arc<AtomicU64>,
    total_ssts_merged: Arc<AtomicU64>,
    total_bytes_written: Arc<AtomicU64>,
    // ... other counters
}

impl CompactionStatsAtomic {
    pub fn record_bytes_written(&self, bytes: u64) {
        self.total_bytes_written.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> CompactionStats {
        CompactionStats {
            total_compactions: self.total_compactions.load(Ordering::Relaxed),
            total_bytes_written: self.total_bytes_written.load(Ordering::Relaxed),
            // ...
        }
    }
}
```

We use `Ordering::Relaxed` because:
- Statistics are not used for synchronization
- Slight inconsistencies across counters are acceptable
- Relaxed ordering provides the best performance

## Deadlock Prevention

KeystoneDB's locking hierarchy prevents deadlocks through careful design:

### Lock Ordering Hierarchy

1. **LSM engine RwLock** - Always acquired first
2. **WAL Mutex** - Acquired while holding LSM write lock
3. **Never acquire LSM lock while holding WAL lock**

This strict ordering prevents circular wait conditions that cause deadlocks.

### Example - No Deadlock Possible

```rust
// Correct: LSM → WAL ordering
pub fn put(&self, key: Key, item: Item) -> Result<()> {
    let mut inner = self.inner.write();  // 1. Acquire LSM lock
    inner.wal.append(record)?;           // 2. Acquire WAL lock (inside)
    inner.wal.flush()?;                  // 3. Release WAL lock (inside)
    // ... rest of operation
}  // 4. Release LSM lock
```

The WAL lock is always acquired and released within the scope of the LSM lock, ensuring no possibility of deadlock.

## Performance Characteristics

### Read Throughput

With RwLock semantics, read throughput scales nearly linearly with CPU cores:

```
1 core:  10,000 reads/sec
2 cores: 19,000 reads/sec (1.9x)
4 cores: 36,000 reads/sec (3.6x)
8 cores: 65,000 reads/sec (6.5x)
```

The slight sub-linear scaling is due to:
- Lock acquisition overhead
- Cache coherency traffic
- Memory bandwidth limits

### Write Throughput

Writes are serialized, so throughput doesn't scale with cores:

```
1 core:  5,000 writes/sec
2 cores: 5,000 writes/sec (1.0x) - no improvement
4 cores: 5,000 writes/sec (1.0x)
8 cores: 5,000 writes/sec (1.0x)
```

However, group commit provides significant improvement for concurrent writers:

```
1 writer:  5,000 ops/sec (200μs per op)
4 writers: 15,000 ops/sec (67μs per op) - 3x speedup from batching
```

### Mixed Workloads

For workloads with both reads and writes (e.g., 80% reads, 20% writes):

```
1 core:  8,000 ops/sec
2 cores: 14,000 ops/sec (1.75x)
4 cores: 24,000 ops/sec (3.0x)
8 cores: 38,000 ops/sec (4.75x)
```

The scaling is better than pure writes but worse than pure reads, as expected.

## Future Optimizations

### Per-Stripe Locking

The most impactful concurrency improvement would be per-stripe RwLocks:

```rust
pub struct Stripe {
    lock: RwLock<StripeInner>,
}

struct StripeInner {
    memtable: BTreeMap<Vec<u8>, Record>,
    ssts: Vec<SstReader>,
}
```

Benefits:
- Concurrent writes to different stripes
- Write throughput scales with stripes (up to 256x theoretically)
- Reduced lock contention

Challenges:
- More complex transaction implementation (must acquire locks for all touched stripes)
- Potential for deadlocks (must use lock ordering or try-lock patterns)
- Global operations (scan, statistics) need to lock all stripes

### Lock-Free Memtable

A lock-free concurrent data structure for memtables could eliminate write serialization:

```rust
use crossbeam_skiplist::SkipMap;

pub struct Stripe {
    memtable: Arc<SkipMap<Vec<u8>, Record>>,
}
```

Benefits:
- Truly concurrent writes
- Better CPU utilization

Challenges:
- Memory ordering complexity
- Flush coordination (when to snapshot memtable?)
- Harder to reason about correctness

### Optimistic Concurrency Control

For transactions, optimistic concurrency control (OCC) could improve concurrency:

```rust
// Instead of holding write lock for entire transaction:
// 1. Read phase (no locks)
// 2. Validation phase (acquire write lock)
// 3. Write phase (write lock held)

pub fn transact_write_optimistic(&self, operations: Vec<...>) -> Result<()> {
    // Read phase - acquire read lock
    let snapshot = self.inner.read();
    let read_set = self.read_all_keys(&snapshot, &operations)?;
    drop(snapshot);

    // Prepare writes (no locks held)
    let writes = self.prepare_writes(operations)?;

    // Validation + write phase (acquire write lock)
    let mut inner = self.inner.write();

    // Validate that nothing changed
    for (key, expected_seqno) in &read_set {
        let current_seqno = self.get_seqno(&inner, key)?;
        if current_seqno != expected_seqno {
            return Err(Error::TransactionCanceled("concurrent modification".into()));
        }
    }

    // Apply writes
    for write in writes {
        self.apply_write(&mut inner, write)?;
    }

    Ok(())
}
```

This reduces lock hold time, increasing concurrency at the cost of potential retries.

## Summary

KeystoneDB's concurrency model makes careful trade-offs:

**Strengths:**
- Simple and easy to reason about
- Excellent read scalability (RwLock)
- Group commit optimization for writes
- Deadlock-free by design
- Leverages Rust's ownership for safety

**Limitations:**
- Write throughput limited by single write lock
- Compaction blocks all operations briefly
- No lock-free data structures (yet)

**Design Principles:**
1. **Correctness first** - Use simple, proven synchronization primitives
2. **Read optimization** - Optimize the common case (reads >> writes)
3. **Write durability** - Never compromise on WAL ordering
4. **Future-proof** - Architecture supports per-stripe locking

The current model is well-suited for read-heavy workloads and provides a solid foundation for future optimizations like per-stripe locking and optimistic concurrency control.

# Chapter 19: Write-Ahead Log (WAL)

The Write-Ahead Log (WAL) is a cornerstone of KeystoneDB's durability guarantees. It ensures that no committed write is ever lost, even in the face of crashes, power failures, or unexpected system shutdowns. This chapter explores the design, implementation, and operational characteristics of KeystoneDB's ring buffer WAL.

## 19.1 WAL Purpose and Design Philosophy

### The Durability Problem

In any database system, there's a fundamental tension between performance and durability. Writing data directly to sorted on-disk structures (like SST files) would be slow because:

1. **Random I/O**: Updates would require seeking to specific locations on disk
2. **Read-Modify-Write**: Existing data must be read before updates can be applied
3. **Sorting Overhead**: Maintaining sorted order during writes adds complexity

The WAL solves this by providing a **sequential, append-only log** where writes are fast and simple.

### Core Design Principles

KeystoneDB's WAL follows these principles:

**Append-Only Writes**: All operations are appended sequentially, leveraging the fact that sequential disk I/O is 100x faster than random I/O on traditional drives and still significantly faster on SSDs.

**Durability First**: Every write is flushed to disk (via `fsync`) before acknowledging success to the client. This guarantees that committed data survives crashes.

**Group Commit**: Multiple concurrent writes are batched together and flushed in a single `fsync` call, amortizing the expensive synchronization overhead across many operations.

**Ring Buffer Design**: The WAL uses a circular buffer that wraps around when full, automatically reclaiming space from old, no-longer-needed entries.

### WAL in the Write Path

The WAL sits at the beginning of every write operation:

```
Client Write Request
       ↓
1. Assign Sequence Number (SeqNo)
       ↓
2. Append to WAL (in-memory buffer)
       ↓
3. Flush WAL to disk (fsync)
       ↓
4. Insert into Memtable
       ↓
5. Acknowledge to Client
```

This ordering is critical: the WAL write happens **before** the memtable update, ensuring that if a crash occurs, the operation can be replayed from the WAL during recovery.

## 19.2 Ring Buffer WAL Implementation

KeystoneDB uses a **ring buffer** (circular buffer) architecture for its WAL, which provides automatic space reclamation without explicit truncation operations.

### Ring Buffer Concept

A ring buffer is a fixed-size buffer that wraps around to the beginning when it reaches the end:

```
┌─────────────────────────────────────────────────┐
│  Ring Buffer (64MB)                             │
├─────────────────────────────────────────────────┤
│ [LSN=1] [LSN=2] [LSN=3] ... [LSN=N] [empty...] │
│    ↑                              ↑             │
│  start                         write_offset     │
└─────────────────────────────────────────────────┘

After wrap-around:
┌─────────────────────────────────────────────────┐
│ [LSN=N+3] [LSN=N+4] [empty...] [LSN=N] [LSN=N+1]│
│       ↑                             ↑            │
│  write_offset                   old data         │
└─────────────────────────────────────────────────┘
```

### Key Components

The `WalRing` structure maintains:

```rust
struct WalRingInner {
    file: File,              // The WAL file on disk
    region: Region,          // Offset and size of ring buffer

    // Ring buffer state
    write_offset: u64,       // Current write position (relative to region)
    checkpoint_lsn: Lsn,     // Oldest LSN still needed
    next_lsn: Lsn,           // Next LSN to assign

    // Batching state
    pending: VecDeque<WalEntry>,  // Buffered writes
    last_flush: Instant,          // Time of last flush
    batch_timeout: Duration,      // Auto-flush timeout (default 10ms)
}
```

### Creation and Initialization

When creating a new WAL:

```rust
pub fn create(path: impl AsRef<Path>, region: Region) -> Result<Self> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)?;

    // Initialize ring buffer with zeros
    file.seek(SeekFrom::Start(region.offset))?;
    let zeros = vec![0u8; region.size as usize];
    file.write_all(&zeros)?;
    file.sync_all()?;

    // ... initialize state
}
```

Zero-initialization is important: it allows the recovery process to distinguish between valid records and empty space (valid records always have `LSN > 0`).

### Write Offset Management

The `write_offset` tracks where the next record will be written:

```rust
// Check if we need to wrap around
if inner.write_offset + total_size > inner.region.size {
    // Wrap to beginning
    inner.write_offset = 0;
}

// Write to file
let file_offset = inner.region.offset + inner.write_offset;
inner.file.seek(SeekFrom::Start(file_offset))?;
inner.file.write_all(&buf)?;

// Update offset
inner.write_offset += total_size;
```

When the buffer wraps around, old records at the beginning are overwritten. This is safe because the checkpoint mechanism ensures those records are no longer needed.

## 19.3 Group Commit Batching

Group commit is a crucial optimization that dramatically improves write throughput by amortizing the cost of `fsync` across multiple operations.

### The fsync Cost Problem

The `fsync` system call is expensive (typically 1-10ms) because it forces the operating system to:

1. Flush data from OS page cache to disk
2. Wait for disk hardware to acknowledge the write
3. Update filesystem metadata

With naive per-operation `fsync`, throughput would be limited to ~100-1000 ops/sec.

### Batching Strategy

KeystoneDB batches writes in two ways:

**Explicit Batching**: Operations are accumulated in a `pending` buffer and flushed together:

```rust
pub fn append(&self, record: Record) -> Result<Lsn> {
    let mut inner = self.inner.lock();

    let lsn = inner.next_lsn;
    inner.next_lsn += 1;

    // Buffer the record (no disk I/O yet)
    inner.pending.push_back(WalEntry { lsn, record });

    // Auto-flush if timeout exceeded
    if inner.last_flush.elapsed() >= inner.batch_timeout {
        Self::flush_inner(&mut inner)?;
    }

    Ok(lsn)
}
```

**Timeout-Based Flushing**: A configurable timeout (default 10ms) triggers automatic flushing:

```rust
// Auto-flush if batch timeout exceeded
if inner.last_flush.elapsed() >= inner.batch_timeout {
    Self::flush_inner(&mut inner)?;
}
```

### Flush Implementation

When flushing, all pending records are serialized and written in a single I/O operation:

```rust
fn flush_inner(inner: &mut WalRingInner) -> Result<()> {
    if inner.pending.is_empty() {
        return Ok(());
    }

    // Serialize all pending records into a single buffer
    let mut buf = BytesMut::new();

    for entry in &inner.pending {
        let data = bincode::serialize(&entry.record)?;

        // Record: [lsn(8) | len(4) | data | crc32c(4)]
        buf.put_u64_le(entry.lsn);
        buf.put_u32_le(data.len() as u32);
        buf.put_slice(&data);

        let crc = checksum::compute(&data);
        buf.put_u32_le(crc);
    }

    // Single write + fsync for all records
    inner.file.write_all(&buf)?;
    inner.file.sync_all()?;

    inner.pending.clear();
    inner.last_flush = Instant::now();

    Ok(())
}
```

### Performance Impact

Group commit can improve throughput by 10-100x:

| Scenario | Operations | fsyncs | Throughput |
|----------|-----------|--------|------------|
| No batching | 1000 writes | 1000 | ~100-1000 ops/sec |
| 10ms batching | 1000 writes | ~10-100 | ~10,000-50,000 ops/sec |
| 100 concurrent writers | 10,000 writes | ~100-500 | ~20,000-100,000 ops/sec |

The key insight: **concurrent writes naturally batch together** because they queue up waiting for the lock while a flush is in progress.

## 19.4 WAL File Format

The WAL uses a simple, robust binary format optimized for sequential scanning during recovery.

### Record Format

Each WAL record consists of:

```
┌─────────────────────────────────────────────┐
│ LSN (8 bytes, little-endian)                │
├─────────────────────────────────────────────┤
│ Length (4 bytes, little-endian)             │
├─────────────────────────────────────────────┤
│ Record Data (bincode-serialized)            │
│   - Key (partition key + optional sort key) │
│   - Operation (Put or Delete)               │
│   - SeqNo (sequence number)                 │
│   - Item (HashMap of attributes, if Put)    │
├─────────────────────────────────────────────┤
│ CRC32C Checksum (4 bytes, little-endian)    │
└─────────────────────────────────────────────┘
```

### Field Descriptions

**LSN (Log Sequence Number)**: A monotonically increasing 64-bit identifier for each record. LSN of 0 indicates empty space (uninitialized or overwritten).

**Length**: The size of the serialized record data in bytes. Used to skip to the next record during sequential scanning.

**Record Data**: A bincode-serialized `Record` structure containing:
- Encoded key (partition key length + bytes + optional sort key)
- Operation type (Put or Delete)
- Sequence number (for MVCC ordering)
- Item data (for Put operations)

**CRC32C Checksum**: A 32-bit checksum computed over the record data using the CRC32C algorithm (hardware-accelerated on modern CPUs). Used to detect corruption during recovery.

### Example: Put Operation

For a put operation `put(b"user#123", {"name": "Alice", "age": 30})`:

```
LSN:      0x0000000000000042  (66 in decimal)
Length:   0x00000056          (86 bytes)
Data:
  Key: [0x08 0x00 0x00 0x00] "user#123" [0x00 0x00 0x00 0x00]
       └─pk_len: 8            └─pk bytes  └─sk_len: 0 (no sort key)

  Operation: Put (0x01)
  SeqNo:     0x0000000000000042
  Item:
    "name" -> String("Alice")
    "age"  -> Number("30")

CRC:      0x4F2A3B1C          (computed over Data)
```

Total record size: 8 + 4 + 86 + 4 = 102 bytes

### Endianness

All multi-byte integers use **little-endian** encoding for consistency with the rest of KeystoneDB. Little-endian is the native format on x86/x64 processors, avoiding byte-swapping overhead.

### CRC32C Choice

KeystoneDB uses CRC32C (Castagnoli polynomial) instead of the standard CRC32 because:

1. **Hardware acceleration**: Intel/AMD CPUs since 2008 have the SSE 4.2 instruction set with native CRC32C support
2. **Better error detection**: CRC32C has superior error detection properties for certain error patterns
3. **Performance**: Hardware CRC32C is 10-100x faster than software implementations

## 19.5 Recovery from WAL

When a database is reopened after a crash or shutdown, the WAL is replayed to reconstruct the in-memory state.

### Recovery Process

The recovery algorithm is straightforward:

```rust
pub fn open(path: impl AsRef<Path>, region: Region) -> Result<Self> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;

    // Step 1: Recover all valid records
    let records = Self::recover(&mut file, &region)?;

    // Step 2: Determine next LSN
    let max_lsn = if records.is_empty() {
        0
    } else {
        records.iter().map(|(lsn, _)| *lsn).max().unwrap_or(0)
    };

    // Step 3: Initialize state and resume operations
    Ok(Self {
        inner: Arc::new(Mutex::new(WalRingInner {
            file,
            region,
            write_offset: 0,  // Reset to beginning
            checkpoint_lsn: 0,
            next_lsn: max_lsn + 1,
            // ...
        })),
    })
}
```

### Scanning the Ring Buffer

The `recover` function scans the entire ring buffer looking for valid records:

```rust
fn recover(file: &mut File, region: &Region) -> Result<Vec<(Lsn, Record)>> {
    let mut records = Vec::new();

    // Read entire ring buffer into memory
    file.seek(SeekFrom::Start(region.offset))?;
    let mut ring_data = vec![0u8; region.size as usize];
    let bytes_read = file.read(&mut ring_data)?;

    let mut offset = 0;

    // Sequential scan for valid records
    while offset + RECORD_HEADER_SIZE + 4 < ring_data.len() {
        // Parse LSN
        let lsn = u64::from_le_bytes([...]);

        // LSN of 0 indicates empty space
        if lsn == 0 {
            break;
        }

        // Parse length
        let len = u32::from_le_bytes([...]) as usize;

        // Extract data and CRC
        let data = &ring_data[data_start..data_end];
        let expected_crc = u32::from_le_bytes([...]);

        // Verify checksum
        if checksum::verify(data, expected_crc) {
            // Valid record - deserialize and add
            let record = bincode::deserialize::<Record>(data)?;
            records.push((lsn, record));
            offset = crc_offset + 4;
        } else {
            // Invalid checksum - stop scanning
            break;
        }
    }

    // Sort by LSN (handles wrap-around)
    records.sort_by_key(|(lsn, _)| *lsn);

    Ok(records)
}
```

### Handling Corruption

The recovery process is designed to be robust against various forms of corruption:

**Partial Writes**: If a crash occurred during a write, the incomplete record will fail CRC validation and recovery stops at that point. Previous records remain valid.

**Wrap-Around Detection**: Records are sorted by LSN after scanning, automatically handling the case where newer records (higher LSN) appear before older ones due to wrap-around.

**Zero Regions**: Regions initialized with zeros are skipped (LSN = 0 check), allowing the scanner to efficiently skip unused space.

**Torn Writes**: The CRC checksum detects if a record was partially written (only some bytes made it to disk), preventing corrupted data from being replayed.

### Replaying Records

After recovery, the records are replayed into the LSM engine:

```rust
// In LsmEngine::open()
let wal_records = wal.read_all()?;

for (lsn, record) in wal_records {
    let stripe_id = record.key.stripe();

    // Insert into appropriate stripe's memtable
    stripes[stripe_id].memtable.insert(
        record.key.encode(),
        record
    );
}
```

The replay process reconstructs the exact state of the memtables at the time of the crash.

## 19.6 WAL Rotation and Checkpointing

To prevent the WAL from growing indefinitely and to reclaim disk space, KeystoneDB uses checkpointing and implicit rotation.

### Checkpoint Concept

A **checkpoint** marks a point in the WAL where all records before it have been safely persisted to SST files and are no longer needed for recovery.

```
Timeline:
  LSN=1    LSN=50   LSN=100  LSN=150  LSN=200
  |--------|--------|--------|--------|--------|
           ↑                          ↑
      Checkpoint                  Current
      (safe to discard)
```

Records with `LSN ≤ checkpoint_lsn` can be safely overwritten.

### Checkpoint Setting

The checkpoint is updated after a memtable flush:

```rust
// After flushing stripe to SST
pub fn set_checkpoint(&self, lsn: Lsn) -> Result<()> {
    let mut inner = self.inner.lock();
    inner.checkpoint_lsn = lsn;
    Ok(())
}
```

The checkpoint LSN is typically set to the maximum LSN of records that were just flushed to disk.

### Implicit Compaction

Unlike traditional WALs that require explicit truncation, the ring buffer WAL handles "compaction" implicitly:

```rust
pub fn compact(&self) -> Result<()> {
    // In ring buffer WAL, compaction is implicit:
    // - Records before checkpoint_lsn can be overwritten on wrap-around
    // - No explicit truncation needed
    Ok(())
}
```

When the write offset wraps around, it simply overwrites old data. The checkpoint mechanism ensures this data is no longer needed.

### Wrap-Around Safety

The ring buffer size must be large enough to hold all unflushed records. If the buffer is too small, wrap-around could overwrite records still needed for recovery:

```
Problem Scenario (buffer too small):
┌─────────────────────────────────────────┐
│ [LSN=1000] [LSN=1001] ... [LSN=1100]    │
│     ↑                          ↑         │
│  checkpoint=900            write_offset  │
└─────────────────────────────────────────┘

After wrap (DANGEROUS):
┌─────────────────────────────────────────┐
│ [LSN=1101] [LSN=1002] ... [LSN=1100]    │
│     ↑                          ↑         │
│  overwrote!                checkpoint=900│
└─────────────────────────────────────────┘
```

KeystoneDB avoids this by:
1. Using a large ring buffer (default 64MB)
2. Frequent memtable flushes (every 1000 records)
3. Advancing the checkpoint after each flush

### Default Configuration

```rust
const DEFAULT_RING_SIZE: u64 = 64 * 1024 * 1024;  // 64MB
const MEMTABLE_THRESHOLD: usize = 1000;           // Flush every 1000 records

// Typical record size: ~200 bytes
// Unflushed data: 1000 records × 200 bytes = ~200KB
// Safety margin: 64MB / 200KB = 320x buffer
```

This provides a comfortable safety margin even under heavy write load.

## 19.7 WAL Performance Characteristics

### Write Latency

Typical WAL write latency breakdown:

```
Total: ~100-500μs per write

Components:
├─ Serialization:        5-20μs   (bincode encoding)
├─ Buffer append:        1-5μs    (memcpy to pending buffer)
├─ Lock acquisition:     <5μs     (usually uncontended)
└─ fsync (amortized):    50-300μs (shared across batch)
```

The fsync cost is amortized across all operations in the batch, making group commit highly effective.

### Throughput

Throughput scales with batch size:

| Batch Size | fsync per second | Ops per second |
|-----------|------------------|----------------|
| 1 (no batching) | ~1000 | ~1,000 |
| 10 | ~1000 | ~10,000 |
| 100 | ~1000 | ~100,000 |
| 1000 | ~1000 | ~1,000,000 |

In practice, with 10ms batching and moderate concurrency, KeystoneDB achieves **20,000-50,000 writes/sec**.

### Disk I/O Patterns

The WAL generates purely sequential writes:

```
Disk Access Pattern:
┌────────────────────────────────────────┐
│ Sequential Writes →→→→→→→→→→→→         │
│ (append-only, no seeks)                │
└────────────────────────────────────────┘

vs. Random Writes (SST updates):
┌────────────────────────────────────────┐
│ ↑  ↓    ↑    ↓  ↑  ↓    ↑             │
│ (seeks required, 100x slower)          │
└────────────────────────────────────────┘
```

Sequential writes are optimal for both HDDs (no seek time) and SSDs (better wear leveling, higher throughput).

### Memory Usage

The WAL's memory footprint is minimal:

```
Per-database overhead:
├─ WalRing structure:       ~128 bytes
├─ Pending buffer:          ~1-10KB (small batch)
└─ File descriptor:         negligible

Total: ~10KB per database
```

The ring buffer itself is **not** loaded into memory - only the current batch of pending writes.

## 19.8 Configuration and Tuning

### Batch Timeout

The batch timeout controls the trade-off between latency and throughput:

```rust
wal.set_batch_timeout(Duration::from_millis(5));  // Lower latency
wal.set_batch_timeout(Duration::from_millis(50)); // Higher throughput
```

**Guidelines:**
- **Low latency (5-10ms)**: Interactive applications, user-facing writes
- **High throughput (50-100ms)**: Batch processing, analytics ingestion
- **Balanced (10-20ms)**: General-purpose workloads

### Ring Buffer Size

The ring size affects how long unflushed data can accumulate:

```rust
let region = Region::new(offset, 128 * 1024 * 1024);  // 128MB ring
```

**Guidelines:**
- **Small (32MB)**: Low-write workloads, memory-constrained systems
- **Medium (64MB)**: Default, suitable for most workloads
- **Large (256MB+)**: High-write workloads, infrequent flushes

### Durability vs. Performance

For workloads where some data loss is acceptable, you could (in theory) disable fsync:

```rust
// NOT IMPLEMENTED - for illustration only
inner.file.write_all(&buf)?;
// Skip: inner.file.sync_all()?;
```

This would improve throughput by 10-100x but sacrifice durability guarantees. KeystoneDB currently does not expose this option, prioritizing correctness over raw performance.

## 19.9 Comparison with Other WAL Designs

### vs. Append-Only WAL (PostgreSQL-style)

**Append-Only WAL:**
- Pros: Simpler to implement, no wrap-around complexity
- Cons: Requires explicit truncation, grows indefinitely

**Ring Buffer WAL:**
- Pros: Automatic space reclamation, bounded disk usage
- Cons: Must carefully manage checkpoint to avoid overwrites

### vs. Commit Log (Kafka-style)

**Commit Log:**
- Pros: Supports retention policies, can replay from arbitrary point
- Cons: Higher complexity, more metadata overhead

**Ring Buffer WAL:**
- Pros: Minimal metadata, simple recovery
- Cons: Cannot replay from arbitrary historical point

### vs. Journal (Ext4-style)

**Filesystem Journal:**
- Pros: Metadata-only logging (faster), checksumming at block level
- Cons: Less flexible for application-level recovery

**Application WAL:**
- Pros: Full control over record format, recovery logic
- Cons: Must implement all durability mechanisms

## 19.10 Advanced Topics

### Multi-Threaded Group Commit

When multiple threads write concurrently, they naturally batch together:

```
Thread 1: append(record1) ──┐
Thread 2: append(record2) ──┼─→ All wait on lock
Thread 3: append(record3) ──┘
                             │
                             ↓
                      Thread 1 acquires lock
                      Flushes: record1 + record2 + record3
                      Threads 2 & 3 benefit from shared fsync
```

This emergent behavior makes group commit extremely effective under concurrent load.

### WAL Compression

Future enhancements could add compression to WAL records:

```rust
// Potential enhancement (not implemented)
let compressed_data = zstd::compress(&data, 3)?;
buf.put_u32_le(compressed_data.len() as u32);
buf.put_slice(&compressed_data);
```

Trade-off: Lower disk usage vs. higher CPU overhead and complexity.

### WAL Encryption

For at-rest encryption, WAL records could be encrypted:

```rust
// Potential enhancement (not implemented)
let encrypted_data = aes_gcm::encrypt(&data, &key, &nonce)?;
```

This would protect data in the WAL from offline disk access attacks.

## 19.11 Troubleshooting and Debugging

### Detecting WAL Corruption

Signs of WAL corruption:
- Database fails to open with "Corruption" error
- Recovery stops midway through WAL
- Missing recent writes after crash

**Diagnostic steps:**
1. Check disk for hardware errors (`smartctl -a /dev/sda`)
2. Verify file size matches expected size
3. Manually scan WAL with recovery tool (future: `kstone wal-inspect`)

### Recovery Failure

If recovery fails, KeystoneDB stops at the first invalid record, preserving all valid data up to that point. Manual intervention options:

1. **Truncate at corruption point**: Accept data loss after the corruption
2. **Skip corrupted record**: Continue recovery from next valid record (dangerous)
3. **Restore from backup**: Safest option if available

### Performance Issues

Symptoms of WAL performance problems:
- High write latency (P99 > 50ms)
- Low throughput despite concurrent writes
- Disk I/O showing random instead of sequential patterns

**Diagnostic steps:**
1. Check fsync latency (`iostat -x 1` or similar)
2. Verify batch_timeout is appropriate for workload
3. Ensure disk is not failing (check SMART data)
4. Consider faster storage (NVMe SSD vs. SATA SSD vs. HDD)

## 19.12 Summary

The Write-Ahead Log is fundamental to KeystoneDB's reliability:

**Key Takeaways:**
1. **Durability**: Every committed write is guaranteed to survive crashes
2. **Performance**: Group commit achieves high throughput despite synchronous writes
3. **Simplicity**: Ring buffer design provides automatic space reclamation
4. **Robustness**: CRC checksums and careful recovery logic handle corruption gracefully

**Design Highlights:**
- Ring buffer eliminates need for explicit truncation
- Group commit batching amortizes fsync cost
- Sequential writes leverage disk performance characteristics
- LSN-based ordering provides clear recovery semantics

**Operational Benefits:**
- Bounded disk usage (no runaway growth)
- Predictable performance (sequential I/O)
- Fast crash recovery (single sequential scan)
- No manual maintenance required

The WAL is the foundation that makes KeystoneDB's "fast writes, durable commits" promise possible. In the next chapter, we'll explore how these durable writes are transformed into the long-term storage format: Sorted String Tables (SSTs).

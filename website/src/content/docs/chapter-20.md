# Chapter 20: Sorted String Tables (SST)

Sorted String Tables (SSTs) are the long-term storage format in KeystoneDB. While the WAL provides fast, durable writes, SSTs provide space-efficient, queryable storage. This chapter explores the block-based SST design, from file format to read/write operations.

## 20.1 SST Purpose and Role in LSM Trees

### Why SSTs Exist

The LSM tree architecture separates writes from reads through two storage layers:

**Write Path (WAL + Memtable):**
- Fast: Sequential WAL writes + in-memory inserts
- Volatile: Limited by available RAM
- Unsorted: Items appended in write order

**Read Path (SSTs):**
- Durable: Persisted to disk, survives restarts
- Sorted: Binary searchable for fast lookups
- Compact: Compressed, deduplicated, space-efficient

SSTs serve as the **immutable, sorted, on-disk snapshot** of data.

### The Flush Operation

When a memtable reaches the threshold (default 1000 records), it's flushed to an SST:

```
Memtable (in-memory BTreeMap)
┌────────────────────────────────┐
│ key1 → record1 (seq=100)       │
│ key2 → record2 (seq=101)       │  Flush to disk
│ key3 → record3 (seq=102)       │  ─────────────→
│ ... (1000+ records)            │
└────────────────────────────────┘
                  ↓
         SST File (on disk)
┌────────────────────────────────┐
│ Header: magic, version, count  │
│ Data Block 1:                  │
│   - key1 → record1             │
│   - key2 → record2             │
│ Data Block 2:                  │
│   - key3 → record3             │
│   - ...                        │
│ Index Block: key → block offset│
│ Bloom Filter Block             │
│ Footer: metadata + checksum    │
└────────────────────────────────┘
```

### Immutability

Once written, SSTs are **never modified**. This immutability provides several benefits:

1. **Concurrent reads**: Multiple readers can safely access an SST without locks
2. **Crash safety**: No torn writes or partial updates
3. **Simplified caching**: OS page cache can safely cache blocks
4. **Compaction simplicity**: Old SSTs are deleted atomically after compaction

Updates to existing keys result in new versions in the memtable or newer SSTs, shadowing older versions through multi-version concurrency control (MVCC).

## 20.2 Block-Based SST Design

KeystoneDB uses a **block-based architecture** inspired by LevelDB and RocksDB, where data is organized into fixed-size blocks.

### Why Blocks?

**4KB Block Size** aligns with:
- Operating system page size (typically 4KB)
- SSD erase block size (often multiples of 4KB)
- Filesystem block size (ext4, XFS use 4KB by default)

This alignment provides:
- **Efficient I/O**: Reading a block requires exactly one page fault
- **Better caching**: OS page cache operates on 4KB pages
- **Optimal for SSDs**: Aligned writes reduce write amplification

### Block Types

An SST file consists of multiple block types:

```
┌─────────────────────────────────────────────────┐
│ Data Blocks (4KB each)                          │
│ ┌────────────┬────────────┬────────────┐        │
│ │ Block 1    │ Block 2    │ Block 3    │ ...   │
│ │ Records:   │ Records:   │ Records:   │        │
│ │ key001-100 │ key101-200 │ key201-300 │        │
│ └────────────┴────────────┴────────────┘        │
├─────────────────────────────────────────────────┤
│ Index Block (4KB)                               │
│ Maps first key of each data block to offset:   │
│   key001 → offset 0                             │
│   key101 → offset 4096                          │
│   key201 → offset 8192                          │
├─────────────────────────────────────────────────┤
│ Bloom Filter Block (4KB)                        │
│ One bloom filter per data block                 │
│   Block 1 bloom: [bit array]                    │
│   Block 2 bloom: [bit array]                    │
│   Block 3 bloom: [bit array]                    │
├─────────────────────────────────────────────────┤
│ Footer Block (4KB)                              │
│   num_data_blocks: 3                            │
│   index_offset: 12288                           │
│   bloom_offset: 16384                           │
│   crc32c: 0x4F2A3B1C                            │
└─────────────────────────────────────────────────┘
```

### Constants and Configuration

```rust
const BLOCK_SIZE: usize = 4096;              // 4KB blocks
const MAX_RECORDS_PER_BLOCK: usize = 100;    // Limit for simplicity
const BITS_PER_KEY: usize = 10;              // ~1% false positive rate
const FOOTER_SIZE: usize = 24;               // Fixed footer size
```

These constants balance:
- **Block size**: Large enough to amortize metadata overhead, small enough for granular reads
- **Records per block**: Ensures reasonable block utilization without overflow
- **Bloom filter bits**: 10 bits/key provides ~1% false positive rate

## 20.3 SST File Format

### Overall Structure

The SST file format is carefully designed for efficient reading:

```
File Offset 0:
┌─────────────────────────────────────────────────┐
│ Data Block 0 (4096 bytes)                       │
│   Record count: u32                             │
│   For each record:                              │
│     - Shared prefix length: u32                 │
│     - Unshared suffix length: u32               │
│     - Key suffix: [u8]                          │
│     - Record data length: u32                   │
│     - Bincode-serialized record: [u8]           │
├─────────────────────────────────────────────────┤
│ Data Block 1 (4096 bytes)                       │
│   (same format as Block 0)                      │
├─────────────────────────────────────────────────┤
│ ...                                             │
├─────────────────────────────────────────────────┤
│ Index Block (4096 bytes)                        │
│   Entry count: u32                              │
│   For each entry:                               │
│     - Key length: u32                           │
│     - Key bytes: [u8]                           │
│     - Block offset: u64                         │
├─────────────────────────────────────────────────┤
│ Bloom Filter Block (4096 bytes)                 │
│   Filter count: u32                             │
│   For each filter:                              │
│     - Filter length: u32                        │
│     - Encoded bloom filter: [u8]                │
├─────────────────────────────────────────────────┤
│ Footer Block (4096 bytes)                       │
│   num_data_blocks: u32                          │
│   index_offset: u64                             │
│   bloom_offset: u64                             │
│   crc32c: u32                                   │
│   (padding to 4KB)                              │
└─────────────────────────────────────────────────┘
```

### Reading Strategy

The footer-first reading strategy enables efficient random access:

1. **Seek to end**: Read the last 4KB block (footer)
2. **Parse metadata**: Extract index_offset and bloom_offset
3. **Load index**: Read index block into memory (typically < 4KB)
4. **Load bloom filters**: Read bloom filter block into memory
5. **Ready for queries**: Can now perform efficient lookups

This approach means only **2-3 blocks** need to be read to initialize an SST reader, regardless of file size.

## 20.4 Prefix Compression

To reduce storage space, KeystoneDB uses **prefix compression** within data blocks.

### The Compression Algorithm

Since records within a block are sorted, consecutive keys often share common prefixes:

```
Original keys:
  user#alice#profile
  user#alice#settings
  user#alice#preferences
  user#bob#profile

Prefix-compressed:
  user#alice#profile      (shared=0, unshared=19, "user#alice#profile")
  user#alice#settings     (shared=12, unshared=8, "settings")
  user#alice#preferences  (shared=12, unshared=11, "preferences")
  user#bob#profile        (shared=5, unshared=11, "bob#profile")
```

### Encoding Implementation

```rust
fn encode_data_block(&self, records: &[Record]) -> Result<Bytes> {
    let mut buf = BytesMut::new();
    buf.put_u32_le(records.len() as u32);

    let mut prev_key = Bytes::new();

    for record in records {
        let key_enc = record.key.encode();

        // Calculate shared prefix length
        let shared = Self::shared_prefix_len(&prev_key, &key_enc);
        let unshared = key_enc.len() - shared;

        // Encode: shared length, unshared length, unshared bytes
        buf.put_u32_le(shared as u32);
        buf.put_u32_le(unshared as u32);
        buf.put_slice(&key_enc[shared..]);  // Only unshared portion

        // Encode record data
        let rec_data = bincode::serialize(record)?;
        buf.put_u32_le(rec_data.len() as u32);
        buf.put_slice(&rec_data);

        prev_key = key_enc;
    }

    Ok(buf.freeze())
}
```

### Decoding Implementation

Decoding reconstructs keys by maintaining the previous key:

```rust
fn decode_data_block(&self, data: &Bytes) -> Result<Vec<Record>> {
    let mut buf = data.clone();
    let count = buf.get_u32_le() as usize;

    let mut records = Vec::new();
    let mut prev_key = BytesMut::new();

    for _ in 0..count {
        let shared = buf.get_u32_le() as usize;
        let unshared = buf.get_u32_le() as usize;

        // Reconstruct key: keep shared prefix, append unshared suffix
        prev_key.truncate(shared);
        prev_key.extend_from_slice(&buf.copy_to_bytes(unshared));

        // Decode record
        let rec_len = buf.get_u32_le() as usize;
        let rec_data = buf.copy_to_bytes(rec_len);
        let record = bincode::deserialize(&rec_data)?;

        records.push(record);
    }

    Ok(records)
}
```

### Compression Ratio

Typical compression ratios for different key patterns:

| Key Pattern | Example | Compression Ratio |
|------------|---------|-------------------|
| Random UUIDs | `550e8400-e29b-...` | ~1.1x (minimal benefit) |
| User partitions | `user#123#profile` | ~2-3x (high shared prefix) |
| Time-series | `sensor#456#2024-01-01T00:00:00` | ~3-5x (date/time prefix) |
| Hierarchical | `org/dept/team/user` | ~2-4x (path prefix) |

### Trade-offs

**Pros:**
- Reduced storage space (2-5x for typical workloads)
- Fewer disk I/Os (more records fit per block)
- Lower memory usage (smaller blocks in cache)

**Cons:**
- Decoding overhead (~5-10μs per block)
- Sequential dependency (can't decode record N without record N-1)
- Slightly more complex implementation

The storage savings typically outweigh the small CPU overhead.

## 20.5 Index Blocks and Metadata

### Index Block Purpose

The index block provides a **sparse index** that maps keys to data blocks:

```
Index Block:
┌───────────────────────────────────────┐
│ Entry 1: "key001" → offset 0          │
│ Entry 2: "key101" → offset 4096       │
│ Entry 3: "key201" → offset 8192       │
│ Entry 4: "key301" → offset 12288      │
└───────────────────────────────────────┘

Data Blocks:
┌─────────────┬─────────────┬─────────────┐
│ Block 0     │ Block 1     │ Block 2     │
│ key001-100  │ key101-200  │ key201-300  │
└─────────────┴─────────────┴─────────────┘
```

Each index entry stores the **first key** of a data block, allowing binary search to find the correct block.

### Binary Search Algorithm

To find a key, binary search the index:

```rust
fn find_block(&self, key: &Bytes) -> Result<Option<u64>> {
    let mut result = None;

    // Linear scan (could be binary search for large indexes)
    for (first_key, offset) in &self.index {
        if key >= first_key {
            result = Some(*offset);  // This block or later
        } else {
            break;  // Found the right block
        }
    }

    Ok(result)
}
```

For example, searching for "key150":
1. "key150" >= "key001" → might be in block 0
2. "key150" >= "key101" → might be in block 1
3. "key150" < "key201" → stop, must be in block 1

### Index Size

Index size is proportional to the number of data blocks:

```
Index entry size:
├─ Key length: 4 bytes
├─ Key data: ~10-50 bytes (typical)
└─ Offset: 8 bytes
Total: ~20-60 bytes per entry

For 1000-block SST:
1000 entries × 40 bytes = ~40KB index

Typical SST (10 blocks):
10 entries × 40 bytes = ~400 bytes index
```

The index is **always kept in memory** during SST reads, providing O(log n) lookup time.

### Footer Format

The footer is a fixed-size structure at the end of the file:

```rust
struct Footer {
    num_data_blocks: u32,    // How many data blocks
    index_offset: u64,       // Where index block starts
    bloom_offset: u64,       // Where bloom filter block starts
    crc32c: u32,             // Checksum of above fields
}
```

Size: 4 + 8 + 8 + 4 = 24 bytes (padded to 4KB block)

The CRC checksum protects against corruption of critical metadata.

## 20.6 SST Reading and Writing

### Writing an SST

The write process transforms sorted records into a block-based file:

```rust
pub fn finish(
    mut self,
    file: &mut File,
    allocator: &ExtentAllocator,
) -> Result<SstBlockHandle> {
    // Step 1: Sort records by key
    self.records.sort_by(|a, b| {
        a.key.encode().cmp(&b.key.encode())
    });

    // Step 2: Split into data blocks (max 100 records each)
    let data_blocks = self.split_into_blocks();

    // Step 3: Allocate space on disk
    let estimated_blocks = data_blocks.len() + 2;  // data + index + bloom
    let extent = allocator.allocate(estimated_blocks * BLOCK_SIZE)?;

    // Step 4: Write data blocks
    let mut writer = BlockWriter::new(file.try_clone()?);
    let mut block_index = Vec::new();
    let mut bloom_filters = Vec::new();
    let mut current_offset = extent.offset;

    for (idx, records) in data_blocks.iter().enumerate() {
        let first_key = records[0].key.encode();

        // Build bloom filter for this block
        let mut bloom = BloomFilter::new(records.len(), BITS_PER_KEY);
        for rec in records {
            bloom.add(&rec.key.encode());
        }
        bloom_filters.push(bloom);

        // Encode and write data block
        let block_data = self.encode_data_block(records)?;
        let block = Block::new(idx as u64, block_data);
        writer.write(&block, current_offset)?;

        block_index.push((first_key, current_offset));
        current_offset += BLOCK_SIZE as u64;
    }

    // Step 5: Write index block
    let index_offset = current_offset;
    let index_data = self.encode_index_block(&block_index)?;
    writer.write(&Block::new(..., index_data), current_offset)?;
    current_offset += BLOCK_SIZE as u64;

    // Step 6: Write bloom filter block
    let bloom_offset = current_offset;
    let bloom_data = self.encode_bloom_block(&bloom_filters)?;
    writer.write(&Block::new(..., bloom_data), current_offset)?;
    current_offset += BLOCK_SIZE as u64;

    // Step 7: Write footer
    let footer_data = self.encode_footer(
        data_blocks.len(), index_offset, bloom_offset
    )?;
    writer.write(&Block::new(..., footer_data), current_offset)?;

    writer.flush()?;

    Ok(SstBlockHandle { extent, num_data_blocks, index_offset, bloom_offset })
}
```

### Reading an SST

Opening an SST reader loads metadata into memory:

```rust
pub fn open(file: File, handle: SstBlockHandle) -> Result<Self> {
    let mut reader = BlockReader::new(file.try_clone()?);

    // Read index block
    let index_block = reader.read(
        handle.num_data_blocks as u64,
        handle.index_offset
    )?;
    let index = Self::decode_index_block(&index_block.data)?;

    // Read bloom filters
    let bloom_block = reader.read(
        (handle.num_data_blocks + 1) as u64,
        handle.bloom_offset
    )?;
    let blooms = Self::decode_bloom_block(&bloom_block.data)?;

    Ok(Self { file, index, blooms })
}
```

Once opened, the reader can perform efficient lookups.

### Point Lookup

Finding a specific key involves multiple steps:

```rust
pub fn get(&self, key: &Key) -> Result<Option<Record>> {
    let key_enc = key.encode();

    // Step 1: Find which block might contain the key
    let block_offset = self.find_block(&key_enc)?;

    if let Some(offset) = block_offset {
        // Step 2: Check bloom filter
        let block_idx = self.index.values().position(|&o| o == offset).unwrap();
        if !self.blooms[block_idx].contains(&key_enc) {
            return Ok(None);  // Definitely not present
        }

        // Step 3: Read and decode data block
        let mut reader = BlockReader::new(self.file.try_clone()?);
        let block = reader.read(block_idx as u64, offset)?;
        let records = self.decode_data_block(&block.data)?;

        // Step 4: Linear search within block
        for record in records {
            if record.key == *key {
                return Ok(Some(record));
            }
        }
    }

    Ok(None)
}
```

### Performance Characteristics

**Best case (bloom filter negative):**
- Index lookup: O(log n) in-memory
- Bloom check: O(k) hash computations (k ≈ 7)
- Total: ~1-5μs, no disk I/O

**Typical case (key found):**
- Index lookup: ~1μs
- Bloom check: ~1μs
- Read block: ~100-1000μs (disk I/O)
- Decode block: ~5-10μs
- Linear search: ~1-5μs
- Total: ~100-1000μs

**Worst case (key not found, false positive):**
- Same as typical case, but key not in decoded records
- Total: ~100-1000μs wasted on false positive

## 20.7 Immutability Benefits

### Concurrent Access

Immutability enables lock-free concurrent reads:

```rust
// Multiple threads can read simultaneously
thread 1: sst_reader.get(key1)?;  ─┐
thread 2: sst_reader.get(key2)?;  ─┼─ No locks required
thread 3: sst_reader.get(key3)?;  ─┘
```

No coordination is needed because the data never changes.

### Simplified Caching

The OS page cache can aggressively cache SST blocks:

```
First read of block 5:
  disk read → page cache → application

Subsequent reads:
  page cache → application (no disk I/O)
```

Since blocks never change, cached data is always valid.

### Atomic Updates

SSTs are created atomically and swapped in:

```
Compaction:
1. Create new SST file (temp name)
2. Write all data, flush, fsync
3. Rename to final name (atomic operation)
4. Update manifest
5. Delete old SST files

At no point do readers see partial data.
```

The filesystem rename operation provides atomicity guarantees.

### Crash Safety

If a crash occurs during SST creation:

```
Scenario: Crash during write
Before:  old.sst (valid)
During:  old.sst (valid), new.sst.tmp (partial)
After:   old.sst (valid), new.sst.tmp (ignored)
```

Incomplete SSTs use a temporary extension and are ignored during recovery. The old SST remains valid.

### Copy-on-Write Semantics

Updates create new versions rather than modifying existing data:

```
Timeline:
t=0: Put(key1, "v1") → SST1 contains key1="v1"
t=1: Put(key1, "v2") → Memtable contains key1="v2", SST1 unchanged
t=2: Flush           → SST2 contains key1="v2", SST1 still unchanged

Read at t=0.5: Returns "v1" from SST1
Read at t=1.5: Returns "v2" from memtable (shadows SST1)
Read at t=2.5: Returns "v2" from SST2 (SST1 can be deleted)
```

This MVCC approach enables consistent reads without blocking writes.

## 20.8 SST Metadata and Handles

### SstBlockHandle Structure

The handle contains all information needed to read an SST:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SstBlockHandle {
    pub extent: Extent,          // Disk location (offset + size)
    pub num_data_blocks: usize,  // How many data blocks
    pub index_offset: u64,       // Where to find index
    pub bloom_offset: u64,       // Where to find bloom filters
    pub compressed: bool,        // Whether data is compressed
}
```

This structure is stored in the **manifest** (covered in a future chapter) and allows opening an SST without scanning the file.

### Extent Allocation

SSTs allocate contiguous space on disk:

```rust
pub struct Extent {
    pub id: u64,      // Unique extent ID
    pub offset: u64,  // Starting byte offset in file
    pub size: u64,    // Size in bytes
}
```

Contiguous allocation improves:
- **Sequential read performance**: One seek covers entire SST
- **Prefetching efficiency**: OS can read ahead effectively
- **Simplicity**: No fragmentation tracking needed

### Stripe Association

SSTs are associated with specific stripes in the 256-stripe LSM tree:

```
Database directory:
├── 000-1.sst   (stripe 0, SST ID 1)
├── 000-2.sst   (stripe 0, SST ID 2)
├── 042-1.sst   (stripe 42, SST ID 1)
├── 042-2.sst   (stripe 42, SST ID 2)
└── 255-1.sst   (stripe 255, SST ID 1)
```

Filename format: `{stripe:03}-{sst_id}.sst`

This organization allows:
- Easy identification of which stripe an SST belongs to
- Independent flushing per stripe
- Efficient compaction (only SSTs in same stripe are merged)

## 20.9 Advanced Features and Future Enhancements

### Compression (Planned)

Future versions may add Zstd compression to data blocks:

```rust
fn compress_data(&self, data: &Bytes) -> Result<Bytes> {
    // Current: No compression
    Ok(data.clone())

    // Future: Zstd compression
    // let compressed = zstd::encode_all(data.as_ref(), 3)?;
    // Ok(Bytes::from(compressed))
}
```

Expected benefits:
- 3-10x compression ratio for typical data
- Lower disk usage
- Potentially faster reads (less I/O despite CPU overhead)

### Block Cache

A block cache could avoid repeated disk reads:

```rust
struct BlockCache {
    cache: LruCache<(SstId, BlockIdx), Arc<Block>>,
    max_size: usize,
}
```

This would provide:
- Hot block caching in application memory
- Finer-grained control than OS page cache
- Cache hits avoid kernel context switch

### Checksums Per Block

Currently, only the footer has a checksum. Per-block checksums would improve corruption detection:

```rust
struct DataBlock {
    records: Vec<Record>,
    checksum: u32,  // CRC32C of encoded block
}
```

### Tiered Storage

Future enhancements could store older SSTs on slower, cheaper storage:

```
Hot tier (NVMe SSD):  Recent SSTs (last 7 days)
Warm tier (SATA SSD): Medium-age SSTs (7-90 days)
Cold tier (HDD/S3):   Old SSTs (90+ days)
```

## 20.10 Troubleshooting and Debugging

### Corrupt SST Detection

Signs of SST corruption:
- Read errors when accessing specific keys
- Checksum validation failures
- Deserialization errors

**Diagnostic steps:**
1. Check footer checksum
2. Verify file size matches extent size
3. Try reading index and bloom filter blocks
4. Scan data blocks sequentially

### Performance Issues

Symptoms of SST performance problems:
- High read latency despite memtable misses
- Excessive disk I/O
- Low bloom filter effectiveness

**Diagnostic steps:**
1. Check bloom filter false positive rate
2. Verify blocks are properly cached (check cache hit rate)
3. Measure disk I/O latency (iostat)
4. Check for SST file fragmentation

### Debugging Tools

Useful commands for inspecting SSTs:

```bash
# View SST file size
ls -lh mydb.keystone/*.sst

# Check for fragmentation (Linux)
filefrag mydb.keystone/000-1.sst

# Monitor disk I/O
iostat -x 1

# View block-level details (future tool)
kstone sst-inspect 000-1.sst
```

## 20.11 Summary

Sorted String Tables provide the durable, queryable storage layer in KeystoneDB:

**Key Takeaways:**
1. **Block-based design**: 4KB blocks aligned with OS and hardware
2. **Prefix compression**: 2-5x space savings for typical workloads
3. **Sparse indexing**: O(log n) key location with minimal memory
4. **Bloom filters**: Avoid 99% of unnecessary disk reads
5. **Immutability**: Enables concurrent access and crash safety

**Design Highlights:**
- Footer-first reading for efficient initialization
- Two-tier structure: index in memory, data on disk
- One bloom filter per data block for granular filtering
- Sorted records enable binary search and range queries

**Performance Characteristics:**
- **Write**: ~1ms to create 100-record SST
- **Read (hit)**: ~100-1000μs including disk I/O
- **Read (miss)**: ~1-5μs (bloom filter only)
- **Space**: 40-60% of uncompressed size (with prefix compression)

**Operational Benefits:**
- No maintenance required (immutable files)
- Works seamlessly with OS page cache
- Atomic creation and deletion
- Efficient range scans (sorted data)

In the next chapter, we'll explore **compaction**, the process that merges multiple SSTs to reclaim space and improve read performance.

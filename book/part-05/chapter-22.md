# Chapter 22: Bloom Filters & Optimization

Bloom filters are the secret weapon that makes KeystoneDB's reads fast. By answering "Is this key definitely NOT in this SST?" in microseconds, bloom filters eliminate up to 99% of unnecessary disk reads. This chapter explores the theory, implementation, and tuning of bloom filters in KeystoneDB.

## 22.1 What Are Bloom Filters?

### The Negative Lookup Problem

Consider searching for a key across multiple SSTs:

```
Query: get("user#999")

Check SST-1: Read bloom filter → "might contain"
            → Read data block → NOT FOUND (false positive!)

Check SST-2: Read bloom filter → "might contain"
            → Read data block → NOT FOUND (false positive!)

Check SST-3: Read bloom filter → "definitely not present"
            → Skip disk read (bloom filter saved us!)

Check SST-4: Read bloom filter → "might contain"
            → Read data block → FOUND!
```

Without bloom filters, we'd need to read data blocks from all SSTs. With bloom filters, we skip most reads.

### The Bloom Filter Data Structure

A bloom filter is a **probabilistic data structure** that supports two operations:

1. **add(key)**: Mark a key as present
2. **contains(key)**: Test if a key might be present

**Key properties:**
- **No false negatives**: If `contains(key)` returns false, the key is definitely not present
- **Possible false positives**: If `contains(key)` returns true, the key might not actually be present
- **Space-efficient**: Uses bits, not bytes (typically 10 bits per key for 1% false positive rate)
- **Constant-time**: Both operations are O(k) where k is the number of hash functions (typically 7)

### How Bloom Filters Work

A bloom filter consists of:
1. A **bit array** of m bits (all initially 0)
2. A set of **k hash functions** (h₁, h₂, ..., hₖ)

**Adding a key:**
```
add("user#123"):
  1. Compute h₁("user#123") = 42  → Set bit 42 to 1
  2. Compute h₂("user#123") = 157 → Set bit 157 to 1
  3. Compute h₃("user#123") = 891 → Set bit 891 to 1
  ...
  k. Compute hₖ("user#123") = 523 → Set bit 523 to 1
```

**Testing a key:**
```
contains("user#123"):
  1. Compute h₁("user#123") = 42  → Check bit 42 (is 1? ✓)
  2. Compute h₂("user#123") = 157 → Check bit 157 (is 1? ✓)
  3. Compute h₃("user#123") = 891 → Check bit 891 (is 1? ✓)
  ...
  k. Compute hₖ("user#123") = 523 → Check bit 523 (is 1? ✓)
  All bits are 1 → return true (might be present)

contains("user#456"):
  1. Compute h₁("user#456") = 103 → Check bit 103 (is 1? ✓)
  2. Compute h₂("user#456") = 299 → Check bit 299 (is 1? ✗)
  At least one bit is 0 → return false (definitely not present)
```

### Visual Example

```
Bit array (20 bits):
[0 1 0 0 1 0 1 0 0 0 1 0 0 1 0 0 0 1 0 0]
 ↑   ↑   ↑       ↑     ↑   ↑     ↑

add("alice"):
  h₁=1, h₂=4, h₃=10 → Set bits 1, 4, 10

add("bob"):
  h₁=6, h₂=13, h₃=17 → Set bits 6, 13, 17

contains("alice"):
  Check bits 1, 4, 10 → All are 1 → true (correct!)

contains("carol"):
  Check bits 3, 9, 15 → bit 3 is 0 → false (correct!)

contains("dave"):
  Check bits 1, 6, 13 → All are 1 → true (FALSE POSITIVE!)
  (Bits were set by "alice" and "bob", not "dave")
```

## 22.2 Implementation in KeystoneDB

KeystoneDB creates one bloom filter per data block in each SST.

### BloomFilter Structure

```rust
pub struct BloomFilter {
    bits: Vec<u8>,        // Bit array (8 bits per byte)
    num_bits: usize,      // Total number of bits
    num_hashes: u32,      // Number of hash functions (k)
}
```

### Creating a Bloom Filter

```rust
pub fn new(num_items: usize, bits_per_key: usize) -> Self {
    // Calculate total bits needed
    let num_bits = num_items * bits_per_key;
    let num_bytes = (num_bits + 7) / 8;  // Round up to nearest byte

    // Optimal number of hash functions: k = (m/n) * ln(2)
    // For bits_per_key = 10: k ≈ 10 * 0.693 = 6.93 ≈ 7
    let num_hashes = ((bits_per_key as f64) * 0.693).ceil() as u32;
    let num_hashes = num_hashes.max(1).min(30);  // Clamp to 1-30

    Self {
        bits: vec![0u8; num_bytes],
        num_bits,
        num_hashes,
    }
}
```

**Example:**
```rust
// 100 records, 10 bits per key
let bloom = BloomFilter::new(100, 10);

// Results:
// num_bits = 100 * 10 = 1000 bits
// num_bytes = (1000 + 7) / 8 = 125 bytes
// num_hashes = 10 * 0.693 = 6.93 ≈ 7 hash functions
```

### Adding Keys

```rust
pub fn add(&mut self, key: &[u8]) {
    let hash = Self::hash(key);  // Primary hash

    for i in 0..self.num_hashes {
        // Double hashing: combine two hash values
        let bit_pos = Self::bloom_hash(hash, i) % (self.num_bits as u64);
        self.set_bit(bit_pos as usize);
    }
}

fn set_bit(&mut self, pos: usize) {
    let byte_idx = pos / 8;       // Which byte
    let bit_idx = pos % 8;        // Which bit in that byte
    if byte_idx < self.bits.len() {
        self.bits[byte_idx] |= 1 << bit_idx;  // Set bit to 1
    }
}
```

### Testing Keys

```rust
pub fn contains(&self, key: &[u8]) -> bool {
    let hash = Self::hash(key);

    for i in 0..self.num_hashes {
        let bit_pos = Self::bloom_hash(hash, i) % (self.num_bits as u64);
        if !self.get_bit(bit_pos as usize) {
            return false;  // Definitely not present
        }
    }

    true  // Might be present
}

fn get_bit(&self, pos: usize) -> bool {
    let byte_idx = pos / 8;
    let bit_idx = pos % 8;
    if byte_idx < self.bits.len() {
        (self.bits[byte_idx] & (1 << bit_idx)) != 0
    } else {
        false
    }
}
```

### Hash Functions

KeystoneDB uses **FNV-1a hash** as the primary hash function:

```rust
fn hash(key: &[u8]) -> u64 {
    // FNV-1a: Fast, good distribution
    let mut hash = 0xcbf29ce484222325u64;  // FNV offset basis
    for &byte in key {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);  // FNV prime
    }
    hash
}
```

**Why FNV-1a?**
- Fast (just XOR and multiply per byte)
- Good distribution (low collision rate)
- Simple (no lookup tables or complex operations)
- No dependencies (pure Rust implementation)

For multiple hash functions, **double hashing** is used:

```rust
fn bloom_hash(hash: u64, i: u32) -> u64 {
    let h1 = hash;
    let h2 = hash.wrapping_shr(32);  // Upper 32 bits
    h1.wrapping_add((i as u64).wrapping_mul(h2))
}
```

This technique derives k independent hash functions from a single hash value, avoiding the overhead of computing k separate hashes.

### Serialization

Bloom filters are serialized to disk as part of SST files:

```rust
pub fn encode(&self) -> Bytes {
    let mut buf = BytesMut::new();
    buf.put_u32_le(self.num_bits as u32);
    buf.put_u32_le(self.num_hashes);
    buf.put_slice(&self.bits);
    buf.freeze()
}

pub fn decode(data: &[u8]) -> Option<Self> {
    if data.len() < 8 { return None; }

    let num_bits = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let num_hashes = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);

    let num_bytes = (num_bits + 7) / 8;
    if data.len() < 8 + num_bytes { return None; }

    let bits = data[8..8 + num_bytes].to_vec();

    Some(Self { bits, num_bits, num_hashes })
}
```

Format:
```
[num_bits: u32] [num_hashes: u32] [bit_array: bytes]
     4 bytes         4 bytes       variable length
```

## 22.3 False Positive Rates

### Probability Theory

The false positive rate depends on three parameters:
- **m**: Number of bits in the filter
- **n**: Number of items added
- **k**: Number of hash functions

**Formula:**
```
FPR = (1 - e^(-kn/m))^k

Where:
- e ≈ 2.71828 (Euler's number)
- k is optimal when k = (m/n) * ln(2)
```

### Practical Examples

For 10 bits per key (m/n = 10):

```rust
k = 10 * 0.693 = 6.93 ≈ 7 hash functions
FPR = (1 - e^(-7*n/(10*n)))^7
    = (1 - e^(-0.7))^7
    = (1 - 0.4966)^7
    = 0.5034^7
    ≈ 0.0081
    ≈ 0.8%
```

So with 10 bits per key, we get approximately **0.8-1% false positive rate**.

### False Positive Rate vs. Bits Per Key

| Bits/Key | Hash Functions (k) | FPR | Memory per 1000 keys |
|----------|-------------------|-----|----------------------|
| 4 | 3 | ~18% | 500 bytes |
| 6 | 4 | ~6% | 750 bytes |
| 8 | 6 | ~2% | 1000 bytes |
| 10 | 7 | ~0.8% | 1250 bytes |
| 12 | 8 | ~0.4% | 1500 bytes |
| 16 | 11 | ~0.05% | 2000 bytes |
| 20 | 14 | ~0.008% | 2500 bytes |

KeystoneDB uses **10 bits/key as the default**, providing a good balance of memory usage and false positive rate.

### Impact on Read Performance

With 10 SSTs to check:

**Without bloom filters:**
```
10 SSTs × 1ms disk read = 10ms total
```

**With bloom filters (1% FPR):**
```
10 SSTs × (1% chance of false positive) × 1ms = 0.1ms average
Plus 10 bloom filter checks × 1μs = 0.01ms
Total: ~0.11ms (100x faster!)
```

Even with false positives, bloom filters provide a massive performance improvement.

### Measuring False Positive Rate

Test the actual false positive rate:

```rust
let mut bloom = BloomFilter::new(1000, 10);

// Add 1000 keys
for i in 0..1000 {
    let key = format!("key{}", i);
    bloom.add(key.as_bytes());
}

// Test 10000 keys that weren't added
let mut false_positives = 0;
for i in 1000..11000 {
    let key = format!("key{}", i);
    if bloom.contains(key.as_bytes()) {
        false_positives += 1;
    }
}

let fpr = (false_positives as f64) / 10000.0;
println!("False positive rate: {:.2}%", fpr * 100.0);
// Output: False positive rate: 0.81%
```

## 22.4 Memory vs Accuracy Trade-offs

### Memory Budget

For a database with 1 million keys:

| Bits/Key | Total Memory | FPR | Read Efficiency |
|----------|-------------|-----|-----------------|
| 4 | 500 KB | 18% | Poor (many false positives) |
| 8 | 1 MB | 2% | Good |
| 10 | 1.25 MB | 0.8% | Very good (default) |
| 16 | 2 MB | 0.05% | Excellent |
| 32 | 4 MB | 0.0000001% | Overkill |

### Tuning for Different Workloads

**Memory-Constrained Systems:**
```rust
// Use 5 bits/key instead of 10
// FPR: ~6% instead of ~1%
// Memory: 50% reduction
const BITS_PER_KEY: usize = 5;
```

Trade-off: More false positives (6 per 100 queries) but half the memory.

**Read-Heavy Workloads:**
```rust
// Use 16 bits/key for near-perfect accuracy
// FPR: ~0.05%
// Memory: 60% increase
const BITS_PER_KEY: usize = 16;
```

Trade-off: Higher memory usage but 16x fewer false positives.

**Write-Heavy Workloads:**
```rust
// Use 8 bits/key (smaller filters = faster to build)
// FPR: ~2%
// Memory: 20% reduction
const BITS_PER_KEY: usize = 8;
```

Trade-off: Slightly more false positives but faster SST creation.

### Per-Block vs. Per-SST Bloom Filters

KeystoneDB uses **per-block bloom filters**:

```
SST with 10 data blocks:
├─ Block 1: 100 records → Bloom filter (1000 bits)
├─ Block 2: 100 records → Bloom filter (1000 bits)
├─ ...
└─ Block 10: 100 records → Bloom filter (1000 bits)

Total: 10 bloom filters × 125 bytes = 1250 bytes
```

**Alternative: Per-SST bloom filter:**
```
SST with 1000 total records:
└─ Bloom filter (10000 bits = 1250 bytes)
```

**Per-block advantages:**
- Finer granularity (skip individual blocks)
- Better cache locality (only load relevant block's filter)
- Parallelizable (check blocks independently)

**Per-SST advantages:**
- Simpler implementation
- Slightly less memory overhead
- Single filter check instead of multiple

KeystoneDB chooses per-block for the finer granularity.

## 22.5 Read Optimization Techniques

### Short-Circuit on First Negative

When checking multiple SSTs, stop on the first negative:

```rust
for sst in ssts.iter().rev() {  // Newest to oldest
    if !sst.bloom.contains(&key) {
        continue;  // Skip this SST entirely
    }

    // Bloom says "might contain" - read the block
    if let Some(record) = sst.read_block_and_search(&key)? {
        return Ok(Some(record));  // Found! Stop searching
    }
}

Ok(None)  // Not found in any SST
```

### Bloom Filter Caching

Keep bloom filters in memory for all open SSTs:

```rust
struct SstReader {
    file: File,
    index: BTreeMap<Bytes, u64>,
    blooms: Vec<BloomFilter>,  // All loaded in memory
}
```

This avoids disk reads for bloom filter checks (they're always in RAM).

### Batch Bloom Filter Checks

For range queries, batch bloom filter checks:

```rust
// Check if any key in range might be in this SST
fn might_contain_range(&self, start: &[u8], end: &[u8]) -> bool {
    // Use index to find relevant blocks
    let blocks = self.index.range(start..end);

    // Check bloom filter for each block
    for block_idx in blocks {
        // If any block might contain keys, SST might be relevant
        if !self.blooms[block_idx].is_empty() {
            return true;
        }
    }

    false  // No blocks in range, skip this SST
}
```

### Adaptive Bloom Filter Sizing

Adjust bloom filter size based on block size:

```rust
fn create_bloom_for_block(records: &[Record]) -> BloomFilter {
    let num_records = records.len();

    // More records = larger bloom filter
    let bits_per_key = if num_records > 500 {
        12  // Larger blocks get more accurate filters
    } else if num_records > 100 {
        10  // Default
    } else {
        8   // Small blocks don't need high accuracy
    };

    BloomFilter::new(num_records, bits_per_key)
}
```

## 22.6 Performance Impact

### Bloom Filter Check Latency

Measuring bloom filter performance:

```rust
let bloom = BloomFilter::new(1000, 10);

// Add 1000 keys
for i in 0..1000 {
    bloom.add(format!("key{}", i).as_bytes());
}

// Benchmark contains() calls
let start = Instant::now();
for i in 0..100000 {
    bloom.contains(format!("key{}", i % 2000).as_bytes());
}
let elapsed = start.elapsed();

println!("Average: {}ns per check", elapsed.as_nanos() / 100000);
// Output: Average: ~50-200ns per check
```

Typical latencies:
- **Bloom filter check**: 50-200ns (nanoseconds!)
- **Disk block read**: 100,000-1,000,000ns (100-1000μs)

Bloom filter is **500-10,000x faster** than disk read.

### Impact on Overall Read Latency

Read latency breakdown:

**Without bloom filters:**
```
Total: ~5000μs
├─ Lock acquisition: 5μs
├─ Memtable lookup: 10μs (miss)
└─ SST scans: ~5000μs
   ├─ SST 1: Read block (1000μs) + search (5μs)
   ├─ SST 2: Read block (1000μs) + search (5μs)
   ├─ SST 3: Read block (1000μs) + search (5μs)
   ├─ SST 4: Read block (1000μs) + search (5μs)
   └─ SST 5: Read block (1000μs) + search (5μs)
```

**With bloom filters (1% FPR):**
```
Total: ~120μs
├─ Lock acquisition: 5μs
├─ Memtable lookup: 10μs (miss)
└─ SST scans: ~105μs
   ├─ SST 1: Bloom check (0.1μs) → SKIP
   ├─ SST 2: Bloom check (0.1μs) → SKIP
   ├─ SST 3: Bloom check (0.1μs) → SKIP
   ├─ SST 4: Bloom check (0.1μs) → SKIP
   └─ SST 5: Bloom check (0.1μs) + Read block (100μs) + search (5μs)

Speedup: 5000μs / 120μs = ~42x faster!
```

### Throughput Impact

For a read-heavy workload:

| Scenario | Throughput (ops/sec) | P99 Latency |
|----------|---------------------|-------------|
| No bloom filters | ~1,000 | 10ms |
| 8 bits/key (2% FPR) | ~8,000 | 2ms |
| 10 bits/key (1% FPR) | ~10,000 | 1.5ms |
| 16 bits/key (0.05% FPR) | ~12,000 | 1ms |

Returns diminish beyond 10 bits/key - the default is well-chosen.

### Memory Access Patterns

Bloom filters exhibit excellent cache locality:

```
Bloom filter size: 1000 bits = 125 bytes
L1 cache line: 64 bytes

Accessing bloom filter:
├─ Load bytes 0-63 (cache line 1) → L1 hit
└─ Load bytes 64-127 (cache line 2) → L1 hit

Total: 2 cache lines, both fit in L1
```

Contrast with disk block:
```
Data block size: 4096 bytes = 64 cache lines
Often evicted from cache (too large for L1/L2)
Requires disk I/O (100-1000μs)
```

## 22.7 Bloom Filter Size Estimation

### Calculating Memory Requirements

For a database with:
- 1 million records
- 256 stripes
- Average 100 SSTs per stripe
- Average 100 records per SST

**Per-SST bloom filter memory:**
```
100 records × 10 bits/key = 1000 bits = 125 bytes
```

**Total bloom filter memory:**
```
256 stripes × 100 SSTs/stripe × 125 bytes/SST = 3.2 MB
```

This is tiny compared to typical database sizes (GBs to TBs).

### Bloom Filter Overhead Ratio

```
Data size: 1 million records × 200 bytes/record = 200 MB
Bloom filters: 3.2 MB

Overhead: 3.2 MB / 200 MB = 1.6%
```

Bloom filters add negligible overhead (<2% of data size) while providing massive read speedup.

### Scaling with Database Size

| Database Size | Records | SSTs | Bloom Memory | Overhead |
|---------------|---------|------|--------------|----------|
| 10 MB | 50,000 | 5 | 62 KB | 0.6% |
| 100 MB | 500,000 | 50 | 625 KB | 0.6% |
| 1 GB | 5,000,000 | 500 | 6.25 MB | 0.6% |
| 10 GB | 50,000,000 | 5,000 | 62.5 MB | 0.6% |
| 100 GB | 500,000,000 | 50,000 | 625 MB | 0.6% |

Overhead remains consistently low (~0.6%) regardless of database size.

## 22.8 Advanced Topics

### Counting Bloom Filters

Standard bloom filters can't be deleted from. **Counting bloom filters** solve this:

```rust
// Instead of bits, use counts
struct CountingBloomFilter {
    counts: Vec<u8>,  // 4-bit or 8-bit counters
    num_counts: usize,
    num_hashes: u32,
}

impl CountingBloomFilter {
    fn add(&mut self, key: &[u8]) {
        for i in 0..self.num_hashes {
            let pos = self.hash(key, i);
            self.counts[pos] = self.counts[pos].saturating_add(1);
        }
    }

    fn remove(&mut self, key: &[u8]) {
        for i in 0..self.num_hashes {
            let pos = self.hash(key, i);
            self.counts[pos] = self.counts[pos].saturating_sub(1);
        }
    }
}
```

KeystoneDB doesn't currently use counting bloom filters because SSTs are immutable (no need to remove).

### Compressed Bloom Filters

Compress bloom filters for storage:

```rust
// Before compression: 1250 bytes
let bloom_data = bloom.encode();

// After compression: ~400 bytes (3x reduction)
let compressed = zstd::compress(&bloom_data, 3)?;
```

Trade-off: CPU overhead to decompress vs. disk space savings.

### Bloom Filter Partitioning

Partition large bloom filters for better cache performance:

```rust
struct PartitionedBloomFilter {
    partitions: Vec<BloomFilter>,  // e.g., 8 partitions
}

impl PartitionedBloomFilter {
    fn contains(&self, key: &[u8]) -> bool {
        let partition_idx = hash(key) % self.partitions.len();
        self.partitions[partition_idx].contains(key)
    }
}
```

Benefits: Each partition fits in cache, reducing memory bandwidth.

## 22.9 Troubleshooting

### High False Positive Rate

**Symptoms:**
- Many disk reads despite bloom filters
- Performance not as good as expected

**Causes:**
1. Bloom filter too small (bits_per_key too low)
2. Too many items in bloom filter (filter oversaturated)
3. Hash function collisions

**Diagnosis:**
```rust
// Measure actual FPR
let mut false_positives = 0;
let mut total_checks = 0;

for key in test_keys {
    total_checks += 1;
    if bloom.contains(key) && !actually_present(key) {
        false_positives += 1;
    }
}

let fpr = false_positives as f64 / total_checks as f64;
println!("Actual FPR: {:.2}%", fpr * 100.0);
```

**Fixes:**
```rust
// Increase bits per key
const BITS_PER_KEY: usize = 16;  // Instead of 10

// Or split large blocks into smaller ones
const MAX_RECORDS_PER_BLOCK: usize = 50;  // Instead of 100
```

### Bloom Filter Memory Overhead

**Symptoms:**
- High memory usage
- OOM (out of memory) errors

**Causes:**
1. Too many SSTs (compaction not running)
2. Bits per key set too high
3. Very large blocks (many records per filter)

**Diagnosis:**
```rust
let stats = engine.compaction_stats();
let bloom_memory = estimate_bloom_memory(&stats);
println!("Bloom filter memory: {}MB", bloom_memory / 1_000_000);
```

**Fixes:**
```rust
// Reduce bits per key
const BITS_PER_KEY: usize = 8;  // Instead of 10

// Trigger more aggressive compaction
CompactionConfig::new().with_sst_threshold(5)

// Reduce block size
const MAX_RECORDS_PER_BLOCK: usize = 50
```

### Bloom Filter Not Effective

**Symptoms:**
- Bloom filters present but still seeing many disk reads
- No performance improvement

**Causes:**
1. Workload is mostly positive lookups (keys exist)
2. False positive rate too high
3. Bloom filters not actually being checked

**Diagnosis:**
```rust
// Check if bloom filters are loaded
assert!(!sst_reader.blooms.is_empty());

// Check if they're being used
let start = Instant::now();
let result = sst_reader.get(&key)?;
let elapsed = start.elapsed();

// Should be < 10μs if bloom filter said "not present"
// Should be ~100-1000μs if bloom filter said "might be present"
```

## 22.10 Summary

Bloom filters are a critical optimization in KeystoneDB:

**Key Takeaways:**
1. **Probabilistic data structure**: No false negatives, small false positive rate
2. **Space-efficient**: 10 bits per key provides ~1% FPR with only 1.25 bytes overhead
3. **Fast**: Checks complete in 50-200ns (nanoseconds), 1000x faster than disk
4. **Effective**: Eliminates 99% of unnecessary disk reads
5. **Scalable**: Overhead remains constant ~0.6% regardless of database size

**Implementation Highlights:**
- FNV-1a hash with double hashing for multiple hash functions
- 7 hash functions optimal for 10 bits/key
- Per-block filters for finer granularity
- All filters kept in memory for instant access

**Configuration Guidelines:**
- **Default (balanced)**: 10 bits/key, ~1% FPR, 1.25 bytes overhead
- **Memory-constrained**: 6-8 bits/key, ~2-6% FPR, 0.75-1 bytes overhead
- **Read-optimized**: 16 bits/key, ~0.05% FPR, 2 bytes overhead

**Performance Impact:**
- Read latency: 100-1000μs → 10-200μs (10-100x improvement)
- Throughput: 1,000 ops/sec → 10,000 ops/sec (10x improvement)
- Memory overhead: ~0.6% of data size

In the final chapter of this part, we'll bring everything together and explore **Performance Tuning** - how to configure and optimize KeystoneDB for different workloads.

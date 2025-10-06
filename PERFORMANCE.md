# KeystoneDB Performance Guide

This document provides performance characteristics, optimization techniques, and best practices for KeystoneDB.

## Table of Contents

1. [Performance Characteristics](#performance-characteristics)
2. [Best Practices](#best-practices)
3. [Optimization Techniques](#optimization-techniques)
4. [Monitoring and Tuning](#monitoring-and-tuning)
5. [Benchmarking](#benchmarking)

## Performance Characteristics

### Write Performance

**Throughput:** ~10-50k operations/second

**Factors:**
- **Memtable flush frequency** (default: 4MB per stripe, with 10k record ceiling)
- **WAL fsync overhead** (durability vs performance trade-off)
- **Group commit effectiveness** (more concurrent writes = better batching)
- **Stripe distribution** (256 stripes enable parallel writes)
- **Compression enabled** (CPU overhead for smaller files)

**Characteristics:**
- ✅ **Fast writes**: O(log n) memtable insert + sequential WAL append
- ✅ **No read amplification**: Writes don't require reading existing data
- ⚠️ **Write amplification**: Compaction rewrites data (typical for LSM trees)
- ⚠️ **Flush pauses**: Brief stalls when memtable flushes to SST

**Write Latency Breakdown:**
```
Total: ~100-500μs per write
├─ Memtable insert: 5-10μs (in-memory BTreeMap)
├─ WAL append: 50-200μs (disk write + fsync)
└─ Lock acquisition: <5μs (usually uncontended)
```

### Read Performance

**Throughput:**
- **Hot data (memtable):** ~100k+ ops/second
- **Cold data (SST):** ~10k ops/second

**Factors:**
- **Data location**: Memtable (fast) vs SST (slower)
- **SST count**: More SSTs = more files to check
- **Bloom filter effectiveness**: Reduces unnecessary disk reads
- **Compaction state**: Fewer SSTs after compaction = faster reads

**Characteristics:**
- ✅ **Fast memtable reads**: O(log n) in-memory lookup
- ✅ **Bloom filter optimization**: Skips SSTs that don't contain key
- ⚠️ **Read amplification**: May need to check multiple SSTs
- ⚠️ **Cache cold starts**: First read of SST requires disk I/O

**Read Latency Breakdown:**
```
Memtable hit: ~10-50μs
├─ Acquire read lock: <5μs
└─ BTreeMap lookup: 5-45μs

SST hit: ~100-1000μs
├─ Bloom filter check: 1-5μs per SST
├─ Binary search: 10-100μs per SST
└─ Disk I/O: 100-1000μs (mmap or read)
```

### Query Performance

**Partition Key Query (Efficient):**
- **Throughput:** ~10-50k ops/second
- **Pattern:** `Query::new().partition_key(b"user#123")`
- **Why fast:** Direct stripe selection, range scan within stripe

**Sort Key Range Query:**
- **Throughput:** ~5-20k ops/second (depends on range size)
- **Pattern:** `Query::new().partition_key(pk).sort_key_between(a, b)`
- **Why slower:** Must scan range in memtable + all SSTs

**Scan (Full Table, Slow):**
- **Throughput:** ~1-5k ops/second (highly variable)
- **Pattern:** `ScanBuilder::new().build()`
- **Why slow:** Must scan all 256 stripes, all SSTs
- **Mitigation:** Use parallel scan for large tables

**Performance Comparison:**
```
Operation                    Relative Performance
────────────────────────────────────────────────────
Get by pk                    ████████████ Fastest
Query by pk + sk             ██████████░░ Fast
Query by pk (range)          ████████░░░░ Moderate
Scan with filter             ███░░░░░░░░░ Slow
Scan (full table)            █░░░░░░░░░░░ Slowest
```

### Scan Performance

**Sequential Scan:**
- **Single-threaded:** ~1-5k items/second
- **256 stripes scanned sequentially**

**Parallel Scan:**
- **Multi-threaded:** ~5-20k items/second
- **Up to 256 segments** (1 segment per stripe)
- **Linear scalability** with segment count

**Example Parallel Scan:**
```rust
// Segment 0 of 4 (processes stripes 0, 4, 8, ...)
let scan1 = ScanBuilder::new().parallel(4, 0).build();

// Segment 1 of 4 (processes stripes 1, 5, 9, ...)
let scan2 = ScanBuilder::new().parallel(4, 1).build();

// Run in parallel
let (result1, result2) = rayon::join(
    || db.scan(scan1),
    || db.scan(scan2),
);
```

### Compaction Impact

**During Compaction:**
- **Write throughput:** ~70-90% of normal (compaction uses write lock)
- **Read throughput:** Mostly unaffected (uses read lock)
- **Disk I/O:** Increases significantly
- **Latency:** May see occasional spikes (100-500ms)

**After Compaction:**
- **Read throughput:** +20-50% improvement (fewer SSTs to scan)
- **Write throughput:** Returns to normal
- **Disk space:** Reduced (tombstones and duplicates removed)

### Size-Based Memtable Flushing

**Default behavior** (as of v0.1.1):
- **Primary trigger**: Memtable reaches 4MB in size
- **Safety ceiling**: 10,000 records (rarely hit with normal-sized records)
- **Benefits**: Predictable memory usage, better handling of variable record sizes

**Configuration:**
```rust
use kstone_api::DatabaseConfig;

// Default: 4MB per stripe (~1GB total for 256 stripes)
let config = DatabaseConfig::new();

// Adjust for high-memory systems (8MB per stripe)
let config = DatabaseConfig::new()
    .with_max_memtable_size_bytes(8 * 1024 * 1024)
    .with_max_memtable_records(20_000);

// Or for memory-constrained systems (2MB per stripe)
let config = DatabaseConfig::new()
    .with_max_memtable_size_bytes(2 * 1024 * 1024)
    .with_max_memtable_records(5_000);
```

**Performance Impact:**

| Memtable Size | Memory (256 stripes) | Flushes per 1M writes | Write Throughput |
|---------------|---------------------|----------------------|------------------|
| 2MB | ~512MB | ~500 | Baseline |
| 4MB (default) | ~1GB | ~250 | +20% |
| 8MB | ~2GB | ~125 | +35% |

Larger memtables = fewer flushes = higher throughput, but longer recovery time.

### Compression Trade-offs

**Zstd Compression** (optional, disabled by default):
```rust
use kstone_api::DatabaseConfig;

// Enable compression with default level (3)
let config = DatabaseConfig::new()
    .with_compression();

// Or specify level (1-22)
let config = DatabaseConfig::new()
    .with_compression_level(6);  // Higher compression, more CPU
```

**Performance Characteristics:**

| Compression Level | Write Speed | Storage Savings | CPU Usage |
|-------------------|-------------|-----------------|-----------|
| Disabled | 100% | 0% | Minimal |
| Level 1 | 90-95% | 20-40% | Low |
| Level 3 (default) | 80-90% | 40-60% | Medium |
| Level 6 | 60-70% | 50-70% | High |
| Level 10+ | 30-40% | 60-80% | Very High |

**When to Use Compression:**
- ✅ **Large text data** (JSON, XML, logs): 3-5x compression
- ✅ **Repetitive data**: 2-4x compression
- ✅ **Storage-constrained systems**: Trade CPU for disk space
- ❌ **Already-compressed data** (images, videos): Minimal benefit
- ❌ **CPU-constrained systems**: Not worth the overhead
- ❌ **Hot data path**: Better for cold storage/archives

**Recommendation:** Start with compression disabled. Enable level 3 if storage becomes an issue and CPU is not a bottleneck.

### Schema Validation Overhead

**Schema validation** adds minimal overhead:
- **Put operations**: +5-10μs per attribute validated
- **Get operations**: No overhead (validation only on writes)
- **Typical overhead**: <1% for most workloads

```rust
use kstone_api::{TableSchema, AttributeSchema, AttributeType, ValueConstraint};

let schema = TableSchema::new()
    .with_attribute(
        AttributeSchema::new("email", AttributeType::String)
            .required()
            .with_constraint(ValueConstraint::Pattern(r"^.+@.+\..+$".to_string()))
    );

// Validation happens automatically on put/update operations
// Minimal performance impact: ~5-10μs per validated attribute
```

## Best Practices

### 1. Always Use Partition Key for Queries

**❌ Avoid:**
```rust
// Scan entire table (SLOW)
let scan = ScanBuilder::new()
    .filter_expression("email = :email")
    .expression_value(":email", "alice@example.com")
    .build();
```

**✅ Prefer:**
```rust
// Query by partition key (FAST)
let query = Query::new()
    .partition_key(b"user#123");
```

**Why:** Queries with partition key go directly to the correct stripe and use efficient range scans. Scans must check all 256 stripes.

### 2. Use Indexes for Non-Key Queries

**Scenario:** Need to query users by email address

**❌ Avoid:**
```rust
// Full table scan (SLOW)
let scan = ScanBuilder::new()
    .filter_expression("email = :email")
    .expression_value(":email", "alice@example.com")
    .build();
```

**✅ Better:**
```rust
// Create GSI on email
let db = Database::create_with_options(path, options)?;

// Add GSI configuration in schema
// gsi_pk = email, gsi_sk = timestamp

// Query GSI (FAST)
let query = Query::new()
    .partition_key(b"alice@example.com")
    .index_name("by-email");
```

**Why:** GSI allows efficient queries by non-key attributes. Trade-off is additional storage and write overhead.

### 3. Use LIMIT to Reduce Data Transfer

**❌ Inefficient:**
```rust
// Fetch all results, process only 10
let response = db.scan(ScanBuilder::new().build())?;
let first_10: Vec<_> = response.items.into_iter().take(10).collect();
```

**✅ Efficient:**
```rust
// Fetch only 10 items
let response = db.scan(
    ScanBuilder::new()
        .limit(10)
        .build()
)?;
```

**Why:** LIMIT reduces I/O, memory usage, and network transfer (if remote). Storage engine can short-circuit after finding N items.

### 4. Use SELECT Projection

**❌ Wasteful:**
```sql
-- Fetch all attributes, use only name and email
SELECT * FROM users WHERE pk = 'user#123'
```

**✅ Efficient:**
```sql
-- Fetch only needed attributes
SELECT name, email FROM users WHERE pk = 'user#123'
```

**Why:** Reduces memory allocation, serialization overhead, and network transfer. Especially important for items with large attributes.

### 5. Batch Operations for Bulk Writes

**❌ Slow:**
```rust
// Individual writes (many fsync calls)
for item in items {
    db.put(item.key, item.value)?;
}
```

**✅ Fast:**
```rust
// Batch write (fewer fsync calls)
let mut batch = db.batch_write();
for item in items {
    batch = batch.put(item.key, item.value);
}
batch.execute()?;
```

**Why:** Batch writes enable group commit, reducing fsync overhead from N calls to 1-2 calls.

**Performance Gain:** 5-10x improvement for bulk inserts

### 6. Use Transactions for Atomicity

**Use Case:** Multi-item operations that must succeed or fail together

```rust
// All-or-nothing write
db.transact_write()
    .put(b"user#123", user_item)
    .put(b"account#456", account_item)
    .update(b"counter#1", "SET count = count + 1", None)
    .execute()?;
```

**Why:** Transactions ensure consistency without application-level rollback logic. Slight overhead (~10-20%) compared to individual writes.

### 7. Parallel Scan for Large Tables

**❌ Slow:**
```rust
// Single-threaded scan
let response = db.scan(ScanBuilder::new().build())?;
```

**✅ Fast:**
```rust
use rayon::prelude::*;

// Parallel scan with 4 segments
let results: Vec<_> = (0..4).into_par_iter().map(|segment| {
    let scan = ScanBuilder::new()
        .parallel(4, segment)
        .build();
    db.scan(scan).unwrap()
}).collect();

// Merge results
let all_items: Vec<_> = results.into_iter()
    .flat_map(|r| r.items)
    .collect();
```

**Performance Gain:** Near-linear scaling (4 segments ≈ 4x faster)

### 8. Design Keys for Query Patterns

**Good Key Design:**
```
pk = "user#<user_id>"          # User entity
pk = "user#<user_id>", sk = "profile"      # User profile
pk = "user#<user_id>", sk = "post#<timestamp>"  # User posts
```

**Why:**
- Same partition key groups related items
- Sort key enables range queries (e.g., posts in date range)
- Efficient queries without scans

**Bad Key Design:**
```
pk = "user", sk = "<user_id>"   # All users in one partition (hotspot!)
pk = "<random_uuid>"            # No query pattern, requires full scan
```

## Optimization Techniques

### 1. Tune Memtable Flush Threshold

**Default:** 1000 records per stripe

**Configuration:**
```rust
// In kstone-core/src/lsm.rs
const MEMTABLE_THRESHOLD: usize = 1000;  // Increase for better write throughput
```

**Trade-offs:**

| Threshold | Write Throughput | Memory Usage | Recovery Time |
|-----------|------------------|--------------|---------------|
| 500       | Lower            | Low          | Fast          |
| 1000      | Medium (default) | Medium       | Medium        |
| 5000      | Higher           | High         | Slow          |
| 10000     | Highest          | Very High    | Very Slow     |

**Recommendation:**
- **Write-heavy:** Increase to 5000 or 10000
- **Memory-constrained:** Decrease to 500
- **Balanced:** Keep default (1000)

### 2. Adjust Compaction Frequency

**Default:** Check every 5 seconds, compact if ≥4 SSTs

**Configuration:**
```rust
// In kstone-core/src/compaction.rs
const COMPACTION_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const MIN_SST_COUNT_FOR_COMPACTION: usize = 4;
```

**Trade-offs:**

| Strategy | Read Perf | Write Perf | Space Usage |
|----------|-----------|------------|-------------|
| Aggressive (≥2 SSTs) | High | Lower | Low |
| Moderate (≥4 SSTs, default) | Medium | Medium | Medium |
| Lazy (≥10 SSTs) | Lower | Higher | High |

**Recommendation:**
- **Read-heavy:** Aggressive (≥2 SSTs)
- **Write-heavy:** Lazy (≥10 SSTs)
- **Balanced:** Moderate (default)

### 3. Bloom Filter Tuning

**Default:** ~1% false positive rate

**Configuration:**
```rust
// In kstone-core/src/bloom.rs
let bloom = BloomFilter::new(record_count, 0.01);  // 1% FPR
```

**Trade-offs:**

| False Positive Rate | Memory per 1000 keys | Unnecessary Disk Reads |
|---------------------|----------------------|------------------------|
| 0.1% (aggressive)   | ~1.8 KB              | 1 per 1000 lookups     |
| 1% (default)        | ~1.2 KB              | 10 per 1000 lookups    |
| 5% (relaxed)        | ~0.8 KB              | 50 per 1000 lookups    |

**Recommendation:**
- **Memory-constrained:** 5% FPR
- **Read-optimized:** 0.1% FPR
- **Balanced:** 1% FPR (default)

### 4. Memory-Mapped File I/O

**Current:** Memory-mapped reads for SST files

**Benefits:**
- OS page cache automatically caches hot data
- Avoids read() system call overhead
- Zero-copy reads

**Considerations:**
- Memory usage appears high (virtual memory)
- Actual memory usage determined by OS page cache
- Works best with sufficient RAM

### 5. WAL Group Commit

**Automatic:** Multiple writes share single fsync

**To maximize benefit:**
```rust
// Write in batches from multiple threads
use rayon::prelude::*;

items.par_iter().for_each(|item| {
    db.put(item.key, item.value).unwrap();
});
```

**Why:** Concurrent writes increase chance of group commit, reducing fsync calls from N to ~sqrt(N).

## Monitoring and Tuning

### Key Metrics to Monitor

#### 1. SST File Count per Stripe

**Check:**
```bash
# Count SST files for stripe 0
ls mydb.keystone/000_*.sst | wc -l
```

**Interpretation:**
- **0-3 SSTs:** Healthy, no action needed
- **4-10 SSTs:** Normal, compaction running
- **10+ SSTs:** Compaction falling behind, may need tuning

**Action:**
- Increase compaction frequency
- Reduce memtable flush threshold
- Investigate write rate spike

#### 2. Database Size

**Check:**
```bash
du -sh mydb.keystone
```

**Interpretation:**
- **Size growth:** Normal with writes
- **Sudden spike:** Check for tombstone accumulation
- **Not shrinking after deletes:** Compaction not running

**Action:**
- Force compaction manually (if implemented)
- Check compaction manager is running

#### 3. Query vs Scan Ratio

**Measure in application:**
```rust
let query_count = counter.query.load(Ordering::Relaxed);
let scan_count = counter.scan.load(Ordering::Relaxed);
let ratio = scan_count as f64 / query_count as f64;
```

**Interpretation:**
- **Ratio < 0.1:** Excellent (mostly queries)
- **Ratio 0.1-0.5:** Good (some scans)
- **Ratio > 0.5:** Poor (too many scans)

**Action:**
- Add indexes (GSI) for common scan patterns
- Redesign access patterns to use partition keys

#### 4. Write Latency P99

**Measure:**
```rust
let start = std::time::Instant::now();
db.put(key, item)?;
let duration = start.elapsed();
```

**Healthy Values:**
- **P50:** 100-300μs
- **P99:** 500-2000μs
- **P99.9:** 2-10ms

**Concerning Values:**
- **P99 > 10ms:** Potential WAL fsync issues
- **Spikes to 100ms+:** Likely compaction pauses

### Tuning Workflow

1. **Identify bottleneck:**
   - High write latency? → Tune memtable threshold
   - High read latency? → Check SST count, add indexes
   - High memory usage? → Reduce memtable threshold

2. **Make one change at a time**
3. **Measure impact** (before/after comparison)
4. **Iterate**

## Benchmarking

### Benchmark Suite

KeystoneDB includes a comprehensive benchmark suite using Rust's built-in benchmarking framework.

#### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench crash_recovery

# Run benchmarks with output
cargo bench -- --nocapture
```

#### Available Benchmarks

1. **Crash Recovery Benchmarks** (`benches/crash_recovery.rs`)
   - WAL replay performance
   - Recovery time vs data size
   - Multi-stripe recovery

2. **Core Operations** (when added)
   - Put/Get/Delete throughput
   - Query performance
   - Scan performance
   - Batch operation performance

3. **Compaction** (when added)
   - Compaction throughput
   - Compaction latency impact
   - Write amplification measurement

#### Benchmark Output Format

Benchmarks produce detailed reports in `target/criterion/` directory:

```
crash_recovery/
├── base/
│   └── estimates.json      # Performance estimates
├── change/
│   └── estimates.json      # Comparison to previous run
└── report/
    └── index.html          # Visual report
```

**Example Output:**
```
crash_recovery_100_records
                        time:   [1.2345 ms 1.2567 ms 1.2789 ms]
                        change: [-2.3% +0.5% +3.2%] (p = 0.52 > 0.05)
                        No change in performance detected.

crash_recovery_1000_records
                        time:   [12.345 ms 12.567 ms 12.789 ms]
                        thrpt:  [78.2 ops/s 79.5 ops/s 81.0 ops/s]
```

#### Interpreting Results

**Time:**
- First value: Lower bound (fastest observed)
- Second value: Estimate (median)
- Third value: Upper bound (slowest observed)

**Change:**
- Percentage change from previous benchmark run
- p-value indicates statistical significance
- p < 0.05 means change is statistically significant

**Throughput:**
- Operations per second
- Higher is better
- Calculated as `1 / time_per_operation`

### Custom Benchmarks

Create custom benchmarks using the template:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kstone_api::{Database, ItemBuilder};
use tempfile::TempDir;

fn benchmark_writes(c: &mut Criterion) {
    c.bench_function("write_1000_items", |b| {
        b.iter_batched(
            || {
                let dir = TempDir::new().unwrap();
                Database::create(dir.path()).unwrap()
            },
            |db| {
                for i in 0..1000 {
                    let key = format!("key{:010}", i);
                    let item = ItemBuilder::new()
                        .string("value", format!("data{}", i))
                        .build();
                    db.put(key.as_bytes(), item).unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, benchmark_writes);
criterion_main!(benches);
```

**Save as:** `benches/my_benchmark.rs`

**Run with:** `cargo bench --bench my_benchmark`

### Simple Performance Testing

For quick performance checks without the full benchmark suite:

```rust
use std::time::Instant;

fn benchmark_writes(db: &Database, count: usize) {
    let start = Instant::now();

    for i in 0..count {
        let key = format!("key{:010}", i);
        let item = ItemBuilder::new()
            .string("value", format!("data{}", i))
            .build();
        db.put(key.as_bytes(), item).unwrap();
    }

    let duration = start.elapsed();
    let ops_per_sec = count as f64 / duration.as_secs_f64();

    println!("Writes: {:.0} ops/sec", ops_per_sec);
}

fn benchmark_reads(db: &Database, count: usize) {
    let start = Instant::now();

    for i in 0..count {
        let key = format!("key{:010}", i);
        db.get(key.as_bytes()).unwrap();
    }

    let duration = start.elapsed();
    let ops_per_sec = count as f64 / duration.as_secs_f64();

    println!("Reads: {:.0} ops/sec", ops_per_sec);
}

fn benchmark_queries(db: &Database, count: usize) {
    let start = Instant::now();

    for i in 0..count {
        let key = format!("key{:010}", i);
        let query = Query::new().partition_key(key.as_bytes());
        db.query(query).unwrap();
    }

    let duration = start.elapsed();
    let ops_per_sec = count as f64 / duration.as_secs_f64();

    println!("Queries: {:.0} ops/sec", ops_per_sec);
}
```

### Expected Results (Reference Hardware)

**Hardware:** Modern laptop (SSD, 16GB RAM, 4 cores)

```
Operation          Throughput        P50 Latency    P99 Latency
────────────────────────────────────────────────────────────────
Put                20k ops/sec       50μs           500μs
Get (memtable)     100k ops/sec      10μs           50μs
Get (SST)          15k ops/sec       50μs           1ms
Query (pk)         25k ops/sec       40μs           400μs
Scan (full)        2k ops/sec        500ms          2s
Batch Write (100)  5k batches/sec    20ms           100ms
```

**Note:** Results vary significantly based on:
- Disk type (NVMe SSD > SATA SSD > HDD)
- Memory size (affects page cache hit rate)
- CPU speed (affects compression, hashing)
- Workload (hot vs cold data)

### Performance Optimization Using Configuration

KeystoneDB provides `DatabaseConfig` for tuning performance (Phase 8+):

```rust
use kstone_api::Database;
use kstone_core::DatabaseConfig;

// Create config with custom settings
let config = DatabaseConfig {
    memtable_threshold: 5000,        // Larger memtable = fewer flushes
    compaction_threshold: 8,         // Less aggressive compaction
    max_concurrent_compactions: 2,   // Lower resource usage
    ..Default::default()
};

// Create database with config
let db = Database::create_with_config("mydb.keystone", config)?;
```

**Tuning Guidelines:**

| Workload | memtable_threshold | compaction_threshold | max_concurrent_compactions |
|----------|-------------------|---------------------|---------------------------|
| Write-heavy | 5000-10000 | 8-10 | 2-4 |
| Read-heavy | 1000-2000 | 2-4 | 4-8 |
| Balanced | 1000-3000 | 4-6 | 4 |
| Memory-constrained | 500-1000 | 2-4 | 1-2 |

## Summary

### Key Takeaways

1. **Use partition keys** for all queries when possible
2. **Add indexes (GSI)** for non-key attribute queries
3. **Batch writes** for bulk operations
4. **Use LIMIT and projection** to reduce data transfer
5. **Parallel scan** for large table scans
6. **Monitor SST count** as indicator of compaction health
7. **Design keys** to match query patterns

### Performance Hierarchy

```
Fastest →
    Get by partition key (memtable)
    Query by partition key
    Get by partition key (SST)
    Query by partition key + sort key range
    Scan with filter (indexed attributes)
    Scan with filter (non-indexed)
    Full table scan
← Slowest
```

### When to Optimize

- ✅ **Optimize when:** P99 latency > 10ms, throughput < 1k ops/sec
- ❌ **Don't optimize when:** Performance meets requirements
- 🎯 **Focus on:** Query patterns first, then tuning parameters

For more details on internal architecture, see [ARCHITECTURE.md](ARCHITECTURE.md).

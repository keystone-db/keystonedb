# Chapter 21: Compaction & Space Management

Compaction is the housekeeping process that keeps KeystoneDB performant and space-efficient. While writes create new SST files, compaction merges them, removes obsolete data, and maintains optimal read performance. This chapter explores why compaction is essential, how it works, and how to configure it for different workloads.

## 21.1 Why Compaction Is Needed

### The Accumulation Problem

Without compaction, SST files accumulate indefinitely:

```
Day 1:  [SST-1]
Day 2:  [SST-1] [SST-2]
Day 3:  [SST-1] [SST-2] [SST-3]
Day 7:  [SST-1] [SST-2] ... [SST-7]
Day 30: [SST-1] [SST-2] ... [SST-30]
```

This creates several problems:

**Read Amplification**: To find a key, the system must check all SST files:

```
Query for key="user#123":
├─ Check memtable: miss
├─ Check SST-30: miss
├─ Check SST-29: miss
├─ ...
├─ Check SST-2: miss
└─ Check SST-1: HIT (but 30 files checked!)
```

Each check involves:
1. Bloom filter lookup (~1μs)
2. Potential disk read (~100-1000μs if not cached)

With 30 SSTs, a single key lookup could require 30 bloom filter checks and potentially 30 disk reads (though bloom filters help significantly).

**Space Amplification**: Multiple versions of the same key waste space:

```
SST-1:  key="user#123" → {name: "Alice", version: 1}
SST-5:  key="user#123" → {name: "Alice", version: 2}
SST-12: key="user#123" → {name: "Alice Smith", version: 3}
SST-20: key="user#123" → DELETE

Space used: 4 entries
Space needed: 0 (key was deleted!)
```

**Tombstone Accumulation**: Deleted keys leave tombstones:

```
SST-15: key="temp#456" → DELETE
SST-23: key="cache#789" → DELETE
SST-27: key="session#111" → DELETE

These tombstones consume disk space but provide no value
after the original keys have been fully removed.
```

### The Compaction Solution

Compaction merges multiple SSTs into one, keeping only the latest version of each key:

```
Before compaction:
SST-1: [key1=v1, key2=v1, key3=v1]
SST-2: [key1=v2, key4=v1]
SST-3: [key2=DELETE, key5=v1]

After compaction:
SST-NEW: [key1=v2, key3=v1, key4=v1, key5=v1]
         └─ key2 removed (tombstone processed)
         └─ key1 deduplicated (v2 kept, v1 discarded)
```

Benefits:
- **Reduced read amplification**: 3 SSTs → 1 SST (3x fewer checks)
- **Space reclamation**: 5 records → 4 records (tombstone removed)
- **Better cache efficiency**: Fewer files = higher cache hit rate

## 21.2 Compaction Algorithm (K-Way Merge)

KeystoneDB uses a **k-way merge algorithm** that merges k SST files into a single output SST.

### The Merge Process

The algorithm operates in four phases:

**Phase 1: Collect All Records**

Read all records from all input SSTs into a sorted map:

```rust
let mut records_by_key: BTreeMap<Vec<u8>, Record> = BTreeMap::new();

for sst in ssts {
    for record in sst.scan()? {
        let encoded_key = record.key.encode().to_vec();

        // Keep record with highest SeqNo (latest version)
        records_by_key
            .entry(encoded_key)
            .and_modify(|existing| {
                if record.seq > existing.seq {
                    *existing = record.clone();
                }
            })
            .or_insert(record);
    }
}
```

The `BTreeMap` automatically sorts keys, and the logic keeps only the newest version (highest `SeqNo`) of each key.

**Phase 2: Filter Tombstones**

Remove deletion markers since they're no longer needed:

```rust
let records_to_write: Vec<Record> = records_by_key
    .into_values()
    .filter(|record| !record.is_tombstone())
    .collect();
```

A tombstone is a record where `record_type == Delete` and `value.is_none()`.

**Phase 3: Write New SST**

Create a new SST file with the merged, deduplicated records:

```rust
let new_sst_path = self.dir.join(format!("{:03}-{}.sst", stripe_id, next_sst_id));
let mut writer = SstWriter::new();

for record in records_to_write {
    writer.add(record);
}

writer.finish(&new_sst_path)?;
```

**Phase 4: Atomic Swap**

Update the manifest and delete old SST files:

```rust
// 1. Open new SST reader
let new_reader = SstReader::open(&new_sst_path)?;

// 2. Update manifest (atomic operation)
manifest.replace_ssts(old_sst_ids, new_sst_id)?;

// 3. Delete old SST files
for path in old_sst_paths {
    fs::remove_file(&path)?;
}
```

The manifest update is atomic, ensuring readers never see an inconsistent state.

### Example: Merging Three SSTs

```
Input SSTs:
┌─────────────────────────────────────────────────────────┐
│ SST-1 (seq 1-100):                                      │
│   key1 → {name: "Alice", seq: 10}                       │
│   key2 → {name: "Bob", seq: 20}                         │
│   key3 → {name: "Carol", seq: 30}                       │
├─────────────────────────────────────────────────────────┤
│ SST-2 (seq 101-200):                                    │
│   key1 → {name: "Alice Smith", seq: 105}                │
│   key4 → {name: "Dave", seq: 150}                       │
├─────────────────────────────────────────────────────────┤
│ SST-3 (seq 201-300):                                    │
│   key2 → DELETE (seq: 210)                              │
│   key5 → {name: "Eve", seq: 250}                        │
└─────────────────────────────────────────────────────────┘

Merge process:
1. Collect all records, deduplicate by SeqNo:
   - key1: Choose seq=105 (discard seq=10)
   - key2: Choose seq=210 DELETE (discard seq=20)
   - key3: Keep seq=30
   - key4: Keep seq=150
   - key5: Keep seq=250

2. Filter tombstones:
   - Remove key2 (tombstone)

3. Output SST-NEW:
   ┌──────────────────────────────────────────┐
   │ key1 → {name: "Alice Smith", seq: 105}   │
   │ key3 → {name: "Carol", seq: 30}          │
   │ key4 → {name: "Dave", seq: 150}          │
   │ key5 → {name: "Eve", seq: 250}           │
   └──────────────────────────────────────────┘

Result:
- 3 SSTs → 1 SST (67% reduction)
- 6 records → 4 records (33% space savings)
- 1 tombstone removed
- 1 duplicate removed
```

### Correctness Guarantees

The k-way merge maintains several invariants:

1. **Newest version wins**: SeqNo comparison ensures latest data is kept
2. **Sorted order**: BTreeMap maintains lexicographic key order
3. **Atomicity**: Manifest update happens before old SST deletion
4. **Idempotency**: Running compaction multiple times produces the same result

## 21.3 Background Compaction Manager

KeystoneDB runs compaction in the background to avoid blocking client operations.

### Compaction Manager Architecture

The compaction manager operates per-stripe:

```rust
pub struct CompactionManager {
    stripe_id: usize,     // Which stripe this manages
    dir: PathBuf,         // Database directory
}
```

Each of the 256 stripes has its own compaction needs tracked independently.

### Trigger Conditions

Compaction is triggered when a stripe accumulates too many SSTs:

```rust
pub fn needs_compaction(&self, sst_count: usize) -> bool {
    sst_count >= COMPACTION_THRESHOLD  // Default: 10 SSTs
}
```

The threshold is configurable via `CompactionConfig`:

```rust
pub struct CompactionConfig {
    pub enabled: bool,               // Enable/disable automatic compaction
    pub sst_threshold: usize,        // Trigger when stripe has N SSTs
    pub check_interval_secs: u64,    // How often to check (default: 60s)
    pub max_concurrent_compactions: usize,  // Max parallel compactions
}
```

### Background Worker Thread

A background thread periodically checks for compaction opportunities:

```rust
// Conceptual implementation (actual implementation may vary)
fn background_compaction_worker(engine: Arc<LsmEngine>) {
    loop {
        thread::sleep(Duration::from_secs(60));  // Check interval

        for stripe_id in 0..256 {
            let sst_count = engine.get_stripe_sst_count(stripe_id);

            if sst_count >= 10 {  // Threshold
                // Trigger compaction for this stripe
                engine.compact_stripe(stripe_id)?;
            }
        }
    }
}
```

### Concurrent Compaction

Multiple stripes can be compacted simultaneously:

```rust
// CompactionConfig
max_concurrent_compactions: 4  // Up to 4 stripes at once
```

This parallelism is safe because:
1. Each stripe has independent SST files
2. Compaction acquires a write lock on its stripe
3. Different stripes have different locks (no contention)

Example timeline:

```
Time →
t=0:   Compact stripe 5 starts
t=1:   Compact stripe 5 continues, stripe 17 starts
t=2:   Compact stripe 5 finishes, stripe 17 continues, stripe 42 starts
t=3:   Compact stripe 17 finishes, stripe 42 continues, stripe 99 starts
```

### Manual Compaction Trigger

Applications can manually trigger compaction:

```rust
// Trigger compaction for a specific stripe
engine.trigger_compaction(stripe_id)?;

// Or compact all stripes that need it
for stripe_id in 0..256 {
    if engine.needs_compaction(stripe_id) {
        engine.trigger_compaction(stripe_id)?;
    }
}
```

This is useful for:
- Batch processing (compact after large bulk load)
- Maintenance windows (controlled timing)
- Testing and benchmarking

## 21.4 CompactionConfig Parameters

### sst_threshold

**Controls when compaction is triggered.**

```rust
let config = CompactionConfig::new()
    .with_sst_threshold(5);  // Compact at 5 SSTs instead of 10
```

**Trade-offs:**

| Threshold | Read Perf | Write Perf | Disk I/O | Space Usage |
|-----------|-----------|------------|----------|-------------|
| 2 (aggressive) | Excellent | Lower | High | Low |
| 5 | Good | Good | Medium | Medium |
| 10 (default) | Good | Good | Medium | Medium |
| 20 (lazy) | Fair | Excellent | Low | High |

**Recommendations:**
- **Read-heavy workload**: Lower threshold (2-5) for faster queries
- **Write-heavy workload**: Higher threshold (15-20) to reduce overhead
- **Balanced workload**: Default (10) works well

### check_interval_secs

**Controls how often the background worker checks for compaction needs.**

```rust
let config = CompactionConfig::new()
    .with_check_interval(30);  // Check every 30 seconds instead of 60
```

**Trade-offs:**

| Interval | Responsiveness | CPU Overhead | Typical Use Case |
|----------|----------------|--------------|------------------|
| 10s | Very fast | Higher | High-churn workloads |
| 60s (default) | Good | Low | General purpose |
| 300s (5min) | Slow | Very low | Read-heavy, stable data |

**Recommendations:**
- **High write rate**: Shorter interval (10-30s) to keep up
- **Low write rate**: Longer interval (300s+) to save CPU
- **Batch workloads**: Disable automatic, run manually after batch

### max_concurrent_compactions

**Controls how many stripes can be compacted simultaneously.**

```rust
let config = CompactionConfig::new()
    .with_max_concurrent(2);  // Limit to 2 parallel compactions
```

**Trade-offs:**

| Concurrency | Throughput | CPU Usage | Disk I/O | Latency Impact |
|-------------|------------|-----------|----------|----------------|
| 1 | Low | Low | Low | Minimal |
| 4 (default) | High | Medium | Medium | Low |
| 8 | Highest | High | High | Medium |
| 16 | Diminishing | Very high | Very high | High |

**Recommendations:**
- **CPU cores = N**: Set to N/2 (leave headroom for queries)
- **SSD storage**: Higher concurrency (8-16) exploits parallel I/O
- **HDD storage**: Lower concurrency (1-2) to avoid thrashing
- **Latency-sensitive**: Lower concurrency (1-2) to minimize impact

### Disabling Compaction

```rust
let config = CompactionConfig::disabled();
```

**Use cases:**
- **Testing**: Examine SST accumulation behavior
- **Data loading**: Disable during bulk import, run once after
- **Read-only**: If database won't receive more writes
- **Custom scheduling**: Implement your own compaction logic

**Warning:** Disabling compaction indefinitely will cause:
- Unbounded SST growth
- Degraded read performance
- Wasted disk space

## 21.5 Write Amplification

Write amplification is the ratio of data written to disk vs. data written by the application.

### Sources of Write Amplification

**Initial Write (1x):**
```
Application writes 1MB → WAL writes 1MB → Memtable flush writes 1MB to SST
Total: 2MB written (WAL + SST)
Amplification: 2x
```

**Compaction (Nx):**
```
Compact 10 SSTs of 1MB each:
- Read: 10MB
- Write: 8MB (after deduplication)
- Additional disk writes: 8MB

If this data gets compacted 3 times during its lifetime:
Total writes: 2MB + 8MB + 8MB + 8MB = 26MB
Original data: 1MB
Amplification: 26x
```

### Calculating Write Amplification

For a typical workload:

```
Writes per day: 1 million records × 200 bytes = 200MB/day
Compaction levels: 3 (L0 → L1 → L2)
Average compaction overhead: 4x (rewrite 4 times)

Total disk writes: 200MB × (1 + 4) = 1GB/day
Application writes: 200MB/day
Write amplification: 5x
```

### Minimizing Write Amplification

**Increase SST threshold:**
```rust
CompactionConfig::new().with_sst_threshold(20)
```
Fewer compactions = less rewriting, but more read amplification.

**Larger memtable:**
```rust
DatabaseConfig::new().with_max_memtable_records(5000)
```
Fewer, larger SSTs = less compaction overhead.

**Tiered compaction (future enhancement):**
```
L0: Small, frequent compactions (10 SSTs → 1 SST)
L1: Medium compactions (5 SSTs → 1 SST)
L2: Large, infrequent compactions (all SSTs → 1 SST)
```

### Write Amplification vs. Read Amplification

There's an inherent trade-off:

```
              Read Amplification
                      ↑
                      │
                      │  ╱
                      │ ╱
                      │╱
        ──────────────┼──────────────→
                     ╱│   Write Amplification
                    ╱ │
                   ╱  │
                  ╱   ↓
```

- **Low write amp**: Fewer compactions → more SSTs → higher read amp
- **Low read amp**: Frequent compactions → fewer SSTs → higher write amp

KeystoneDB's defaults aim for a balanced middle ground.

## 21.6 Tombstone Removal

Tombstones (deletion markers) are eventually removed during compaction.

### Tombstone Lifecycle

```
t=0:   Put(key1, value1) → SST-1: [key1=value1]
t=100: Delete(key1)      → SST-2: [key1=TOMBSTONE]
t=200: Compact SST-1 + SST-2 → SST-3: [] (tombstone removed)
```

The tombstone is needed in SST-2 to shadow the value in SST-1. Once they're compacted together, the tombstone can be discarded.

### Garbage Collection

Compaction acts as a garbage collector for deleted data:

```
Before compaction (100MB used):
SST-1: [key1=v1, key2=v1, key3=v1, key4=v1, key5=v1]  (20MB)
SST-2: [key1=DELETE, key3=DELETE]                      (2MB)
SST-3: [key6=v1, key7=v1]                              (8MB)
SST-4: [key2=v2, key8=v1]                              (10MB)

After compaction (60MB used):
SST-NEW: [key2=v2, key4=v1, key5=v1, key6=v1, key7=v1, key8=v1]
         └─ key1, key3 fully removed (tombstones processed)
         └─ key2 deduplicated (only v2 kept)

Space reclaimed: 40MB (40% reduction)
```

### Eager vs. Lazy Deletion

**Eager (aggressive compaction):**
- Tombstones removed quickly (within minutes)
- Low space usage
- High write amplification

**Lazy (relaxed compaction):**
- Tombstones may persist for hours/days
- Higher space usage
- Low write amplification

KeystoneDB uses **lazy deletion** by default (threshold = 10 SSTs), balancing space and performance.

### Counting Tombstone Removal

The `CompactionStats` tracks tombstones removed:

```rust
pub struct CompactionStats {
    pub total_tombstones_removed: u64,
    pub total_bytes_reclaimed: u64,
    // ...
}

// After compaction
let stats = engine.compaction_stats();
println!("Removed {} tombstones", stats.total_tombstones_removed);
println!("Reclaimed {} bytes", stats.total_bytes_reclaimed);
```

## 21.7 Manual vs Automatic Compaction

### Automatic Compaction (Default)

**Pros:**
- No manual intervention required
- Maintains consistent performance automatically
- Adapts to write patterns

**Cons:**
- May run at inopportune times
- Fixed scheduling (every 60s check)
- Limited control over resource usage

**Best for:**
- Production databases with steady traffic
- Applications without strict latency requirements
- General-purpose workloads

### Manual Compaction

**Pros:**
- Full control over when compaction runs
- Can align with maintenance windows
- Avoid impacting peak traffic periods

**Cons:**
- Requires application logic to trigger
- Risk of forgetting to compact (performance degrades)
- More complex operational model

**Best for:**
- Batch processing systems
- Applications with predictable quiet periods
- Testing and development

### Hybrid Approach

Many applications use both:

```rust
// Enable automatic as safety net
let config = CompactionConfig::new()
    .with_sst_threshold(20)      // Lazy automatic
    .with_check_interval(300);   // Infrequent checks

// Manually compact after known write-heavy operations
fn after_bulk_import() {
    for stripe_id in 0..256 {
        engine.trigger_compaction(stripe_id)?;
    }
}
```

This provides:
- Guaranteed compaction after bulk loads
- Automatic compaction as fallback if manual is missed
- Lower automatic compaction overhead

## 21.8 Compaction Performance Characteristics

### Throughput

Compaction throughput depends on SST size and count:

```
Compacting 10 SSTs of 1MB each:
├─ Read time: 10MB ÷ 500MB/s = 20ms
├─ Merge/dedup: ~10-50ms (CPU-bound)
├─ Write time: 8MB ÷ 300MB/s = 27ms
└─ Total: ~60-100ms

Throughput: 10 SSTs / 0.1s = 100 SSTs/second
```

With `max_concurrent_compactions = 4`:
- Effective throughput: 400 SSTs/second
- Can keep up with ~40 memtable flushes/second per stripe

### Impact on Reads

During compaction:

**Before manifest update:**
- Reads check both old and new SSTs (slight overhead)
- Old SSTs still valid, queries return correct results

**After manifest update:**
- Reads only check new SST (faster!)
- Old SSTs can be deleted safely

**Latency impact:**
- Median (P50): No change
- P99: +10-50ms (occasional cache eviction)
- P99.9: +100-500ms (rare compaction pauses)

### Impact on Writes

Compaction holds the write lock on its stripe:

```
Write to stripe 42:
├─ Acquire write lock
│  └─ Blocked if compaction in progress
├─ Append to WAL
├─ Insert to memtable
└─ Release lock

Typical contention: <5% of operations
```

With 256 stripes, impact is minimal:
- Only 1/256 of writes affected by any single compaction
- Most writes proceed unblocked

### Resource Usage

CPU usage during compaction:

```
Components:
├─ Reading SSTs: 10-20% CPU (I/O bound)
├─ Deduplication: 30-50% CPU (BTreeMap operations)
├─ Writing SST: 10-20% CPU (serialization)
└─ Total: ~50-90% of one core per compaction

With max_concurrent = 4:
Total CPU: 2-4 cores actively used
```

Disk I/O:

```
Read: 10-100MB/s per compaction
Write: 5-50MB/s per compaction (after deduplication)

With max_concurrent = 4:
Total: 40-400MB/s read, 20-200MB/s write
```

Modern SSDs can handle this easily (500+ MB/s sustained).

## 21.9 Compaction Statistics

KeystoneDB tracks detailed compaction metrics:

```rust
pub struct CompactionStats {
    pub total_compactions: u64,         // How many compactions ran
    pub total_ssts_merged: u64,         // SSTs processed
    pub total_ssts_created: u64,        // New SSTs created
    pub total_bytes_read: u64,          // Data read from old SSTs
    pub total_bytes_written: u64,       // Data written to new SSTs
    pub total_bytes_reclaimed: u64,     // Space freed
    pub total_records_deduplicated: u64, // Duplicate versions removed
    pub total_tombstones_removed: u64,  // Deletion markers removed
    pub active_compactions: u64,        // Currently running
}
```

### Accessing Statistics

```rust
let stats = engine.compaction_stats();

println!("Compactions: {}", stats.total_compactions);
println!("Space reclaimed: {}MB", stats.total_bytes_reclaimed / 1_000_000);
println!("Write amplification: {:.1}x",
    stats.total_bytes_written as f64 / original_writes as f64);
```

### Interpreting Metrics

**Healthy compaction:**
```
total_compactions: 150
total_ssts_merged: 1500       (10 SSTs per compaction average)
total_ssts_created: 150       (1 output SST per compaction)
total_bytes_read: 15GB
total_bytes_written: 10GB     (33% space savings)
total_bytes_reclaimed: 5GB
```

**Warning signs:**
```
total_compactions: 5          (too few - SSTs accumulating)
total_bytes_reclaimed: 0      (no deduplication - check for unique keys)
active_compactions: 10        (exceeds max_concurrent - possible stall)
```

### Monitoring and Alerting

Recommended monitoring thresholds:

```rust
// Alert if compaction falls behind
if stats.active_compactions == 0 && sst_count > threshold * 2 {
    alert("Compaction not running, SSTs accumulating");
}

// Alert if write amplification is too high
let write_amp = stats.total_bytes_written / original_writes;
if write_amp > 10.0 {
    alert("Write amplification > 10x, consider tuning");
}

// Alert if space reclamation is low
let reclaim_ratio = stats.total_bytes_reclaimed / stats.total_bytes_read;
if reclaim_ratio < 0.1 {
    alert("Low space reclamation, check for many unique keys");
}
```

## 21.10 Advanced Topics

### Leveled Compaction (Future)

KeystoneDB currently uses **tiered compaction** (merge all SSTs in a stripe). Future versions may add **leveled compaction**:

```
L0: [SST-1] [SST-2] [SST-3] [SST-4]  (unsorted, overlapping)
     ↓ compact (merge all)
L1: [SST-5────────────────────────]  (sorted, non-overlapping)
     ↓ compact (merge overlapping ranges)
L2: [SST-6──────] [SST-7──────] ...  (larger, sorted)
```

Benefits:
- Lower write amplification (only compact overlapping ranges)
- Predictable read performance (bounded levels to check)

### Partial Compaction

Instead of compacting all SSTs in a stripe, compact subsets:

```rust
// Only compact oldest N SSTs
compact_oldest_ssts(stripe_id, n=5)?;

// Only compact SSTs older than timestamp
compact_old_ssts(stripe_id, before=timestamp)?;
```

Benefits:
- Lower latency (smaller compactions)
- More frequent (incremental progress)
- Better resource control

### Compaction Prioritization

Prioritize stripes based on need:

```rust
struct CompactionPriority {
    stripe_id: usize,
    sst_count: usize,
    score: f64,  // Higher = more urgent
}

fn calculate_priority(stripe: &Stripe) -> f64 {
    let sst_count_score = stripe.sst_count as f64;
    let size_score = stripe.total_size as f64 / 1_000_000.0;
    let tombstone_score = stripe.tombstone_ratio * 10.0;

    sst_count_score + size_score + tombstone_score
}

// Compact highest-priority stripes first
```

### Compaction Throttling

Limit compaction rate to avoid overwhelming system:

```rust
struct CompactionThrottle {
    max_bytes_per_sec: u64,
    current_bytes: u64,
    last_reset: Instant,
}

impl CompactionThrottle {
    fn check(&mut self, bytes_to_write: u64) {
        if self.current_bytes + bytes_to_write > self.max_bytes_per_sec {
            // Sleep until next second
            thread::sleep(self.time_until_reset());
        }
        self.current_bytes += bytes_to_write;
    }
}
```

## 21.11 Troubleshooting

### Compaction Not Running

**Symptoms:**
- SST count growing unbounded
- Read performance degrading
- `active_compactions == 0` in stats

**Causes:**
1. Compaction disabled in config
2. Background worker thread panicked
3. Threshold too high (never reached)

**Fixes:**
```rust
// Check config
let config = engine.compaction_config();
assert!(config.enabled);

// Manually trigger
engine.trigger_compaction(stripe_id)?;

// Lower threshold
engine.set_compaction_config(
    CompactionConfig::new().with_sst_threshold(5)
)?;
```

### Compaction Too Aggressive

**Symptoms:**
- High disk I/O (100% busy)
- Write latency spikes
- High CPU usage

**Causes:**
1. Threshold too low (constant compaction)
2. Too many concurrent compactions
3. Slow disk (can't keep up)

**Fixes:**
```rust
// Raise threshold
CompactionConfig::new().with_sst_threshold(20)

// Reduce concurrency
CompactionConfig::new().with_max_concurrent(2)

// Increase check interval
CompactionConfig::new().with_check_interval(300)
```

### Space Not Reclaimed

**Symptoms:**
- `total_bytes_reclaimed` is low
- Disk usage not decreasing after deletes

**Causes:**
1. No duplicates or tombstones (mostly unique keys)
2. Deletes haven't been compacted yet
3. Old SST files not actually deleted (filesystem issue)

**Fixes:**
```rust
// Force compaction
for stripe_id in 0..256 {
    engine.trigger_compaction(stripe_id)?;
}

// Check filesystem
// Deleted files may still appear until file handles close
```

## 21.12 Summary

Compaction is essential for maintaining KeystoneDB's long-term performance:

**Key Takeaways:**
1. **Necessary for performance**: Prevents unbounded SST growth and read amplification
2. **K-way merge**: Efficient algorithm that merges multiple SSTs in one pass
3. **Background operation**: Runs automatically without blocking client operations
4. **Configurable**: Tunable thresholds balance read/write amplification
5. **Space reclamation**: Removes tombstones and duplicates, freeing disk space

**Design Highlights:**
- BTreeMap-based deduplication (keeps newest version)
- Tombstone filtering during merge
- Atomic manifest updates for crash safety
- Per-stripe independence enables parallelism

**Configuration Guidelines:**
- **Default (balanced)**: threshold=10, interval=60s, concurrent=4
- **Read-optimized**: threshold=5, interval=30s, concurrent=8
- **Write-optimized**: threshold=20, interval=300s, concurrent=2

**Performance Impact:**
- Write amplification: 2-10x typical (depends on configuration)
- Read improvement: 3-10x faster after compaction
- CPU usage: 50-90% per active compaction
- Disk I/O: 10-100MB/s per active compaction

In the next chapter, we'll explore **Bloom Filters**, the probabilistic data structure that makes reads fast by avoiding unnecessary disk I/O.

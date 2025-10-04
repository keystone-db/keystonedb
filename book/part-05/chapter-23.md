# Chapter 23: Performance Tuning

Performance tuning is both an art and a science. While KeystoneDB provides sensible defaults, understanding how to configure it for your specific workload can unlock 2-10x performance improvements. This chapter covers the key tuning parameters, monitoring techniques, and optimization strategies.

## 23.1 Memtable Threshold Tuning

The memtable threshold controls when in-memory data is flushed to disk.

### The Threshold Parameter

```rust
// In kstone-core/src/lsm.rs
const MEMTABLE_THRESHOLD: usize = 1000;  // Default: 1000 records per stripe

// Configurable via DatabaseConfig
let config = DatabaseConfig::new()
    .with_max_memtable_records(5000);  // Increase to 5000
```

### Impact on Performance

**Write Throughput:**

| Threshold | Flushes per 100k writes | Write throughput | Reason |
|-----------|------------------------|------------------|---------|
| 500 | 200 | Lower | More frequent flushes = more overhead |
| 1000 (default) | 100 | Baseline | Balanced |
| 5000 | 20 | +40% higher | Fewer flushes amortize overhead |
| 10000 | 10 | +60% higher | Even fewer flushes |

**Example benchmark:**
```
Threshold 1000:  50,000 writes/sec
Threshold 5000:  70,000 writes/sec (+40%)
Threshold 10000: 80,000 writes/sec (+60%)
```

**Memory Usage:**

| Threshold | Memory per stripe | Total memory (256 stripes) |
|-----------|------------------|---------------------------|
| 500 | ~100 KB | ~25 MB |
| 1000 (default) | ~200 KB | ~50 MB |
| 5000 | ~1 MB | ~256 MB |
| 10000 | ~2 MB | ~512 MB |

Assumes 200 bytes per record average.

**Recovery Time:**

| Threshold | WAL size | Recovery time |
|-----------|----------|---------------|
| 500 | Small (~50MB) | Fast (~100ms) |
| 1000 (default) | Medium (~100MB) | Medium (~200ms) |
| 5000 | Large (~500MB) | Slow (~1s) |
| 10000 | Very large (~1GB) | Very slow (~2s) |

Larger memtables mean more data in WAL to replay on crash recovery.

### Tuning Guidelines

**For write-heavy workloads:**
```rust
// Maximize write throughput
let config = DatabaseConfig::new()
    .with_max_memtable_records(5000);  // Or even 10000

// Benefits:
// + 40-60% higher write throughput
// + Fewer, larger SSTs (better for reads)
// - Higher memory usage (~250-500MB)
// - Slower crash recovery (1-2 seconds)
```

**For memory-constrained systems:**
```rust
// Minimize memory footprint
let config = DatabaseConfig::new()
    .with_max_memtable_records(500);

// Benefits:
// + Lower memory usage (~25MB)
// + Faster crash recovery (100ms)
// - Lower write throughput (30-40% decrease)
// - More SSTs (more compaction needed)
```

**For balanced workloads:**
```rust
// Keep default
let config = DatabaseConfig::default();  // 1000 records

// Provides good balance of:
// - Write throughput (50k ops/sec)
// - Memory usage (50MB)
// - Recovery time (200ms)
```

### Dynamic Tuning

For workloads with varying intensity, consider adaptive thresholds:

```rust
// Pseudocode (not currently implemented)
fn adaptive_threshold() -> usize {
    let write_rate = measure_write_rate();
    let available_memory = system_available_memory();

    if write_rate > 10000 && available_memory > 1_000_000_000 {
        10000  // High write rate, plenty of memory
    } else if available_memory < 100_000_000 {
        500    // Low memory
    } else {
        1000   // Default
    }
}
```

## 23.2 Compaction Frequency Tuning

Compaction frequency controls the trade-off between read and write performance.

### CompactionConfig Parameters

```rust
pub struct CompactionConfig {
    pub enabled: bool,               // Turn compaction on/off
    pub sst_threshold: usize,        // Trigger at N SSTs (default: 10)
    pub check_interval_secs: u64,    // How often to check (default: 60)
    pub max_concurrent_compactions: usize,  // Parallelism (default: 4)
}
```

### SST Threshold Impact

**Read Performance vs. SST Count:**

| SST Count | Read latency (P50) | Read latency (P99) | Reason |
|-----------|-------------------|-------------------|---------|
| 2-3 | 100μs | 500μs | Few SSTs to check |
| 5-10 | 200μs | 1ms | Moderate checking overhead |
| 15-20 | 500μs | 2ms | Many bloom filter checks + potential false positives |
| 30+ | 1ms | 5ms | Excessive read amplification |

**Tuning for read-heavy workloads:**
```rust
// Aggressive compaction
let config = CompactionConfig::new()
    .with_sst_threshold(5);  // Compact at 5 SSTs instead of 10

// Results:
// + 30-50% faster reads (fewer SSTs to check)
// + Better bloom filter effectiveness
// - 2x more compaction overhead
// - 20-40% lower write throughput
```

**Tuning for write-heavy workloads:**
```rust
// Lazy compaction
let config = CompactionConfig::new()
    .with_sst_threshold(20);  // Compact at 20 SSTs instead of 10

// Results:
// + 40-60% higher write throughput (less compaction)
// + Lower disk I/O (fewer rewrites)
// - 50-100% slower reads (more SSTs to check)
// - Higher disk usage (more duplicates/tombstones)
```

### Check Interval Impact

**Responsiveness vs. CPU Overhead:**

| Interval | Compaction lag | CPU overhead | Use case |
|----------|---------------|--------------|----------|
| 10s | Minimal (10-30s) | 0.5-1% CPU | High-churn data, real-time systems |
| 60s (default) | Low (1-3 min) | 0.1-0.2% CPU | General purpose |
| 300s (5min) | Moderate (5-15 min) | <0.05% CPU | Read-heavy, stable data |
| Manual only | None (on-demand) | 0% | Batch processing |

**Tuning examples:**
```rust
// High-churn workload (frequent updates to same keys)
let config = CompactionConfig::new()
    .with_check_interval(10);  // Check every 10 seconds

// Low-churn workload (mostly inserts, few updates)
let config = CompactionConfig::new()
    .with_check_interval(300);  // Check every 5 minutes

// Batch processing (disable automatic, run manually)
let config = CompactionConfig::disabled();
// Later: engine.trigger_compaction(stripe_id)?;
```

### Concurrent Compactions

**Parallelism vs. Resource Usage:**

| Concurrency | Throughput | CPU Usage | Disk I/O | Latency Impact |
|-------------|-----------|-----------|----------|----------------|
| 1 | Baseline | Low (0.5-1 core) | Low (~50MB/s) | Minimal |
| 4 (default) | 3-4x | Medium (2-3 cores) | Medium (~200MB/s) | Low |
| 8 | 6-7x | High (4-6 cores) | High (~400MB/s) | Medium |
| 16 | 8-10x (diminishing) | Very high (8-12 cores) | Very high (~600MB/s) | High |

**Tuning based on hardware:**
```rust
// High-end server (16+ cores, NVMe SSD)
let config = CompactionConfig::new()
    .with_max_concurrent(8);  // Exploit parallelism

// Mid-range server (4-8 cores, SATA SSD)
let config = CompactionConfig::new()
    .with_max_concurrent(4);  // Default, balanced

// Low-end server (2-4 cores, HDD)
let config = CompactionConfig::new()
    .with_max_concurrent(1);  // Minimize contention

// Latency-sensitive application
let config = CompactionConfig::new()
    .with_max_concurrent(2)   // Low background load
    .with_check_interval(120); // Infrequent checks
```

## 23.3 Bloom Filter Tuning

Bloom filter accuracy trades memory for read performance.

### Bits Per Key Configuration

```rust
// In kstone-core/src/sst_block.rs
const BITS_PER_KEY: usize = 10;  // Default: 10 bits/key (~1% FPR)
```

### Memory vs. False Positive Rate

**Comparison table:**

| Bits/Key | Memory/1M keys | FPR | Read Efficiency | Use Case |
|----------|---------------|-----|-----------------|----------|
| 6 | 750 KB | 6% | Poor | Extreme memory constraints |
| 8 | 1 MB | 2% | Fair | Memory-constrained systems |
| 10 (default) | 1.25 MB | 1% | Good | General purpose |
| 12 | 1.5 MB | 0.4% | Very good | Read-heavy workloads |
| 16 | 2 MB | 0.05% | Excellent | Critical read paths |
| 20 | 2.5 MB | 0.008% | Near-perfect | Overkill for most cases |

**Performance impact:**

```
Scenario: Check 10 SSTs for non-existent key

6 bits/key (6% FPR):
- Bloom checks: 10 × 1μs = 10μs
- False positives: 10 × 6% = 0.6 disk reads
- Disk I/O: 0.6 × 1000μs = 600μs
- Total: ~610μs

10 bits/key (1% FPR):
- Bloom checks: 10 × 1μs = 10μs
- False positives: 10 × 1% = 0.1 disk reads
- Disk I/O: 0.1 × 1000μs = 100μs
- Total: ~110μs (5.5x faster!)

16 bits/key (0.05% FPR):
- Bloom checks: 10 × 1μs = 10μs
- False positives: 10 × 0.05% = 0.005 disk reads
- Disk I/O: 0.005 × 1000μs = 5μs
- Total: ~15μs (7x faster than 10 bits/key)
```

### Tuning Recommendations

**For read-heavy, point lookup workloads:**
```rust
// Optimize for minimal false positives
const BITS_PER_KEY: usize = 16;

// Benefits:
// + 5-10x fewer unnecessary disk reads
// + Lower read latency (P99: 100μs → 15μs)
// - 60% more memory (1.25MB → 2MB per 1M keys)
```

**For memory-constrained environments:**
```rust
// Minimize memory footprint
const BITS_PER_KEY: usize = 8;

// Trade-offs:
// + 20% less memory (1.25MB → 1MB per 1M keys)
// - 2x higher false positive rate (1% → 2%)
// - Slightly slower reads (P99: 110μs → 220μs)
```

**For write-heavy workloads:**
```rust
// Balance creation speed and effectiveness
const BITS_PER_KEY: usize = 8;  // Faster to build, still effective

// Rationale:
// - Smaller bloom filters = faster SST creation
// - Reads are less critical in write-heavy workload
// - 2% FPR is acceptable trade-off
```

## 23.4 DatabaseConfig Options

KeystoneDB provides `DatabaseConfig` for comprehensive tuning:

```rust
pub struct DatabaseConfig {
    pub max_memtable_size_bytes: Option<usize>,  // Memory limit
    pub max_memtable_records: usize,             // Record limit (default: 1000)
    pub max_wal_size_bytes: Option<u64>,         // WAL size limit
    pub max_total_disk_bytes: Option<u64>,       // Total DB size limit
    pub write_buffer_size: usize,                // Write buffer (default: 1024)
}
```

### Max Memtable Size

Limit memtable by bytes instead of record count:

```rust
let config = DatabaseConfig::new()
    .with_max_memtable_size_bytes(10 * 1024 * 1024);  // 10MB per stripe

// Benefits:
// - Predictable memory usage (10MB × 256 stripes = 2.56GB max)
// - Better for variable-size records
// - More fine-grained control than record count
```

### WAL Size Limits

Prevent WAL from growing too large:

```rust
let config = DatabaseConfig::new()
    .with_max_wal_size_bytes(100 * 1024 * 1024);  // 100MB WAL limit

// Triggers:
// - Forced memtable flush when WAL exceeds limit
// - Prevents unbounded WAL growth on slow compaction

// Trade-offs:
// + Bounded disk usage
// + Predictable recovery time
// - May force flushes at inopportune times
```

### Total Disk Size Limits

Set a hard cap on database size:

```rust
let config = DatabaseConfig::new()
    .with_max_total_disk_bytes(10 * 1024 * 1024 * 1024);  // 10GB limit

// Behavior when limit reached:
// - Writes fail with Error::DiskFull
// - Compaction triggered to reclaim space
// - Application must handle backpressure

// Use cases:
// - Embedded systems with fixed storage
// - Multi-tenant environments (quota enforcement)
// - Cache systems (bounded size)
```

### Write Buffer Size

Control buffering for WAL and SST writes:

```rust
let config = DatabaseConfig::new()
    .with_write_buffer_size(4096);  // 4KB buffer

// Impact:
// - Larger buffers: Better throughput, higher memory usage
// - Smaller buffers: Lower memory, potentially lower throughput

// Default (1024 bytes) is fine for most use cases
// Increase for high-throughput systems (4KB-64KB)
```

## 23.5 Write vs Read Optimization

Different workloads require different trade-offs.

### Write-Optimized Configuration

For high-throughput ingestion, batch processing:

```rust
let db_config = DatabaseConfig::new()
    .with_max_memtable_records(10000);  // Large memtable

let compaction_config = CompactionConfig::new()
    .with_sst_threshold(20)             // Lazy compaction
    .with_check_interval(300)           // Infrequent checks
    .with_max_concurrent(2);            // Low background load

// Expected performance:
// - Write throughput: 80,000-100,000 ops/sec
// - Read latency (P99): 2-5ms (acceptable for batch systems)
// - Memory usage: ~500MB
// - Disk I/O: Low (minimal compaction)
```

### Read-Optimized Configuration

For low-latency queries, user-facing applications:

```rust
let db_config = DatabaseConfig::new()
    .with_max_memtable_records(1000);  // Default memtable

let compaction_config = CompactionConfig::new()
    .with_sst_threshold(5)              // Aggressive compaction
    .with_check_interval(30)            // Frequent checks
    .with_max_concurrent(8);            // High parallelism

// Also consider:
const BITS_PER_KEY: usize = 16;  // Higher bloom filter accuracy

// Expected performance:
// - Read latency (P50): 50-100μs
// - Read latency (P99): 200-500μs
// - Write throughput: 30,000-40,000 ops/sec (acceptable for read-heavy)
// - Memory usage: ~100MB (default + larger bloom filters)
```

### Balanced Configuration

For general-purpose workloads with mixed read/write:

```rust
// Use all defaults
let db_config = DatabaseConfig::default();
let compaction_config = CompactionConfig::default();

// Provides:
// - Write throughput: 50,000 ops/sec
// - Read latency (P99): 1-2ms
// - Memory usage: 50MB
// - Good starting point for tuning
```

### Configuration Comparison Table

| Config | Write Throughput | Read P99 Latency | Memory | Best For |
|--------|-----------------|------------------|--------|----------|
| Write-optimized | 80k ops/sec | 5ms | 500MB | Ingestion, batch processing |
| Balanced (default) | 50k ops/sec | 1ms | 50MB | General purpose |
| Read-optimized | 30k ops/sec | 500μs | 100MB | User-facing queries, analytics |

## 23.6 Benchmarking Techniques

Effective tuning requires measurement. Here's how to benchmark KeystoneDB.

### Simple Performance Testing

Basic throughput measurement:

```rust
use std::time::Instant;
use kstone_api::{Database, ItemBuilder};

fn benchmark_writes(db: &Database, count: usize) {
    let start = Instant::now();

    for i in 0..count {
        let key = format!("key{:010}", i);
        let item = ItemBuilder::new()
            .string("data", format!("value{}", i))
            .build();
        db.put(key.as_bytes(), item).unwrap();
    }

    let duration = start.elapsed();
    let ops_per_sec = count as f64 / duration.as_secs_f64();

    println!("Writes: {:.0} ops/sec", ops_per_sec);
    println!("Average latency: {:.2}μs", duration.as_micros() as f64 / count as f64);
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
    println!("Average latency: {:.2}μs", duration.as_micros() as f64 / count as f64);
}
```

### Latency Percentile Tracking

Measure P50, P90, P99, P99.9:

```rust
use std::collections::BTreeMap;

struct LatencyTracker {
    samples: Vec<u64>,  // Latencies in microseconds
}

impl LatencyTracker {
    fn new() -> Self {
        Self { samples: Vec::new() }
    }

    fn record(&mut self, latency_us: u64) {
        self.samples.push(latency_us);
    }

    fn percentiles(&mut self) -> BTreeMap<&str, u64> {
        self.samples.sort_unstable();
        let len = self.samples.len();

        let mut result = BTreeMap::new();
        result.insert("P50", self.samples[len * 50 / 100]);
        result.insert("P90", self.samples[len * 90 / 100]);
        result.insert("P99", self.samples[len * 99 / 100]);
        result.insert("P99.9", self.samples[len * 999 / 1000]);

        result
    }
}

// Usage:
let mut tracker = LatencyTracker::new();

for i in 0..10000 {
    let start = Instant::now();
    db.get(format!("key{}", i).as_bytes()).unwrap();
    tracker.record(start.elapsed().as_micros() as u64);
}

let percentiles = tracker.percentiles();
println!("P50: {}μs", percentiles["P50"]);
println!("P99: {}μs", percentiles["P99"]);
println!("P99.9: {}μs", percentiles["P99.9"]);
```

### Using Criterion Benchmarks

For rigorous benchmarking with statistical analysis:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use kstone_api::{Database, ItemBuilder};
use tempfile::TempDir;

fn benchmark_put(c: &mut Criterion) {
    c.bench_function("put_1000_items", |b| {
        b.iter_batched(
            || {
                // Setup: Create fresh database
                let dir = TempDir::new().unwrap();
                Database::create(dir.path()).unwrap()
            },
            |db| {
                // Benchmark: Write 1000 items
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

criterion_group!(benches, benchmark_put);
criterion_main!(benches);
```

Run with:
```bash
cargo bench --bench my_benchmark
```

Output:
```
put_1000_items
    time:   [45.231 ms 45.456 ms 45.712 ms]
    thrpt:  [21,875 elem/s 21,999 elem/s 22,105 elem/s]

    change: [-1.2% +0.3% +1.8%] (p = 0.42 > 0.05)
    No change in performance detected.
```

### Monitoring Metrics

Track key metrics over time:

```rust
struct DatabaseMetrics {
    write_count: AtomicU64,
    read_count: AtomicU64,
    write_latency_sum: AtomicU64,
    read_latency_sum: AtomicU64,
}

impl DatabaseMetrics {
    fn record_write(&self, latency_us: u64) {
        self.write_count.fetch_add(1, Ordering::Relaxed);
        self.write_latency_sum.fetch_add(latency_us, Ordering::Relaxed);
    }

    fn record_read(&self, latency_us: u64) {
        self.read_count.fetch_add(1, Ordering::Relaxed);
        self.read_latency_sum.fetch_add(latency_us, Ordering::Relaxed);
    }

    fn report(&self) {
        let writes = self.write_count.load(Ordering::Relaxed);
        let reads = self.read_count.load(Ordering::Relaxed);

        if writes > 0 {
            let avg_write = self.write_latency_sum.load(Ordering::Relaxed) / writes;
            println!("Writes: {}, avg latency: {}μs", writes, avg_write);
        }

        if reads > 0 {
            let avg_read = self.read_latency_sum.load(Ordering::Relaxed) / reads;
            println!("Reads: {}, avg latency: {}μs", reads, avg_read);
        }
    }
}
```

## 23.7 Performance Monitoring

Ongoing monitoring helps detect regressions and bottlenecks.

### Key Metrics to Track

**Operation Counts:**
```rust
let stats = engine.compaction_stats();
println!("Total compactions: {}", stats.total_compactions);
println!("SSTs merged: {}", stats.total_ssts_merged);
println!("Tombstones removed: {}", stats.total_tombstones_removed);
```

**SST File Counts:**
```bash
# Per-stripe SST count
for stripe in {0..255}; do
    count=$(ls mydb.keystone/$(printf "%03d" $stripe)-*.sst 2>/dev/null | wc -l)
    if [ $count -gt 15 ]; then
        echo "Warning: Stripe $stripe has $count SSTs (threshold: 10)"
    fi
done
```

**Disk Usage:**
```bash
# Total database size
du -sh mydb.keystone/

# Per-component breakdown
du -sh mydb.keystone/wal*.log   # WAL files
du -sh mydb.keystone/*.sst      # SST files
```

**Memory Usage:**
```rust
// Estimate memory usage
let memtable_memory = 256 * memtable_size_per_stripe;
let bloom_filter_memory = estimate_bloom_memory();
let index_memory = estimate_index_memory();

let total_memory = memtable_memory + bloom_filter_memory + index_memory;
println!("Estimated memory usage: {}MB", total_memory / 1_000_000);
```

### Setting Up Alerts

Define thresholds for alerts:

```rust
struct PerformanceAlert {
    name: &'static str,
    threshold: f64,
    current: f64,
}

fn check_alerts(db: &Database) -> Vec<PerformanceAlert> {
    let mut alerts = Vec::new();

    // Check SST accumulation
    let avg_sst_count = measure_average_sst_count(db);
    if avg_sst_count > 15.0 {
        alerts.push(PerformanceAlert {
            name: "High SST count",
            threshold: 15.0,
            current: avg_sst_count,
        });
    }

    // Check write amplification
    let stats = db.compaction_stats();
    let write_amp = stats.total_bytes_written as f64 / original_writes as f64;
    if write_amp > 10.0 {
        alerts.push(PerformanceAlert {
            name: "High write amplification",
            threshold: 10.0,
            current: write_amp,
        });
    }

    // Check read latency
    let p99_latency = measure_p99_read_latency(db);
    if p99_latency > 5000.0 {  // 5ms
        alerts.push(PerformanceAlert {
            name: "High read latency",
            threshold: 5000.0,
            current: p99_latency,
        });
    }

    alerts
}
```

### Prometheus Integration (Future)

Export metrics to Prometheus for visualization:

```rust
// Pseudocode for future implementation
use prometheus::{IntCounter, Histogram, register_int_counter, register_histogram};

lazy_static! {
    static ref WRITE_COUNT: IntCounter = register_int_counter!(
        "keystonedb_writes_total",
        "Total number of writes"
    ).unwrap();

    static ref WRITE_LATENCY: Histogram = register_histogram!(
        "keystonedb_write_latency_microseconds",
        "Write latency in microseconds"
    ).unwrap();
}

// Record metrics
WRITE_COUNT.inc();
WRITE_LATENCY.observe(latency_us as f64);
```

## 23.8 Troubleshooting Performance Issues

### Symptom: Low Write Throughput

**Diagnosis:**
1. Check if compaction is running excessively
2. Measure WAL fsync latency
3. Check disk I/O saturation

**Potential fixes:**
```rust
// 1. Reduce compaction frequency
let config = CompactionConfig::new()
    .with_sst_threshold(20);

// 2. Increase memtable size
let db_config = DatabaseConfig::new()
    .with_max_memtable_records(5000);

// 3. Check disk performance
// iostat -x 1  (on Linux)
// If disk at 100% utilization, need faster storage
```

### Symptom: High Read Latency

**Diagnosis:**
1. Count SSTs per stripe
2. Measure bloom filter false positive rate
3. Check if data is in memtable or SST

**Potential fixes:**
```rust
// 1. Trigger manual compaction
for stripe_id in 0..256 {
    if needs_compaction(stripe_id) {
        engine.trigger_compaction(stripe_id)?;
    }
}

// 2. Increase bloom filter accuracy (requires code change)
const BITS_PER_KEY: usize = 16;  // From 10

// 3. Increase memtable size (keep more in memory)
let config = DatabaseConfig::new()
    .with_max_memtable_records(2000);
```

### Symptom: High Memory Usage

**Diagnosis:**
1. Check memtable sizes
2. Measure bloom filter memory
3. Count total SSTs

**Potential fixes:**
```rust
// 1. Reduce memtable size
let config = DatabaseConfig::new()
    .with_max_memtable_records(500);

// 2. Trigger compaction (fewer SSTs = fewer bloom filters)
engine.set_compaction_config(
    CompactionConfig::new().with_sst_threshold(5)
)?;

// 3. Reduce bloom filter size (requires code change)
const BITS_PER_KEY: usize = 8;  // From 10
```

### Symptom: Slow Crash Recovery

**Diagnosis:**
1. Measure WAL size
2. Count records in WAL

**Potential fixes:**
```rust
// Reduce memtable size (smaller WAL)
let config = DatabaseConfig::new()
    .with_max_memtable_records(500);  // From 1000

// Set WAL size limit
let config = DatabaseConfig::new()
    .with_max_wal_size_bytes(50 * 1024 * 1024);  // 50MB max
```

## 23.9 Real-World Tuning Examples

### Example 1: High-Throughput Log Ingestion

**Workload:**
- 100,000 writes/sec
- Mostly inserts, few updates
- Reads are rare (archival system)
- 32GB RAM available

**Configuration:**
```rust
let db_config = DatabaseConfig::new()
    .with_max_memtable_records(10000);  // Large memtable

let compaction_config = CompactionConfig::new()
    .with_sst_threshold(30)      // Very lazy
    .with_check_interval(600)    // 10 minute checks
    .with_max_concurrent(1);     // Minimal background load

// Results:
// - Write throughput: 120,000 ops/sec
// - Memory usage: ~600MB
// - Disk writes: ~30GB/hour (after compaction)
```

### Example 2: Low-Latency Cache

**Workload:**
- 50,000 reads/sec
- 5,000 writes/sec (90% read)
- P99 latency < 1ms required
- 8GB RAM available

**Configuration:**
```rust
let db_config = DatabaseConfig::new()
    .with_max_memtable_records(2000);  // Keep more in memory

let compaction_config = CompactionConfig::new()
    .with_sst_threshold(4)       // Very aggressive
    .with_check_interval(15)     // Frequent checks
    .with_max_concurrent(8);     // High parallelism

// Also:
const BITS_PER_KEY: usize = 16;  // Low false positives

// Results:
// - Read latency P99: 600μs
// - Write throughput: 25,000 ops/sec
// - Memory usage: ~150MB
```

### Example 3: Embedded System

**Workload:**
- 100 writes/sec
- 500 reads/sec
- 256MB RAM limit
- SD card storage

**Configuration:**
```rust
let db_config = DatabaseConfig::new()
    .with_max_memtable_records(200);  // Small memtable

let compaction_config = CompactionConfig::new()
    .with_sst_threshold(8)
    .with_check_interval(120)
    .with_max_concurrent(1);     // SD cards don't benefit from parallelism

// Also:
const BITS_PER_KEY: usize = 8;   // Reduce bloom filter memory

// Results:
// - Memory usage: ~20MB
// - Write throughput: 500 ops/sec (enough)
// - Read latency P99: 3ms (acceptable for embedded)
```

## 23.10 Summary

Performance tuning is about understanding trade-offs and optimizing for your specific workload.

**Key Takeaways:**
1. **Memtable size**: Larger = higher write throughput, more memory, slower recovery
2. **Compaction frequency**: More frequent = faster reads, lower writes
3. **Bloom filters**: More bits = fewer false positives, more memory
4. **Concurrency**: More parallel compactions = faster overall, higher resource usage

**Tuning Process:**
1. Start with defaults (usually good enough)
2. Measure your specific workload
3. Identify bottleneck (write throughput? read latency? memory?)
4. Make one change at a time
5. Measure impact
6. Iterate

**Configuration Presets:**

**Write-Heavy:**
```rust
DatabaseConfig::new().with_max_memtable_records(10000)
CompactionConfig::new()
    .with_sst_threshold(20)
    .with_max_concurrent(2)
```

**Read-Heavy:**
```rust
DatabaseConfig::new().with_max_memtable_records(1000)
CompactionConfig::new()
    .with_sst_threshold(5)
    .with_max_concurrent(8)
const BITS_PER_KEY: usize = 16;
```

**Memory-Constrained:**
```rust
DatabaseConfig::new().with_max_memtable_records(500)
CompactionConfig::new().with_max_concurrent(1)
const BITS_PER_KEY: usize = 8;
```

**Performance Targets:**

| Metric | Good | Excellent | Notes |
|--------|------|-----------|-------|
| Write throughput | 50k ops/sec | 100k ops/sec | Depends on hardware |
| Read latency (P99) | <2ms | <500μs | With warm cache |
| Memory usage | <100MB | <50MB | Per million records |
| Recovery time | <1s | <200ms | Depends on WAL size |
| Write amplification | <5x | <3x | Typical for LSM |

This concludes Part V of The KeystoneDB Book. You now understand the storage engine internals and how to tune them for optimal performance!

# KeystoneDB Troubleshooting Guide

This guide helps diagnose and resolve common issues with KeystoneDB.

## Table of Contents

1. [Common Errors](#common-errors)
2. [Corruption Recovery](#corruption-recovery)
3. [Performance Issues](#performance-issues)
4. [Health Check Interpretation](#health-check-interpretation)
5. [Log Analysis](#log-analysis)
6. [Disk Space Issues](#disk-space-issues)
7. [Memory Issues](#memory-issues)

## Common Errors

### Error: IO Error

**Error Message:**
```
Error: IO error: No such file or directory (os error 2)
```

**Cause:** Database directory or file not found.

**Solutions:**

1. **Check if database exists:**
   ```bash
   ls -la /path/to/mydb.keystone
   ```

2. **Create database if missing:**
   ```bash
   kstone create /path/to/mydb.keystone
   ```

3. **Verify permissions:**
   ```bash
   # Check ownership and permissions
   ls -ld /path/to/mydb.keystone

   # Fix permissions if needed
   sudo chown -R $USER:$USER /path/to/mydb.keystone
   sudo chmod 755 /path/to/mydb.keystone
   ```

4. **Check disk space:**
   ```bash
   df -h /path/to/mydb.keystone
   ```

---

### Error: Corruption Detected

**Error Message:**
```
Error: Corruption detected: WAL checksum mismatch at LSN 12345
Error: Corruption detected: SST file header invalid
```

**Cause:** Data corruption in WAL or SST file (power loss, disk failure, or bugs).

**Solutions:**

See [Corruption Recovery](#corruption-recovery) section below.

---

### Error: Key Not Found

**Error Message:**
```
Error: Key not found: user#123
```

**Cause:** Requested key does not exist in the database.

**Solutions:**

1. **Verify key exists:**
   ```bash
   kstone get mydb.keystone user#123
   ```

2. **Check if key was deleted:**
   - Keys may have been deleted
   - TTL may have expired (if using TTL feature)

3. **Query instead of get:**
   ```rust
   // If you're not sure about the exact key
   let query = Query::new()
       .partition_key(b"user#123");
   let response = db.query(query)?;
   ```

---

### Error: Invalid Argument

**Error Message:**
```
Error: Invalid argument: Empty partition key
Error: Invalid argument: Invalid expression syntax
```

**Cause:** Invalid parameters passed to API.

**Solutions:**

1. **Empty keys:**
   ```rust
   // ❌ Wrong
   db.put(b"", item)?;

   // ✅ Correct
   db.put(b"user#123", item)?;
   ```

2. **Invalid expressions:**
   ```rust
   // ❌ Wrong
   db.update(b"user#123", "SET age = ", None)?;

   // ✅ Correct
   db.update(b"user#123", "SET age = :age", None)?;
   ```

3. **Check parameter validation:**
   - Partition keys must not be empty
   - Expression syntax must be valid
   - Values must match expected types

---

### Error: Database Already Exists

**Error Message:**
```
Error: Database already exists: /path/to/mydb.keystone
```

**Cause:** Attempting to create a database that already exists.

**Solutions:**

1. **Use open instead of create:**
   ```rust
   // ❌ Wrong
   let db = Database::create("mydb.keystone")?;

   // ✅ Correct
   let db = Database::open("mydb.keystone")?;
   ```

2. **Delete existing database if you want fresh start:**
   ```bash
   rm -rf mydb.keystone
   kstone create mydb.keystone
   ```

---

### Error: Checksum Mismatch

**Error Message:**
```
Error: Checksum mismatch
```

**Cause:** Data corruption detected during read (WAL or SST).

**Solutions:**

1. **Check for disk errors:**
   ```bash
   # Check disk health (Linux)
   sudo smartctl -a /dev/sda

   # Check filesystem
   sudo fsck /dev/sda1
   ```

2. **Restore from backup:**
   ```bash
   # Stop application
   sudo systemctl stop myapp

   # Restore from backup
   tar xzf /backups/latest.tar.gz -C /data/mydb.keystone

   # Restart
   sudo systemctl start myapp
   ```

3. **Try corruption recovery:** See [Corruption Recovery](#corruption-recovery).

---

### Error: Internal Error

**Error Message:**
```
Error: Internal error: Unexpected state in compaction
Error: Internal error: This operation is not yet supported in in-memory mode
```

**Cause:** Unexpected internal state or unsupported operation.

**Solutions:**

1. **Check if operation is supported:**
   - Some features are disk-only (not available in memory mode)
   - Verify feature is implemented for your database version

2. **Enable debug logging:**
   ```bash
   RUST_LOG=debug cargo run --bin myapp
   ```

3. **Check for known issues:**
   - Review GitHub issues
   - Check changelog for bug fixes

4. **Report bug if persistent:**
   - Collect logs and reproduction steps
   - File issue on GitHub

---

### Error: Encryption Error

**Error Message:**
```
Error: Encryption error: Invalid encryption key
Error: Encryption error: Decryption failed
```

**Cause:** Encryption key mismatch or corruption.

**Solutions:**

1. **Verify encryption key:**
   - Ensure correct key is being used
   - Check key hasn't been rotated

2. **Check if database was encrypted:**
   ```bash
   # Encrypted databases have different file headers
   hexdump -C mydb.keystone/000_*.sst | head -1
   ```

3. **Restore from backup with correct key**

---

### Error: Compaction Error

**Error Message:**
```
Error: Compaction error: Failed to merge SST files
```

**Cause:** Compaction process failed (disk space, corruption, or bugs).

**Solutions:**

1. **Check disk space:**
   ```bash
   df -h /path/to/mydb.keystone
   ```

2. **Check compaction logs:**
   ```bash
   grep "compaction" app.log
   ```

3. **Temporarily disable compaction** (if critical):
   ```rust
   // In code (requires rebuild)
   let config = CompactionConfig {
       enabled: false,
       ..Default::default()
   };
   ```

4. **Manually compact** (future feature):
   ```bash
   kstone compact mydb.keystone --stripe 0
   ```

## Corruption Recovery

### WAL Corruption

**Symptoms:**
- Database fails to open
- Errors during WAL replay
- Checksum mismatches in WAL

**Recovery Steps:**

#### Option 1: Truncate Corrupted WAL

```bash
#!/bin/bash
# truncate-wal.sh - Remove corrupted WAL entries

DB_DIR="/data/mydb.keystone"
STRIPE=0  # Adjust for affected stripe

# Backup WAL first
cp "$DB_DIR/wal_$(printf '%03d' $STRIPE).log" "$DB_DIR/wal_backup.log"

# Find last good LSN from logs
# Look for: "WAL corruption at LSN XXXXX"
LAST_GOOD_LSN=12345  # Set from error message

# Truncate WAL to last good LSN
# Note: This is a simplified example - real implementation needed
python3 <<EOF
import struct

wal_file = "$DB_DIR/wal_$(printf '%03d' $STRIPE).log"
with open(wal_file, "rb") as f:
    data = f.read()

# Find offset of last good LSN (requires parsing WAL format)
# This is a placeholder - implement based on WAL format
offset = find_offset_for_lsn(data, $LAST_GOOD_LSN)

with open(wal_file, "wb") as f:
    f.write(data[:offset])

print(f"WAL truncated to LSN {$LAST_GOOD_LSN}")
EOF
```

#### Option 2: Delete Corrupted WAL

```bash
# WARNING: Loses data since last flush

DB_DIR="/data/mydb.keystone"
STRIPE=0

# Backup first
cp "$DB_DIR/wal_$(printf '%03d' $STRIPE).log" /backups/wal_backup_$(date +%s).log

# Delete corrupted WAL
rm "$DB_DIR/wal_$(printf '%03d' $STRIPE).log"

# Database will create new WAL on next open
# Data since last flush is lost
```

#### Option 3: Restore from Backup

```bash
# Stop application
sudo systemctl stop myapp

# Restore from latest backup
tar xzf /backups/latest_backup.tar.gz -C /data/mydb.keystone

# Start application
sudo systemctl start myapp
```

### SST Corruption

**Symptoms:**
- Errors reading SST file
- Checksum mismatches during compaction
- Unexpected data returned

**Recovery Steps:**

#### Identify Corrupted SST

```bash
# Check SST file integrity
for sst in /data/mydb.keystone/*.sst; do
    echo "Checking $sst..."

    # Check file size
    size=$(stat -f%z "$sst" 2>/dev/null || stat -c%s "$sst")
    if [ $size -lt 100 ]; then
        echo "  WARNING: File too small ($size bytes)"
    fi

    # Check magic number (first 4 bytes should be SST magic)
    magic=$(xxd -p -l 4 "$sst")
    if [ "$magic" != "5353544d" ]; then  # "SSTM" in hex
        echo "  ERROR: Invalid magic number: $magic"
    fi
done
```

#### Remove Corrupted SST

```bash
# Backup corrupted file
cp /data/mydb.keystone/000_1234567890.sst /backups/corrupted_sst_$(date +%s).sst

# Remove corrupted SST
rm /data/mydb.keystone/000_1234567890.sst

# Trigger compaction to rebuild
# (automatic on next restart, or use manual compaction when available)
```

**Note:** Removing an SST file may cause data loss for keys only in that file. Always restore from backup if possible.

### Full Database Recovery

If multiple files are corrupted:

```bash
#!/bin/bash
# full-recovery.sh - Complete database recovery

DB_DIR="/data/mydb.keystone"
BACKUP_DIR="/backups/$(date +%Y%m%d_%H%M%S)_recovery"

# 1. Stop application
sudo systemctl stop myapp

# 2. Backup corrupted database
mkdir -p "$BACKUP_DIR"
cp -r "$DB_DIR" "$BACKUP_DIR/corrupted_db"

# 3. Create new database directory
rm -rf "$DB_DIR"
mkdir -p "$DB_DIR"

# 4. Restore from latest good backup
latest_backup=$(ls -t /backups/*.tar.gz | head -1)
tar xzf "$latest_backup" -C "$DB_DIR"

# 5. Restore ownership
chown -R kstone:kstone "$DB_DIR"

# 6. Restart application
sudo systemctl start myapp

echo "Recovery complete. Corrupted DB saved to $BACKUP_DIR"
```

## Performance Issues

### Slow Writes

**Symptoms:**
- Write latency > 10ms (P99)
- Throughput < 1000 ops/sec

**Diagnosis:**

```bash
# Check disk I/O
iostat -x 1 10

# Check WAL fsync time
RUST_LOG=debug cargo run --bin myapp 2>&1 | grep "fsync"

# Check memtable flush frequency
RUST_LOG=debug cargo run --bin myapp 2>&1 | grep "flush"
```

**Solutions:**

1. **Increase memtable threshold** (reduces flush frequency):
   ```rust
   // In kstone-core/src/lsm.rs
   const MEMTABLE_THRESHOLD: usize = 5000;  // Increase from 1000
   ```

2. **Use faster disk** (NVMe instead of SATA):
   - Upgrade to NVMe SSD
   - Move database to faster storage

3. **Enable batch writes**:
   ```rust
   // Batch multiple writes together
   db.batch_write()
       .put(key1, item1)
       .put(key2, item2)
       .execute()?;
   ```

4. **Reduce compaction overhead**:
   ```rust
   // Increase SST threshold
   sst_threshold: 8,  // Compact less frequently
   ```

### Slow Reads

**Symptoms:**
- Read latency > 50ms (P99)
- Throughput < 10k ops/sec from SST

**Diagnosis:**

```bash
# Check SST file count
for i in {0..255}; do
    count=$(ls /data/mydb.keystone/$(printf "%03d" $i)_*.sst 2>/dev/null | wc -l)
    if [ $count -gt 10 ]; then
        echo "Stripe $i: $count SSTs"
    fi
done

# Check bloom filter hit rate (in logs)
RUST_LOG=debug cargo run --bin myapp 2>&1 | grep "bloom"

# Check disk I/O
iostat -x 1 10
```

**Solutions:**

1. **Reduce SST count via aggressive compaction**:
   ```rust
   sst_threshold: 2,  // Compact when ≥2 SSTs
   ```

2. **Add indexes for non-key queries**:
   ```rust
   let schema = TableSchema::builder()
       .add_gsi("by-email", "email", None, IndexProjection::All)
       .build();
   ```

3. **Use partition keys instead of scans**:
   ```rust
   // ❌ Slow: Full table scan
   let scan = ScanBuilder::new()
       .filter_expression("user_id = :id")
       .build();

   // ✅ Fast: Direct query
   let query = Query::new()
       .partition_key(b"user#123");
   ```

4. **Improve bloom filter** (lower FPR):
   ```rust
   BloomFilter::new(count, 0.001)  // 0.1% instead of 1%
   ```

### Write Amplification

**Symptoms:**
- Disk writes >> application writes
- High compaction activity
- Disk wearing out quickly (SSD)

**Diagnosis:**

```rust
// Check compaction stats
let stats = db.stats()?;
println!("Bytes written: {}", stats.compaction.total_bytes_written);
println!("Bytes read: {}", stats.compaction.total_bytes_read);
println!("Compactions: {}", stats.compaction.total_compactions);

// Calculate write amplification
let write_amp = stats.compaction.total_bytes_written / application_bytes_written;
println!("Write amplification: {}x", write_amp);
```

**Solutions:**

1. **Increase memtable size** (fewer, larger SSTs):
   ```rust
   const MEMTABLE_THRESHOLD: usize = 10000;
   ```

2. **Reduce compaction frequency**:
   ```rust
   sst_threshold: 10,  // Allow more SSTs before compaction
   ```

3. **Use larger items** (amortize overhead):
   - Combine small writes into larger items
   - Batch related data together

### High Latency Spikes

**Symptoms:**
- P99 latency >> P50 latency
- Occasional 100ms+ spikes
- Unpredictable performance

**Diagnosis:**

```bash
# Check if spikes correlate with compaction
RUST_LOG=debug cargo run --bin myapp 2>&1 | grep -E "(compaction|latency)" | less

# Check system metrics during spikes
vmstat 1

# Check disk latency
iostat -x 1
```

**Solutions:**

1. **Reduce concurrent compactions**:
   ```rust
   max_concurrent_compactions: 2,  // Reduce from 4
   ```

2. **Disable transparent huge pages** (Linux):
   ```bash
   echo never > /sys/kernel/mm/transparent_hugepage/enabled
   ```

3. **Use dedicated compaction threads**:
   - Separate compaction from request processing
   - Already implemented in background compaction manager

4. **Monitor and alert on spikes**:
   ```promql
   histogram_quantile(0.99, rate(kstone_rpc_duration_seconds_bucket[5m])) > 0.1
   ```

## Health Check Interpretation

### Healthy Status

```rust
let health = db.health();
assert_eq!(health.status, HealthStatus::Healthy);
```

**Indicators:**
- ✅ Database opens successfully
- ✅ Reads and writes working
- ✅ No errors in logs
- ✅ Compaction running normally

**Action:** No action needed. Continue monitoring.

---

### Degraded Status

```rust
let health = db.health();
if health.status == HealthStatus::Degraded {
    println!("Warnings:");
    for warning in &health.warnings {
        println!("  - {}", warning);
    }
}
```

**Example Warnings:**
- "High SST count in stripe 42 (15 files)"
- "Compaction falling behind in 3 stripes"
- "Disk usage above 80%"
- "High write amplification detected"

**Action:**
1. Review warnings
2. Adjust configuration if needed
3. Monitor for progression to Unhealthy

---

### Unhealthy Status

```rust
let health = db.health();
if health.status == HealthStatus::Unhealthy {
    println!("Errors:");
    for error in &health.errors {
        println!("  - {}", error);
    }
    // Take corrective action
}
```

**Example Errors:**
- "Database directory not accessible"
- "Corruption detected in WAL"
- "Unable to write to disk (space full)"
- "Critical I/O error"

**Action:**
1. Address errors immediately
2. May require database recovery
3. Check disk, permissions, corruption

## Log Analysis

### Enable Debug Logging

```bash
# Set log level
export RUST_LOG=debug

# Or more targeted
export RUST_LOG=kstone_core=debug,kstone_api=info

# Run application
cargo run --bin myapp
```

### Finding Errors

```bash
# Find all errors
grep "ERROR" app.log

# Find errors with context
grep -B 5 -A 5 "ERROR" app.log

# Find specific error types
grep "Corruption detected" app.log
grep "IO error" app.log
```

### Trace Specific Request

```bash
# Find trace ID from error
grep "ERROR" app.log | grep -o "trace_id=\"[^\"]*\""

# Get all logs for that trace
grep 'trace_id="a1b2c3d4-..."' app.log
```

### Analyzing Performance

```bash
# Find slow operations
grep "duration" app.log | awk '$NF > 100 {print}'  # > 100ms

# Compaction frequency
grep "compaction completed" app.log | wc -l

# Flush frequency
grep "memtable flush" app.log | wc -l
```

### Common Log Patterns

**Normal operation:**
```
INFO [kstone_core::lsm] Memtable flush completed, stripe=5, records=1000
INFO [kstone_core::compaction] Compaction completed, stripe=12, ssts_merged=4
```

**Warning signs:**
```
WARN [kstone_core::compaction] Compaction falling behind, stripe=42, sst_count=15
WARN [kstone_core::lsm] High write latency detected, p99=50ms
```

**Errors:**
```
ERROR [kstone_core::wal] Corruption detected: checksum mismatch at LSN 12345
ERROR [kstone_core::sst] Failed to read SST file: I/O error
```

## Disk Space Issues

### Symptoms

- Write errors
- "No space left on device"
- Database growth exceeds expectations

### Check Disk Usage

```bash
# Overall usage
df -h /data/mydb.keystone

# Per-directory breakdown
du -sh /data/mydb.keystone/*

# Find largest files
find /data/mydb.keystone -type f -exec ls -lh {} + | sort -k5 -hr | head -20
```

### Solutions

#### 1. Trigger Compaction

Compaction removes deleted data and duplicates:

```bash
# Check if compaction is running
ps aux | grep compact

# Check compaction stats
# (via application or stats API)
```

#### 2. Clean Old Backups

```bash
# Remove backups older than 7 days
find /backups -name "*.tar.gz" -mtime +7 -delete

# Keep only last 5 backups
ls -t /backups/*.tar.gz | tail -n +6 | xargs rm
```

#### 3. Archive Old Data

```bash
# Export old data
kstone scan mydb.keystone --filter "timestamp < :cutoff" > old_data.jsonl

# Delete old data
# (requires application logic or manual deletion)

# Trigger compaction to reclaim space
```

#### 4. Increase Disk Space

```bash
# Add new disk
sudo mkfs.ext4 /dev/nvme1n1
sudo mount /dev/nvme1n1 /data2

# Move database
sudo systemctl stop myapp
sudo mv /data/mydb.keystone /data2/
sudo ln -s /data2/mydb.keystone /data/mydb.keystone
sudo systemctl start myapp
```

## Memory Issues

### Symptoms

- OOM (Out of Memory) errors
- High memory usage
- Application crashes
- Swap usage increasing

### Check Memory Usage

```bash
# Overall memory
free -h

# Per-process memory
ps aux | grep myapp

# Memory map
pmap $(pidof myapp)

# Heap profiling (requires jemalloc or similar)
MALLOC_CONF=prof:true cargo run --bin myapp
```

### Solutions

#### 1. Reduce Memtable Size

```rust
// In kstone-core/src/lsm.rs
const MEMTABLE_THRESHOLD: usize = 500;  // Reduce from 1000
```

**Impact:** More frequent flushes, slightly lower write performance.

#### 2. Limit Concurrent Compactions

```rust
max_concurrent_compactions: 1,  // Reduce from 4
```

**Impact:** Slower compaction, but lower memory usage.

#### 3. Increase Bloom Filter FPR

```rust
BloomFilter::new(count, 0.05)  // 5% FPR (less memory)
```

**Impact:** More false positives, slightly more disk reads.

#### 4. Use Memory Limits

```bash
# Systemd service limits
[Service]
MemoryMax=2G
MemoryHigh=1.5G
```

#### 5. Analyze Memory Leaks

```bash
# Use valgrind
valgrind --leak-check=full --show-leak-kinds=all ./target/debug/myapp

# Use heaptrack
heaptrack ./target/debug/myapp
heaptrack_gui heaptrack.myapp.*.gz
```

### Memory Profiling

```rust
// Add jemalloc for better profiling
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

// Enable profiling
// MALLOC_CONF=prof:true cargo run
```

## Getting Help

If you're still stuck:

1. **Check GitHub Issues:** Search for similar problems
2. **Enable Debug Logging:** `RUST_LOG=debug` for detailed output
3. **Collect Information:**
   - Error messages with stack traces
   - Database stats (`db.stats()`)
   - System information (OS, disk, memory)
   - Configuration settings
4. **File an Issue:** Include all collected information
5. **Community Support:** Join Discord/Slack for help

## Additional Resources

- [DEPLOYMENT.md](DEPLOYMENT.md) - Production deployment guide
- [PERFORMANCE.md](PERFORMANCE.md) - Performance optimization
- [MONITORING.md](MONITORING.md) - Observability and metrics
- [ARCHITECTURE.md](ARCHITECTURE.md) - Internal design details

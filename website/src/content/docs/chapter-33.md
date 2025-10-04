# Chapter 33: Troubleshooting

Even well-designed systems encounter issues. This chapter provides systematic approaches to diagnosing and resolving common KeystoneDB problems, from simple configuration errors to complex corruption recovery scenarios.

## Common Errors and Solutions

### IO Error: No such file or directory

**Error Message:**
```
Error: IO error: No such file or directory (os error 2)
```

**Symptoms:**
- Application fails to start
- Database operations fail immediately
- Error occurs on `Database::open()` or `Database::create()`

**Root Causes:**
1. Database directory doesn't exist
2. Incorrect path specified
3. Permission denied (appears as ENOENT on some systems)
4. Symlink pointing to non-existent location

**Diagnosis:**

```bash
# Check if database exists
ls -la /path/to/database.keystone

# Check parent directory
ls -la $(dirname /path/to/database.keystone)

# Check for symlinks
readlink -f /path/to/database.keystone

# Check permissions
stat /path/to/database.keystone
```

**Solutions:**

```bash
# Solution 1: Create database if missing
kstone create /path/to/database.keystone

# Solution 2: Verify path in configuration
# Check systemd service file or application config

# Solution 3: Fix permissions
sudo chown -R keystonedb:keystonedb /path/to/database.keystone
sudo chmod 755 $(dirname /path/to/database.keystone)
sudo chmod 700 /path/to/database.keystone

# Solution 4: Check disk space
df -h /path/to/database.keystone

# Solution 5: Verify filesystem is mounted
mount | grep $(dirname /path/to/database.keystone)
```

### Corruption Detected

**Error Messages:**
```
Error: Corruption detected: WAL checksum mismatch at LSN 12345
Error: Corruption detected: SST file header invalid
Error: Checksum mismatch in SST file
```

**Symptoms:**
- Database fails to open
- Random crashes during operations
- Data inconsistencies
- Missing or corrupted records

**Root Causes:**
1. Power loss during write
2. Disk hardware failure
3. Filesystem corruption
4. Process killed during write
5. Bugs in database code

**Diagnosis:**

```bash
# Check system logs for disk errors
sudo dmesg | grep -i "error\|ata\|sda"
sudo journalctl -k | grep -i "error\|disk"

# Check SMART status
sudo smartctl -a /dev/sda

# Check filesystem
sudo fsck -n /dev/sda1  # -n for dry-run

# Identify corrupted files
for file in /path/to/db.keystone/*.sst; do
    echo "Checking: $file"
    # Check magic number
    magic=$(xxd -p -l 4 "$file")
    if [ "$magic" != "5353544d" ]; then  # "SSTM"
        echo "  CORRUPTED: Invalid magic number"
    fi
done

# Check WAL integrity
grep "corruption\|checksum" /var/log/keystonedb/server.log
```

**Solutions:**

**Option 1: Restore from Backup** (Recommended)
```bash
# Stop application
sudo systemctl stop keystonedb

# Restore from latest backup
/opt/scripts/restore.sh /var/backups/keystonedb/backup-latest.tar.gz

# Start application
sudo systemctl start keystonedb

# Verify data
kstone-cli --db-path /path/to/db.keystone --command "stats"
```

**Option 2: Remove Corrupted SST Files** (Data Loss)
```bash
# Backup corrupted database first
cp -r /path/to/db.keystone /path/to/db.keystone.corrupted

# Identify and remove corrupted SST files
# WARNING: This causes data loss for keys in removed SST
rm /path/to/db.keystone/042-5.sst

# Restart database (will rebuild from remaining SSTs + WAL)
sudo systemctl restart keystonedb
```

**Option 3: Truncate Corrupted WAL** (Data Loss)
```bash
# Backup WAL
cp /path/to/db.keystone/wal.log /path/to/db.keystone/wal.log.backup

# Option 3a: Remove corrupted WAL entirely
# WARNING: Loses all data since last flush
rm /path/to/db.keystone/wal.log

# Option 3b: Manually truncate WAL to last good LSN
# (Requires custom tool or manual binary editing)

# Restart database
sudo systemctl restart keystonedb
```

### Permission Denied

**Error Message:**
```
Error: Permission denied (os error 13)
```

**Symptoms:**
- Database fails to open
- Write operations fail
- Cannot create new files

**Root Causes:**
1. Wrong file ownership
2. Insufficient permissions
3. SELinux or AppArmor restrictions
4. Parent directory permissions

**Diagnosis:**

```bash
# Check ownership and permissions
ls -la /path/to/db.keystone/
ls -ld /path/to/db.keystone/

# Check process user
ps aux | grep kstone-server

# Check SELinux context
ls -Z /path/to/db.keystone/
sestatus

# Check AppArmor
sudo aa-status
```

**Solutions:**

```bash
# Fix ownership
sudo chown -R keystonedb:keystonedb /path/to/db.keystone

# Fix permissions
sudo chmod 700 /path/to/db.keystone
sudo chmod 600 /path/to/db.keystone/*

# Fix parent directory
sudo chmod 755 $(dirname /path/to/db.keystone)

# SELinux: Allow access
sudo semanage fcontext -a -t var_lib_t "/path/to/db.keystone(/.*)?"
sudo restorecon -R /path/to/db.keystone

# AppArmor: Add rule (edit profile)
sudo nano /etc/apparmor.d/usr.local.bin.kstone-server
# Add: /path/to/db.keystone/** rw,
sudo systemctl reload apparmor
```

### Database Already Exists

**Error Message:**
```
Error: Database already exists at /path/to/db.keystone
```

**Symptoms:**
- `Database::create()` fails
- Application can't initialize

**Root Causes:**
1. Calling `create()` instead of `open()`
2. Previous database not cleaned up
3. Partial initialization

**Solutions:**

```bash
# Solution 1: Use open() instead of create()
# In code: Database::open() instead of Database::create()

# Solution 2: Delete existing database (DESTRUCTIVE)
rm -rf /path/to/db.keystone
kstone create /path/to/db.keystone

# Solution 3: Use different path for new database
kstone create /path/to/db-v2.keystone
```

### Out of Disk Space

**Error Message:**
```
Error: No space left on device (os error 28)
```

**Symptoms:**
- Write operations fail
- Database won't start
- Compaction fails

**Diagnosis:**

```bash
# Check disk usage
df -h /path/to/db.keystone

# Check inode usage
df -i /path/to/db.keystone

# Find largest files
du -h /path/to/db.keystone/* | sort -hr | head -20

# Check for deleted but open files
lsof | grep deleted | grep keystonedb
```

**Solutions:**

```bash
# Solution 1: Clean old backups
find /var/backups/keystonedb -name "*.tar.gz" -mtime +7 -delete

# Solution 2: Trigger compaction to reclaim space
# (compaction removes deleted data and duplicates)
# Manual trigger if supported, or wait for automatic

# Solution 3: Remove old log files
find /var/log/keystonedb -name "*.log" -mtime +30 -delete

# Solution 4: Move database to larger filesystem
sudo systemctl stop keystonedb
rsync -av /path/to/db.keystone/ /new/path/db.keystone/
# Update configuration with new path
sudo systemctl start keystonedb

# Solution 5: Increase disk space
# Extend volume, add new disk, etc.
```

### High Memory Usage / Out of Memory

**Error Message:**
```
Error: Cannot allocate memory
killed (OOM Killer)
```

**Symptoms:**
- Process killed by OOM killer
- Slow performance
- Swap usage high
- System becomes unresponsive

**Diagnosis:**

```bash
# Check memory usage
free -h

# Check process memory
ps aux --sort=-%mem | head -10

# Check OOM killer logs
dmesg | grep -i "killed process"
sudo journalctl -k | grep -i "out of memory"

# Check memory map
pmap $(pgrep kstone-server)

# Memory usage over time
vmstat 1 10
```

**Solutions:**

```bash
# Solution 1: Increase system memory
# Add more RAM or adjust cloud instance size

# Solution 2: Reduce memtable size
# Modify kstone-core/src/lsm.rs:
const MEMTABLE_THRESHOLD: usize = 500;  # Reduce from 1000

# Solution 3: Limit concurrent compactions
# Modify compaction config:
max_concurrent_compactions: 1  # Reduce from 4

# Solution 4: Increase swap (temporary)
sudo fallocate -l 8G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# Solution 5: Set memory limits (systemd)
[Service]
MemoryMax=8G
MemoryHigh=6G

# Solution 6: Configure larger bloom filter FPR
# Use 5% instead of 1% (uses less memory)
```

### Connection Refused (Server Mode)

**Error Message:**
```
Error: Connection refused (os error 111)
```

**Symptoms:**
- Clients can't connect to server
- gRPC calls fail immediately

**Diagnosis:**

```bash
# Check if server is running
sudo systemctl status keystonedb
ps aux | grep kstone-server

# Check listening ports
sudo netstat -tlnp | grep 50051
sudo ss -tlnp | grep 50051

# Check firewall
sudo iptables -L -n | grep 50051
sudo ufw status

# Test connectivity
telnet localhost 50051
nc -zv localhost 50051

# Check logs
sudo journalctl -u keystonedb -n 100
```

**Solutions:**

```bash
# Solution 1: Start server
sudo systemctl start keystonedb

# Solution 2: Check bind address
# Ensure server binds to correct interface
# --host 0.0.0.0 (all interfaces) vs 127.0.0.1 (localhost only)

# Solution 3: Open firewall
sudo ufw allow 50051/tcp
sudo iptables -A INPUT -p tcp --dport 50051 -j ACCEPT

# Solution 4: Check port conflict
# If port is in use by another process
sudo lsof -i :50051
# Kill conflicting process or use different port

# Solution 5: Verify server configuration
grep -i "port\|host" /etc/systemd/system/keystonedb.service
```

## Performance Issues

### Slow Writes

**Symptoms:**
- Write latency >10ms (P99)
- Throughput <1000 ops/sec
- Application feels sluggish

**Diagnosis:**

```bash
# Monitor disk I/O
iostat -x 1 10

# Check WAL fsync time
RUST_LOG=debug cargo run 2>&1 | grep "fsync\|wal"

# Check memtable flush frequency
RUST_LOG=debug cargo run 2>&1 | grep "flush"

# Check compaction activity
RUST_LOG=debug cargo run 2>&1 | grep "compaction"

# Monitor with stats API
# Check write amplification, active compactions
```

**Root Causes:**
1. Slow disk (HDD instead of SSD)
2. Frequent memtable flushes (small threshold)
3. Aggressive compaction
4. Disk I/O scheduler misconfigured

**Solutions:**

```bash
# Solution 1: Use faster disk
# Upgrade to NVMe SSD

# Solution 2: Increase memtable threshold
# In kstone-core/src/lsm.rs:
const MEMTABLE_THRESHOLD: usize = 5000;  # Increase from 1000

# Solution 3: Reduce compaction frequency
# In compaction config:
sst_threshold: 8  # Increase from 4

# Solution 4: Use batch writes
# Application code:
db.batch_write()
    .put(key1, item1)
    .put(key2, item2)
    .put(key3, item3)
    .execute()?;

# Solution 5: Optimize I/O scheduler
echo none | sudo tee /sys/block/nvme0n1/queue/scheduler

# Solution 6: Disable barrier (if safe)
# Remount with barrier=0 (only if UPS or battery-backed RAID)
```

### Slow Reads

**Symptoms:**
- Read latency >50ms (P99)
- Query operations slow
- Get operations slow

**Diagnosis:**

```bash
# Check SST file count per stripe
for i in {0..255}; do
    count=$(ls /path/to/db.keystone/$(printf "%03d" $i)_*.sst 2>/dev/null | wc -l)
    if [ $count -gt 10 ]; then
        echo "Stripe $i: $count SSTs (high)"
    fi
done

# Check bloom filter effectiveness
RUST_LOG=debug cargo run 2>&1 | grep "bloom"

# Monitor disk I/O
iostat -x 1 10
```

**Root Causes:**
1. Too many SST files (compaction falling behind)
2. No indexes for query pattern
3. Full table scans instead of targeted queries
4. Bloom filter false positives

**Solutions:**

```bash
# Solution 1: Aggressive compaction
# Reduce SST threshold:
sst_threshold: 2  # Compact when ≥2 SSTs

# Solution 2: Add indexes
# Use LSI/GSI for common query patterns
let schema = TableSchema::new()
    .add_global_index(GlobalSecondaryIndex::new("email-index", "email"));

# Solution 3: Use targeted queries
# Avoid scans, use partition keys:
# ❌ Slow: Scan with filter
# ✅ Fast: Query by partition key
let query = Query::new(b"user#123");

# Solution 4: Improve bloom filters
# Lower false positive rate (uses more memory):
BloomFilter::new(count, 0.001)  # 0.1% instead of 1%

# Solution 5: Increase page cache
# Add more RAM for OS to cache SST files
```

### High Latency Spikes

**Symptoms:**
- P99 >> P50 latency
- Occasional 100ms+ spikes
- Unpredictable performance

**Diagnosis:**

```bash
# Correlate spikes with compaction
RUST_LOG=debug cargo run 2>&1 | grep -E "(compaction|latency)"

# Monitor system metrics
vmstat 1 60

# Check for THP (Transparent Huge Pages)
cat /sys/kernel/mm/transparent_hugepage/enabled

# Check for memory pressure
free -h
cat /proc/meminfo | grep -i "dirty\|writeback"
```

**Root Causes:**
1. Compaction blocking operations
2. Transparent Huge Pages (THP)
3. Memory pressure / swapping
4. Disk I/O contention

**Solutions:**

```bash
# Solution 1: Reduce concurrent compactions
max_concurrent_compactions: 2  # Reduce from 4

# Solution 2: Disable THP
echo never | sudo tee /sys/kernel/mm/transparent_hugepage/enabled
echo never | sudo tee /sys/kernel/mm/transparent_hugepage/defrag

# Solution 3: Reduce memory pressure
# Add more RAM or reduce memtable size

# Solution 4: Tune I/O scheduler
echo mq-deadline | sudo tee /sys/block/sda/queue/scheduler

# Solution 5: Monitor and alert on spikes
# Prometheus alert:
histogram_quantile(0.99, rate(kstone_rpc_duration_seconds_bucket[5m])) > 0.1
```

### Write Amplification

**Symptoms:**
- Disk writes >> application writes
- High compaction activity
- SSD wearing out quickly

**Diagnosis:**

```rust
// Check compaction stats
let stats = db.stats()?;
let write_amp = stats.compaction.total_bytes_written as f64
    / stats.compaction.total_bytes_read.max(1) as f64;

println!("Write amplification: {:.2}x", write_amp);
```

**Solutions:**

```bash
# Solution 1: Increase memtable size
const MEMTABLE_THRESHOLD: usize = 10000;  # Larger SSTs

# Solution 2: Reduce compaction frequency
sst_threshold: 10  # Allow more SSTs before compaction

# Solution 3: Use larger items
# Combine small records into larger items

# Solution 4: Batch writes
# Amortize WAL overhead across multiple records
```

## Log Analysis Techniques

### Enable Debug Logging

```bash
# Temporary (current session)
export RUST_LOG=debug
cargo run --bin kstone-server

# Permanent (systemd)
sudo systemctl edit keystonedb
# Add:
[Service]
Environment="RUST_LOG=debug"

sudo systemctl restart keystonedb
```

### Finding Errors

```bash
# All errors
grep "ERROR" /var/log/keystonedb/server.log

# Errors with context
grep -B 5 -A 5 "ERROR" /var/log/keystonedb/server.log

# Specific error types
grep "Corruption detected" /var/log/keystonedb/server.log
grep "IO error" /var/log/keystonedb/server.log
grep "Permission denied" /var/log/keystonedb/server.log

# Recent errors (last hour)
journalctl -u keystonedb --since "1 hour ago" | grep ERROR
```

### Trace Specific Request

```bash
# Extract trace ID from error
TRACE_ID=$(grep "ERROR" /var/log/keystonedb/server.log | \
    grep -o 'trace_id="[^"]*"' | head -1 | cut -d'"' -f2)

# Get all logs for that request
grep "trace_id=\"$TRACE_ID\"" /var/log/keystonedb/server.log

# Or one-liner:
grep "$(grep "ERROR" /var/log/keystonedb/server.log | \
    grep -o 'trace_id="[^"]*"' | head -1)" \
    /var/log/keystonedb/server.log
```

### Analyzing Performance

```bash
# Find slow operations (>100ms)
grep "duration_ms" /var/log/keystonedb/server.log | \
    awk '$NF > 100 {print}'

# Compaction activity
grep "compaction completed" /var/log/keystonedb/server.log

# Flush frequency
grep "memtable flush" /var/log/keystonedb/server.log | wc -l

# Request distribution
grep "Received" /var/log/keystonedb/server.log | \
    grep -o 'method="[^"]*"' | sort | uniq -c
```

### Log Rotation

Configure logrotate for KeystoneDB logs:

```bash
# Create /etc/logrotate.d/keystonedb
sudo tee /etc/logrotate.d/keystonedb <<EOF
/var/log/keystonedb/*.log {
    daily
    rotate 30
    compress
    delaycompress
    missingok
    notifempty
    create 0644 keystonedb keystonedb
    sharedscripts
    postrotate
        systemctl reload keystonedb > /dev/null 2>&1 || true
    endscript
}
EOF

# Test configuration
sudo logrotate -d /etc/logrotate.d/keystonedb

# Force rotation
sudo logrotate -f /etc/logrotate.d/keystonedb
```

## Health Check Interpretation

### Healthy Status

**Indicators:**
- ✅ Database opens successfully
- ✅ All operations functioning
- ✅ Compaction keeping pace
- ✅ Disk space available
- ✅ No errors in logs

**Action:** None required, continue monitoring

### Degraded Status

**Warning Conditions:**
- ⚠️ High SST count (10-15 files per stripe)
- ⚠️ Compaction falling behind
- ⚠️ Disk usage >80%
- ⚠️ High write amplification

**Actions:**
```bash
# Check SST counts
ls /path/to/db.keystone/*.sst | wc -l

# Trigger manual compaction (if supported)
# Or adjust compaction thresholds

# Free up disk space
/opt/scripts/cleanup-backups.sh

# Monitor closely, may progress to unhealthy
```

### Unhealthy Status

**Error Conditions:**
- ✗ Database directory inaccessible
- ✗ Corruption detected
- ✗ Disk full
- ✗ Critical I/O errors

**Actions:**
```bash
# IMMEDIATE: Page on-call engineer

# Check system status
df -h
dmesg | tail -50
sudo systemctl status keystonedb

# Attempt recovery
sudo systemctl restart keystonedb

# If restart fails, restore from backup
/opt/scripts/restore.sh /var/backups/keystonedb/latest.tar.gz
```

## Getting Help

When troubleshooting complex issues:

1. **Collect Information:**
   - Error messages (full stack trace)
   - Database stats (`db.stats()`)
   - System info (OS, disk, memory)
   - Recent changes
   - Logs (last 100 lines)

2. **Check Documentation:**
   - This troubleshooting guide
   - DEPLOYMENT.md
   - MONITORING.md
   - GitHub issues

3. **Search for Similar Issues:**
   - GitHub issue tracker
   - Stack Overflow
   - Community forums

4. **File a Bug Report:**
   - Include all collected information
   - Minimal reproduction case
   - Expected vs actual behavior
   - Environment details

5. **Community Support:**
   - Discord/Slack channel
   - Mailing list
   - Professional support (if available)

With systematic troubleshooting and these diagnostic techniques, most KeystoneDB issues can be quickly identified and resolved.

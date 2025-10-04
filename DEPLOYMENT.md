# KeystoneDB Production Deployment Guide

This guide covers production deployment, configuration, and operational best practices for KeystoneDB.

## Table of Contents

1. [System Requirements](#system-requirements)
2. [Installation](#installation)
3. [Configuration](#configuration)
4. [Production Best Practices](#production-best-practices)
5. [Performance Tuning](#performance-tuning)
6. [Backup and Restore](#backup-and-restore)
7. [Monitoring Setup](#monitoring-setup)
8. [Security Considerations](#security-considerations)

## System Requirements

### Minimum Requirements

- **OS**: Linux (x86_64), macOS (Intel/Apple Silicon), Windows
- **CPU**: 2 cores
- **Memory**: 2 GB RAM
- **Disk**: 10 GB available space
- **Filesystem**: ext4, XFS, APFS, or NTFS

### Recommended for Production

- **OS**: Linux (Ubuntu 20.04+, RHEL 8+, or similar)
- **CPU**: 4-8 cores (more cores improve parallel scan and compaction)
- **Memory**: 8-16 GB RAM
- **Disk**: SSD (NVMe preferred) with 100+ GB available
- **Filesystem**: ext4 or XFS with noatime mount option

### Performance Considerations

**Disk Type Impact:**
- **NVMe SSD**: Best performance (100k+ IOPS)
- **SATA SSD**: Good performance (10-50k IOPS)
- **HDD**: Not recommended for production (100-200 IOPS)

**Memory Impact:**
- 1 GB per stripe for hot data caching
- Additional memory for memtable (configurable)
- OS page cache for SST files (benefits from more RAM)

## Installation

### From Source

```bash
# Clone repository
git clone https://github.com/yourusername/keystonedb.git
cd keystonedb

# Build release binary
cargo build --release

# Install to system path (optional)
sudo cp target/release/kstone /usr/local/bin/
sudo cp target/release/kstone-server /usr/local/bin/  # If using server mode
```

### Binary Distribution

```bash
# Download latest release
curl -LO https://github.com/yourusername/keystonedb/releases/latest/download/kstone-linux-x64.tar.gz

# Extract
tar xzf kstone-linux-x64.tar.gz

# Install
sudo mv kstone /usr/local/bin/
sudo chmod +x /usr/local/bin/kstone
```

### Verify Installation

```bash
kstone --version
# Output: kstone 0.1.0
```

## Configuration

### Database Creation

```bash
# Create database with default settings
kstone create /data/mydb.keystone

# Verify creation
ls -lh /data/mydb.keystone/
# Should show: wal_*.log files and possibly *.sst files
```

### DatabaseConfig Options

KeystoneDB uses sensible defaults, but can be configured for specific workloads:

```rust
use kstone_api::Database;
use kstone_core::compaction::CompactionConfig;

// Default configuration (recommended for most use cases)
let db = Database::create("/data/mydb.keystone")?;

// Custom configuration
let compaction_config = CompactionConfig {
    enabled: true,
    sst_threshold: 4,              // Compact when ≥4 SSTs
    check_interval_secs: 5,         // Check every 5 seconds
    max_concurrent_compactions: 4,  // Max parallel compactions
};

// Note: API for passing custom config is in development
// Currently, configuration is hardcoded in kstone-core
```

### Configuration Parameters

#### Memtable Settings

**Location**: `kstone-core/src/lsm.rs`

```rust
const MEMTABLE_THRESHOLD: usize = 1000;  // Records per stripe before flush
```

**Tuning Guide:**
- **Write-heavy workload**: Increase to 5000-10000 (reduces flush frequency)
- **Memory-constrained**: Decrease to 500 (reduces memory usage)
- **Balanced (default)**: 1000

#### Compaction Settings

**Location**: `kstone-core/src/compaction.rs`

```rust
CompactionConfig {
    enabled: true,                  // Enable background compaction
    sst_threshold: 4,               // Compact when stripe has ≥N SSTs
    check_interval_secs: 5,         // Check frequency
    max_concurrent_compactions: 4,  // Parallel compaction limit
}
```

**Tuning Guide:**
- **Read-heavy**: Lower threshold (2-3) for fewer SSTs
- **Write-heavy**: Higher threshold (8-10) to reduce compaction overhead
- **Balanced (default)**: 4

#### Bloom Filter Settings

**Location**: `kstone-core/src/bloom.rs`

```rust
BloomFilter::new(record_count, 0.01)  // 1% false positive rate
```

**Tuning Guide:**
- **Memory-constrained**: 5% FPR (less memory per filter)
- **Read-optimized**: 0.1% FPR (more memory, fewer disk reads)
- **Balanced (default)**: 1% FPR

## Production Best Practices

### 1. Filesystem Configuration

**Recommended mount options for database directory:**

```bash
# /etc/fstab entry for ext4
/dev/nvme0n1  /data  ext4  noatime,data=ordered  0  2

# Mount with noatime to reduce write overhead
mount -o remount,noatime /data
```

**Why noatime?**
- Reduces write I/O by not updating file access times
- Improves performance by 5-10% for read-heavy workloads

### 2. Disable Transparent Huge Pages (Linux)

```bash
# Check current status
cat /sys/kernel/mm/transparent_hugepage/enabled

# Disable (add to /etc/rc.local for persistence)
echo never > /sys/kernel/mm/transparent_hugepage/enabled
echo never > /sys/kernel/mm/transparent_hugepage/defrag
```

**Why?** THP can cause unpredictable latency spikes during memory allocation.

### 3. Set File Descriptor Limits

```bash
# Check current limits
ulimit -n

# Set higher limits (add to /etc/security/limits.conf)
*  soft  nofile  65536
*  hard  nofile  65536

# Apply immediately
ulimit -n 65536
```

**Why?** Each stripe opens multiple file descriptors (WAL + SSTs).

### 4. Database Directory Structure

```bash
# Recommended directory structure
/data/
├── mydb.keystone/          # Database files
│   ├── wal_000.log         # WAL for stripe 0
│   ├── wal_001.log         # WAL for stripe 1
│   ├── ...
│   ├── 000_1234567890.sst # SST for stripe 0
│   └── 001_1234567891.sst # SST for stripe 1
├── backups/                # Backup location
└── logs/                   # Application logs
```

### 5. Systemd Service Configuration

```ini
# /etc/systemd/system/myapp.service
[Unit]
Description=My KeystoneDB Application
After=network.target

[Service]
Type=simple
User=kstone
Group=kstone
WorkingDirectory=/opt/myapp
ExecStart=/opt/myapp/bin/myapp --db-path /data/mydb.keystone
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

# Resource limits
LimitNOFILE=65536
MemoryMax=16G

# Environment
Environment="RUST_LOG=info"
Environment="RUST_BACKTRACE=1"

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable myapp
sudo systemctl start myapp
```

### 6. Logging Configuration

```bash
# Set log level via environment variable
export RUST_LOG=info  # Production default

# Available levels (least to most verbose):
# - error: Only errors
# - warn: Warnings and errors
# - info: General information (recommended for production)
# - debug: Detailed debugging information
# - trace: Very verbose tracing
```

## Performance Tuning

### Identifying Bottlenecks

#### Check SST File Count

```bash
# Count SSTs per stripe
for i in {0..255}; do
    count=$(ls /data/mydb.keystone/$(printf "%03d" $i)_*.sst 2>/dev/null | wc -l)
    if [ $count -gt 5 ]; then
        echo "Stripe $i: $count SSTs (consider tuning)"
    fi
done
```

**Interpretation:**
- **0-3 SSTs**: Healthy
- **4-10 SSTs**: Normal, compaction running
- **>10 SSTs**: Compaction falling behind

#### Monitor Database Size

```bash
# Check total database size
du -sh /data/mydb.keystone

# Check size growth over time
watch -n 60 "du -sh /data/mydb.keystone"
```

### Tuning for Write-Heavy Workloads

1. **Increase memtable threshold** (requires code change):
   ```rust
   // In kstone-core/src/lsm.rs
   const MEMTABLE_THRESHOLD: usize = 5000;  // Increased from 1000
   ```

2. **Reduce compaction frequency**:
   ```rust
   // In kstone-core/src/compaction.rs
   sst_threshold: 8,  // Increased from 4
   ```

3. **Use batch writes**:
   ```rust
   let batch = db.batch_write()
       .put(key1, item1)
       .put(key2, item2)
       .put(key3, item3)
       .execute()?;
   ```

### Tuning for Read-Heavy Workloads

1. **Aggressive compaction**:
   ```rust
   sst_threshold: 2,  // Decreased from 4 (more frequent compaction)
   ```

2. **Lower bloom filter FPR**:
   ```rust
   BloomFilter::new(count, 0.001)  // 0.1% instead of 1%
   ```

3. **Add indexes for common queries**:
   ```rust
   let schema = TableSchema::builder()
       .add_gsi("by-email", "email", None, IndexProjection::All)
       .build();
   let db = Database::create_with_schema(path, schema)?;
   ```

### Tuning for Memory-Constrained Systems

1. **Reduce memtable size**:
   ```rust
   const MEMTABLE_THRESHOLD: usize = 500;  // Decreased from 1000
   ```

2. **Limit concurrent compactions**:
   ```rust
   max_concurrent_compactions: 2,  // Decreased from 4
   ```

3. **Use higher bloom filter FPR**:
   ```rust
   BloomFilter::new(count, 0.05)  // 5% FPR (less memory)
   ```

## Backup and Restore

### Backup Strategy

KeystoneDB uses immutable SST files, making backups straightforward.

#### Full Backup Procedure

```bash
#!/bin/bash
# backup.sh - Full database backup

DB_DIR="/data/mydb.keystone"
BACKUP_DIR="/data/backups/$(date +%Y%m%d_%H%M%S)"

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Stop writes (optional - for consistent snapshot)
# kill -STOP $(pidof myapp)

# Copy all SST files (immutable)
cp "$DB_DIR"/*.sst "$BACKUP_DIR/" 2>/dev/null || true

# Copy WAL files (contains recent writes)
cp "$DB_DIR"/wal_*.log "$BACKUP_DIR/"

# Resume writes
# kill -CONT $(pidof myapp)

# Compress backup
tar czf "$BACKUP_DIR.tar.gz" -C "$BACKUP_DIR" .
rm -rf "$BACKUP_DIR"

echo "Backup completed: $BACKUP_DIR.tar.gz"
```

#### Incremental Backup (SST files only)

```bash
#!/bin/bash
# incremental-backup.sh - Backup only new SST files

DB_DIR="/data/mydb.keystone"
BACKUP_DIR="/data/backups/incremental"
LAST_BACKUP="$BACKUP_DIR/.last_backup"

mkdir -p "$BACKUP_DIR"

# Copy SST files newer than last backup
if [ -f "$LAST_BACKUP" ]; then
    find "$DB_DIR" -name "*.sst" -newer "$LAST_BACKUP" -exec cp {} "$BACKUP_DIR/" \;
else
    cp "$DB_DIR"/*.sst "$BACKUP_DIR/" 2>/dev/null || true
fi

# Update timestamp
touch "$LAST_BACKUP"

echo "Incremental backup completed"
```

#### Automated Backup with Cron

```bash
# /etc/cron.d/keystonedb-backup
# Full backup daily at 2 AM
0 2 * * * kstone /opt/scripts/backup.sh

# Incremental backup every 4 hours
0 */4 * * * kstone /opt/scripts/incremental-backup.sh

# Clean old backups (keep 7 days)
0 3 * * * find /data/backups -name "*.tar.gz" -mtime +7 -delete
```

### Restore Procedure

```bash
#!/bin/bash
# restore.sh - Restore database from backup

BACKUP_FILE="/data/backups/20250115_020000.tar.gz"
DB_DIR="/data/mydb.keystone"

# Stop application
sudo systemctl stop myapp

# Backup current state (just in case)
mv "$DB_DIR" "$DB_DIR.old.$(date +%s)"

# Create database directory
mkdir -p "$DB_DIR"

# Extract backup
tar xzf "$BACKUP_FILE" -C "$DB_DIR"

# Set permissions
chown -R kstone:kstone "$DB_DIR"

# Start application (will replay WAL)
sudo systemctl start myapp

echo "Restore completed from $BACKUP_FILE"
```

### Point-in-Time Recovery

KeystoneDB's WAL enables point-in-time recovery:

1. Restore SST files from backup
2. Copy WAL files from backup
3. Database automatically replays WAL on open
4. All writes since last SST flush are recovered

### Backup Best Practices

1. **Store backups on different storage**: Use network storage or cloud
2. **Encrypt backups**: Use GPG or similar for sensitive data
3. **Test restores regularly**: Verify backup integrity monthly
4. **Monitor backup size**: Alert on unexpected growth
5. **Automate verification**: Script to restore and validate data

## Monitoring Setup

### Using stats() API

```rust
use kstone_api::Database;

let db = Database::open("/data/mydb.keystone")?;

// Get database statistics
let stats = db.stats()?;

println!("Total SST files: {}", stats.total_sst_files);
println!("Compactions performed: {}", stats.compaction.total_compactions);
println!("SSTs merged: {}", stats.compaction.total_ssts_merged);
println!("Bytes reclaimed: {}", stats.compaction.total_bytes_reclaimed);
```

### Using health() API

```rust
let health = db.health();

match health.status {
    HealthStatus::Healthy => println!("Database is healthy"),
    HealthStatus::Degraded => {
        println!("Database is degraded:");
        for warning in &health.warnings {
            println!("  - {}", warning);
        }
    }
    HealthStatus::Unhealthy => {
        println!("Database is unhealthy:");
        for error in &health.errors {
            println!("  - {}", error);
        }
    }
}
```

### Health Check Script

```bash
#!/bin/bash
# health-check.sh - Periodic health monitoring

DB_DIR="/data/mydb.keystone"

# Check if database directory exists
if [ ! -d "$DB_DIR" ]; then
    echo "ERROR: Database directory not found"
    exit 1
fi

# Check disk space (warn if <20% free)
DISK_USAGE=$(df -h "$DB_DIR" | tail -1 | awk '{print $5}' | sed 's/%//')
if [ "$DISK_USAGE" -gt 80 ]; then
    echo "WARNING: Disk usage is ${DISK_USAGE}%"
fi

# Check SST file count per stripe
MAX_SST=0
for i in {0..255}; do
    count=$(ls "$DB_DIR"/$(printf "%03d" $i)_*.sst 2>/dev/null | wc -l)
    if [ $count -gt $MAX_SST ]; then
        MAX_SST=$count
    fi
done

if [ $MAX_SST -gt 20 ]; then
    echo "WARNING: Stripe has $MAX_SST SST files (compaction may be falling behind)"
fi

# Check file descriptors
FD_COUNT=$(lsof -p $(pidof myapp) 2>/dev/null | wc -l)
FD_LIMIT=$(ulimit -n)
FD_PCT=$((FD_COUNT * 100 / FD_LIMIT))

if [ $FD_PCT -gt 80 ]; then
    echo "WARNING: Using ${FD_PCT}% of file descriptor limit"
fi

echo "Health check completed"
```

### Prometheus Metrics (Server Mode)

If using kstone-server, metrics are exposed at `:9090/metrics`:

```bash
# Scrape metrics
curl http://localhost:9090/metrics

# Example Prometheus config
scrape_configs:
  - job_name: 'keystonedb'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
```

See [MONITORING.md](MONITORING.md) for detailed metrics documentation.

## Security Considerations

### 1. File Permissions

```bash
# Create dedicated user
sudo useradd -r -s /bin/false kstone

# Set restrictive permissions
sudo chown -R kstone:kstone /data/mydb.keystone
sudo chmod 700 /data/mydb.keystone
sudo chmod 600 /data/mydb.keystone/*
```

### 2. Encryption at Rest

KeystoneDB supports block-level encryption (Phase 1+):

```rust
// Note: Encryption API is in development
let db = Database::create_with_encryption(
    "/data/mydb.keystone",
    encryption_key,
)?;
```

**Alternative**: Use filesystem-level encryption (LUKS, dm-crypt):

```bash
# Create encrypted volume
sudo cryptsetup luksFormat /dev/nvme0n1p1
sudo cryptsetup luksOpen /dev/nvme0n1p1 encrypted_db

# Mount and use
sudo mkfs.ext4 /dev/mapper/encrypted_db
sudo mount /dev/mapper/encrypted_db /data
```

### 3. Network Security (Server Mode)

```bash
# Firewall rules (allow only from application servers)
sudo ufw allow from 10.0.0.0/24 to any port 50051 proto tcp

# Use TLS for gRPC
kstone-server --db-path /data/mydb.keystone \
    --tls-cert /etc/ssl/certs/server.crt \
    --tls-key /etc/ssl/private/server.key
```

### 4. Backup Encryption

```bash
# Encrypt backups with GPG
tar czf - /data/mydb.keystone | \
    gpg --encrypt --recipient admin@example.com \
    > backup_$(date +%Y%m%d).tar.gz.gpg

# Decrypt for restore
gpg --decrypt backup_20250115.tar.gz.gpg | tar xzf -
```

### 5. Audit Logging

```rust
// Enable audit logging (example pattern)
use tracing::info;

info!(
    user_id = %user_id,
    operation = "put",
    key = %key,
    "Database write operation"
);
```

### 6. Access Control

```rust
// Application-level access control (example)
fn authorize_write(user: &User, key: &[u8]) -> Result<()> {
    if !user.can_write(key) {
        return Err(Error::PermissionDenied("User cannot write to this key".into()));
    }
    Ok(())
}

// Use before database operations
authorize_write(&user, b"user#123")?;
db.put(b"user#123", item)?;
```

## Troubleshooting

For common issues and solutions, see [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

## Additional Resources

- [PERFORMANCE.md](PERFORMANCE.md) - Performance optimization guide
- [MONITORING.md](MONITORING.md) - Observability and metrics
- [ARCHITECTURE.md](ARCHITECTURE.md) - Internal design details
- [README.md](README.md) - General documentation

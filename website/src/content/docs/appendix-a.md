# Appendix A: Configuration Reference

This appendix provides a comprehensive reference for all configuration options available in KeystoneDB. Configuration can be provided via code (DatabaseConfig), environment variables, or configuration files.

## DatabaseConfig

The primary configuration struct for runtime database parameters.

### Structure Definition

```rust
pub struct DatabaseConfig {
    pub max_memtable_size_bytes: Option<usize>,
    pub max_memtable_records: usize,
    pub max_wal_size_bytes: Option<u64>,
    pub max_total_disk_bytes: Option<u64>,
    pub write_buffer_size: usize,
}
```

### Fields

#### max_memtable_size_bytes

**Type:** `Option<usize>`
**Default:** `None` (unlimited)
**Description:** Maximum size in bytes for a single stripe's memtable before flushing to SST.
**Range:** 1 MB to 1 GB recommended
**Example:**
```rust
DatabaseConfig::new()
    .with_max_memtable_size_bytes(10 * 1024 * 1024)  // 10 MB
```

**Tuning Guidance:**
- **Low memory:** 1-5 MB
- **Balanced:** 10-50 MB
- **High memory:** 100-500 MB

**Trade-offs:**
- Larger = fewer flushes, better write throughput
- Smaller = less memory usage, faster recovery

#### max_memtable_records

**Type:** `usize`
**Default:** `1000`
**Description:** Maximum number of records in memtable before flushing.
**Range:** 100 to 100,000 recommended
**Example:**
```rust
DatabaseConfig::new()
    .with_max_memtable_records(5000)
```

**Tuning Guidance:**
- **Small records:** Higher limit (10,000+)
- **Large records:** Lower limit (500-1000)
- **Write-heavy:** Higher limit (5,000-10,000)
- **Memory-constrained:** Lower limit (500-1,000)

**Note:** Flush occurs when EITHER max_memtable_size_bytes OR max_memtable_records is exceeded.

#### max_wal_size_bytes

**Type:** `Option<u64>`
**Default:** `None` (unlimited)
**Description:** Maximum size of WAL file before rotation.
**Range:** 1 MB to 1 GB recommended
**Example:**
```rust
DatabaseConfig::new()
    .with_max_wal_size_bytes(100 * 1024 * 1024)  // 100 MB
```

**Tuning Guidance:**
- **Fast recovery:** Smaller WAL (10-50 MB)
- **Write throughput:** Larger WAL (100-500 MB)
- **Disk space:** Limit based on available space

**Impact:**
- Larger WAL = longer recovery time after crash
- Smaller WAL = more frequent flushes (lower throughput)

#### max_total_disk_bytes

**Type:** `Option<u64>`
**Default:** `None` (unlimited)
**Description:** Maximum total disk space for database.
**Range:** 100 MB to terabytes
**Example:**
```rust
DatabaseConfig::new()
    .with_max_total_disk_bytes(100 * 1024 * 1024 * 1024)  // 100 GB
```

**Behavior:**
- When limit is reached, writes return `Error::ResourceExhausted`
- Compaction continues (may free space)
- Reads continue to work normally

**Use Cases:**
- Multi-tenant systems (quota enforcement)
- Embedded devices (limited storage)
- Testing (simulate disk full scenarios)

#### write_buffer_size

**Type:** `usize`
**Default:** `1024` (1 KB)
**Description:** Buffer size for WAL and SST write operations.
**Range:** 512 bytes to 1 MB
**Example:**
```rust
DatabaseConfig::new()
    .with_write_buffer_size(4096)  // 4 KB
```

**Tuning Guidance:**
- **Default (1 KB):** Good for most workloads
- **Large writes:** 4-8 KB
- **Small writes:** 512 bytes - 1 KB

**Trade-offs:**
- Larger = fewer system calls, better throughput
- Smaller = less memory per write operation

### Builder Methods

```rust
impl DatabaseConfig {
    // Create with defaults
    pub fn new() -> Self;

    // Set max memtable size in bytes
    pub fn with_max_memtable_size_bytes(self, size: usize) -> Self;

    // Set max memtable records
    pub fn with_max_memtable_records(self, records: usize) -> Self;

    // Set max WAL size
    pub fn with_max_wal_size_bytes(self, size: u64) -> Self;

    // Set max total disk size
    pub fn with_max_total_disk_bytes(self, size: u64) -> Self;

    // Set write buffer size
    pub fn with_write_buffer_size(self, size: usize) -> Self;

    // Validate configuration
    pub fn validate(&self) -> Result<(), String>;
}
```

### Validation Rules

The `validate()` method checks:

1. **max_memtable_records > 0**
2. **write_buffer_size > 0**
3. **max_memtable_size_bytes > 0** (if set)

Invalid configurations will return an error when passed to `Database::create_with_config()`.

### Complete Example

```rust
use kstone_api::Database;
use kstone_core::config::DatabaseConfig;

let config = DatabaseConfig::new()
    .with_max_memtable_size_bytes(50 * 1024 * 1024)    // 50 MB per stripe
    .with_max_memtable_records(5000)                    // 5000 records per stripe
    .with_max_wal_size_bytes(200 * 1024 * 1024)        // 200 MB WAL
    .with_max_total_disk_bytes(10 * 1024 * 1024 * 1024) // 10 GB total
    .with_write_buffer_size(8192);                      // 8 KB buffer

// Validate before use
config.validate().expect("Invalid configuration");

let db = Database::create_with_config("/path/to/db", config)?;
```

## CompactionConfig

Configuration for background compaction behavior.

### Structure Definition

```rust
pub struct CompactionConfig {
    pub enabled: bool,
    pub sst_threshold: usize,
    pub check_interval_secs: u64,
    pub max_concurrent_compactions: usize,
}
```

### Fields

#### enabled

**Type:** `bool`
**Default:** `true`
**Description:** Enable or disable automatic background compaction.
**Example:**
```rust
CompactionConfig::new()  // enabled = true
CompactionConfig::disabled()  // enabled = false
```

**When to disable:**
- Testing scenarios
- Manual compaction control
- Read-only deployments
- Initial bulk load (re-enable after)

#### sst_threshold

**Type:** `usize`
**Default:** `10`
**Minimum:** `2`
**Description:** Trigger compaction when stripe has this many SST files.
**Example:**
```rust
CompactionConfig::new()
    .with_sst_threshold(8)
```

**Tuning Guidance:**
- **Read-heavy:** Lower threshold (4-6) - fewer SSTs = faster reads
- **Write-heavy:** Higher threshold (10-15) - less compaction overhead
- **Balanced:** Default (10)

**Trade-offs:**
- Lower = better read performance, more compaction CPU
- Higher = less compaction overhead, worse read performance

#### check_interval_secs

**Type:** `u64`
**Default:** `60` (1 minute)
**Range:** 10 seconds to 1 hour
**Description:** How often to check for compaction opportunities.
**Example:**
```rust
CompactionConfig::new()
    .with_check_interval(300)  // Check every 5 minutes
```

**Tuning Guidance:**
- **Write-heavy:** Shorter interval (30-60 seconds)
- **Read-heavy:** Longer interval (5-10 minutes)
- **Low CPU:** Longer interval (10-30 minutes)

#### max_concurrent_compactions

**Type:** `usize`
**Default:** `4`
**Minimum:** `1`
**Range:** 1 to number of CPU cores
**Description:** Maximum stripes to compact in parallel.
**Example:**
```rust
CompactionConfig::new()
    .with_max_concurrent(2)
```

**Tuning Guidance:**
- **Many CPU cores:** Higher (8-16)
- **Few CPU cores:** Lower (1-4)
- **Shared resources:** Lower (1-2)

### Builder Methods

```rust
impl CompactionConfig {
    // Create with defaults (enabled)
    pub fn new() -> Self;

    // Create disabled config
    pub fn disabled() -> Self;

    // Set SST threshold
    pub fn with_sst_threshold(self, threshold: usize) -> Self;

    // Set check interval
    pub fn with_check_interval(self, seconds: u64) -> Self;

    // Set max concurrent compactions
    pub fn with_max_concurrent(self, max: usize) -> Self;
}
```

### Example Usage

```rust
use kstone_core::compaction::CompactionConfig;

// Aggressive compaction for read-heavy workload
let config = CompactionConfig::new()
    .with_sst_threshold(4)         // Compact early
    .with_check_interval(30)       // Check frequently
    .with_max_concurrent(8);       // Use 8 threads

db.set_compaction_config(config)?;

// Lazy compaction for write-heavy workload
let config = CompactionConfig::new()
    .with_sst_threshold(15)        // Compact late
    .with_check_interval(300)      // Check infrequently
    .with_max_concurrent(2);       // Minimize CPU usage

db.set_compaction_config(config)?;

// Disable compaction for bulk load
let config = CompactionConfig::disabled();
db.set_compaction_config(config)?;

// ... perform bulk load ...

// Re-enable and trigger compaction
let config = CompactionConfig::new();
db.set_compaction_config(config)?;
```

## TableSchema

Configuration for indexes, TTL, and streams.

### Structure Definition

```rust
pub struct TableSchema {
    local_indexes: Vec<LocalSecondaryIndex>,
    global_indexes: Vec<GlobalSecondaryIndex>,
    ttl_attribute_name: Option<String>,
    stream_config: Option<StreamConfig>,
}
```

### Local Secondary Index (LSI)

```rust
pub struct LocalSecondaryIndex {
    name: String,
    sort_key_attribute: String,
    projection: IndexProjection,
}
```

**Fields:**
- `name`: Index name (unique within table)
- `sort_key_attribute`: Attribute to use as sort key
- `projection`: What attributes to include in index

**Example:**
```rust
use kstone_api::{TableSchema, LocalSecondaryIndex};

let schema = TableSchema::new()
    .add_local_index(
        LocalSecondaryIndex::new("email-index", "email")
    )
    .add_local_index(
        LocalSecondaryIndex::new("score-index", "score")
            .keys_only()  // Only store keys, not full items
    );
```

### Global Secondary Index (GSI)

```rust
pub struct GlobalSecondaryIndex {
    name: String,
    partition_key_attribute: String,
    sort_key_attribute: Option<String>,
    projection: IndexProjection,
}
```

**Fields:**
- `name`: Index name (unique within table)
- `partition_key_attribute`: Attribute to use as GSI partition key
- `sort_key_attribute`: Optional attribute for GSI sort key
- `projection`: What attributes to include in index

**Example:**
```rust
use kstone_api::{TableSchema, GlobalSecondaryIndex};

let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::new("status-index", "status")
    )
    .add_global_index(
        GlobalSecondaryIndex::with_sort_key("category-price-index", "category", "price")
            .include(vec!["name".to_string(), "description".to_string()])
    );
```

### Index Projection Types

```rust
pub enum IndexProjection {
    All,                          // Include all attributes
    KeysOnly,                     // Include only keys
    Include(Vec<String>),         // Include specific attributes
}
```

**Example:**
```rust
// All attributes (default)
LocalSecondaryIndex::new("idx1", "attr1")

// Keys only
LocalSecondaryIndex::new("idx2", "attr2")
    .keys_only()

// Specific attributes
LocalSecondaryIndex::new("idx3", "attr3")
    .include(vec!["name".to_string(), "email".to_string()])
```

### TTL Configuration

**Field:** `ttl_attribute_name: Option<String>`
**Description:** Name of attribute containing expiration timestamp
**Format:** Seconds since Unix epoch (i64 or Number) or milliseconds (Timestamp)
**Example:**

```rust
let schema = TableSchema::new()
    .with_ttl("expiresAt");

// Items with expiresAt attribute will auto-expire
let now = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs() as i64;

db.put(b"session#123", ItemBuilder::new()
    .string("token", "abc...")
    .number("expiresAt", now + 3600)  // Expires in 1 hour
    .build())?;
```

### Stream Configuration

```rust
pub struct StreamConfig {
    enabled: bool,
    view_type: StreamViewType,
    buffer_size: usize,
}

pub enum StreamViewType {
    KeysOnly,
    NewImage,
    OldImage,
    NewAndOldImages,
}
```

**Example:**
```rust
use kstone_api::{TableSchema, StreamConfig, StreamViewType};

let schema = TableSchema::new()
    .with_stream(
        StreamConfig::enabled()
            .with_view_type(StreamViewType::NewAndOldImages)
            .with_buffer_size(1000)  // Keep last 1000 changes
    );
```

## Environment Variables

KeystoneDB supports configuration via environment variables:

### Database Configuration

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `KSTONE_MAX_MEMTABLE_SIZE_BYTES` | usize | None | Max memtable size in bytes |
| `KSTONE_MAX_MEMTABLE_RECORDS` | usize | 1000 | Max records before flush |
| `KSTONE_MAX_WAL_SIZE_BYTES` | u64 | None | Max WAL size |
| `KSTONE_MAX_TOTAL_DISK_BYTES` | u64 | None | Max total disk usage |
| `KSTONE_WRITE_BUFFER_SIZE` | usize | 1024 | Write buffer size |

### Compaction Configuration

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `KSTONE_COMPACTION_ENABLED` | bool | true | Enable background compaction |
| `KSTONE_COMPACTION_SST_THRESHOLD` | usize | 10 | SST count trigger |
| `KSTONE_COMPACTION_CHECK_INTERVAL` | u64 | 60 | Check interval (seconds) |
| `KSTONE_COMPACTION_MAX_CONCURRENT` | usize | 4 | Max parallel compactions |

### Server Configuration

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `KSTONE_SERVER_HOST` | string | 127.0.0.1 | Server bind address |
| `KSTONE_SERVER_PORT` | u16 | 50051 | gRPC server port |
| `KSTONE_METRICS_PORT` | u16 | 9090 | Metrics/health port |
| `RUST_LOG` | string | info | Log level (error/warn/info/debug/trace) |

### Example Usage

```bash
# Configure database limits
export KSTONE_MAX_MEMTABLE_SIZE_BYTES=52428800  # 50 MB
export KSTONE_MAX_MEMTABLE_RECORDS=5000
export KSTONE_MAX_TOTAL_DISK_BYTES=10737418240  # 10 GB

# Configure compaction
export KSTONE_COMPACTION_SST_THRESHOLD=8
export KSTONE_COMPACTION_CHECK_INTERVAL=120  # 2 minutes
export KSTONE_COMPACTION_MAX_CONCURRENT=2

# Run server
kstone-server --db-path ./mydb.keystone
```

## Configuration File (Future)

Planned support for TOML configuration files:

```toml
# keystonedb.toml

[database]
max_memtable_size_bytes = 52428800  # 50 MB
max_memtable_records = 5000
max_wal_size_bytes = 209715200      # 200 MB
max_total_disk_bytes = 10737418240  # 10 GB
write_buffer_size = 8192

[compaction]
enabled = true
sst_threshold = 8
check_interval_secs = 120
max_concurrent_compactions = 4

[server]
host = "0.0.0.0"
port = 50051
metrics_port = 9090

[logging]
level = "info"
format = "json"
output = "stdout"
```

Load configuration:

```rust
let config = DatabaseConfig::from_file("keystonedb.toml")?;
let db = Database::create_with_config(path, config)?;
```

## Configuration Precedence

When multiple configuration sources are available:

1. **Code (highest priority)** - Explicit DatabaseConfig in code
2. **Environment variables** - Override defaults
3. **Configuration file** - Override defaults
4. **Defaults (lowest priority)** - Built-in defaults

Example:
```rust
// Default: max_memtable_records = 1000
// Config file: max_memtable_records = 3000
// Environment: KSTONE_MAX_MEMTABLE_RECORDS=5000
// Code: config.with_max_memtable_records(2000)

// Result: 2000 (code wins)
```

## Performance Tuning Presets

Pre-configured settings for common workloads:

### Read-Heavy Workload

```rust
let config = DatabaseConfig::new()
    .with_max_memtable_records(1000)       // Standard memtable
    .with_max_memtable_size_bytes(10 * 1024 * 1024);  // 10 MB

let compaction = CompactionConfig::new()
    .with_sst_threshold(4)                 // Aggressive compaction
    .with_check_interval(30)               // Frequent checks
    .with_max_concurrent(8);               // Many threads

db.set_compaction_config(compaction)?;
```

### Write-Heavy Workload

```rust
let config = DatabaseConfig::new()
    .with_max_memtable_records(10000)      // Large memtable
    .with_max_memtable_size_bytes(100 * 1024 * 1024);  // 100 MB

let compaction = CompactionConfig::new()
    .with_sst_threshold(15)                // Lazy compaction
    .with_check_interval(300)              // Infrequent checks
    .with_max_concurrent(2);               // Fewer threads

db.set_compaction_config(compaction)?;
```

### Memory-Constrained

```rust
let config = DatabaseConfig::new()
    .with_max_memtable_records(500)        // Small memtable
    .with_max_memtable_size_bytes(5 * 1024 * 1024)   // 5 MB
    .with_write_buffer_size(512);          // Small buffer

let compaction = CompactionConfig::new()
    .with_max_concurrent(1);               // Single thread

db.set_compaction_config(compaction)?;
```

### Balanced (Default)

```rust
let config = DatabaseConfig::default();
let compaction = CompactionConfig::default();
```

## Summary

Key configuration categories:

1. **Database:** Resource limits, memtable size, WAL size
2. **Compaction:** Automatic cleanup, SST thresholds, parallelism
3. **Schema:** Indexes, TTL, streams
4. **Server:** Network settings, logging

Best practices:
- Start with defaults
- Monitor performance metrics
- Tune one parameter at a time
- Validate configurations before deployment
- Document your tuning decisions

For specific tuning guidance, see [Appendix E: Benchmarking Results](#appendix-e-benchmarking-results).

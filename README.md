# KeystoneDB

**Single-file, embedded, DynamoDB-style database written in Rust**

Fast, persistent, ACID-compliant storage with a familiar DynamoDB API, PartiQL support, and local-first performance.

## Features

### Core Operations
- **Put/Get/Delete**: Basic CRUD operations with partition and sort keys
- **Query**: Efficient queries using partition key with optional sort key conditions
- **Scan**: Full table scans with parallel execution across 256 segments
- **Batch Operations**: BatchGet and BatchWrite for bulk operations (up to 25 items)
- **Transactions**: ACID transactions with TransactGet and TransactWrite

### Advanced Features
- **Indexes**
  - **LSI** (Local Secondary Index): Alternate sort keys on the same partition
  - **GSI** (Global Secondary Index): Query by non-key attributes
- **TTL** (Time To Live): Automatic expiration of items with lazy deletion
- **Streams**: Change Data Capture (CDC) with configurable view types
- **Update Expressions**: DynamoDB-style SET, REMOVE, ADD operations
- **Conditional Operations**: Put/Update/Delete with condition expressions
- **PartiQL**: Full SQL-like query language
  - SELECT with WHERE, LIMIT, OFFSET, ORDER BY
  - Projection (`SELECT attr1, attr2`)
  - INSERT, UPDATE, DELETE statements
  - Scan filtering on non-key attributes

### Storage Engine
- **LSM Tree**: 256-stripe Log-Structured Merge-tree for write performance
- **WAL**: Write-Ahead Log with group commit for durability
- **SST Files**: Immutable sorted string tables with bloom filters
- **Background Compaction**: Automatic space reclamation and tombstone removal
- **Crash Recovery**: Automatic recovery from WAL on database open
- **Encryption**: Optional block-level encryption support
- **Zstd Compression**: Configurable compression for SST files (2-5x storage savings)
- **Size-Based Flushing**: Predictable memory usage with 4MB memtable default
- **Schema Validation**: Attribute-level type checking and value constraints

### Language Bindings
KeystoneDB supports multiple programming languages:
- **Go**: Embedded (CGO/FFI) and gRPC client
- **Python**: Embedded (PyO3) and gRPC client
- **JavaScript/TypeScript**: gRPC client
- **Rust**: Native embedded API

See [BINDINGS.md](BINDINGS.md) for installation and usage guides

## Installation

### Homebrew (macOS/Linux)

```bash
# Add KeystoneDB tap
brew tap keystone-db/keystonedb

# Install CLI
brew install kstone

# Install server (optional)
brew install kstone-server
```

### Docker

```bash
# Pull CLI image
docker pull parkerdgabel/kstone:latest

# Pull server image
docker pull parkerdgabel/kstone-server:latest

# Run CLI
docker run -v $(pwd)/data:/data parkerdgabel/kstone create /data/mydb.keystone

# Run server
docker run -p 50051:50051 -p 9090:9090 \
  -v $(pwd)/data:/data \
  parkerdgabel/kstone-server --db-path /data/mydb.keystone
```

### Pre-built Binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/keystone-db/keystonedb/releases):

- **Linux** (x86_64, ARM64)
- **macOS** (Intel, Apple Silicon)
- **Windows** (x86_64)

Extract and add to your PATH:

```bash
# Linux/macOS
tar xzf kstone-*.tar.gz
sudo mv kstone /usr/local/bin/
sudo chmod +x /usr/local/bin/kstone

# Verify installation
kstone --version
```

### Build from Source

```bash
# Clone repository
git clone https://github.com/keystone-db/keystonedb.git
cd keystonedb

# Build release binary
cargo build --release

# Binaries will be at:
# - target/release/kstone (CLI)
# - target/release/kstone-server (server)

# Optional: Install to system
sudo cp target/release/kstone /usr/local/bin/
sudo cp target/release/kstone-server /usr/local/bin/
```

## Quick Start

```bash
# Create a new database
kstone create mydb.keystone

# Put an item
kstone put mydb.keystone user#123 '{"name":"Alice","age":30,"email":"alice@example.com"}'

# Get an item
kstone get mydb.keystone user#123

# Delete an item
kstone delete mydb.keystone user#123
```

## PartiQL Queries

KeystoneDB supports SQL-like queries through PartiQL:

```bash
# SELECT with WHERE clause
kstone query mydb.keystone "SELECT * FROM users WHERE pk = 'user#123'"

# SELECT with projection (specific attributes)
kstone query mydb.keystone "SELECT name, email FROM users WHERE pk = 'user#123'"

# SELECT with LIMIT and OFFSET (pagination)
kstone query mydb.keystone "SELECT * FROM users LIMIT 10 OFFSET 20"

# Scan with filtering
kstone query mydb.keystone "SELECT * FROM users WHERE age > 25"

# INSERT
kstone query mydb.keystone "INSERT INTO users VALUE {'pk': 'user#456', 'name': 'Bob', 'age': 35}"

# UPDATE with arithmetic
kstone query mydb.keystone "UPDATE users SET age = age + 1, email = 'bob@example.com' WHERE pk = 'user#456'"

# DELETE
kstone query mydb.keystone "DELETE FROM users WHERE pk = 'user#456'"
```

### Query Output Formats

```bash
# Table format (default, human-readable)
kstone query mydb.keystone "SELECT * FROM users WHERE pk = 'user#123'" -o table

# Pretty JSON
kstone query mydb.keystone "SELECT * FROM users WHERE pk = 'user#123'" -o json

# JSON Lines (one per line, great for pipelines)
kstone query mydb.keystone "SELECT * FROM users" -o jsonl

# CSV format
kstone query mydb.keystone "SELECT name, age, email FROM users" -o csv

# Limit results
kstone query mydb.keystone "SELECT * FROM users" --limit 100
```

## Rust API

```rust
use kstone_api::{Database, ItemBuilder, Query, ScanBuilder};

// Create or open database
let db = Database::create("mydb.keystone")?;
let db = Database::open("mydb.keystone")?;

// Create with custom configuration (Phase 8+)
use kstone_core::DatabaseConfig;
let config = DatabaseConfig {
    memtable_threshold: 5000,        // Records per stripe before flush
    compaction_threshold: 4,         // SSTs before compaction
    max_concurrent_compactions: 4,   // Parallel compaction limit
    ..Default::default()
};
let db = Database::create_with_config("mydb.keystone", config)?;

// Put an item
let item = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .bool("active", true)
    .build();
db.put(b"user#123", item)?;

// Get an item
if let Some(item) = db.get(b"user#123")? {
    println!("Found: {:?}", item);
}

// Query with partition key
let query = Query::new()
    .partition_key(b"user#123")
    .limit(10);
let response = db.query(query)?;

// Scan with filter
let scan = ScanBuilder::new()
    .filter_expression("age > :min_age")
    .expression_value(":min_age", 25)
    .limit(100)
    .build();
let response = db.scan(scan)?;

// Parallel scan
let scan = ScanBuilder::new()
    .parallel(4, 0)  // 4 segments, reading segment 0
    .build();
let response = db.scan(scan)?;

// Execute PartiQL
let sql = "SELECT name, age FROM users WHERE pk = 'user#123' LIMIT 10";
let response = db.execute_statement(sql)?;

// Batch operations
let batch_get = db.batch_get()
    .add_key(b"user#123")
    .add_key(b"user#456")
    .execute()?;

let batch_write = db.batch_write()
    .put(b"user#789", item1)
    .delete(b"user#999")
    .execute()?;

// Transactions
let txn_get = db.transact_get()
    .add_get(b"user#123")
    .add_get(b"user#456")
    .execute()?;

let txn_write = db.transact_write()
    .put(b"user#111", item1)
    .update(b"user#222", "SET age = age + 1", None)
    .delete(b"user#333")
    .execute()?;

// Conditional operations
db.put_if_not_exists(b"user#123", item)?;

db.update(
    b"user#123",
    "SET age = :new_age",
    Some("age < :new_age"),
)?;

// Update expressions
db.update(b"user#123", "SET age = age + 1, visits = visits + 1", None)?;
db.update(b"user#123", "REMOVE temp_field", None)?;

// Database statistics and health (Phase 8+)
let stats = db.stats()?;
println!("Total compactions: {}", stats.compaction.total_compactions);
println!("Bytes reclaimed: {}", stats.compaction.total_bytes_reclaimed);

let health = db.health();
match health.status {
    HealthStatus::Healthy => println!("Database is healthy"),
    HealthStatus::Degraded => println!("Warnings: {:?}", health.warnings),
    HealthStatus::Unhealthy => println!("Errors: {:?}", health.errors),
}
```

## Architecture

KeystoneDB is organized as a Cargo workspace with 4 crates:

- **kstone-core**: Storage engine implementation
  - LSM tree with 256 stripes for parallelism
  - Write-Ahead Log (WAL) for durability
  - Sorted String Tables (SST) with bloom filters
  - Background compaction manager
  - PartiQL parser, validator, and translator

- **kstone-api**: Public API layer
  - Database operations (Put/Get/Delete/Query/Scan)
  - Batch and transaction operations
  - Index management (LSI/GSI)
  - PartiQL execute_statement

- **kstone-cli**: Command-line interface
  - Database creation and management
  - Item operations
  - PartiQL query interface with multiple output formats

- **kstone-tests**: Integration test suite
  - End-to-end tests across all features

## Project Status

**Phase 8 Complete** - Production-ready with observability and configuration

- ✅ **Phase 0**: Walking Skeleton (Put/Get/Delete, WAL, SST, LSM)
- ✅ **Phase 1**: Full storage engine (256 stripes, flush, recovery)
- ✅ **Phase 2**: Complete API (Query, Scan, Batch, Transactions, Conditionals, Updates)
- ✅ **Phase 3**: Indexes and features (LSI, GSI, TTL, Streams)
- ✅ **Phase 4**: PartiQL support (Parser, Translator, ExecuteStatement API)
- ✅ **Phase 5**: Optimization and enhancements (Background compaction, LIMIT/OFFSET, projection, scan filtering, CLI improvements)
- ✅ **Phase 6**: In-memory mode and test utilities
- ✅ **Phase 7**: Advanced error handling and crash recovery tests
- ✅ **Phase 8**: Observability and configuration (stats(), health(), DatabaseConfig, benchmarks)

**All tests passing** across all crates

## Performance

- **Writes**: ~10-50k ops/sec (depends on memtable flush frequency)
- **Reads**: ~100k+ ops/sec from memtable, ~10k from SST
- **Queries**: Efficient with partition key equality, slower for scans
- **Parallel Scan**: 256-way parallelism for full table scans

See [PERFORMANCE.md](PERFORMANCE.md) for optimization guidance and best practices.

## Documentation

### User Guides
- [README.md](README.md) - This file: Getting started and feature overview
- [DEPLOYMENT.md](DEPLOYMENT.md) - Production deployment and configuration guide
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md) - Common issues and solutions
- [PERFORMANCE.md](PERFORMANCE.md) - Performance characteristics and optimization
- [MONITORING.md](MONITORING.md) - Observability, metrics, and health checks

### Developer Guides
- [ARCHITECTURE.md](ARCHITECTURE.md) - Internal design and implementation details
- [CLAUDE.md](CLAUDE.md) - Development guide for Claude Code

## Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p kstone-core
cargo test -p kstone-api

# Run specific test
cargo test -p kstone-core test_lsm_put_get

# Run integration tests
cargo test -p kstone-tests
```

## License

[Add your license here]

## Contributing

Contributions welcome! Please open an issue or pull request.

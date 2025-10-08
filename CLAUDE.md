# CLAUDE.md

This file provides guidance to Claude Code when working with KeystoneDB.

## Project Overview

KeystoneDB is an embedded, DynamoDB-compatible database written in Rust. It supports both local (embedded) and remote (gRPC) access.

**Status:** v0.2.0 - Phases 0-10 complete. Multi-language bindings, cloud sync, and interactive notebook interface available. Target: Full Dynamo-model database with enhanced cloud sync, FTS/vector indexes, and multi-database attachments.

## Quick Commands

```bash
# Build & Test
cargo build --release
cargo test

# CLI
kstone create <path>
kstone put <path> <key> '<json-item>'
kstone get <path> <key>
kstone shell <path>  # Interactive REPL
kstone notebook <path>  # Interactive notebook interface (v0.2.0+)

# gRPC Server
kstone-server --db-path <path> --port 50051

# Language Bindings (v0.2.0+)
# Python
pip install keystonedb
python -c "import keystonedb; db = keystonedb.Database.create('./test.db')"

# Go
go get github.com/keystone-db/keystonedb/bindings/go/embedded
# See bindings/go/embedded/README.md

# JavaScript
npm install @keystonedb/client
# See bindings/javascript/client/README.md
```

## Architecture

### Workspace Structure
- **kstone-core**: Storage engine (WAL, SST, LSM, 256 stripes)
- **kstone-api**: Public database API
- **kstone-proto**: gRPC protocol definitions
- **kstone-server**: gRPC server
- **kstone-client**: gRPC client library
- **kstone-cli**: Command-line interface (includes notebook)
- **kstone-sync**: Cloud sync engine (S3, Merkle trees, conflict resolution) 🆕 v0.2.0
- **kstone-tests**: Integration tests
- **c-ffi**: C FFI layer for language bindings 🆕 v0.2.0
- **bindings/**: Multi-language bindings (Go, Python, JavaScript) 🆕 v0.2.0

### Core Components

**Storage Engine (256-Stripe LSM):**
- Keys route to stripes via `crc32(pk) % 256`
- Each stripe: independent memtable + SST list
- Write path: WAL → memtable → flush to SST at 4MB threshold
- Read path: memtable → SST scan (newest first)
- Automatic compaction at 10 SSTs/stripe (K-way merge)

**File Structure:**
```
mydb.keystone/
├── wal.log                    # Write-ahead log
└── {stripe:03}-{sst_id}.sst  # Striped SSTs (e.g., 042-5.sst)
```

**Value Types:**
`S` (String), `N` (Number), `B` (Binary), `Bool`, `Null`, `L` (List), `M` (Map), `VecF32` (embeddings), `Ts` (Timestamp)

**Encoding:**
- All multi-byte integers use **little-endian** (except magic numbers)
- Key format: `[pk_len(4) | pk_bytes | sk_len(4) | sk_bytes]`
- SST files: sorted records with bloom filters, prefix compression

### Concurrency
- LSM engine: `RwLock` (multiple readers OR single writer)
- WAL: `Mutex` (serialized writes for group commit)

## Common Patterns

### Basic Operations
```rust
use kstone_api::{Database, ItemBuilder, Query, Scan};

// Create database
let db = Database::create(path)?;
let db = Database::create_in_memory()?;  // In-memory mode

// Put/Get/Delete
let item = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .build();
db.put(b"user#123", item)?;
let result = db.get(b"user#123")?;
db.delete(b"user#123")?;

// Query (single partition)
let query = Query::new(b"user#123")
    .sk_begins_with(b"post#")
    .limit(10);
let response = db.query(query)?;

// Scan (full table)
let scan = Scan::new().limit(100);
let response = db.scan(scan)?;
```

### Updates & Transactions
```rust
use kstone_api::{Update, TransactWriteRequest};
use kstone_core::Value;

// Update expression
let update = Update::new(b"user#123")
    .expression("SET age = age + :inc REMOVE temp")
    .value(":inc", Value::number(1));
db.update(update)?;

// Atomic transaction
let request = TransactWriteRequest::new()
    .update_with_condition(b"account#src", "SET balance = balance - :amt", "balance >= :amt")
    .update(b"account#dst", "SET balance = balance + :amt")
    .value(":amt", Value::number(100));
db.transact_write(request)?;
```

### Indexes & Advanced Features
```rust
use kstone_api::{TableSchema, LocalSecondaryIndex, GlobalSecondaryIndex, StreamConfig};

// Schema with indexes, TTL, streams
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"))
    .add_global_index(GlobalSecondaryIndex::new("status-index", "status"))
    .with_ttl("expiresAt")
    .with_stream(StreamConfig::enabled());

let db = Database::create_with_schema(path, schema)?;

// Query by index
let query = Query::new(b"active")
    .index("status-index")
    .limit(20);
let response = db.query(query)?;

// Read stream (change data capture)
let records = db.read_stream(None)?;
```

### gRPC Client
```rust
use kstone_client::{Client, RemoteQuery};

let mut client = Client::connect("http://localhost:50051").await?;
client.put(b"user#123", item).await?;
let result = client.get(b"user#123").await?;
```

## Key Implementation Details

### Testing
- Use `.to_vec()` for byte literals: `Key::new(b"key".to_vec())`
- For `Value::N`: `match value { Value::N(n) => assert_eq!(n, "42"), ... }`

### Memtable Flushing
- **Primary trigger:** 4MB size per stripe (`max_memtable_size_bytes`)
- **Safety ceiling:** 10,000 records (`max_memtable_records`)
- Configure via `DatabaseConfig::with_max_memtable_size_bytes()`

### Recovery
`Database::open()` auto-replays WAL, scans SSTs, determines next SeqNo.

### Expression Syntax
```rust
// Conditions
"age >= :min AND active = :val"
"attribute_exists(email)"
"begins_with(name, :prefix)"

// Updates
"SET age = age + :inc, status = :new REMOVE temp ADD count :val"
```

## Development Status

**✅ COMPLETE:**
- **Phase 0:** Walking skeleton (basic LSM, WAL, SST)
- **Phase 1:** Core storage (256 stripes, compaction, bloom filters, encryption support)
- **Phase 2:** Complete DynamoDB API (Query, Scan, Update, Batch, Transactions)
- **Phase 3:** Indexes (LSI, GSI, TTL, Streams/CDC)
- **Phase 4:** PartiQL query language (SQL-compatible queries)
- **Phase 5:** In-memory database mode
- **Phase 6:** gRPC server & client
- **Phase 7:** Interactive CLI (REPL with autocomplete, history, formatting)
- **Phase 8:** Language bindings (Go, Python, JavaScript embedded & gRPC clients)
- **Phase 9:** Cloud sync (kstone-sync with S3-compatible storage, Merkle trees, conflict resolution)
- **Phase 10:** Interactive notebook interface (web-based Jupyter-like database exploration)

**🚧 FUTURE:**
- **Phase 11:** Attachment framework enhancements (multi-database attachments, enhanced DynamoDB sync)

## Module Reference

**kstone-core modules:**
- `types.rs` - Core types (Value, Key, Record, Item)
- `error.rs` - Error handling
- `lsm.rs` - 256-stripe LSM engine
- `wal.rs`, `wal_ring.rs` - Write-ahead log
- `sst.rs`, `sst_block.rs` - Sorted string tables
- `bloom.rs` - Bloom filters
- `manifest.rs` - Metadata catalog
- `compaction.rs` - Compaction logic
- `iterator.rs` - Query/scan iterators
- `expression.rs` - Condition/update expressions
- `index.rs` - LSI/GSI, TTL, schema
- `stream.rs` - Change data capture
- `memory_lsm.rs`, `memory_wal.rs`, `memory_sst.rs` - In-memory storage

**kstone-api modules:**
- `query.rs`, `scan.rs` - Query/scan builders
- `update.rs` - Update operations
- `batch.rs` - Batch operations
- `transaction.rs` - Transactions

**Interactive Shell:**
- `kstone shell <path>` - REPL with autocomplete
- Meta-commands: `.help`, `.schema`, `.indexes`, `.format <type>`, `.timer <on|off>`, `.exit`
- Tab completion for PartiQL keywords and meta-commands
- Persistent history in `~/.keystone_history`

**kstone-sync modules** (v0.2.0+):
- `sync_engine.rs` - Bidirectional sync engine
- `merkle.rs` - Merkle tree for change tracking
- `conflict.rs` - Conflict resolution with vector clocks
- `offline_queue.rs` - Offline operation queue
- `change_tracker.rs` - Track database changes
- `metadata.rs` - Sync metadata management
- `protocol.rs` - Sync protocol implementation
- `vector_clock.rs` - Vector clock for causality

**Notebook modules** (v0.2.0+):
- `kstone-cli/src/notebook/server.rs` - Axum HTTP/WebSocket server
- `kstone-cli/src/notebook/handlers.rs` - REST API endpoints
- `kstone-cli/src/notebook/websocket.rs` - Real-time communication
- `kstone-cli/src/notebook/storage.rs` - Notebook persistence
- `kstone-cli/src/notebook/assets.rs` - Static file serving
- `kstone-cli/src/notebook/static/` - Frontend assets

**C FFI & Bindings** (v0.2.0+):
- `c-ffi/` - C FFI layer for embedded bindings
- `bindings/go/` - Go embedded (cgo) and gRPC client
- `bindings/python/` - Python embedded (PyO3) and gRPC client
- `bindings/javascript/` - JavaScript/TypeScript gRPC client

# Changelog

All notable changes to KeystoneDB will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2025-10-07

### Added

#### Language Bindings 🎉
- **Go Bindings** - Full embedded (cgo) and gRPC client support
  - Embedded bindings via C FFI layer with 5/5 tests passing
  - gRPC client with full feature parity
  - Examples: `examples/go-embedded/`
- **Python Bindings** - PyO3 embedded and gRPC client support
  - Embedded bindings with 7/7 tests passing
  - Installable via PyPI: `pip install keystonedb`
  - gRPC client with context manager support
  - Examples: `examples/python-embedded/`
- **JavaScript/TypeScript Bindings** - gRPC client support
  - Promise-based async API with full type safety
  - Published to npm: `@keystonedb/client`
  - Embedded bindings via napi-rs (in progress)
- **C FFI Layer** - Foundation for all embedded bindings
  - Auto-generated C headers via cbindgen
  - Multi-platform support (Linux, macOS, Windows)

#### Cloud Sync (kstone-sync crate) 🌐
- **S3-Compatible Storage Sync** - Bidirectional sync with cloud storage
  - Merkle tree-based change tracking
  - Vector clock conflict resolution
  - Offline queue for network interruptions
  - Metadata tracking and protocol support
  - `Database::scan_with_keys()` for efficient syncing
  - Example: `examples/s3-backup/`

#### PartiQL Support 🔍
- **SQL Query Language** - Execute SQL-compatible queries
  - `SELECT * FROM users WHERE pk = 'user#123'`
  - `INSERT INTO users VALUE {'pk': 'user#1', 'name': 'Alice'}`
  - `UPDATE users SET age = age + 1 WHERE pk = 'user#1'`
  - `DELETE FROM users WHERE pk = 'user#1'`
  - Full integration with kstone-api
  - 29KB implementation in `kstone-api/src/partiql.rs`

#### Interactive Notebook Interface 📓
- **Web-Based REPL** - Jupyter-like interface for KeystoneDB
  - WebSocket support for real-time queries
  - Interactive query execution
  - Static assets included
  - Launch with `kstone notebook <path>` (coming soon)
  - Located in `kstone-cli/src/notebook/`

#### Storage Enhancements
- **Zstd Compression** - Efficient data compression support
  - Configurable compression levels
  - Transparent compression/decompression
- **Schema Validation** - Enforce data schemas
  - Attribute type checking
  - Value constraints
  - Validator module in `kstone-core/src/validation.rs`

#### Configuration & Reliability
- **Database Configuration** - Fine-grained control
  - Custom memtable sizes
  - Compaction settings
  - Retry policies with exponential backoff
  - `DatabaseConfig` in kstone-core
- **Retry Logic** - Robust error handling
  - Exponential backoff with jitter
  - Configurable retry policies
  - `retry_with_policy()` and `retry()` helpers

### Enhanced

- **S3 Sync** - Improved S3 sync capabilities
  - Exposed `Database::path()` for sync operations
  - Added `scan_with_keys()` for efficient data extraction
  - Fixed bidirectional sync data transfer issues
- **Documentation** - Comprehensive updates
  - Added `RELEASING.md` with full release process
  - Updated `bindings/README.md` with multi-language support
  - Added `bindings/BUILD_STATUS.md` with build status tracking
  - Version bump script: `scripts/bump-bindings-version.sh`
- **Build System** - Improved CI/CD
  - `.github/workflows/bindings-release.yml` for bindings
  - `.github/workflows/bindings-test.yml` for testing
  - Protobuf compiler installation in CI
  - Cross-platform build support

### Fixed

- **S3 Sync Limitations** - Addressed synchronization edge cases
- **Bidirectional Sync** - Fixed data transfer in cloud sync
- **Build Warnings** - Cleaned up unused code warnings
- **.gitignore** - Added generated files to gitignore

### Development

- **Examples Added**
  - `examples/go-embedded/` - Go embedded database usage
  - `examples/python-embedded/` - Python embedded usage
  - `examples/grpc-client/` - Multi-language gRPC clients
  - `examples/s3-backup/` - Cloud backup with S3

### Notes

- **Backwards Compatible** - No breaking changes from v0.1.0
- **JavaScript Embedded** - napi-rs implementation has known Buffer API issues, to be addressed in v0.2.1
- **Phase Completion** - Phases 0-7 complete, Phase 8 (Cloud Sync) and Notebook interface added

---

## [0.1.0] - 2024-XX-XX

### Added

- **Core LSM Storage Engine** - 256-stripe LSM tree with WAL
  - Automatic compaction at 10 SSTs/stripe threshold
  - Bloom filters for efficient lookups
  - Prefix compression in SST files
- **DynamoDB-Compatible API** - Full feature parity
  - Put/Get/Delete operations with sort keys
  - Query & Scan with pagination
  - Batch operations (BatchGet, BatchWrite)
  - Transactions (TransactGet, TransactWrite)
  - Update expressions (SET, REMOVE, ADD)
  - Conditional operations
- **Indexes** - LSI and GSI support
  - Local Secondary Indexes
  - Global Secondary Indexes
  - Index projection support
- **Advanced Features**
  - TTL (Time To Live) with lazy deletion
  - Streams/CDC (Change Data Capture)
  - In-memory database mode
  - Expression system (attribute_exists, begins_with, comparisons)
- **gRPC Server & Client**
  - Full-featured gRPC server (`kstone-server`)
  - Rust gRPC client library (`kstone-client`)
  - Protocol definitions in `kstone-proto`
- **CLI Tools**
  - Interactive shell (REPL) with autocomplete
  - Tab completion for commands
  - Persistent history (`~/.keystone_history`)
  - Multiple output formats (table, JSON, CSV)
  - Meta-commands: `.help`, `.schema`, `.indexes`, `.format`, `.timer`
- **Testing & Examples**
  - Comprehensive test suite in `kstone-tests`
  - Example applications: url-shortener, cache-server, todo-api, blog-engine

### Initial Release

- Complete implementation of Phases 0-7
- Production-ready embedded database
- Full DynamoDB API compatibility
- Multi-process access via gRPC

---

[0.2.0]: https://github.com/keystone-db/keystonedb/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/keystone-db/keystonedb/releases/tag/v0.1.0

# Changelog

All notable changes to KeystoneDB will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Nothing yet

### Changed
- Nothing yet

### Fixed
- Nothing yet

## [0.1.0] - 2024-12-27

### Added

#### Core Database Engine
- Single-file embedded DynamoDB-compatible database
- LSM tree storage engine with 256-stripe architecture
- Write-ahead log (WAL) with crash recovery
- Block-based SST files with Zstd compression
- Bloom filters for fast negative lookups
- AES-256-GCM encryption support for blocks
- Automatic background compaction

#### DynamoDB-Compatible API
- Full CRUD operations (Put, Get, Delete)
- Query with sort key conditions (eq, lt, lte, gt, gte, between, begins_with)
- Scan with parallel segment support
- Update expressions (SET, REMOVE, ADD)
- Conditional operations with expression evaluation
- Batch operations (BatchGet, BatchWrite)
- Transactions (TransactGet, TransactWrite) with ACID guarantees

#### Secondary Indexes
- Local Secondary Indexes (LSI) for alternate sort keys
- Global Secondary Indexes (GSI) for cross-partition queries
- All projection types (ALL, KEYS_ONLY, INCLUDE)

#### Advanced Features
- Time To Live (TTL) with lazy deletion
- Change Data Capture (CDC) streams
- PartiQL SQL-compatible query language
- Schema validation with attribute constraints

#### Network Layer
- gRPC server for remote database access
- gRPC client library for Rust applications
- Rate limiting and connection management
- Server metrics endpoint

#### Cloud Sync
- Bidirectional sync engine with vector clocks
- S3 backend for cloud storage
- Filesystem backend for local sync
- Merkle tree-based diff detection
- Conflict resolution strategies (last-writer-wins, first-writer-wins, custom)

#### CLI & Tools
- `kstone` CLI for local database operations
- Interactive shell with PartiQL support
- Tab completion and command history
- Multiple output formats (table, JSON, compact)

#### Language Bindings
- Python bindings via PyO3/Maturin
- Node.js bindings via NAPI-RS
- C FFI for cross-language support

#### Examples & Documentation
- URL shortener example application
- Cache server example
- Todo API example
- Blog engine example
- Comprehensive CLAUDE.md development guide

### Security
- Block-level AES-256-GCM encryption
- CRC32C checksums for data integrity
- No known security vulnerabilities

---

## Version History

| Version | Date | Highlights |
|---------|------|------------|
| 0.1.0 | 2024-12-27 | Initial release with full DynamoDB-compatible API |

## Upgrade Guide

### Upgrading to 0.1.0

This is the initial release. No upgrade steps required.

## Links

- [GitHub Repository](https://github.com/keystone-db/keystonedb)
- [Documentation](https://github.com/keystone-db/keystonedb#readme)
- [Issue Tracker](https://github.com/keystone-db/keystonedb/issues)

[Unreleased]: https://github.com/keystone-db/keystonedb/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/keystone-db/keystonedb/releases/tag/v0.1.0

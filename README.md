# KeystoneDB

Single-file, embedded, Dynamo-model database with local-first speed and cloud sync.

## Walking Skeleton (Phase 0)

Current implementation: minimal end-to-end system with Put/Get operations.

```bash
# Build
cargo build --release

# Create database and test operations
cargo run --bin kstone -- create test.keystone
cargo run --bin kstone -- put test.keystone mykey '{"name":"Alice","age":30}'
cargo run --bin kstone -- get test.keystone mykey

# Run tests
cargo test
```

## Architecture

- `kstone-core` - Storage engine (WAL, SST, LSM)
- `kstone-api` - Public API and data model
- `kstone-cli` - Command-line tool
- `kstone-tests` - Integration tests

## Status

Phase 0 (Walking Skeleton) - In Progress

# KeystoneDB Language Bindings

KeystoneDB provides language bindings for **Go**, **Python**, and **JavaScript/TypeScript**, enabling you to use the database from your favorite programming language. Each language offers two modes of access:

- **Embedded Bindings**: Direct in-process access via FFI/native bindings (fastest)
- **gRPC Client**: Remote access to a KeystoneDB server over the network

## Table of Contents

- [Installation](#installation)
- [Quick Start (Build from Source)](#quick-start-build-from-source)
- [Embedded Bindings](#embedded-bindings)
  - [Go Embedded](#go-embedded)
  - [Python Embedded](#python-embedded)
  - [JavaScript Embedded](#javascript-embedded)
- [gRPC Clients](#grpc-clients)
  - [Go gRPC Client](#go-grpc-client)
  - [Python gRPC Client](#python-grpc-client)
  - [JavaScript gRPC Client](#javascript-grpc-client)
- [Examples](#examples)
- [Feature Comparison](#feature-comparison)
- [Performance Considerations](#performance-considerations)
- [Building from Source](#building-from-source)

## Installation

The easiest way to use KeystoneDB bindings is to install from package registries:

### Python (PyPI)

```bash
# Install from PyPI
pip install keystonedb

# Or with specific version
pip install keystonedb==0.1.0
```

Then use in your code:

```python
import keystonedb

db = keystonedb.Database.create("my.keystone")
db.put(b"user#123", {"name": "Alice", "age": 30})
item = db.get(b"user#123")
```

**Requirements**: Python 3.9+

**Platforms**: Linux (x64, ARM64), macOS (x64, ARM64), Windows (x64)

### JavaScript/TypeScript (npm)

```bash
# Install from npm
npm install @keystonedb/client

# Or with specific version
npm install @keystonedb/client@0.1.0
```

Then use in your code:

```javascript
const { KeystoneClient } = require('@keystonedb/client');

const client = new KeystoneClient('localhost:50051');

await client.put({
    partitionKey: Buffer.from('user#123'),
    item: {
        attributes: {
            name: { S: 'Alice' },
            age: { N: '30' },
        },
    },
});
```

**Requirements**: Node.js 18+

**Note**: This is the **gRPC client** for remote server access. Embedded bindings for JavaScript are currently not available due to napi-rs compatibility issues.

### Go (Go Modules)

```bash
# Embedded bindings (requires C FFI library)
go get github.com/keystone-db/keystonedb/bindings/go/embedded@bindings-v0.1.0

# gRPC client (no additional dependencies)
go get github.com/keystone-db/keystonedb/bindings/go/client@bindings-v0.1.0
```

Then use in your code:

**Embedded**:
```go
import kstone "github.com/keystone-db/keystonedb/bindings/go/embedded"

db, _ := kstone.Create("my.keystone")
db.Put("user#123", "name", "Alice")
```

**gRPC Client**:
```go
import (
    "google.golang.org/grpc"
    pb "github.com/keystone-db/keystonedb/bindings/go/client/pb"
)

conn, _ := grpc.NewClient("localhost:50051")
client := pb.NewKeystoneDbClient(conn)
```

**Requirements**: Go 1.21+

**Note**: Embedded bindings require building the C FFI library separately (see [Building from Source](#building-from-source)).

### C FFI Library

For Go embedded bindings or custom FFI integrations, download pre-built libraries from [GitHub Releases](https://github.com/keystone-db/keystonedb/releases):

```bash
# Example: macOS Apple Silicon
wget https://github.com/keystone-db/keystonedb/releases/download/bindings-v0.1.0/kstone-ffi-aarch64-apple-darwin.tar.gz
tar xzf kstone-ffi-aarch64-apple-darwin.tar.gz
```

Available platforms:
- `kstone-ffi-x86_64-unknown-linux-gnu.tar.gz` - Linux x64
- `kstone-ffi-x86_64-apple-darwin.tar.gz` - macOS Intel
- `kstone-ffi-aarch64-apple-darwin.tar.gz` - macOS Apple Silicon
- `kstone-ffi-x86_64-pc-windows-msvc.zip` - Windows x64

## Quick Start (Build from Source)

### Go Embedded

```bash
# Build C FFI library
cargo build --release -p kstone-ffi

# Set environment variables
export CGO_ENABLED=1
export CGO_LDFLAGS="-L$(pwd)/target/release -lkstone_ffi"
export CGO_CFLAGS="-I$(pwd)/c-ffi/include"
export DYLD_LIBRARY_PATH="$(pwd)/target/release"  # macOS
# export LD_LIBRARY_PATH="$(pwd)/target/release"  # Linux

# Use in Go code
go get github.com/keystone-db/keystonedb/bindings/go/embedded
```

```go
import kstone "github.com/keystone-db/keystonedb/bindings/go/embedded"

db, _ := kstone.Create("my.keystone")
defer db.Close()

db.Put("user#123", "name", "Alice")
item, _ := db.Get("user#123")
```

### Python Embedded

```bash
# Build wheel
maturin build --manifest-path bindings/python/embedded/Cargo.toml --release

# Install
pip install bindings/python/embedded/target/wheels/keystonedb-*.whl
```

```python
import keystonedb

db = keystonedb.Database.create("my.keystone")
db.put(b"user#123", {"name": "Alice", "age": 30})
item = db.get(b"user#123")
db.flush()
```

### gRPC (All Languages)

```bash
# Start server
cargo build --release --bin kstone-server
./target/release/kstone-server --db-path my.keystone --port 50051
```

Then use the language-specific gRPC client (see sections below).

---

## Embedded Bindings

Embedded bindings provide **direct in-process access** to KeystoneDB. This is the fastest option but requires building native libraries.

### Go Embedded

**Technology**: cgo → C FFI

**Installation**:

1. Build the C FFI library:
   ```bash
   cargo build --release -p kstone-ffi
   ```

2. Set environment variables:
   ```bash
   export CGO_ENABLED=1
   export CGO_LDFLAGS="-L$(pwd)/target/release -lkstone_ffi"
   export CGO_CFLAGS="-I$(pwd)/c-ffi/include"
   export DYLD_LIBRARY_PATH="$(pwd)/target/release"  # macOS
   ```

3. Use in your project:
   ```bash
   go get github.com/keystone-db/keystonedb/bindings/go/embedded
   ```

**Basic Usage**:

```go
package main

import (
    "log"
    kstone "github.com/keystone-db/keystonedb/bindings/go/embedded"
)

func main() {
    // Create database
    db, err := kstone.Create("app.keystone")
    if err != nil {
        log.Fatal(err)
    }
    defer db.Close()

    // Put item
    err = db.Put("user#alice", "email", "alice@example.com")
    if err != nil {
        log.Fatal(err)
    }

    // Get item
    item, err := db.Get("user#alice")
    if err == kstone.ErrNotFound {
        log.Println("Item not found")
    } else if err != nil {
        log.Fatal(err)
    }

    // Delete item
    err = db.Delete("user#alice")
}
```

**With Sort Keys**:

```go
// Put with partition key + sort key
db.PutWithSK("org#acme", "user#alice", "role", "admin")

// Get with sort key
item, err := db.GetWithSK("org#acme", "user#alice")

// Delete with sort key
db.DeleteWithSK("org#acme", "user#alice")
```

**API Reference**:

- `Create(path string) (*Database, error)` - Create new database
- `Open(path string) (*Database, error)` - Open existing database
- `CreateInMemory() (*Database, error)` - Create in-memory database
- `Put(pk, attrName, value string) error` - Store attribute
- `PutWithSK(pk, sk, attrName, value string) error` - Store with sort key
- `Get(pk string) (*Item, error)` - Retrieve item
- `GetWithSK(pk, sk string) (*Item, error)` - Retrieve with sort key
- `Delete(pk string) error` - Delete item
- `DeleteWithSK(pk, sk string) error` - Delete with sort key
- `Close() error` - Close database

**Example**: See `examples/go-embedded/`

**Test**: See `bindings/go/embedded/smoke_test.go`

---

### Python Embedded

**Technology**: PyO3 (direct Rust → Python, no C layer)

**Installation**:

1. Build the wheel:
   ```bash
   maturin build --manifest-path bindings/python/embedded/Cargo.toml --release
   ```

2. Install:
   ```bash
   pip install bindings/python/embedded/target/wheels/keystonedb-*.whl
   ```

**Basic Usage**:

```python
import keystonedb

# Create database
db = keystonedb.Database.create("app.keystone")

# Put item (keys are bytes, values are dicts)
db.put(b"user#alice", {
    "name": "Alice",
    "email": "alice@example.com",
    "age": 30,
    "active": True
})

# Get item (returns dict or None)
item = db.get(b"user#alice")
if item:
    print(item["name"])  # "Alice"
    print(item["age"])   # 30

# Delete item
db.delete(b"user#alice")

# Flush to disk
db.flush()
```

**With Sort Keys**:

```python
# Put with partition key + sort key
db.put_with_sk(b"org#acme", b"user#alice", {
    "role": "admin",
    "department": "Engineering"
})

# Get with sort key
item = db.get_with_sk(b"org#acme", b"user#alice")

# Delete with sort key
db.delete_with_sk(b"org#acme", b"user#alice")
```

**Supported Value Types**:

```python
item = {
    "string": "hello",
    "number": 42,
    "float": 3.14,
    "boolean": True,
    "null": None,
    "list": [1, 2, "three"],
    "nested": {
        "inner": "value",
        "count": 10
    }
}
db.put(b"key", item)
```

**API Reference**:

- `Database.create(path: str) -> Database` - Create new database
- `Database.open(path: str) -> Database` - Open existing database
- `Database.create_in_memory() -> Database` - Create in-memory database
- `put(pk: bytes, item: dict)` - Store item
- `put_with_sk(pk: bytes, sk: bytes, item: dict)` - Store with sort key
- `get(pk: bytes) -> dict | None` - Retrieve item
- `get_with_sk(pk: bytes, sk: bytes) -> dict | None` - Retrieve with sort key
- `delete(pk: bytes)` - Delete item
- `delete_with_sk(pk: bytes, sk: bytes)` - Delete with sort key
- `flush()` - Flush to disk

**Example**: See `examples/python-embedded/`

**Test**: See `bindings/python/embedded/test_smoke.py`

---

### JavaScript Embedded

**Technology**: napi-rs

**Status**: ⚠️ **Currently has build issues** with napi-rs 2.16 API compatibility. Use the gRPC client instead for JavaScript/TypeScript projects.

**Known Issues**:
- Buffer API changes in napi-rs
- Type naming mismatches
- Array conversion issues

**Workaround**: Use [JavaScript gRPC Client](#javascript-grpc-client) instead.

---

## gRPC Clients

gRPC clients provide **remote access** to a KeystoneDB server. This enables:
- Multiple clients accessing the same database
- Network-based architecture (microservices, etc.)
- Language interoperability
- Server-side scalability

### Starting the Server

All gRPC clients require a running KeystoneDB server:

```bash
# Build server
cargo build --release --bin kstone-server

# Start server
./target/release/kstone-server \
  --db-path /path/to/db.keystone \
  --port 50051 \
  --host 127.0.0.1
```

### Go gRPC Client

**Technology**: protoc-gen-go + gRPC

**Installation**:

```bash
go get github.com/keystone-db/keystonedb/bindings/go/client
```

**Basic Usage**:

```go
import (
    "context"
    "google.golang.org/grpc"
    "google.golang.org/grpc/credentials/insecure"
    pb "github.com/keystone-db/keystonedb/bindings/go/client/pb"
)

func main() {
    // Connect to server
    conn, _ := grpc.NewClient("localhost:50051",
        grpc.WithTransportCredentials(insecure.NewCredentials()))
    defer conn.Close()

    client := pb.NewKeystoneDbClient(conn)
    ctx := context.Background()

    // Put item
    item := &pb.Item{
        Attributes: map[string]*pb.Value{
            "name":  {Kind: &pb.Value_S{S: "Alice"}},
            "age":   {Kind: &pb.Value_N{N: "30"}},
        },
    }
    req := &pb.PutRequest{
        PartitionKey: []byte("user#alice"),
        Item:         item,
    }
    client.Put(ctx, req)

    // Get item
    getReq := &pb.GetRequest{
        PartitionKey: []byte("user#alice"),
    }
    resp, _ := client.Get(ctx, getReq)

    // Query items
    queryReq := &pb.QueryRequest{
        PartitionKey: []byte("org#acme"),
        Limit:        10,
    }
    queryResp, _ := client.Query(ctx, queryReq)
    for _, item := range queryResp.Items {
        // Process items
    }
}
```

**Example**: See `examples/grpc-client/go/`

---

### Python gRPC Client

**Technology**: grpcio + protobuf

**Installation**:

```bash
pip install grpcio grpcio-tools protobuf
```

Then generate protobuf files (from repo root):

```bash
python -m grpc_tools.protoc \
  --python_out=bindings/python/client/keystonedb \
  --grpc_python_out=bindings/python/client/keystonedb \
  --proto_path=kstone-proto/proto \
  kstone-proto/proto/keystone.proto
```

**Basic Usage**:

```python
from keystonedb import Client
from keystonedb.builders import PutRequestBuilder

async def main():
    async with Client("localhost:50051") as client:
        # Put item
        attributes = {
            "name": {"S": "Alice"},
            "age": {"N": "30"},
        }
        request = (
            PutRequestBuilder()
            .partition_key(b"user#alice")
            .attributes(attributes)
            .build()
        )
        await client.put(request)

        # Get item
        from keystonedb.builders import GetRequestBuilder
        get_req = GetRequestBuilder().partition_key(b"user#alice").build()
        response = await client.get(get_req)

        # Query items
        from keystonedb.builders import QueryRequestBuilder
        query_req = (
            QueryRequestBuilder()
            .partition_key(b"org#acme")
            .limit(10)
            .build()
        )
        query_resp = await client.query(query_req)
        for item in query_resp.items:
            # Process items
            pass

import asyncio
asyncio.run(main())
```

**Example**: See `examples/grpc-client/python/`

---

### JavaScript gRPC Client

**Technology**: @grpc/grpc-js + TypeScript

**Installation**:

```bash
npm install @grpc/grpc-js @grpc/proto-loader
```

Or use the pre-built client:

```bash
cd bindings/javascript/client
npm install
npm run build
```

**Basic Usage**:

```javascript
const { KeystoneClient } = require('./bindings/javascript/client/dist');

const client = new KeystoneClient('localhost:50051');

// Put item
await client.put({
    partitionKey: Buffer.from('user#alice'),
    item: {
        attributes: {
            name: { S: 'Alice' },
            age: { N: '30' },
        },
    },
});

// Get item
const response = await client.get({
    partitionKey: Buffer.from('user#alice'),
});

// Query items
const queryResp = await client.query({
    partitionKey: Buffer.from('org#acme'),
    limit: 10,
});

queryResp.items.forEach(item => {
    // Process items
});

// Clean up
client.close();
```

**TypeScript Support**:

```typescript
import { KeystoneClient } from './bindings/javascript/client/dist';

const client = new KeystoneClient('localhost:50051');
// Full type safety with .d.ts definitions
```

**Example**: See `examples/grpc-client/javascript/`

---

## Examples

### Embedded Examples

| Language | Example | Description |
|----------|---------|-------------|
| Go | `examples/go-embedded/` | CRUD operations, sort keys, error handling |
| Python | `examples/python-embedded/` | Contact manager CLI, value types, in-memory mode |

### gRPC Examples

| Language | Example | Description |
|----------|---------|-------------|
| Go | `examples/grpc-client/go/` | Task management with remote server |
| Python | `examples/grpc-client/python/` | Async task management |
| JavaScript | `examples/grpc-client/javascript/` | Promise-based task management |

See `examples/grpc-client/README.md` for a multi-language interoperability demo.

---

## Feature Comparison

### Embedded vs gRPC

| Feature | Embedded | gRPC |
|---------|----------|------|
| Performance | Fastest (in-process) | Network latency |
| Deployment | Requires native library | Client/server architecture |
| Concurrency | Process-local | Multi-client |
| Operations | Put, Get, Delete, Flush | Full API (Query, Scan, Batch, etc.) |
| Best For | Single-process apps, embedded systems | Distributed systems, microservices |

### Language Feature Matrix

| Feature | Go Embedded | Python Embedded | Go gRPC | Python gRPC | JS gRPC |
|---------|-------------|-----------------|---------|-------------|---------|
| Put/Get/Delete | ✅ | ✅ | ✅ | ✅ | ✅ |
| Sort Keys | ✅ | ✅ | ✅ | ✅ | ✅ |
| In-Memory Mode | ✅ | ✅ | N/A | N/A | N/A |
| Query | ❌ | ❌ | ✅ | ✅ | ✅ |
| Scan | ❌ | ❌ | ✅ | ✅ | ✅ |
| Batch Operations | ❌ | ❌ | ✅ | ✅ | ✅ |
| Transactions | ❌ | ❌ | 🚧 | 🚧 | 🚧 |
| Update Expressions | ❌ | ❌ | 🚧 | 🚧 | 🚧 |
| PartiQL | ❌ | ❌ | 🚧 | 🚧 | 🚧 |

Legend: ✅ Supported | ❌ Not Available | 🚧 Server not implemented yet

---

## Performance Considerations

### Embedded Bindings

**Pros**:
- Zero network latency
- Direct memory access
- Minimal serialization overhead
- Best for single-process applications

**Cons**:
- Requires native library build
- Process-local only (no multi-client)
- Platform-specific binaries

**Use Cases**:
- CLI tools
- Desktop applications
- Embedded systems
- Serverless functions (when supported)
- High-performance local caching

### gRPC Clients

**Pros**:
- Language interoperability
- Multi-client support
- Horizontal scalability
- Remote access

**Cons**:
- Network latency
- Serialization overhead (protobuf)
- Requires running server

**Use Cases**:
- Microservices
- Web applications
- Mobile apps
- Distributed systems
- Multi-language projects

---

## Recent Performance Optimizations

KeystoneDB v0.1.1+ includes significant performance and efficiency improvements:

### 1. Size-Based Memtable Flushing
- **Default**: 4MB per stripe (vs. 1000 record count)
- **Benefits**: Predictable memory usage (~1GB for all 256 stripes)
- **Configuration**: Adjustable via `DatabaseConfig`

**Impact**: Better handling of variable record sizes, more predictable memory footprint.

### 2. Zstd Compression
- **Default**: Disabled (opt-in)
- **Benefits**: 2-5x storage reduction for compressible data
- **Levels**: 1-22 (default: 3 when enabled)

**Impact**: Significant storage savings with minimal CPU overhead on modern systems.

### 3. Schema Validation
- **Feature**: Attribute-level type checking and value constraints
- **Overhead**: +5-10μs per validated attribute (<1% typical)
- **Constraints**: MinValue, MaxValue, MinLength, MaxLength, Pattern, Enum

**Impact**: Data integrity guarantees with negligible performance cost.

**See [PERFORMANCE.md](PERFORMANCE.md) for detailed benchmarks and tuning guides.**

---

## Building from Source

### C FFI Layer (for embedded bindings)

```bash
cargo build --release -p kstone-ffi
```

Output:
- Library: `target/release/libkstone_ffi.{a,dylib,so,dll}`
- Header: `c-ffi/include/keystone.h` (auto-generated via cbindgen)

### Go Embedded

```bash
export CGO_LDFLAGS="-L$(pwd)/target/release -lkstone_ffi"
export CGO_CFLAGS="-I$(pwd)/c-ffi/include"
cd bindings/go/embedded
go build
go test -v
```

### Python Embedded

```bash
maturin build --manifest-path bindings/python/embedded/Cargo.toml --release
pip install bindings/python/embedded/target/wheels/keystonedb-*.whl
pytest bindings/python/embedded/test_smoke.py -v
```

### Go gRPC Client

```bash
# Generate protobuf (requires protoc and plugins)
protoc --go_out=bindings/go/client/pb \
  --go-grpc_out=bindings/go/client/pb \
  --proto_path=kstone-proto/proto \
  kstone-proto/proto/keystone.proto

cd bindings/go/client
go build
```

### Python gRPC Client

```bash
python -m grpc_tools.protoc \
  --python_out=bindings/python/client/keystonedb \
  --grpc_python_out=bindings/python/client/keystonedb \
  --proto_path=kstone-proto/proto \
  kstone-proto/proto/keystone.proto
```

### JavaScript gRPC Client

```bash
cd bindings/javascript/client
npm install
npm run build
```

Output: `dist/index.js` + `dist/index.d.ts`

---

## Troubleshooting

### Go Embedded

**Error**: `cannot find -lkstone_ffi`

**Solution**: Set `CGO_LDFLAGS` to point to `target/release`:
```bash
export CGO_LDFLAGS="-L$(pwd)/target/release -lkstone_ffi"
```

**Error**: `dyld: Library not loaded`

**Solution**: Set runtime library path (macOS):
```bash
export DYLD_LIBRARY_PATH="$(pwd)/target/release"
```

Or on Linux:
```bash
export LD_LIBRARY_PATH="$(pwd)/target/release"
```

### Python Embedded

**Error**: `workspace` error during build

**Solution**: Make sure `bindings/python/embedded` is excluded from workspace in root `Cargo.toml`.

**Error**: `python source path does not exist`

**Solution**: Remove `python-source` line from `pyproject.toml`.

### gRPC Clients

**Error**: `Connection refused`

**Solution**: Start the KeystoneDB server:
```bash
./target/release/kstone-server --db-path test.keystone --port 50051
```

**Error**: `Unimplemented` for TransactWrite/Update/PartiQL

**Solution**: These features are stubbed on the server. Use Put/Get/Delete/Query/Scan/Batch operations.

---

## Next Steps

- Explore [examples/](examples/) for complete working code
- Read the [main README](README.md) for KeystoneDB features
- Check [BUILD_STATUS.md](bindings/BUILD_STATUS.md) for current build status
- Contribute: [GitHub Issues](https://github.com/keystone-db/keystonedb/issues)

---

## License

All bindings: MIT OR Apache-2.0 (same as KeystoneDB)

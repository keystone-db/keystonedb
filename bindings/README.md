# KeystoneDB Language Bindings

**v0.2.0 Release** - Multi-language bindings for KeystoneDB, providing both **remote (gRPC)** and **embedded** database access.

## Overview

KeystoneDB offers two modes of operation for each supported language:

1. **gRPC Client** - Connect to a remote KeystoneDB server via gRPC
2. **Embedded** - Use KeystoneDB as an embedded database directly in your application

## Supported Languages

| Language | gRPC Client | Embedded | Technology |
|----------|-------------|----------|------------|
| **Go** | ✅ | ✅ | protobuf-gen-go, cgo |
| **Python** | ✅ | ✅ | grpcio, PyO3 |
| **JavaScript/TypeScript** | ✅ | ✅ | @grpc/grpc-js, napi-rs |

## Directory Structure

```
bindings/
├── go/
│   ├── client/        # Go gRPC client
│   └── embedded/      # Go embedded (cgo)
├── python/
│   ├── client/        # Python gRPC client
│   └── embedded/      # Python embedded (PyO3)
├── javascript/
│   ├── client/        # JavaScript/TypeScript gRPC client
│   └── embedded/      # Node.js native bindings (napi-rs)
└── README.md          # This file
```

## Quick Start

### Go

#### gRPC Client

```go
import kstone "github.com/keystone-db/keystonedb/bindings/go/client"

client, _ := kstone.Connect("localhost:50051")
defer client.Close()

request := kstone.NewPutRequest([]byte("user#123")).
    WithString("name", "Alice").
    Build()
client.Put(context.Background(), request)
```

#### Embedded

```go
import kstone "github.com/keystone-db/keystonedb/bindings/go/embedded"

db, _ := kstone.Create("./mydb.keystone")
defer db.Close()

db.Put("user#123", "name", "Alice")
item, _ := db.Get("user#123")
```

### Python

#### gRPC Client

```python
from keystonedb import Client, PutRequestBuilder

client = Client.connect("localhost:50051")

request = PutRequestBuilder(b"user#123") \\
    .with_string("name", "Alice") \\
    .build()
client.put(request)
```

#### Embedded

```python
import keystonedb

db = keystonedb.Database.create("./mydb.keystone")

db.put(b"user#123", {"name": "Alice", "age": 30})
item = db.get(b"user#123")
db.flush()
```

### JavaScript/TypeScript

#### gRPC Client

```typescript
import { Client, stringValue } from '@keystonedb/client';

const client = new Client('localhost:50051');

await client.put({
  partitionKey: Buffer.from('user#123'),
  item: {
    attributes: {
      name: stringValue('Alice'),
    },
  },
});

client.close();
```

#### Embedded (Node.js)

```javascript
const { Database } = require('@keystonedb/embedded');

const db = Database.create('./mydb.keystone');

db.put(Buffer.from('user#123'), {
  name: 'Alice',
  age: 30,
});

const item = db.get(Buffer.from('user#123'));
db.flush();
```

## Installation

### Go

```bash
# gRPC client
go get github.com/keystone-db/keystonedb/bindings/go/client

# Embedded
go get github.com/keystone-db/keystonedb/bindings/go/embedded
```

### Python

```bash
# gRPC client
pip install keystonedb-client

# Embedded
pip install keystonedb
```

### JavaScript/TypeScript

```bash
# gRPC client
npm install @keystonedb/client

# Embedded
npm install @keystonedb/embedded
```

## Feature Comparison

| Feature | gRPC Client | Embedded |
|---------|-------------|----------|
| **Put/Get/Delete** | ✅ | ✅ |
| **Sort Keys** | ✅ | ✅ |
| **Query** | ✅ | ❌ |
| **Scan** | ✅ | ❌ |
| **Batch Operations** | ✅ | ❌ |
| **Transactions** | ✅* | ❌ |
| **Update Expressions** | ✅* | ❌ |
| **PartiQL** | ✅* | ❌ |
| **Network Latency** | Yes | No |
| **Process Isolation** | Yes | No |
| **Setup Required** | Server | None |

_* Server support depends on KeystoneDB server version_

## When to Use Which

### Use **gRPC Client** when:
- You need to share a database across multiple applications
- You want process isolation
- You need network-based access
- You're building microservices
- You want centralized database management

### Use **Embedded** when:
- You want maximum performance (no network overhead)
- You need a simple embedded database
- Your application is single-process
- You want to minimize dependencies
- You're building desktop or mobile apps

## Building from Source

### Prerequisites

- Rust 1.70+ (for embedded bindings)
- Go 1.21+ (for Go bindings)
- Python 3.8+ (for Python bindings)
- Node.js 14+ (for JavaScript bindings)
- Protocol Buffers compiler (for gRPC clients)

### Build C FFI Layer (Required for Embedded Bindings)

```bash
cd /path/to/keystonedb
cargo build --release -p kstone-ffi
```

This generates `target/release/libkstone_ffi.{so,dylib,dll}` used by Go and the C header `c-ffi/include/keystone.h`.

### Build Go Bindings

```bash
# Generate protobuf files (gRPC client)
cd bindings/go/client
protoc --go_out=. --go_opt=paths=source_relative \\
       --go-grpc_out=. --go-grpc_opt=paths=source_relative \\
       --proto_path=../../../kstone-proto/proto \\
       ../../../kstone-proto/proto/keystone.proto

# Build
go build

# For embedded bindings, ensure CGO environment is set
export CGO_LDFLAGS="-L/path/to/keystonedb/target/release"
export CGO_CFLAGS="-I/path/to/keystonedb/c-ffi/include"
cd ../embedded
go build
```

### Build Python Bindings

```bash
# gRPC client - generate protobuf
cd bindings/python/client
python -m grpc_tools.protoc \\
    --python_out=keystonedb \\
    --grpc_python_out=keystonedb \\
    --proto_path=../../../kstone-proto/proto \\
    ../../../kstone-proto/proto/keystone.proto

pip install -e .

# Embedded - build with maturin
cd ../embedded
pip install maturin
maturin develop --release
```

### Build JavaScript Bindings

```bash
# gRPC client - generate protobuf and TypeScript
cd bindings/javascript/client
npm install
npm run proto  # Generate protobuf files
npm run build  # Compile TypeScript

# Embedded - build with napi-rs
cd ../embedded
npm install
npm run build  # Build native addon
```

## Testing

Each binding has its own test suite. See individual README files for details:

- [Go gRPC Client](./go/client/README.md)
- [Go Embedded](./go/embedded/README.md)
- [Python gRPC Client](./python/client/README.md)
- [Python Embedded](./python/embedded/README.md)
- [JavaScript gRPC Client](./javascript/client/README.md)
- [JavaScript Embedded](./javascript/embedded/README.md)

## Documentation

For detailed API documentation, examples, and guides, see the README in each binding's directory.

## Current Limitations

### Embedded Bindings
- Currently only support basic CRUD operations (Put, Get, Delete)
- No query/scan support yet
- No batch operations
- No transactions
- No secondary indexes

These features will be added in future releases. The gRPC clients have full feature parity with the Rust API.

## Architecture

### gRPC Clients
All gRPC clients communicate with a `kstone-server` instance:

```
Application Code
       ↓
Language Binding (gRPC Client)
       ↓
   protobuf/gRPC
       ↓
  kstone-server
       ↓
  KeystoneDB (Rust)
```

### Embedded Bindings
Embedded bindings call directly into the Rust library:

**Go (cgo)**:
```
Go Application
      ↓
   cgo bindings
      ↓
  C FFI Layer (c-ffi/)
      ↓
  KeystoneDB (Rust)
```

**Python (PyO3)**:
```
Python Application
      ↓
   PyO3 bindings
      ↓
  KeystoneDB (Rust)
```

**JavaScript (napi-rs)**:
```
Node.js Application
      ↓
  napi-rs bindings
      ↓
  KeystoneDB (Rust)
```

## Contributing

Contributions are welcome! Areas for improvement:

1. **More language bindings**: Java, C#, Ruby, etc.
2. **Advanced features**: Add query/scan/batch support to embedded bindings
3. **Async support**: Async APIs for Node.js and Python embedded bindings
4. **More tests**: Expand test coverage
5. **Examples**: More real-world examples

## License

MIT OR Apache-2.0

# KeystoneDB Language Bindings

This directory contains official language bindings for KeystoneDB, an embedded DynamoDB-compatible database.

## Available Bindings

| Language | Directory | Status | Build System |
|----------|-----------|--------|--------------|
| C/C++ | [c-ffi](../c-ffi) | Stable | Cargo (cdylib) |
| Python | [python](./python) | Stable | Maturin + PyO3 |
| Node.js | [nodejs](./nodejs) | Stable | napi-rs |

## C/C++ Bindings

The C FFI provides a stable ABI for integrating KeystoneDB with any language that can call C functions.

```c
#include "keystonedb.h"

kstone_db_t* db = kstone_db_create("mydb.keystone");
kstone_item_t* item = kstone_item_create();
kstone_item_set_string(item, "name", "Alice");
kstone_item_set_number(item, "age", 30);
kstone_db_put(db, "user#123", 8, item);
kstone_item_free(item);
kstone_db_free(db);
```

See [c-ffi/README.md](../c-ffi/README.md) for full documentation.

## Python Bindings

Native Python bindings using PyO3 for high performance.

```python
from keystonedb import Database

db = Database.create("mydb.keystone")
db.put(b"user#123", {"name": "Alice", "age": 30})
item = db.get(b"user#123")
print(item["name"])  # Alice
```

See [python/README.md](./python/README.md) for full documentation.

## Node.js Bindings

Native Node.js bindings using napi-rs.

```javascript
const { Database } = require('keystonedb');

const db = Database.create('mydb.keystone');
db.put('user#123', { name: 'Alice', age: 30 });
const item = db.get('user#123');
console.log(item.name);  // Alice
```

See [nodejs/README.md](./nodejs/README.md) for full documentation.

## Building from Source

### Prerequisites

- Rust 1.70+ with Cargo
- For Python: Python 3.8+ and `maturin`
- For Node.js: Node.js 16+ and `npm`

### Build All Bindings

```bash
# C-FFI (produces libkeystonedb.so/dll)
cd c-ffi && cargo build --release

# Python (installs into current virtualenv)
cd bindings/python && maturin develop

# Node.js
cd bindings/nodejs && npm install && npm run build
```

## API Compatibility

All bindings provide the same core functionality:

| Operation | C-FFI | Python | Node.js |
|-----------|-------|--------|---------|
| Create DB | `kstone_db_create()` | `Database.create()` | `Database.create()` |
| Open DB | `kstone_db_open()` | `Database.open()` | `Database.open()` |
| In-Memory | `kstone_db_create_in_memory()` | `Database.create_in_memory()` | `Database.createInMemory()` |
| Put | `kstone_db_put()` | `db.put()` | `db.put()` |
| Get | `kstone_db_get()` | `db.get()` | `db.get()` |
| Delete | `kstone_db_delete()` | `db.delete()` | `db.delete()` |
| Query | `kstone_db_query()` | `db.query()` | `db.query()` |
| Scan | `kstone_db_scan()` | `db.scan()` | `db.scan()` |
| Update | `kstone_db_update()` | `db.update()` | `db.update()` |
| Execute SQL | `kstone_db_execute()` | `db.execute()` | `db.execute()` |
| Batch Get | `kstone_db_batch_get()` | `db.batch_get()` | `db.batchGet()` |
| Batch Write | `kstone_db_batch_write()` | `db.batch_write()` | `db.batchWrite()` |
| Flush | `kstone_db_flush()` | `db.flush()` | `db.flush()` |

## Error Handling

Each binding uses idiomatic error handling for its language:

- **C-FFI**: Returns error codes; use `kstone_get_last_error()` for details
- **Python**: Raises `RuntimeError` with descriptive messages
- **Node.js**: Throws `Error` with descriptive messages

## Thread Safety

KeystoneDB uses internal locking for thread safety. Database instances can be shared across threads with proper synchronization:

- Multiple readers allowed concurrently
- Writers acquire exclusive access
- All bindings inherit this behavior

## License

KeystoneDB bindings are dual-licensed under MIT and Apache 2.0.

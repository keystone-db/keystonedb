# Chapter 43: Language Bindings

KeystoneDB provides production-ready native bindings for multiple programming languages, enabling developers to use the embedded database from Python, Node.js, and C/C++ applications. This chapter covers installation, usage, and best practices for each language.

## Available Bindings

| Language | Technology | Status | Features |
|----------|------------|--------|----------|
| **Python** | PyO3 | Stable | Full API, PartiQL, Batch, Transactions |
| **Node.js** | napi-rs | Stable | Full API, PartiQL, TypeScript support |
| **C/C++** | C-FFI | Stable | Core operations, suitable for any C-compatible language |

All bindings provide the same core functionality with idiomatic APIs for each language.

## Python Bindings

### Installation

**From Source (Development):**

```bash
# Install maturin
pip install maturin

# Build and install
cd bindings/python
maturin develop

# Or build a wheel
maturin build --release
pip install target/wheels/keystonedb-*.whl
```

**From PyPI (Coming Soon):**

```bash
pip install keystonedb
```

### Quick Start

```python
from keystonedb import Database

# Create a new database
db = Database.create("mydb.keystone")

# Put an item
db.put(b"user#123", {"name": "Alice", "age": 30, "active": True})

# Get an item
item = db.get(b"user#123")
print(item["name"])  # Alice

# Delete an item
db.delete(b"user#123")
```

### Database Operations

```python
# Create a new database on disk
db = Database.create("path/to/db.keystone")

# Open an existing database
db = Database.open("path/to/db.keystone")

# Create an in-memory database (no persistence)
db = Database.create_in_memory()
```

### CRUD Operations

```python
# Put an item (partition key only)
db.put(b"user#123", {"name": "Alice", "age": 30})

# Put an item with sort key
db.put(b"user#123", {"bio": "Developer"}, sk=b"profile")

# Get an item
item = db.get(b"user#123")  # Returns dict or None

# Get an item with sort key
profile = db.get(b"user#123", sk=b"profile")

# Delete an item
db.delete(b"user#123")
db.delete(b"user#123", sk=b"profile")
```

### Query Operations

```python
# Query all items for a partition key
items = db.query(b"user#123")

# Query with sort key prefix
items = db.query(b"user#123", sk_begins_with=b"post#")

# Query with sort key range
items = db.query(
    b"sensor#456",
    sk_gte=b"2024-01-01",
    sk_lt=b"2024-02-01",
    limit=100
)

# Query in reverse order
items = db.query(b"user#123", forward=False, limit=10)

# Query using a secondary index
items = db.query(b"org#acme", index="email-index", sk_begins_with=b"alice")
```

### Scan Operations

```python
# Scan all items
items = db.scan()

# Scan with limit
items = db.scan(limit=100)

# Parallel scan (for large datasets)
from concurrent.futures import ThreadPoolExecutor

def scan_segment(segment, total):
    return db.scan(segment=segment, total_segments=total)

# Run 4 parallel scans
with ThreadPoolExecutor(max_workers=4) as executor:
    futures = [executor.submit(scan_segment, i, 4) for i in range(4)]
    all_items = []
    for future in futures:
        all_items.extend(future.result())
```

### Update Operations

```python
# Set an attribute
item = db.update(b"user#123", "SET age = :val", values={":val": 31})

# Increment a counter
item = db.update(b"counter#1", "SET count = count + :inc", values={":inc": 1})

# Remove an attribute
item = db.update(b"user#123", "REMOVE temp_field")

# Conditional update
item = db.update(
    b"user#123",
    "SET status = :new",
    values={":new": "active", ":old": "pending"},
    condition="status = :old"
)
```

### Batch Operations

```python
# Batch get multiple items
results = db.batch_get([b"user#1", b"user#2", b"user#3"])
# Returns: {"user#1": {...}, "user#2": {...}, ...}

# Batch write (puts and deletes)
count = db.batch_write(
    puts={
        b"user#4": {"name": "Dave"},
        b"user#5": {"name": "Eve"},
    },
    deletes=[b"user#old1", b"user#old2"]
)
print(f"Processed {count} operations")
```

### PartiQL Execution

```python
# SELECT query
result = db.execute("SELECT * FROM items WHERE pk = 'user#123'")
for item in result["items"]:
    print(item)

# INSERT
result = db.execute("INSERT INTO items VALUE {'pk': 'user#999', 'name': 'New User'}")

# UPDATE
result = db.execute("UPDATE items SET age = 25 WHERE pk = 'user#123'")

# DELETE
result = db.execute("DELETE FROM items WHERE pk = 'user#123'")
```

### ItemBuilder

```python
from keystonedb import item_builder

item = (
    item_builder()
    .string("name", "Alice")
    .number("age", 30)
    .bool("active", True)
    .build()
)

db.put(b"user#123", item)
```

### Data Types

| Python Type | KeystoneDB Type | Example |
|-------------|-----------------|---------|
| `str` | String (S) | `"hello"` |
| `int`, `float` | Number (N) | `42`, `3.14` |
| `bytes` | Binary (B) | `b"\x00\x01"` |
| `bool` | Boolean (Bool) | `True` |
| `None` | Null | `None` |
| `list` | List (L) | `[1, 2, 3]` |
| `dict` | Map (M) | `{"key": "value"}` |

### Thread Safety

```python
import threading

db = Database.create("mydb.keystone")

def worker(thread_id):
    for i in range(100):
        db.put(f"thread#{thread_id}#item#{i}".encode(), {"value": i})

threads = [threading.Thread(target=worker, args=(i,)) for i in range(4)]
for t in threads:
    t.start()
for t in threads:
    t.join()
```

### Error Handling

```python
try:
    item = db.get(b"user#123")
    if item is None:
        print("Item not found")
except RuntimeError as e:
    print(f"Database error: {e}")
```

---

## Node.js Bindings

### Installation

**From Source (Development):**

```bash
cd bindings/nodejs

# Install dependencies
npm install

# Build native module
npm run build

# Or for production
npm run build-release
```

**From npm (Coming Soon):**

```bash
npm install keystonedb
```

### Quick Start

```javascript
const { Database } = require('keystonedb');

// Create a new database
const db = Database.create('mydb.keystone');

// Put an item
db.put('user#123', { name: 'Alice', age: 30, active: true });

// Get an item
const item = db.get('user#123');
console.log(item.name);  // Alice

// Delete an item
db.delete('user#123');
```

### Database Operations

```javascript
const { Database } = require('keystonedb');

// Create a new database on disk
const db = Database.create('path/to/db.keystone');

// Open an existing database
const db = Database.open('path/to/db.keystone');

// Create an in-memory database (no persistence)
const db = Database.createInMemory();
```

### CRUD Operations

```javascript
// Put an item (partition key only)
db.put('user#123', { name: 'Alice', age: 30 });

// Put an item with sort key
db.put('user#123', { bio: 'Developer' }, 'profile');

// Put with Buffer keys
db.put(Buffer.from('user#123'), { name: 'Alice' });

// Get an item
const item = db.get('user#123');  // Returns object or null

// Get an item with sort key
const profile = db.get('user#123', 'profile');

// Delete an item
db.delete('user#123');
db.delete('user#123', 'profile');
```

### Query Operations

```javascript
// Query all items for a partition key
const items = db.query('user#123');

// Query with sort key prefix
const posts = db.query('user#123', { skBeginsWith: 'post#' });

// Query with sort key range
const readings = db.query('sensor#456', {
  skGte: '2024-01-01',
  skLt: '2024-02-01',
  limit: 100
});

// Query in reverse order
const recent = db.query('user#123', { forward: false, limit: 10 });

// Query using a secondary index
const byEmail = db.query('org#acme', {
  index: 'email-index',
  skBeginsWith: 'alice'
});
```

### Query Options

```typescript
interface QueryOptions {
  skBeginsWith?: string | Buffer;  // Sort key prefix filter
  skEq?: string | Buffer;          // Sort key equals
  skGt?: string | Buffer;          // Sort key greater than
  skGte?: string | Buffer;         // Sort key greater than or equal
  skLt?: string | Buffer;          // Sort key less than
  skLte?: string | Buffer;         // Sort key less than or equal
  limit?: number;                  // Maximum items to return
  forward?: boolean;               // true for ascending (default)
  index?: string;                  // Secondary index name
}
```

### Scan Operations

```javascript
// Scan all items
const allItems = db.scan();

// Scan with limit
const items = db.scan({ limit: 100 });

// Parallel scan (for large datasets)
async function parallelScan(db, segments) {
  const promises = [];
  for (let i = 0; i < segments; i++) {
    promises.push(Promise.resolve(db.scan({
      segment: i,
      totalSegments: segments
    })));
  }
  const results = await Promise.all(promises);
  return results.flat();
}

// Usage
const allItems = await parallelScan(db, 4);
```

### Update Operations

```javascript
// Set an attribute
const item = db.update('user#123', 'SET age = :val', {
  values: { ':val': 31 }
});

// Increment a counter
const counter = db.update('counter#1', 'SET count = count + :inc', {
  values: { ':inc': 1 }
});

// Remove an attribute
db.update('user#123', 'REMOVE tempField');

// Conditional update
const updated = db.update('user#123', 'SET status = :new', {
  values: { ':new': 'active', ':old': 'pending' },
  condition: 'status = :old'
});

// Update with sort key
db.update('user#123', 'SET verified = :val', {
  sk: 'profile',
  values: { ':val': true }
});
```

### Batch Operations

```javascript
// Batch get multiple items
const results = db.batchGet(['user#1', 'user#2', 'user#3']);
// Returns: { 'user#1': {...}, 'user#2': {...}, ... }

// Batch write (puts and deletes)
const count = db.batchWrite({
  puts: {
    'user#4': { name: 'Dave' },
    'user#5': { name: 'Eve' }
  },
  deletes: ['user#old1', 'user#old2']
});
console.log(`Processed ${count} operations`);
```

### PartiQL Execution

```javascript
// SELECT query
const result = db.execute("SELECT * FROM items WHERE pk = 'user#123'");
result.items.forEach(item => console.log(item));

// INSERT
const insertResult = db.execute(
  "INSERT INTO items VALUE {'pk': 'user#999', 'name': 'New User'}"
);

// UPDATE
const updateResult = db.execute(
  "UPDATE items SET age = 25 WHERE pk = 'user#123'"
);

// DELETE
const deleteResult = db.execute(
  "DELETE FROM items WHERE pk = 'user#123'"
);
```

### TypeScript Support

```typescript
import { Database, QueryOptions, ScanOptions } from 'keystonedb';

interface User {
  name: string;
  age: number;
  active: boolean;
}

const db = Database.create('mydb.keystone');

// Type-safe operations
db.put<User>('user#123', { name: 'Alice', age: 30, active: true });
const user = db.get<User>('user#123');

if (user) {
  console.log(user.name);  // TypeScript knows this is a string
}
```

### Data Types

| JavaScript Type | KeystoneDB Type | Example |
|-----------------|-----------------|---------|
| `string` | String (S) | `"hello"` |
| `number` | Number (N) | `42`, `3.14` |
| `Buffer` | Binary (B) | `Buffer.from([0, 1])` |
| `boolean` | Boolean (Bool) | `true` |
| `null` | Null | `null` |
| `Array` | List (L) | `[1, 2, 3]` |
| `Object` | Map (M) | `{ key: "value" }` |

### DynamoDB DocumentClient Comparison

| DynamoDB DocumentClient | KeystoneDB |
|-------------------------|------------|
| `new AWS.DynamoDB.DocumentClient()` | `Database.create('path')` |
| `client.put({ TableName, Item })` | `db.put(pk, item, sk?)` |
| `client.get({ TableName, Key })` | `db.get(pk, sk?)` |
| `client.delete({ TableName, Key })` | `db.delete(pk, sk?)` |
| `client.query({ TableName, ... })` | `db.query(pk, options)` |
| `client.scan({ TableName, ... })` | `db.scan(options)` |
| `client.update({ TableName, ... })` | `db.update(pk, expr, options)` |
| `client.batchGet({ RequestItems })` | `db.batchGet(keys)` |
| `client.batchWrite({ RequestItems })` | `db.batchWrite(options)` |

---

## C-FFI Bindings

The C-FFI provides a stable ABI for integrating KeystoneDB with C, C++, or any language with C interop (Go, Zig, Swift, etc.).

### Building

```bash
cd c-ffi
cargo build --release

# Output: target/release/libkeystonedb.so (Linux)
#         target/release/libkeystonedb.dylib (macOS)
#         target/release/keystonedb.dll (Windows)
```

### Header File

```c
// keystonedb.h
#ifndef KEYSTONEDB_H
#define KEYSTONEDB_H

#include <stdint.h>
#include <stddef.h>

typedef struct kstone_db_t kstone_db_t;
typedef struct kstone_item_t kstone_item_t;
typedef struct kstone_query_result_t kstone_query_result_t;

// Database operations
kstone_db_t* kstone_db_create(const char* path);
kstone_db_t* kstone_db_open(const char* path);
kstone_db_t* kstone_db_create_in_memory(void);
void kstone_db_free(kstone_db_t* db);

// Item operations
kstone_item_t* kstone_item_create(void);
void kstone_item_free(kstone_item_t* item);
void kstone_item_set_string(kstone_item_t* item, const char* key, const char* value);
void kstone_item_set_number(kstone_item_t* item, const char* key, int64_t value);
void kstone_item_set_bool(kstone_item_t* item, const char* key, int value);

// CRUD operations
int kstone_db_put(kstone_db_t* db, const char* pk, size_t pk_len, kstone_item_t* item);
int kstone_db_put_with_sk(kstone_db_t* db, const char* pk, size_t pk_len,
                          const char* sk, size_t sk_len, kstone_item_t* item);
kstone_item_t* kstone_db_get(kstone_db_t* db, const char* pk, size_t pk_len);
kstone_item_t* kstone_db_get_with_sk(kstone_db_t* db, const char* pk, size_t pk_len,
                                      const char* sk, size_t sk_len);
int kstone_db_delete(kstone_db_t* db, const char* pk, size_t pk_len);

// Query operations
kstone_query_result_t* kstone_db_query(kstone_db_t* db, const char* pk, size_t pk_len);
kstone_query_result_t* kstone_db_scan(kstone_db_t* db, size_t limit);

// Error handling
const char* kstone_get_last_error(void);

#endif
```

### Usage Example

```c
#include "keystonedb.h"
#include <stdio.h>

int main() {
    // Create database
    kstone_db_t* db = kstone_db_create("mydb.keystone");
    if (db == NULL) {
        printf("Error: %s\n", kstone_get_last_error());
        return 1;
    }

    // Create item
    kstone_item_t* item = kstone_item_create();
    kstone_item_set_string(item, "name", "Alice");
    kstone_item_set_number(item, "age", 30);
    kstone_item_set_bool(item, "active", 1);

    // Put item
    if (kstone_db_put(db, "user#123", 8, item) != 0) {
        printf("Put error: %s\n", kstone_get_last_error());
    }
    kstone_item_free(item);

    // Get item
    kstone_item_t* retrieved = kstone_db_get(db, "user#123", 8);
    if (retrieved != NULL) {
        // Use retrieved item
        kstone_item_free(retrieved);
    }

    // Delete item
    kstone_db_delete(db, "user#123", 8);

    // Clean up
    kstone_db_free(db);
    return 0;
}
```

### Compilation

```bash
# Linux
gcc -o myapp main.c -L./target/release -lkeystonedb -Wl,-rpath,./target/release

# macOS
gcc -o myapp main.c -L./target/release -lkeystonedb -Wl,-rpath,@executable_path/target/release

# Windows (MSVC)
cl main.c /link target\release\keystonedb.dll.lib
```

---

## API Compatibility Matrix

All bindings provide the same core functionality:

| Operation | Python | Node.js | C-FFI |
|-----------|--------|---------|-------|
| Create DB | `Database.create()` | `Database.create()` | `kstone_db_create()` |
| Open DB | `Database.open()` | `Database.open()` | `kstone_db_open()` |
| In-Memory | `Database.create_in_memory()` | `Database.createInMemory()` | `kstone_db_create_in_memory()` |
| Put | `db.put()` | `db.put()` | `kstone_db_put()` |
| Get | `db.get()` | `db.get()` | `kstone_db_get()` |
| Delete | `db.delete()` | `db.delete()` | `kstone_db_delete()` |
| Query | `db.query()` | `db.query()` | `kstone_db_query()` |
| Scan | `db.scan()` | `db.scan()` | `kstone_db_scan()` |
| Update | `db.update()` | `db.update()` | `kstone_db_update()` |
| Execute SQL | `db.execute()` | `db.execute()` | `kstone_db_execute()` |
| Batch Get | `db.batch_get()` | `db.batchGet()` | `kstone_db_batch_get()` |
| Batch Write | `db.batch_write()` | `db.batchWrite()` | `kstone_db_batch_write()` |
| Flush | `db.flush()` | `db.flush()` | `kstone_db_flush()` |

---

## Performance Considerations

### General Tips

1. **Reuse Database Instances**: Create once, use many times
2. **Use Batch Operations**: For bulk inserts/reads, batching is 5-10x faster
3. **Parallel Scans**: For large datasets, use multiple scan segments
4. **In-Memory Mode**: Use for testing or temporary data (no disk I/O)
5. **Flush Strategically**: Call `flush()` when durability is critical

### Python-Specific

```python
# Good: Reuse database instance
db = Database.create("mydb.keystone")
for item in items:
    db.put(item.key, item.data)

# Better: Use batch operations
db.batch_write(puts={item.key: item.data for item in items})
```

### Node.js-Specific

```javascript
// Good: Reuse database instance
const db = Database.create('mydb.keystone');

// Better: Use batch operations for bulk inserts
db.batchWrite({
  puts: Object.fromEntries(items.map(i => [i.key, i.data]))
});
```

### C-Specific

```c
// Minimize allocations
kstone_item_t* item = kstone_item_create();
for (int i = 0; i < count; i++) {
    // Reuse item structure (if API supports clearing)
    kstone_item_set_string(item, "key", values[i]);
    kstone_db_put(db, pks[i], strlen(pks[i]), item);
}
kstone_item_free(item);
```

---

## Error Handling

### Python

```python
try:
    item = db.get(b"user#123")
    if item is None:
        print("Item not found")
except KeyError as e:
    print(f"Key error: {e}")
except RuntimeError as e:
    print(f"Database error: {e}")
except ValueError as e:
    print(f"Invalid argument: {e}")
```

### Node.js

```javascript
try {
  const item = db.get('user#123');
  if (item === null) {
    console.log('Item not found');
  }
} catch (error) {
  console.error('Database error:', error.message);
}
```

### C

```c
if (kstone_db_put(db, "user#123", 8, item) != 0) {
    const char* error = kstone_get_last_error();
    fprintf(stderr, "Put failed: %s\n", error);
}
```

---

## Thread Safety

All bindings inherit KeystoneDB's thread-safe design:

- Multiple concurrent readers are allowed
- Writers acquire exclusive access automatically
- Safe to share a Database instance across threads

**Python Example:**

```python
import threading

db = Database.create("mydb.keystone")

def worker(tid):
    for i in range(1000):
        db.put(f"t{tid}#i{i}".encode(), {"v": i})

threads = [threading.Thread(target=worker, args=(i,)) for i in range(4)]
for t in threads: t.start()
for t in threads: t.join()
```

**Node.js Example:**

```javascript
const { Worker, isMainThread, workerData } = require('worker_threads');

if (isMainThread) {
  for (let i = 0; i < 4; i++) {
    new Worker(__filename, { workerData: { id: i } });
  }
} else {
  const { Database } = require('keystonedb');
  const db = Database.open('shared.keystone');
  for (let j = 0; j < 1000; j++) {
    db.put(`t${workerData.id}#i${j}`, { v: j });
  }
}
```

---

## Future Bindings

The following language bindings are planned for future releases:

- **Go**: Using cgo with C-FFI
- **Java/Kotlin**: JNI wrapper
- **Swift**: Swift package with C interop
- **Ruby**: Native extension using C-FFI

Community contributions for additional language bindings are welcome. See the [Contributing Guide](https://github.com/keystonedb/keystonedb/blob/main/CONTRIBUTING.md) for details.

---

## Summary

KeystoneDB's language bindings provide:

- **Native Performance**: Direct Rust integration, no protocol overhead
- **Idiomatic APIs**: Each binding follows language conventions
- **Feature Complete**: All core operations available in every language
- **Thread Safe**: Safe for concurrent access
- **Production Ready**: Stable APIs with comprehensive error handling

Choose the binding that matches your application's language and enjoy the full power of KeystoneDB's embedded database.

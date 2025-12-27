# KeystoneDB Python Bindings

Native Python bindings for KeystoneDB, an embedded DynamoDB-compatible database.

## Installation

### From Source (Development)

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

### From PyPI (Coming Soon)

```bash
pip install keystonedb
```

## Quick Start

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

## API Reference

### Database Creation

```python
# Create a new database on disk
db = Database.create("path/to/db.keystone")

# Open an existing database
db = Database.open("path/to/db.keystone")

# Create an in-memory database (no persistence)
db = Database.create_in_memory()
```

### Basic Operations

```python
# Put an item (partition key only)
db.put(b"user#123", {"name": "Alice", "age": 30})

# Put an item with sort key
db.put(b"user#123", {"bio": "Developer"}, sk=b"profile")

# Get an item
item = db.get(b"user#123")  # Returns dict or None

# Get an item with sort key
item = db.get(b"user#123", sk=b"profile")

# Delete an item
db.delete(b"user#123")
db.delete(b"user#123", sk=b"profile")
```

### Query Operations

Query items by partition key with optional sort key conditions:

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

Scan all items in the database:

```python
# Scan all items
items = db.scan()

# Scan with limit
items = db.scan(limit=100)

# Parallel scan (for large datasets)
import threading

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

Update items using DynamoDB-style expressions:

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

### Execute PartiQL

Execute SQL-like queries:

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

Build items with a fluent API:

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

### Flush

Force pending writes to disk:

```python
db.flush()
```

## Data Types

KeystoneDB supports DynamoDB-compatible data types:

| Python Type | KeystoneDB Type | Example |
|-------------|-----------------|---------|
| `str` | String (S) | `"hello"` |
| `int`, `float` | Number (N) | `42`, `3.14` |
| `bytes` | Binary (B) | `b"\x00\x01"` |
| `bool` | Boolean (Bool) | `True` |
| `None` | Null | `None` |
| `list` | List (L) | `[1, 2, 3]` |
| `dict` | Map (M) | `{"key": "value"}` |

## Keys

Keys can be either strings or bytes:

```python
# String keys (converted to UTF-8 bytes internally)
db.put("user#123", {"name": "Alice"})

# Byte keys
db.put(b"user#123", {"name": "Alice"})

# Sort keys work the same way
db.put(b"user#123", {"bio": "..."}, sk=b"profile")
db.put("user#123", {"bio": "..."}, sk="profile")
```

## Error Handling

```python
try:
    item = db.get(b"user#123")
    if item is None:
        print("Item not found")
except RuntimeError as e:
    print(f"Database error: {e}")
```

## Thread Safety

The Database object is thread-safe and can be shared across threads:

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

## Performance Tips

1. **Use batch operations** for bulk inserts/reads
2. **Use parallel scans** for large datasets
3. **Use in-memory mode** for testing or temporary data
4. **Flush periodically** for durability if needed

## Version

```python
from keystonedb import version
print(version())  # "0.1.0"
```

## License

MIT OR Apache-2.0

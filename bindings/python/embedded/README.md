# KeystoneDB Python Embedded Bindings

Python bindings for using KeystoneDB as an embedded database. Built with PyO3 and Maturin.

## Installation

### From PyPI (when published)

```bash
pip install keystonedb
```

### From Source

```bash
cd bindings/python/embedded
pip install maturin
maturin develop --release
```

Or for a production wheel:

```bash
maturin build --release
pip install target/wheels/keystonedb-*.whl
```

## Usage

### Basic Operations

```python
import keystonedb

# Create a new database
db = keystonedb.Database.create("./mydb.keystone")

# Put an item
db.put(b"user#123", {
    "name": "Alice",
    "age": 30,
    "active": True,
    "tags": ["python", "rust"],
})

# Get an item
item = db.get(b"user#123")
if item:
    print(f"Name: {item['name']}, Age: {item['age']}")

# Delete an item
db.delete(b"user#123")

# Flush to disk
db.flush()
```

### With Sort Keys

```python
# Put with sort key
db.put_with_sk(b"org#acme", b"user#123", {
    "name": "Alice",
    "role": "admin",
})

# Get with sort key
item = db.get_with_sk(b"org#acme", b"user#123")

# Delete with sort key
db.delete_with_sk(b"org#acme", b"user#123")
```

### In-Memory Database

```python
# Create in-memory database (no persistence)
db = keystonedb.Database.create_in_memory()

# Use same API as persistent database
db.put(b"key1", {"value": "test"})
item = db.get(b"key1")
```

### Opening Existing Database

```python
# Open an existing database
db = keystonedb.Database.open("./mydb.keystone")
```

### Supported Value Types

KeystoneDB supports DynamoDB-style value types, automatically converted to/from Python:

```python
db.put(b"test", {
    "string": "hello",              # String (S)
    "number_int": 42,                # Number (N) - integer
    "number_float": 3.14,            # Number (N) - float
    "binary": b"binary data",        # Binary (B)
    "boolean": True,                 # Boolean (Bool)
    "null": None,                    # Null
    "list": [1, 2, "three"],        # List (L)
    "nested": {                      # Map (M)
        "inner": "value",
        "count": 10,
    },
    "vector": [0.1, 0.2, 0.3],      # Vector (VecF32) - for embeddings
})

item = db.get(b"test")
assert item["string"] == "hello"
assert item["number_int"] == 42
assert item["boolean"] is True
assert item["list"] == [1, 2, "three"]
assert item["nested"]["inner"] == "value"
```

### Context Manager (Automatic Flush)

```python
class DatabaseContext:
    def __init__(self, path):
        self.path = path
        self.db = None

    def __enter__(self):
        self.db = keystonedb.Database.open(self.path)
        return self.db

    def __exit__(self, exc_type, exc_val, exc_tb):
        if self.db:
            self.db.flush()

# Usage
with DatabaseContext("./mydb.keystone") as db:
    db.put(b"key", {"value": "data"})
    # Automatically flushed on exit
```

## API Reference

### Database Class

#### Static Methods

- `Database.create(path: str) -> Database`
  - Create a new database at the specified path
  - Raises IOError if database already exists or path is invalid

- `Database.open(path: str) -> Database`
  - Open an existing database
  - Raises IOError if database doesn't exist or is corrupted

- `Database.create_in_memory() -> Database`
  - Create an in-memory database (no persistence)
  - Data is lost when the database object is dropped

#### Instance Methods

- `db.put(pk: bytes, item: dict) -> None`
  - Store an item with partition key only
  - item: Dictionary mapping attribute names to values
  - Raises ValueError for invalid item structure

- `db.put_with_sk(pk: bytes, sk: bytes, item: dict) -> None`
  - Store an item with partition key and sort key
  - Allows multiple items with same PK but different SKs

- `db.get(pk: bytes) -> dict | None`
  - Retrieve an item by partition key
  - Returns None if item not found
  - Returns dictionary with all attributes

- `db.get_with_sk(pk: bytes, sk: bytes) -> dict | None`
  - Retrieve an item by partition key and sort key
  - Returns None if item not found

- `db.delete(pk: bytes) -> None`
  - Remove an item by partition key
  - No error if item doesn't exist (idempotent)

- `db.delete_with_sk(pk: bytes, sk: bytes) -> None`
  - Remove an item by partition key and sort key

- `db.flush() -> None`
  - Force flush all pending writes to disk
  - Automatically called on normal shutdown
  - Useful for ensuring durability before critical operations

## Building

### Development Build

```bash
maturin develop
```

### Release Build

```bash
maturin build --release
```

### Building Wheels for Distribution

```bash
# Build for current platform
maturin build --release

# Build for multiple Python versions (requires multiple Python installations)
maturin build --release --interpreter python3.8 python3.9 python3.10 python3.11 python3.12
```

## Current Limitations

This is a basic embedded database wrapper. Current limitations:

1. No query/scan operations (only get/put/delete)
2. No batch operations
3. No transactions
4. No conditional writes
5. No TTL support
6. No secondary indexes
7. No PartiQL queries

Advanced features will be added in future releases.

## Performance Tips

1. **Batch writes**: While there's no explicit batch API, you can put multiple items and call `flush()` once
2. **In-memory mode**: Use for testing or temporary data for maximum performance
3. **Composite keys**: Use sort keys to organize related data together

## Error Handling

```python
import keystonedb

try:
    db = keystonedb.Database.open("./mydb.keystone")
except IOError as e:
    print(f"Failed to open database: {e}")

try:
    db.put(b"key", {"invalid": complex_object})
except ValueError as e:
    print(f"Invalid value type: {e}")

try:
    item = db.get(b"nonexistent")
    if item is None:
        print("Item not found")
except Exception as e:
    print(f"Unexpected error: {e}")
```

## License

MIT OR Apache-2.0

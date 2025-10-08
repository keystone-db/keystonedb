# Python Embedded KeystoneDB Example

This example demonstrates using KeystoneDB as an embedded database in Python applications via PyO3 bindings.

## Features Demonstrated

- **Contact Manager**: Full CRUD operations for managing contacts
- **Value Types**: Strings, numbers, booleans, null, lists, and nested dicts
- **Sort Keys**: Hierarchical data organization (company → employees)
- **In-Memory Mode**: Temporary databases without disk persistence
- **Error Handling**: Proper exception handling and validation

## Prerequisites

1. **Build the Python wheel** (from repository root):
   ```bash
   maturin build --manifest-path bindings/python/embedded/Cargo.toml --release
   ```

2. **Install the wheel**:
   ```bash
   pip install bindings/python/embedded/target/wheels/keystonedb-*.whl
   ```

## Running the Example

```bash
# From the repository root or this directory
python examples/python-embedded/contacts.py
```

Or make it executable:

```bash
chmod +x examples/python-embedded/contacts.py
./examples/python-embedded/contacts.py
```

## Expected Output

```
============================================================
KeystoneDB Python Bindings Demo: Contact Manager
============================================================
📂 Created new database: /tmp/.../contacts.keystone

--- Example 1: Adding Contacts ---
✅ Added contact: Alice Johnson
✅ Added contact: Bob Smith
✅ Added contact: Charlie Brown

--- Example 2: Retrieving Contacts ---

Alice Johnson's contact info:
──────────────────────────────────────────────────
  Name         : Alice Johnson
  Email        : alice@example.com
  Phone        : +1-555-0101
  Company      : Acme Corp
──────────────────────────────────────────────────

--- Example 3: Updating Contact ---
✅ Updated contact: Bob Smith

Bob Smith's updated contact info:
──────────────────────────────────────────────────
  Name         : Bob Smith
  Email        : bob@example.com
  Phone        : +1-555-9999
  Company      : New Startup Inc
──────────────────────────────────────────────────

--- Example 4: Deleting Contact ---
✅ Deleted contact: Charlie Brown
✓ Contact successfully deleted

--- Example 5: Persistence ---
💾 Database flushed to disk
Database saved at: /tmp/.../contacts.keystone

--- Example 6: Value Types ---

Demonstrating various value types...
✅ Added contact with complex data types

Retrieved contact with all value types:
──────────────────────────────────────────────────
  Name         : Diana Prince
  Email        : diana@example.com
  Phone        : +1-555-0104
  Age          : 30
  Active       : True
  Department   : None
  Tags         : ['vip', 'enterprise', 'priority']
  Metadata     : {'created_at': '2024-01-01', 'source': 'web', 'score': 95}
──────────────────────────────────────────────────

✅ All examples completed successfully!

============================================================
KeystoneDB Python Bindings Demo: Sort Keys
============================================================
📂 Created new database: /tmp/.../org.keystone

--- Organizing Contacts by Company ---
✅ Added alice to acme-corp
✅ Added bob to acme-corp
✅ Added charlie to tech-innovations
✅ Added diana to tech-innovations

--- Retrieving Employees by Company ---

Alice from Acme Corp:
  Role: CEO
  Department: Executive

--- Removing Employee ---
✅ Removed charlie from tech-innovations
✓ Employee successfully removed

✅ Sort key demo completed!

============================================================
KeystoneDB Python Bindings Demo: In-Memory Database
============================================================

--- Creating In-Memory Database ---
✅ Created in-memory database (no disk I/O)

--- Storing Session Data ---
✅ Stored session#abc123
✅ Stored session#def456
✅ Stored session#ghi789

--- Retrieving Session ---
Session abc123: user=user#1, expires=1704067200

--- Cleanup ---
✅ Session deleted

💡 Note: All data is in memory and will be lost when program exits
✅ In-memory demo completed!

============================================================
🎉 All demos completed successfully!
============================================================
```

## Code Structure

The example is organized into several demonstration functions:

### `ContactManager` Class
A complete contact management system showing real-world usage:
- `add_contact()` - Create new contacts
- `get_contact()` - Retrieve contacts by name
- `update_contact()` - Modify contact information
- `delete_contact()` - Remove contacts
- `flush()` - Persist data to disk

### Demo Functions
- `demo_basic_operations()` - Contact manager CRUD demo
- `demo_value_types()` - All supported Python types
- `demo_sort_keys()` - Company/employee hierarchy
- `demo_in_memory()` - In-memory database for sessions

## API Reference

### Creating/Opening Database

```python
import keystonedb

# Create new database
db = keystonedb.Database.create("/path/to/db.keystone")

# Open existing database
db = keystonedb.Database.open("/path/to/db.keystone")

# Create in-memory database
db = keystonedb.Database.create_in_memory()
```

### Basic Operations

```python
# Put item (bytes key, dict value)
db.put(b"user#123", {
    "name": "Alice",
    "age": 30,
    "active": True
})

# Get item (returns dict or None)
item = db.get(b"user#123")
if item:
    print(item["name"])

# Delete item
db.delete(b"user#123")

# Flush to disk
db.flush()
```

### Sort Key Operations

```python
# Put with sort key
db.put_with_sk(
    b"org#acme",           # Partition key
    b"employee#alice",     # Sort key
    {"role": "CEO"}        # Item data
)

# Get with sort key
item = db.get_with_sk(b"org#acme", b"employee#alice")

# Delete with sort key
db.delete_with_sk(b"org#acme", b"employee#alice")
```

### Supported Value Types

```python
# All Python types are automatically converted
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

### Error Handling

```python
try:
    item = db.get(b"nonexistent")
    if item is None:
        print("Item not found")
except Exception as e:
    print(f"Error: {e}")
```

## Integration in Your Application

To use KeystoneDB in your Python application:

1. **Install the package**:
   ```bash
   pip install keystonedb
   ```
   (Or use the local wheel during development)

2. **Import in your code**:
   ```python
   import keystonedb
   ```

3. **Use the API** as shown in this example

## Use Cases

This example demonstrates KeystoneDB for:
- **Contact Management**: Store and retrieve user profiles
- **Organizational Hierarchies**: Company → Department → Employee structures
- **Session Storage**: In-memory session data (temporary)
- **Configuration Management**: Application settings and metadata
- **Cache Layer**: Fast key-value lookups with persistence

## Performance Tips

- Use `flush()` explicitly for critical data (writes are buffered)
- In-memory mode is fastest for temporary data
- Batch operations when possible
- Keys must be bytes (use `.encode()` for strings)

## Next Steps

- See `../../bindings/python/embedded/test_smoke.py` for more usage examples
- Check `../../bindings/python/embedded/README.md` for full API documentation
- For remote access, see the gRPC client examples in `../grpc-client/python/`
- Explore advanced features: Query, Scan, Batch operations, Transactions (via gRPC)

## Notes

- The database format is `.keystone` (a directory containing WAL and SST files)
- All operations are thread-safe (GIL-free via PyO3)
- Keys must be bytes (b"..." or "...".encode())
- Values are automatically serialized from Python dicts to KeystoneDB items
- The binding uses PyO3 (direct Rust → Python, no C layer)

# Go Embedded KeystoneDB Example

This example demonstrates using KeystoneDB as an embedded database in Go applications via cgo bindings.

## Features Demonstrated

- **Simple CRUD**: Basic Put, Get, Delete operations
- **Sort Keys**: Hierarchical data organization (e.g., org → users)
- **Multiple Items**: Batch data insertion and retrieval
- **Error Handling**: Proper error checking with ErrNotFound

## Prerequisites

1. **Build the C FFI library** (from repository root):
   ```bash
   cargo build --release -p c-ffi
   ```

2. **Set environment variables** (required for cgo):
   ```bash
   export CGO_ENABLED=1
   export CGO_LDFLAGS="-L$(pwd)/target/release -lkstone_ffi"
   export CGO_CFLAGS="-I$(pwd)/c-ffi/include"
   ```

3. **On macOS**: Also set library path for runtime:
   ```bash
   export DYLD_LIBRARY_PATH="$(pwd)/target/release"
   ```

   **On Linux**: Use `LD_LIBRARY_PATH` instead:
   ```bash
   export LD_LIBRARY_PATH="$(pwd)/target/release"
   ```

## Building

From the repository root:

```bash
# Navigate to example directory
cd examples/go-embedded

# Build the example
go build -o go-embedded-example

# Run it
./go-embedded-example
```

Or run directly:

```bash
go run main.go
```

## Expected Output

```
Creating database at: /tmp/example.keystone

--- Example 1: Simple Put/Get/Delete ---
Putting user#alice...
Getting user#alice...
Retrieved item: map[age:30 email:alice@example.com name:Alice Smith]
Deleting user#alice...
Item successfully deleted (ErrNotFound returned)

--- Example 2: Using Sort Keys ---
Creating organization hierarchy...
Getting org#acme/user#alice...
Retrieved: map[role:admin]
Removing user#bob from org#acme...
User successfully removed

--- Example 3: Multiple Items in Partition ---
Creating sensor readings...
Stored sensor#001: 72.5 fahrenheit
Stored sensor#002: 45.2 celsius
Stored sensor#003: 1013.25 hpa

Reading sensor data...
sensor#001: map[unit:fahrenheit value:72.5]
sensor#002: map[unit:celsius value:45.2]
sensor#003: map[hpa:1013.25 unit:hpa]

--- Example 4: Error Handling ---
Attempting to get non-existent item...
✓ Correctly received ErrNotFound
Attempting to get non-existent item with sort key...
✓ Correctly received ErrNotFound

✅ All examples completed successfully!
Database location: /tmp/example.keystone
```

## Code Structure

The example is organized into focused functions:

- `simpleCRUD()` - Basic put/get/delete operations
- `sortKeyExample()` - Demonstrates partition key + sort key pattern
- `multipleItemsExample()` - Shows handling multiple items
- `errorHandlingExample()` - Proper error checking patterns

## API Reference

### Creating/Opening Database

```go
import kstone "github.com/keystone-db/keystonedb/bindings/go/embedded"

// Create new database
db, err := kstone.Create("/path/to/db.keystone")

// Open existing database
db, err := kstone.Open("/path/to/db.keystone")

// Create in-memory database
db, err := kstone.CreateInMemory()

// Always close when done
defer db.Close()
```

### Basic Operations

```go
// Put item (single partition key)
err := db.Put("user#123", "name", "Alice")

// Get item
item, err := db.Get("user#123")
// item is map[string]interface{}

// Delete item
err := db.Delete("user#123")
```

### Sort Key Operations

```go
// Put with sort key (partition key + sort key)
err := db.PutWithSK("org#acme", "user#alice", "role", "admin")

// Get with sort key
item, err := db.GetWithSK("org#acme", "user#alice")

// Delete with sort key
err := db.DeleteWithSK("org#acme", "user#alice")
```

### Error Handling

```go
item, err := db.Get("key")
if err == kstone.ErrNotFound {
    // Item doesn't exist
} else if err != nil {
    // Other error occurred
} else {
    // Success - use item
}
```

## Integration in Your Application

To use KeystoneDB in your Go application:

1. Add to `go.mod`:
   ```go
   require github.com/keystone-db/keystonedb/bindings/go/embedded v0.1.0
   ```

2. Import in your code:
   ```go
   import kstone "github.com/keystone-db/keystonedb/bindings/go/embedded"
   ```

3. Use the API as shown in this example

4. Remember to set `CGO_LDFLAGS`, `CGO_CFLAGS`, and `DYLD_LIBRARY_PATH`/`LD_LIBRARY_PATH` when building and running

## Notes

- The database file format is `.keystone` (a directory containing WAL and SST files)
- All operations are thread-safe
- The binding uses cgo, so cross-compilation may require additional setup
- For production use, consider error handling, connection pooling, and graceful shutdown patterns

## Next Steps

- See `../../bindings/go/embedded/README.md` for full API documentation
- Check `../../bindings/go/embedded/smoke_test.go` for more usage examples
- For remote access, see the gRPC client examples in `../grpc-client/go/`

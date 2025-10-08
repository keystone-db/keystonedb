# KeystoneDB Go Embedded Bindings

Go bindings for using KeystoneDB as an embedded database via CGO.

## Prerequisites

1. Build the KeystoneDB C FFI library:
   ```bash
   cd /path/to/keystonedb
   cargo build --release -p kstone-ffi
   ```

2. Ensure the shared library is in a location your system can find it, or set the appropriate library path environment variable.

## Installation

```bash
go get github.com/keystone-db/keystonedb/bindings/go/embedded
```

## Usage

### Basic Operations

```go
package main

import (
    "fmt"
    "log"

    kstone "github.com/keystone-db/keystonedb/bindings/go/embedded"
)

func main() {
    // Create a new database
    db, err := kstone.Create("./mydb.keystone")
    if err != nil {
        log.Fatal(err)
    }
    defer db.Close()

    // Put an item (simple string attribute)
    err = db.Put("user#123", "name", "Alice")
    if err != nil {
        log.Fatal(err)
    }

    // Get an item
    item, err := db.Get("user#123")
    if err != nil {
        log.Fatal(err)
    }
    defer item.Free()

    fmt.Println("Retrieved item successfully")

    // Delete an item
    err = db.Delete("user#123")
    if err != nil {
        log.Fatal(err)
    }
}
```

### With Sort Keys

```go
// Put with sort key
err = db.PutWithSK("org#acme", "user#123", "name", "Alice")

// Get with sort key
item, err := db.GetWithSK("org#acme", "user#123")
if err != nil {
    log.Fatal(err)
}
defer item.Free()

// Delete with sort key
err = db.DeleteWithSK("org#acme", "user#123")
```

### In-Memory Database

```go
// Create an in-memory database (no persistence)
db, err := kstone.CreateInMemory()
if err != nil {
    log.Fatal(err)
}
defer db.Close()

// Use same API as persistent database
err = db.Put("key1", "attr", "value")
```

### Opening Existing Database

```go
// Open an existing database
db, err := kstone.Open("./mydb.keystone")
if err != nil {
    log.Fatal(err)
}
defer db.Close()
```

## API Reference

### Database Functions

- `Create(path string) (*Database, error)` - Create new database at path
- `Open(path string) (*Database, error)` - Open existing database
- `CreateInMemory() (*Database, error)` - Create in-memory database

### Database Methods

- `Close() error` - Close database
- `Put(pk, attrName, value string) error` - Store item with partition key only
- `PutWithSK(pk, sk, attrName, value string) error` - Store item with sort key
- `Get(pk string) (*Item, error)` - Retrieve item by partition key
- `GetWithSK(pk, sk string) (*Item, error)` - Retrieve item by partition and sort key
- `Delete(pk string) error` - Remove item by partition key
- `DeleteWithSK(pk, sk string) error` - Remove item by partition and sort key
- `PutString(pk, sk, attrName, value string) error` - Low-level put operation

### Item Methods

- `Free()` - Free item handle (must call to avoid memory leaks)

### Error Types

- `ErrNullPointer` - Null pointer argument
- `ErrInvalidUtf8` - Invalid UTF-8 string
- `ErrInvalidArgument` - Invalid argument
- `ErrIo` - I/O error
- `ErrNotFound` - Item not found
- `ErrInternal` - Internal error
- `ErrCorruption` - Corruption detected
- `ErrConditionalCheckFailed` - Conditional check failed

## Memory Management

**Important**: Always call `item.Free()` after you're done with an item to avoid memory leaks:

```go
item, err := db.Get("key")
if err != nil {
    log.Fatal(err)
}
defer item.Free() // Important!
```

## Building

To build your application with these bindings:

```bash
# Make sure the library path is set
export CGO_LDFLAGS="-L/path/to/keystonedb/target/release"
export CGO_CFLAGS="-I/path/to/keystonedb/c-ffi/include"

go build
```

Or use the default paths relative to the module:

```bash
cd bindings/go/embedded
go build
```

## Current Limitations

This is a basic FFI wrapper. Current limitations:

1. Only string attributes supported (no numbers, booleans, maps, lists yet)
2. No query/scan operations
3. No batch operations
4. No transactions
5. Limited item inspection (can't read attribute values from items yet)

These features require extending the C FFI layer with additional functions.

## License

MIT OR Apache-2.0

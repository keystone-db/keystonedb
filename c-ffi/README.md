# KeystoneDB C-FFI

C Foreign Function Interface for KeystoneDB, providing a stable ABI for integration with any language that can call C functions.

## Building

```bash
cd c-ffi

# Build debug
cargo build

# Build release (recommended)
cargo build --release

# The library will be at:
# - Linux: target/release/libkeystonedb.so
# - macOS: target/release/libkeystonedb.dylib
# - Windows: target/release/keystonedb.dll
```

## Usage

### Include the Header

```c
#include "keystonedb.h"
```

### Compile and Link

```bash
# Compile
gcc -c myapp.c -I/path/to/c-ffi/include

# Link
gcc myapp.o -L/path/to/target/release -lkeystonedb -o myapp

# Run (set library path)
export LD_LIBRARY_PATH=/path/to/target/release:$LD_LIBRARY_PATH
./myapp
```

## Quick Start

```c
#include <stdio.h>
#include "keystonedb.h"

int main() {
    // Create a database
    kstone_db_t* db = kstone_db_create("mydb.keystone");
    if (!db) {
        printf("Error: %s\n", kstone_get_last_error());
        return 1;
    }

    // Create an item
    kstone_item_t* item = kstone_item_create();
    kstone_item_set_string(item, "name", "Alice");
    kstone_item_set_number(item, "age", 30);
    kstone_item_set_bool(item, "active", 1);

    // Put the item
    int result = kstone_db_put(db, "user#123", 8, item);
    if (result != KSTONE_OK) {
        printf("Put failed: %s\n", kstone_get_last_error());
    }
    kstone_item_free(item);

    // Get the item
    char* json = kstone_db_get(db, "user#123", 8);
    if (json) {
        printf("Got: %s\n", json);
        kstone_free_string(json);
    }

    // Delete the item
    kstone_db_delete(db, "user#123", 8);

    // Close the database
    kstone_db_free(db);
    return 0;
}
```

## API Reference

### Database Lifecycle

```c
// Create a new database at the given path
kstone_db_t* kstone_db_create(const char* path);

// Open an existing database
kstone_db_t* kstone_db_open(const char* path);

// Create an in-memory database (no persistence)
kstone_db_t* kstone_db_create_in_memory(void);

// Close and free a database
void kstone_db_free(kstone_db_t* db);

// Flush pending writes to disk
int kstone_db_flush(kstone_db_t* db);
```

### Item Operations

```c
// Create a new empty item
kstone_item_t* kstone_item_create(void);

// Free an item
void kstone_item_free(kstone_item_t* item);

// Set string attribute
int kstone_item_set_string(kstone_item_t* item, const char* key, const char* value);

// Set number attribute (as string for precision)
int kstone_item_set_number(kstone_item_t* item, const char* key, double value);

// Set boolean attribute
int kstone_item_set_bool(kstone_item_t* item, const char* key, int value);

// Set null attribute
int kstone_item_set_null(kstone_item_t* item, const char* key);

// Set binary attribute
int kstone_item_set_binary(kstone_item_t* item, const char* key,
                           const unsigned char* data, size_t len);

// Set JSON attribute (for nested objects/arrays)
int kstone_item_set_json(kstone_item_t* item, const char* key, const char* json);
```

### CRUD Operations

```c
// Put an item (partition key only)
int kstone_db_put(kstone_db_t* db, const char* pk, size_t pk_len,
                  kstone_item_t* item);

// Put an item with sort key
int kstone_db_put_with_sk(kstone_db_t* db, const char* pk, size_t pk_len,
                          const char* sk, size_t sk_len, kstone_item_t* item);

// Get an item (returns JSON string, caller must free with kstone_free_string)
char* kstone_db_get(kstone_db_t* db, const char* pk, size_t pk_len);
char* kstone_db_get_with_sk(kstone_db_t* db, const char* pk, size_t pk_len,
                            const char* sk, size_t sk_len);

// Delete an item
int kstone_db_delete(kstone_db_t* db, const char* pk, size_t pk_len);
int kstone_db_delete_with_sk(kstone_db_t* db, const char* pk, size_t pk_len,
                             const char* sk, size_t sk_len);
```

### Query Operations

```c
// Create a query
kstone_query_t* kstone_query_create(const char* pk, size_t pk_len);
void kstone_query_free(kstone_query_t* query);

// Query options
int kstone_query_set_sk_begins_with(kstone_query_t* query, const char* prefix, size_t len);
int kstone_query_set_sk_eq(kstone_query_t* query, const char* value, size_t len);
int kstone_query_set_sk_gt(kstone_query_t* query, const char* value, size_t len);
int kstone_query_set_sk_gte(kstone_query_t* query, const char* value, size_t len);
int kstone_query_set_sk_lt(kstone_query_t* query, const char* value, size_t len);
int kstone_query_set_sk_lte(kstone_query_t* query, const char* value, size_t len);
int kstone_query_set_limit(kstone_query_t* query, int limit);
int kstone_query_set_forward(kstone_query_t* query, int forward);
int kstone_query_set_index(kstone_query_t* query, const char* index_name);

// Execute query
kstone_query_result_t* kstone_db_query(kstone_db_t* db, kstone_query_t* query);
void kstone_query_result_free(kstone_query_result_t* result);

// Access results
int kstone_query_result_count(kstone_query_result_t* result);
char* kstone_query_result_get(kstone_query_result_t* result, int index);
int kstone_query_result_has_more(kstone_query_result_t* result);
```

### Scan Operations

```c
// Create a scan
kstone_scan_t* kstone_scan_create(void);
void kstone_scan_free(kstone_scan_t* scan);

// Scan options
int kstone_scan_set_limit(kstone_scan_t* scan, int limit);
int kstone_scan_set_segment(kstone_scan_t* scan, int segment, int total_segments);

// Execute scan
kstone_scan_result_t* kstone_db_scan(kstone_db_t* db, kstone_scan_t* scan);
void kstone_scan_result_free(kstone_scan_result_t* result);

// Access results (same as query results)
int kstone_scan_result_count(kstone_scan_result_t* result);
char* kstone_scan_result_get(kstone_scan_result_t* result, int index);
```

### Update Operations

```c
// Update an item
char* kstone_db_update(kstone_db_t* db,
                       const char* pk, size_t pk_len,
                       const char* sk, size_t sk_len,  // NULL for no sort key
                       const char* expression,
                       const char* values_json,        // NULL for no values
                       const char* condition);         // NULL for no condition
```

### Batch Operations

```c
// Batch get
kstone_batch_get_t* kstone_batch_get_create(void);
void kstone_batch_get_free(kstone_batch_get_t* batch);
int kstone_batch_get_add_key(kstone_batch_get_t* batch, const char* pk, size_t pk_len);
char* kstone_db_batch_get(kstone_db_t* db, kstone_batch_get_t* batch);

// Batch write
kstone_batch_write_t* kstone_batch_write_create(void);
void kstone_batch_write_free(kstone_batch_write_t* batch);
int kstone_batch_write_add_put(kstone_batch_write_t* batch,
                               const char* pk, size_t pk_len,
                               kstone_item_t* item);
int kstone_batch_write_add_delete(kstone_batch_write_t* batch,
                                  const char* pk, size_t pk_len);
int kstone_db_batch_write(kstone_db_t* db, kstone_batch_write_t* batch);
```

### Execute PartiQL

```c
// Execute SQL statement
char* kstone_db_execute(kstone_db_t* db, const char* sql);
```

### Error Handling

```c
// Error codes
#define KSTONE_OK                0
#define KSTONE_ERR_NULL_PTR     -1
#define KSTONE_ERR_INVALID_KEY  -2
#define KSTONE_ERR_NOT_FOUND    -3
#define KSTONE_ERR_IO           -4
#define KSTONE_ERR_CORRUPTION   -5
#define KSTONE_ERR_INVALID_ARG  -6
#define KSTONE_ERR_INTERNAL     -7
#define KSTONE_ERR_COND_FAILED  -8
#define KSTONE_ERR_TXN_CANCELED -9

// Get last error message (thread-local)
const char* kstone_get_last_error(void);
```

### Memory Management

```c
// Free a string returned by the library
void kstone_free_string(char* s);
```

### Version

```c
// Get version string
const char* kstone_version(void);
```

## Thread Safety

The C-FFI is thread-safe. Each database handle can be used from multiple threads, with internal locking ensuring consistency.

**Important**: Error messages from `kstone_get_last_error()` are thread-local, so each thread sees its own last error.

## Example: Query with Conditions

```c
#include "keystonedb.h"
#include <stdio.h>

int main() {
    kstone_db_t* db = kstone_db_open("mydb.keystone");

    // Create query for user#123 with sort keys starting with "post#"
    kstone_query_t* query = kstone_query_create("user#123", 8);
    kstone_query_set_sk_begins_with(query, "post#", 5);
    kstone_query_set_limit(query, 10);
    kstone_query_set_forward(query, 0);  // Reverse order

    // Execute
    kstone_query_result_t* result = kstone_db_query(db, query);
    if (result) {
        int count = kstone_query_result_count(result);
        printf("Found %d items\n", count);

        for (int i = 0; i < count; i++) {
            char* json = kstone_query_result_get(result, i);
            printf("Item %d: %s\n", i, json);
            kstone_free_string(json);
        }

        kstone_query_result_free(result);
    } else {
        printf("Query failed: %s\n", kstone_get_last_error());
    }

    kstone_query_free(query);
    kstone_db_free(db);
    return 0;
}
```

## Example: Batch Operations

```c
#include "keystonedb.h"
#include <stdio.h>

int main() {
    kstone_db_t* db = kstone_db_create("mydb.keystone");

    // Batch write
    kstone_batch_write_t* batch = kstone_batch_write_create();

    for (int i = 0; i < 100; i++) {
        char key[32];
        snprintf(key, sizeof(key), "item#%d", i);

        kstone_item_t* item = kstone_item_create();
        kstone_item_set_number(item, "id", i);
        kstone_item_set_string(item, "status", "active");

        kstone_batch_write_add_put(batch, key, strlen(key), item);
        kstone_item_free(item);
    }

    int count = kstone_db_batch_write(db, batch);
    printf("Wrote %d items\n", count);

    kstone_batch_write_free(batch);

    // Batch get
    kstone_batch_get_t* get_batch = kstone_batch_get_create();
    kstone_batch_get_add_key(get_batch, "item#1", 7);
    kstone_batch_get_add_key(get_batch, "item#50", 8);
    kstone_batch_get_add_key(get_batch, "item#99", 8);

    char* results = kstone_db_batch_get(db, get_batch);
    if (results) {
        printf("Batch get results: %s\n", results);
        kstone_free_string(results);
    }

    kstone_batch_get_free(get_batch);
    kstone_db_free(db);
    return 0;
}
```

## FFI from Other Languages

The C-FFI can be used from any language with C interoperability:

### Python (ctypes)

```python
import ctypes

lib = ctypes.CDLL("libkeystonedb.so")

# Define return types
lib.kstone_db_create.restype = ctypes.c_void_p
lib.kstone_db_get.restype = ctypes.c_char_p

# Use the library
db = lib.kstone_db_create(b"mydb.keystone")
item = lib.kstone_item_create()
lib.kstone_item_set_string(item, b"name", b"Alice")
lib.kstone_db_put(db, b"user#123", 8, item)
# ...
```

### Ruby (FFI)

```ruby
require 'ffi'

module KeystoneDB
  extend FFI::Library
  ffi_lib 'keystonedb'

  attach_function :kstone_db_create, [:string], :pointer
  attach_function :kstone_db_get, [:pointer, :string, :size_t], :string
  # ...
end

db = KeystoneDB.kstone_db_create("mydb.keystone")
```

### Go (cgo)

```go
package main

// #cgo LDFLAGS: -lkeystonedb
// #include "keystonedb.h"
import "C"
import "unsafe"

func main() {
    path := C.CString("mydb.keystone")
    defer C.free(unsafe.Pointer(path))

    db := C.kstone_db_create(path)
    defer C.kstone_db_free(db)

    // ...
}
```

## License

MIT OR Apache-2.0

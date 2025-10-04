# Chapter 27: Command-Line Interface

The KeystoneDB command-line interface (CLI) provides a simple yet powerful way to interact with your database from the terminal. Built with Rust and leveraging the `clap` library for argument parsing, the `kstone` CLI tool offers a complete set of operations for database management, from basic CRUD operations to advanced PartiQL queries.

## Overview and Installation

The KeystoneDB CLI is distributed as a single binary called `kstone`. This binary provides all the functionality needed to create, manage, and query KeystoneDB databases directly from your command line.

### Building from Source

To build the KeystoneDB CLI from source:

```bash
# Clone the repository
git clone https://github.com/yourusername/keystonedb.git
cd keystonedb

# Build the release binary
cargo build --release

# The binary will be located at:
# target/release/kstone
```

For development and testing, you can build without optimizations:

```bash
# Build debug version (faster compilation, slower execution)
cargo build

# Binary location:
# target/debug/kstone
```

### Installation

After building, you can install the binary to your system:

```bash
# Copy to a directory in your PATH
sudo cp target/release/kstone /usr/local/bin/

# Or create a symlink
sudo ln -s $(pwd)/target/release/kstone /usr/local/bin/kstone

# Verify installation
kstone --help
```

Alternatively, you can run the CLI directly from the target directory without installation:

```bash
./target/release/kstone --help
```

## Command Structure

The KeystoneDB CLI follows a subcommand pattern, where each operation is represented by a specific subcommand. The general syntax is:

```bash
kstone <SUBCOMMAND> [OPTIONS] [ARGUMENTS]
```

Available subcommands include:
- `create` - Create a new database
- `put` - Insert or update an item
- `get` - Retrieve an item by key
- `delete` - Remove an item by key
- `query` - Execute a PartiQL query
- `shell` - Start an interactive REPL session

Each subcommand has its own set of options and arguments, which you can view with:

```bash
kstone <SUBCOMMAND> --help
```

## The `create` Command

The `create` command initializes a new KeystoneDB database at the specified path. This creates a directory structure that will contain the database's WAL (Write-Ahead Log) and SST (Sorted String Table) files.

### Basic Syntax

```bash
kstone create <path>
```

### Parameters

- `<path>` - The file path where the database directory will be created (required)

### Examples

Create a database in the current directory:

```bash
kstone create mydb.keystone
```

Create a database with an absolute path:

```bash
kstone create /var/data/production.keystone
```

Create a database in your home directory:

```bash
kstone create ~/databases/test.keystone
```

### What Happens During Creation

When you run the `create` command, KeystoneDB:

1. Creates a directory at the specified path
2. Initializes an empty WAL file (`wal.log`)
3. Sets up the internal directory structure for SST files
4. Prepares the database for immediate use

After creation, the database is ready to accept read and write operations. The initial state contains no items, and the directory size will be minimal until data is written.

### Error Handling

The `create` command will fail if:
- The specified path already exists
- The parent directory doesn't exist
- You lack write permissions in the target directory
- The filesystem is full or read-only

Error example:

```bash
$ kstone create /restricted/db.keystone
Error: Failed to create database: Permission denied (os error 13)
```

## The `put` Command

The `put` command inserts a new item into the database or updates an existing item with the same key. Items are represented as JSON objects, making it easy to work with structured data from the command line.

### Basic Syntax

```bash
kstone put <path> <key> '<json-item>'
```

### Parameters

- `<path>` - Path to the database directory (required)
- `<key>` - The partition key for the item (required)
- `<json-item>` - The item data as a JSON object (required)

### Examples

Insert a simple item:

```bash
kstone put mydb.keystone user#123 '{"name":"Alice","age":30}'
```

Insert an item with nested attributes:

```bash
kstone put mydb.keystone user#456 '{
  "name": "Bob",
  "email": "bob@example.com",
  "profile": {
    "bio": "Software engineer",
    "location": "San Francisco"
  },
  "tags": ["developer", "rust", "databases"]
}'
```

Insert a numeric item:

```bash
kstone put mydb.keystone counter#global '{"count":0,"updated_at":1704067200}'
```

Insert an item with boolean and null values:

```bash
kstone put mydb.keystone user#789 '{
  "name": "Charlie",
  "active": true,
  "verified": false,
  "deleted_at": null
}'
```

### JSON Value Types

KeystoneDB supports all standard JSON value types, which are automatically converted to the appropriate KeystoneValue types:

- **Strings**: `"Alice"` → `KeystoneValue::S`
- **Numbers**: `42`, `3.14` → `KeystoneValue::N`
- **Booleans**: `true`, `false` → `KeystoneValue::Bool`
- **Null**: `null` → `KeystoneValue::Null`
- **Arrays**: `[1, 2, 3]` → `KeystoneValue::L`
- **Objects**: `{"key": "value"}` → `KeystoneValue::M`

### Shell Quoting Considerations

When using the `put` command, proper shell quoting is essential to prevent the shell from interpreting special characters:

```bash
# Single quotes preserve the JSON exactly
kstone put db.keystone key1 '{"name":"value"}'

# Double quotes require escaping internal quotes
kstone put db.keystone key1 "{\"name\":\"value\"}"

# Multi-line JSON for readability
kstone put db.keystone user#1 '
{
  "name": "Alice",
  "age": 30,
  "email": "alice@example.com"
}
'
```

### Overwrite Behavior

The `put` command uses "upsert" semantics - if an item with the specified key already exists, it will be completely replaced by the new item:

```bash
# Initial insert
kstone put db.keystone user#123 '{"name":"Alice","age":30}'

# This completely replaces the previous item
kstone put db.keystone user#123 '{"name":"Alice","age":31,"city":"NYC"}'

# The old "age" value is gone, replaced by the new complete item
```

### Performance Considerations

Each `put` command:
1. Appends a record to the WAL for durability
2. Updates the in-memory memtable
3. May trigger a flush to SST if the memtable threshold is reached

For bulk inserts, consider using the PartiQL `INSERT` statement with the `query` command, or use the Rust API for better performance.

## The `get` Command

The `get` command retrieves an item from the database by its partition key. The item is returned as formatted JSON to stdout.

### Basic Syntax

```bash
kstone get <path> <key>
```

### Parameters

- `<path>` - Path to the database directory (required)
- `<key>` - The partition key of the item to retrieve (required)

### Examples

Retrieve a single item:

```bash
$ kstone get mydb.keystone user#123
{
  "name": "Alice",
  "age": 30,
  "email": "alice@example.com"
}
```

Retrieve and pipe to `jq` for processing:

```bash
# Extract just the name field
kstone get mydb.keystone user#123 | jq -r '.name'

# Check if email exists
kstone get mydb.keystone user#123 | jq 'has("email")'
```

### Not Found Behavior

If the requested item doesn't exist, the CLI prints a message to stdout:

```bash
$ kstone get mydb.keystone nonexistent
Item not found
```

This makes it easy to check existence in shell scripts:

```bash
if kstone get mydb.keystone user#123 > /dev/null 2>&1; then
  echo "Item exists"
else
  echo "Item not found"
fi
```

### Reading from Different Stripes

KeystoneDB uses a 256-stripe architecture for parallelism. The `get` command automatically:
1. Calculates the stripe ID from the partition key using `crc32(pk) % 256`
2. Routes the read to the appropriate stripe
3. Checks the memtable first, then SSTs from newest to oldest

This routing is transparent to the user - you simply provide the key and KeystoneDB handles the rest.

### Output Format

The `get` command always outputs pretty-printed JSON for human readability. Each JSON type is formatted appropriately:

```bash
# String values
$ kstone get db.keystone key1
{
  "field": "value"
}

# Numeric values (preserved as numbers)
$ kstone get db.keystone key2
{
  "count": 42,
  "price": 19.99
}

# Boolean and null
$ kstone get db.keystone key3
{
  "active": true,
  "deleted_at": null
}

# Nested objects
$ kstone get db.keystone key4
{
  "user": {
    "name": "Alice",
    "profile": {
      "bio": "Developer"
    }
  }
}

# Arrays
$ kstone get db.keystone key5
{
  "tags": ["rust", "database", "storage"]
}
```

### Binary and Special Types

For binary data and special KeystoneValue types:

- **Binary** (`KeystoneValue::B`) - Encoded as Base64 strings
- **Vectors** (`KeystoneValue::VecF32`) - Encoded as JSON arrays of numbers
- **Timestamps** (`KeystoneValue::Ts`) - Encoded as numeric milliseconds since epoch

Example with binary data:

```bash
$ kstone get db.keystone image#1
{
  "name": "avatar.png",
  "data": "iVBORw0KGgoAAAANSUhEUgAAAAUA..."  // Base64 encoded
}
```

## The `delete` Command

The `delete` command removes an item from the database by its partition key. This operation is permanent and cannot be undone (except through backups or point-in-time recovery mechanisms).

### Basic Syntax

```bash
kstone delete <path> <key>
```

### Parameters

- `<path>` - Path to the database directory (required)
- `<key>` - The partition key of the item to delete (required)

### Examples

Delete a single item:

```bash
kstone delete mydb.keystone user#123
Item deleted
```

Delete multiple items in sequence:

```bash
for i in {1..10}; do
  kstone delete mydb.keystone "temp#$i"
done
```

### Delete Semantics

KeystoneDB uses tombstone-based deletion:

1. The `delete` command writes a tombstone record to the WAL
2. The tombstone is added to the memtable
3. During reads, tombstones indicate the item is deleted
4. Tombstones are eventually removed during compaction

This means:
- Deletes are fast (just write a tombstone)
- Disk space isn't immediately reclaimed
- Compaction removes tombstones and reclaims space

### Idempotent Behavior

Deleting a non-existent item succeeds without error:

```bash
$ kstone delete mydb.keystone nonexistent
Item deleted
```

This idempotent behavior simplifies scripting - you don't need to check existence before deletion.

### Silent Deletion

The `delete` command provides minimal output - just a confirmation message. For silent operation in scripts:

```bash
kstone delete mydb.keystone user#123 > /dev/null
```

### Safety Considerations

Since deletion is immediate and permanent, consider:

1. **Backups**: Maintain regular backups before bulk deletions
2. **Testing**: Test deletion scripts on a copy of the database first
3. **Logging**: Log deletions for audit trails
4. **Confirmation**: For interactive use, consider adding confirmation prompts in wrapper scripts

Example safe deletion wrapper:

```bash
#!/bin/bash
# safe-delete.sh

DB_PATH=$1
KEY=$2

echo "About to delete item: $KEY"
read -p "Are you sure? (yes/no): " confirm

if [ "$confirm" = "yes" ]; then
  kstone delete "$DB_PATH" "$KEY"
  echo "Item deleted"
else
  echo "Deletion cancelled"
fi
```

## The `query` Command

The `query` command is the most powerful feature of the KeystoneDB CLI, allowing you to execute PartiQL (SQL-compatible) statements against your database. This enables complex queries, filtering, pagination, and data manipulation using familiar SQL syntax.

### Basic Syntax

```bash
kstone query <path> '<sql-statement>' [OPTIONS]
```

### Parameters

- `<path>` - Path to the database directory (required)
- `<sql-statement>` - PartiQL SQL statement (required)
- `--limit <N>` - Maximum number of items to return (optional)
- `--output <FORMAT>` - Output format: table, json, jsonl, or csv (optional, default: table)

### SELECT Queries

The most common use of the `query` command is to retrieve data using `SELECT` statements.

#### Basic SELECT

Query all attributes for items matching a partition key:

```bash
kstone query mydb.keystone "SELECT * FROM items WHERE pk = 'user#123';"
```

Output:
```
┌─────────────┬───────┬─────┬─────────────────────┐
│ pk          │ name  │ age │ email               │
├─────────────┼───────┼─────┼─────────────────────┤
│ user#123    │ Alice │ 30  │ alice@example.com   │
└─────────────┴───────┴─────┴─────────────────────┘

Count: 1, Scanned: 1
```

#### Projection (Selecting Specific Attributes)

Select only specific attributes:

```bash
kstone query mydb.keystone "SELECT name, email FROM items WHERE pk = 'user#123';"
```

This returns only the `name` and `email` fields, reducing data transfer and improving readability.

#### Filtering with WHERE Clauses

Filter items using various conditions:

```bash
# Age comparison
kstone query mydb.keystone "SELECT * FROM items WHERE age > 25;"

# Multiple conditions
kstone query mydb.keystone "SELECT * FROM items WHERE age > 18 AND active = true;"

# String matching
kstone query mydb.keystone "SELECT * FROM items WHERE status = 'active';"
```

#### Pagination with LIMIT and OFFSET

Control result set size and implement pagination:

```bash
# First page (10 items)
kstone query mydb.keystone "SELECT * FROM items LIMIT 10;"

# Second page (skip first 10, get next 10)
kstone query mydb.keystone "SELECT * FROM items LIMIT 10 OFFSET 10;"

# Third page
kstone query mydb.keystone "SELECT * FROM items LIMIT 10 OFFSET 20;"
```

The `--limit` flag can also be used:

```bash
kstone query mydb.keystone "SELECT * FROM items" --limit 100
```

#### Sorting with ORDER BY

Control the order of results:

```bash
# Ascending order
kstone query mydb.keystone "SELECT * FROM items ORDER BY age ASC;"

# Descending order
kstone query mydb.keystone "SELECT * FROM items ORDER BY created_at DESC;"
```

### INSERT Statements

Insert new items using SQL syntax:

```bash
kstone query mydb.keystone \
  "INSERT INTO items VALUE {'pk': 'user#456', 'name': 'Bob', 'age': 35};"
```

Output:
```
✓ Item inserted successfully
```

Insert with complex nested data:

```bash
kstone query mydb.keystone \
  "INSERT INTO items VALUE {
    'pk': 'user#789',
    'name': 'Charlie',
    'profile': {
      'bio': 'Software engineer',
      'location': 'NYC'
    },
    'tags': ['developer', 'rust']
  };"
```

### UPDATE Statements

Modify existing items with SET operations:

```bash
# Simple update
kstone query mydb.keystone \
  "UPDATE items SET age = 31 WHERE pk = 'user#123';"

# Multiple fields
kstone query mydb.keystone \
  "UPDATE items SET age = 31, city = 'NYC' WHERE pk = 'user#123';"

# Arithmetic operations
kstone query mydb.keystone \
  "UPDATE items SET views = views + 1 WHERE pk = 'post#456';"
```

Output:
```
✓ Item updated successfully

{
  "pk": "user#123",
  "name": "Alice",
  "age": 31,
  "city": "NYC"
}
```

### DELETE Statements

Remove items using SQL syntax:

```bash
kstone query mydb.keystone "DELETE FROM items WHERE pk = 'user#123';"
```

Output:
```
✓ Item deleted successfully
```

Note: PartiQL DELETE statements, like DynamoDB, only support single-item deletes based on partition key. Bulk deletes require iteration:

```bash
# This won't work (no bulk WHERE clause)
kstone query mydb.keystone "DELETE FROM items WHERE age > 100;"

# Instead, use a script to iterate
for key in $(kstone query mydb.keystone "SELECT pk FROM items WHERE age > 100" -o jsonl | jq -r '.pk'); do
  kstone query mydb.keystone "DELETE FROM items WHERE pk = '$key';"
done
```

## Output Formats

The `query` command supports multiple output formats to suit different use cases, from human-readable tables to machine-parseable formats.

### Table Format (Default)

The table format uses the `comfy-table` crate to produce beautifully formatted ASCII tables:

```bash
kstone query mydb.keystone "SELECT * FROM items WHERE pk = 'user#123';" -o table
```

Output:
```
┌─────────────┬───────┬─────┬─────────────────────┬────────┐
│ pk          │ name  │ age │ email               │ active │
├─────────────┼───────┼─────┼─────────────────────┼────────┤
│ user#123    │ Alice │ 30  │ alice@example.com   │ true   │
│ user#456    │ Bob   │ 35  │ bob@example.com     │ true   │
│ user#789    │ Carol │ 28  │ carol@example.com   │ false  │
└─────────────┴───────┴─────┴─────────────────────┴────────┘

Count: 3, Scanned: 3
```

Table format features:
- Automatic column width adjustment
- Header row with attribute names
- Clean borders and separators
- Count and scanned item statistics

Best for:
- Interactive terminal use
- Quick data inspection
- Reports and documentation

### JSON Format

The JSON format outputs results as a pretty-printed JSON array:

```bash
kstone query mydb.keystone "SELECT * FROM items LIMIT 2;" -o json
```

Output:
```json
[
  {
    "pk": "user#123",
    "name": "Alice",
    "age": 30,
    "email": "alice@example.com"
  },
  {
    "pk": "user#456",
    "name": "Bob",
    "age": 35,
    "email": "bob@example.com"
  }
]
```

Best for:
- Consuming results in other programs
- Piping to `jq` for processing
- Generating API responses
- Configuration files

Example with `jq`:

```bash
# Extract all names
kstone query mydb.keystone "SELECT * FROM items" -o json | jq '.[].name'

# Filter and transform
kstone query mydb.keystone "SELECT * FROM items" -o json | \
  jq '[.[] | select(.age > 30) | {name, age}]'

# Count items
kstone query mydb.keystone "SELECT * FROM items" -o json | jq 'length'
```

### JSON Lines Format

JSON Lines (jsonl) format outputs one JSON object per line, without array wrapping:

```bash
kstone query mydb.keystone "SELECT * FROM items" -o jsonl
```

Output:
```json
{"pk":"user#123","name":"Alice","age":30,"email":"alice@example.com"}
{"pk":"user#456","name":"Bob","age":35,"email":"bob@example.com"}
{"pk":"user#789","name":"Carol","age":28,"email":"carol@example.com"}
```

Best for:
- Streaming large datasets
- Log file compatibility
- Line-by-line processing
- Big data pipelines

Example processing:

```bash
# Process each item individually
kstone query mydb.keystone "SELECT * FROM items" -o jsonl | while read line; do
  echo "Processing: $(echo $line | jq -r '.name')"
done

# Filter with grep
kstone query mydb.keystone "SELECT * FROM items" -o jsonl | \
  grep '"active":true'

# Import into another database
kstone query source.keystone "SELECT * FROM items" -o jsonl | \
  while read item; do
    pk=$(echo $item | jq -r '.pk')
    kstone put target.keystone "$pk" "$item"
  done
```

### CSV Format

CSV format outputs comma-separated values with a header row:

```bash
kstone query mydb.keystone "SELECT name, age, email FROM items" -o csv
```

Output:
```csv
age,email,name,pk
30,alice@example.com,Alice,user#123
35,bob@example.com,Bob,user#456
28,carol@example.com,Carol,user#789
```

Features:
- Automatic header generation from all attribute names
- Proper CSV escaping for special characters
- Alphabetically sorted columns for consistency

Best for:
- Importing into spreadsheet applications (Excel, Google Sheets)
- Data analysis with pandas, R, or SQL tools
- Reporting and business intelligence
- Interoperability with legacy systems

Example use cases:

```bash
# Export to file
kstone query mydb.keystone "SELECT * FROM items" -o csv > export.csv

# Import into PostgreSQL
kstone query mydb.keystone "SELECT * FROM items" -o csv | \
  psql -c "COPY temp_table FROM STDIN WITH CSV HEADER"

# Analyze with pandas
kstone query mydb.keystone "SELECT * FROM items" -o csv > data.csv
python3 -c "import pandas as pd; df = pd.read_csv('data.csv'); print(df.describe())"
```

### Handling Complex Types in CSV

Complex types (objects, arrays) are formatted as summary strings in CSV:

```bash
kstone query mydb.keystone "SELECT * FROM items" -o csv
```

Output:
```csv
name,tags,profile
Alice,"[rust,database,storage]","[object]"
```

For complex data, JSON or JSON Lines formats are recommended.

## CLI Best Practices

### Script Integration

The KeystoneDB CLI is designed to work well in shell scripts and automation:

```bash
#!/bin/bash
# backup-users.sh - Export all users to JSON

DB_PATH="production.keystone"
BACKUP_DIR="backups/$(date +%Y%m%d)"

mkdir -p "$BACKUP_DIR"

# Export users
kstone query "$DB_PATH" "SELECT * FROM items WHERE pk LIKE 'user#%'" -o jsonl \
  > "$BACKUP_DIR/users.jsonl"

echo "Backup complete: $(wc -l < $BACKUP_DIR/users.jsonl) users exported"
```

### Error Handling

Always check exit codes in scripts:

```bash
if ! kstone get mydb.keystone user#123 > /dev/null 2>&1; then
  echo "Error: User not found"
  exit 1
fi
```

### Performance Optimization

For bulk operations, minimize CLI invocations:

```bash
# Bad: Many individual commands
for i in {1..1000}; do
  kstone put mydb.keystone "item#$i" "{\"value\":$i}"
done

# Better: Use PartiQL batch or Rust API
# For CLI, batch the input data
cat items.jsonl | while read line; do
  pk=$(echo $line | jq -r '.pk')
  kstone put mydb.keystone "$pk" "$line"
done
```

### Quoting and Escaping

Be careful with shell quoting:

```bash
# Single quotes prevent shell variable expansion
kstone query mydb.keystone 'SELECT * FROM items WHERE pk = "user#123";'

# Double quotes allow variables but need escaping
USER_ID="123"
kstone query mydb.keystone "SELECT * FROM items WHERE pk = \"user#$USER_ID\";"

# Here documents for complex queries
kstone query mydb.keystone <<'EOF'
SELECT name, email, age
FROM items
WHERE age > 18
  AND active = true
LIMIT 100;
EOF
```

### Logging and Debugging

Enable tracing for debugging:

```bash
# Set Rust log level
RUST_LOG=debug kstone query mydb.keystone "SELECT * FROM items"

# Capture stderr for logging
kstone query mydb.keystone "SELECT * FROM items" 2> error.log
```

### Security Considerations

1. **File Permissions**: Ensure database directories have appropriate permissions
   ```bash
   chmod 700 mydb.keystone
   ```

2. **Sensitive Data**: Be cautious with command history containing sensitive data
   ```bash
   # Prefix with space to avoid history (in bash with HISTCONTROL=ignorespace)
    kstone put mydb.keystone secret '{"password":"..."}'

   # Or clear history after sensitive operations
   history -d $(history 1)
   ```

3. **Input Validation**: Validate user input before passing to CLI
   ```bash
   if [[ ! "$USER_INPUT" =~ ^[a-zA-Z0-9_-]+$ ]]; then
     echo "Invalid input"
     exit 1
   fi
   ```

### Monitoring and Observability

Track CLI operations in production:

```bash
#!/bin/bash
# wrapper script with logging

LOG_FILE="/var/log/kstone/operations.log"

log_operation() {
  echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) $USER $*" >> "$LOG_FILE"
}

# Wrapper around kstone
kstone_logged() {
  log_operation "$@"
  kstone "$@"
  local exit_code=$?
  log_operation "Exit code: $exit_code"
  return $exit_code
}

# Use the wrapper
kstone_logged put mydb.keystone user#123 '{"name":"Alice"}'
```

### Backup and Recovery

Regular backups using the CLI:

```bash
#!/bin/bash
# Full database export

DB_PATH="mydb.keystone"
BACKUP_PATH="backup-$(date +%Y%m%d-%H%M%S).jsonl"

# Export all items
kstone query "$DB_PATH" "SELECT * FROM items" -o jsonl > "$BACKUP_PATH"

# Compress
gzip "$BACKUP_PATH"

# Verify
gunzip -c "$BACKUP_PATH.gz" | wc -l
echo "Backup complete: $BACKUP_PATH.gz"
```

Restore from backup:

```bash
#!/bin/bash
# Restore from JSONL backup

BACKUP_FILE="backup-20250103-120000.jsonl.gz"
DB_PATH="restored.keystone"

# Create new database
kstone create "$DB_PATH"

# Restore items
gunzip -c "$BACKUP_FILE" | while read line; do
  pk=$(echo "$line" | jq -r '.pk')
  kstone put "$DB_PATH" "$pk" "$line"
done

echo "Restore complete"
```

### CI/CD Integration

Example GitHub Actions workflow:

```yaml
name: Database Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2

      - name: Build KeystoneDB CLI
        run: cargo build --release

      - name: Run database tests
        run: |
          # Create test database
          ./target/release/kstone create test.keystone

          # Insert test data
          ./target/release/kstone put test.keystone user#1 '{"name":"Alice"}'

          # Verify
          RESULT=$(./target/release/kstone get test.keystone user#1)
          echo "$RESULT" | grep -q "Alice" || exit 1

          # Query test
          ./target/release/kstone query test.keystone "SELECT * FROM items" -o json
```

## Summary

The KeystoneDB command-line interface provides a complete toolkit for database operations:

- **Simple Commands**: `create`, `put`, `get`, `delete` for basic operations
- **Powerful Queries**: PartiQL support with `query` command
- **Flexible Output**: Table, JSON, JSONL, and CSV formats
- **Script-Friendly**: Designed for automation and integration
- **Production-Ready**: Error handling, logging, and monitoring support

Whether you're exploring data interactively, building automation scripts, or integrating with existing systems, the KeystoneDB CLI offers the tools you need to work efficiently with your data.

For interactive exploration and advanced features like autocomplete and history, see the next chapter on the Interactive Shell (REPL).

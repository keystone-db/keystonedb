# Chapter 28: Interactive Shell (REPL)

The KeystoneDB Interactive Shell provides a powerful REPL (Read-Eval-Print Loop) environment for working with your database. Built on top of the `rustyline` library, the shell offers a rich interactive experience with features like command history, autocomplete, multi-line queries, and customizable output formatting.

## Starting the Interactive Shell

The interactive shell is launched using the `shell` subcommand of the `kstone` CLI:

```bash
kstone shell <path>
```

Where `<path>` is the file path to your KeystoneDB database directory. If the path is omitted or specified as `:memory:`, the shell creates an in-memory database:

```bash
# Open existing database
kstone shell mydb.keystone

# Create temporary in-memory database
kstone shell :memory:

# Explicit in-memory mode
kstone shell
```

### Welcome Banner

When you start the shell, you're greeted with a welcome banner displaying important information:

```
╔═══════════════════════════════════════════════════════╗
║                                                       ║
║         KeystoneDB Interactive Shell v0.1.0           ║
║                                                       ║
║  Database: mydb.keystone                              ║
║                                                       ║
║  Quick Start:                                         ║
║    .help           - Show all commands                ║
║    .format <type>  - Change output (table|json|compact)║
║    .exit           - Exit shell                       ║
║                                                       ║
╚═══════════════════════════════════════════════════════╝

  Tip: Multi-line queries supported. End with ; to execute.

kstone>
```

The banner includes:
- Shell version information
- Database path (or `:memory:` indicator)
- Quick reference to essential commands
- Usage tips based on database mode

### In-Memory Mode

When using in-memory mode, the shell displays a special note:

```
  Note: In-memory mode - data is temporary and will be lost on exit.
```

In-memory databases are perfect for:
- Testing and experimentation
- Temporary data processing
- Learning PartiQL syntax
- Prototyping queries before running on production databases

All PartiQL features are available in memory mode, providing the full KeystoneDB experience without disk persistence.

## PartiQL Query Interface

The primary purpose of the interactive shell is to execute PartiQL queries. The shell provides a complete SQL-like query interface with full support for KeystoneDB's DynamoDB-compatible API.

### Basic Queries

Execute queries by typing them at the prompt and ending with a semicolon:

```sql
kstone> SELECT * FROM items WHERE pk = 'user#123';
```

The shell executes the query and displays results using the current output format (table by default):

```
┌─────────────┬───────┬─────┬─────────────────────┐
│ pk          │ name  │ age │ email               │
├─────────────┼───────┼─────┼─────────────────────┤
│ user#123    │ Alice │ 30  │ alice@example.com   │
└─────────────┴───────┴─────┴─────────────────────┘

1 row (12.3ms)
```

### Query Types Supported

The shell supports all PartiQL statement types:

#### SELECT Queries

```sql
-- All attributes
kstone> SELECT * FROM items WHERE pk = 'user#123';

-- Specific attributes (projection)
kstone> SELECT name, email FROM items WHERE pk = 'user#123';

-- With filtering
kstone> SELECT * FROM items WHERE age > 25;

-- With pagination
kstone> SELECT * FROM items LIMIT 10 OFFSET 20;

-- With ordering
kstone> SELECT * FROM items ORDER BY created_at DESC;
```

#### INSERT Statements

```sql
kstone> INSERT INTO items VALUE {
  'pk': 'user#456',
  'name': 'Bob',
  'age': 35,
  'email': 'bob@example.com'
};
✓ Item inserted successfully
```

#### UPDATE Statements

```sql
kstone> UPDATE items SET age = 31 WHERE pk = 'user#123';
✓ Item updated successfully

{
  "pk": "user#123",
  "name": "Alice",
  "age": 31
}
```

With arithmetic operations:

```sql
kstone> UPDATE items SET views = views + 1, last_viewed = 1704067200
        WHERE pk = 'post#789';
```

#### DELETE Statements

```sql
kstone> DELETE FROM items WHERE pk = 'user#123';
✓ Item deleted successfully
```

### Query Results and Timing

By default, the shell displays query timing information after each query:

```
1 row (12.3ms)
```

This shows:
- Number of rows returned
- Query execution time in milliseconds

The timing includes:
- Query parsing and validation
- Database operation time
- Result formatting (minimal)

Timing can be toggled on or off with the `.timer` command.

## Meta-Commands

Meta-commands are special shell commands that start with a dot (`.`) and control shell behavior. Unlike PartiQL queries, meta-commands don't require a semicolon and are always single-line.

### `.help` - Display Help Information

The `.help` command displays comprehensive information about available commands:

```
kstone> .help

Available Commands:

  Meta-commands:
    .help              Show this help message
    .exit, .quit       Exit the shell
    .schema            Display database schema
    .indexes           List all indexes (LSI/GSI)
    .format <type>     Set output format (table|json|compact)
    .timer <on|off>    Toggle query timing display
    .clear             Clear the screen

  SQL Queries:
    SELECT * FROM items WHERE pk = 'key';
    INSERT INTO items VALUE {'pk': 'key', 'name': 'Alice'};
    UPDATE items SET age = 30 WHERE pk = 'key';
    DELETE FROM items WHERE pk = 'key';

  Multi-line Queries:
    Queries without a semicolon will continue on the next line.
    Use Ctrl+C to cancel a multi-line query.

  Keyboard Shortcuts:
    Ctrl+C             Cancel current input
    Ctrl+D             Exit shell
    Up/Down Arrow      Navigate command history
    Tab                Autocomplete commands and keywords
```

The help screen is organized into sections:
- **Meta-commands**: Shell control commands
- **SQL Queries**: PartiQL syntax examples
- **Multi-line Queries**: Instructions for multi-line input
- **Keyboard Shortcuts**: Editor and navigation keys

### `.exit` and `.quit` - Exit the Shell

Exit the interactive shell and return to your terminal:

```
kstone> .exit

Thanks for using KeystoneDB!
  Session saved. Your command history has been preserved.
```

Both `.exit` and `.quit` are equivalent. You can also exit with:
- `Ctrl+D` (EOF signal)
- Typing `exit` or `quit` (though `.exit` is preferred for clarity)

When you exit:
1. Command history is saved to `~/.keystone_history`
2. Any uncommitted data in memory is flushed to disk (for disk-based databases)
3. The shell displays a goodbye message
4. Control returns to your terminal

### `.schema` - Display Database Schema

The `.schema` command shows information about your database:

```
kstone> .schema

Database Schema:
  Path: mydb.keystone

  Storage:
    SST files: 12
    WAL: present
    Total size: 4096000 bytes

  Note:
    Full schema inspection (table schema, partition/sort keys)
    will be available in Phase 3+
```

For in-memory databases:

```
kstone> .schema

Database Schema:
  Path: :memory:

  Mode:
    In-memory database (no disk persistence)
    All data will be lost when shell exits
    Full PartiQL support available
```

The schema command provides:
- Database path and type
- File system statistics (for disk databases)
- Storage usage information
- Mode-specific details

Future enhancements will include:
- Table schema details (partition key, sort key)
- Index definitions (LSI, GSI)
- TTL configuration
- Stream settings

### `.indexes` - List Indexes

Display information about secondary indexes:

```
kstone> .indexes

Indexes:

  Current Phase:
    KeystoneDB is currently in Phase 0 (Walking Skeleton)
    Indexes are not yet implemented.

  Coming in Phase 3:
    • Local Secondary Indexes (LSI) - Alternate sort keys
    • Global Secondary Indexes (GSI) - Alternate partition keys
    • Full-text search indexes
    • Vector similarity indexes
```

Once indexes are implemented (Phase 3+), this command will show:

```
kstone> .indexes

Local Secondary Indexes:
  - email-index (sort_key: email, projection: ALL)
  - score-index (sort_key: score, projection: KEYS_ONLY)

Global Secondary Indexes:
  - status-index (partition_key: status, projection: ALL)
  - category-price-index (pk: category, sk: price, projection: INCLUDE [name, description])
```

### `.format` - Set Output Format

Change the output format for query results:

```
kstone> .format json
Output format set to: Json
```

Available formats:
- `table` - ASCII table with borders (default)
- `json` - Pretty-printed JSON array
- `compact` - Inline key-value pairs

Examples of each format:

**Table Format:**
```
kstone> .format table
kstone> SELECT * FROM items WHERE pk = 'user#123';
┌─────────────┬───────┬─────┐
│ pk          │ name  │ age │
├─────────────┼───────┼─────┤
│ user#123    │ Alice │ 30  │
└─────────────┴───────┴─────┘
```

**JSON Format:**
```
kstone> .format json
kstone> SELECT * FROM items WHERE pk = 'user#123';
[
  {
    "pk": "user#123",
    "name": "Alice",
    "age": 30
  }
]
```

**Compact Format:**
```
kstone> .format compact
kstone> SELECT * FROM items WHERE pk = 'user#123';
[1] pk="user#123", name="Alice", age=30
```

The format setting persists for the entire shell session. Use the format that best suits your task:
- **Table**: Best for visual inspection and human readability
- **JSON**: Best for copying results to other tools or documentation
- **Compact**: Best for quickly scanning large result sets

### `.timer` - Toggle Query Timing

Control whether query execution time is displayed:

```
kstone> .timer off
Timer disabled

kstone> SELECT * FROM items WHERE pk = 'user#123';
┌─────────────┬───────┬─────┐
│ pk          │ name  │ age │
├─────────────┼───────┼─────┤
│ user#123    │ Alice │ 30  │
└─────────────┴───────┴─────┘

kstone> .timer on
Timer enabled

kstone> SELECT * FROM items WHERE pk = 'user#123';
┌─────────────┬───────┬─────┐
│ pk          │ name  │ age │
├─────────────┼───────┼─────┤
│ user#123    │ Alice │ 30  │
└─────────────┴───────┴─────┘

1 row (8.2ms)
```

Timer is enabled by default. Disable it when:
- Taking screenshots for documentation
- Copying output to reports
- Focusing solely on data content

### `.clear` - Clear the Screen

Clear the terminal screen:

```
kstone> .clear
```

This executes the ANSI clear screen sequence (`\x1B[2J\x1B[1;1H`), moving the cursor to the top-left and clearing all content. Useful for decluttering during long sessions.

## Autocomplete Features

The KeystoneDB shell includes intelligent autocomplete powered by `rustyline`. Press `Tab` to trigger completion at any time.

### Meta-Command Completion

When typing a meta-command, Tab completes from available commands:

```
kstone> .he<Tab>
kstone> .help

kstone> .f<Tab>
kstone> .format
```

If multiple matches exist, they're displayed:

```
kstone> .e<Tab>
.exit  .echo

kstone> .ex<Tab>
kstone> .exit
```

### PartiQL Keyword Completion

When writing queries, Tab completes SQL keywords:

```
kstone> SEL<Tab>
kstone> SELECT

kstone> SELECT * FROM items WH<Tab>
kstone> SELECT * FROM items WHERE
```

Supported keywords include:
- `SELECT`, `FROM`, `WHERE`, `LIMIT`, `OFFSET`, `ORDER BY`
- `INSERT`, `INTO`, `VALUE`
- `UPDATE`, `SET`
- `DELETE`
- `AND`, `OR`, `NOT`
- Table name: `items`

Keywords are completed case-insensitively but rendered in uppercase:

```
kstone> sel<Tab>
kstone> SELECT
```

### Context-Aware Completion

The autocomplete engine is context-aware, completing based on cursor position:

```
kstone> SELECT * FROM items WH<Tab>
Suggestions: WHERE

kstone> SELECT * FROM items WHERE pk = 'user#123' LI<Tab>
Suggestions: LIMIT

kstone> SELECT * FROM items WHERE pk = 'user#123' AND a<Tab>
Suggestions: AND
```

This helps you write queries faster and reduces syntax errors.

### Completion Algorithm

The completion system:
1. Identifies word boundaries (whitespace, operators)
2. Extracts the partial word before the cursor
3. Searches for matches in the appropriate dictionary (meta-commands or keywords)
4. Returns matches with the same prefix
5. If exactly one match, auto-completes; if multiple, shows options

## Command History

The interactive shell maintains a persistent command history across sessions, making it easy to recall and reuse previous queries.

### History File

Command history is stored in:
```
~/.keystone_history
```

This file contains all commands entered across all shell sessions (up to the configured limit, typically 1000 entries).

### Navigating History

Use arrow keys to navigate through command history:

- **Up Arrow** (`↑`): Previous command
- **Down Arrow** (`↓`): Next command

Example session:

```
kstone> SELECT * FROM items WHERE pk = 'user#123';
...

kstone> INSERT INTO items VALUE {'pk': 'user#456', 'name': 'Bob'};
...

kstone> ↑
kstone> INSERT INTO items VALUE {'pk': 'user#456', 'name': 'Bob'};

kstone> ↑
kstone> SELECT * FROM items WHERE pk = 'user#123';
```

### Searching History

While `rustyline` provides reverse-search capabilities in some configurations, the KeystoneDB shell currently uses basic up/down navigation. To find a previous command:

1. Press `↑` repeatedly to scroll through history
2. Modify and re-execute the command as needed

### History Persistence

History is automatically:
- **Loaded** when the shell starts
- **Saved** when the shell exits normally (`.exit`, `Ctrl+D`)
- **Updated** incrementally during the session

If the shell crashes or is killed, the current session's history may be lost.

### Privacy Considerations

Since history is persisted to disk, be cautious when entering sensitive data:

```
kstone> INSERT INTO items VALUE {'pk': 'secret#1', 'password': 'secret123'};
```

This command, including the password, will be saved to `~/.keystone_history`. For sensitive operations:

1. Consider using the non-interactive CLI instead
2. Clear history after sensitive commands (manually edit `~/.keystone_history`)
3. Set restrictive file permissions: `chmod 600 ~/.keystone_history`

## Multi-line Query Support

The KeystoneDB shell supports multi-line query input, allowing you to write complex queries across multiple lines for better readability.

### How Multi-line Works

A query spans multiple lines until a semicolon (`;`) is encountered:

```
kstone> SELECT name, email, age
...>   FROM items
...>   WHERE age > 25
...>     AND active = true
...>   LIMIT 10;
```

The shell displays a continuation prompt (`...>`) for each subsequent line.

### When to Use Multi-line

Multi-line input is ideal for:
- Complex queries with multiple conditions
- Queries with long attribute lists
- Formatting for readability
- Queries you want to document or share

Example - readable INSERT:

```
kstone> INSERT INTO items VALUE {
...>   'pk': 'user#789',
...>   'name': 'Charlie',
...>   'email': 'charlie@example.com',
...>   'profile': {
...>     'bio': 'Software engineer',
...>     'location': 'San Francisco',
...>     'interests': ['rust', 'databases', 'distributed systems']
...>   },
...>   'created_at': 1704067200,
...>   'active': true
...> };
✓ Item inserted successfully
```

### Cancelling Multi-line Input

If you make a mistake or want to abandon a multi-line query:

```
kstone> SELECT * FROM items WHERE
...>   pk = 'user#123' AND
...>   age > 30 AND
...> ^C

kstone>
```

Press `Ctrl+C` to cancel and return to a fresh prompt. The accumulated input is discarded.

### Meta-Commands are Single-line

Meta-commands are always single-line and don't support continuation:

```
kstone> .format
...>   table
Error: Unknown command: .format

kstone> .format table
Output format set to: Table
```

### Semicolon Rules

- **PartiQL queries**: MUST end with `;` to execute
- **Meta-commands**: NEVER use `;` (single-line only)

Examples:

```
kstone> SELECT * FROM items WHERE pk = 'user#123'
...>
...> (waiting for more input)

kstone> SELECT * FROM items WHERE pk = 'user#123';
(executes immediately)

kstone> .help;
Error: Unknown command: .help;

kstone> .help
(displays help immediately)
```

### Line Accumulation

The shell accumulates lines in an internal buffer:

```
Line 1: SELECT * FROM items
Line 2:   WHERE pk = 'user#123'
Line 3:   AND age > 30;

Executed as: "SELECT * FROM items WHERE pk = 'user#123' AND age > 30;"
```

Lines are joined with spaces. Indentation is preserved in the accumulator but doesn't affect query execution.

## Result Formatting Options

The shell provides three distinct output formats, each optimized for different use cases. You can switch between formats using the `.format` command.

### Table Format

Table format uses the `comfy-table` crate to render results as ASCII tables.

**Features:**
- Automatic column width adjustment
- Box-drawing characters for borders
- Header row with attribute names
- Handles nested objects and arrays gracefully

**Example:**

```
kstone> .format table
kstone> SELECT * FROM items WHERE pk = 'user#123';
┌─────────────┬───────┬─────┬─────────────────────┬──────────────────┐
│ pk          │ name  │ age │ email               │ tags             │
├─────────────┼───────┼─────┼─────────────────────┼──────────────────┤
│ user#123    │ Alice │ 30  │ alice@example.com   │ [developer,rust] │
└─────────────┴───────┴─────┴─────────────────────┴──────────────────┘

1 row (8.5ms)
```

**Advantages:**
- Highly readable for interactive use
- Easy to scan visually
- Professional appearance for screenshots and documentation

**Limitations:**
- Not machine-parseable
- Wide tables may wrap on narrow terminals
- Complex nested data shown as summaries

### JSON Format

JSON format outputs results as a pretty-printed JSON array.

**Features:**
- Standard JSON syntax
- Proper nesting and indentation
- Preserves all data types
- Compatible with JSON parsers

**Example:**

```
kstone> .format json
kstone> SELECT * FROM items WHERE pk = 'user#123';
[
  {
    "pk": "user#123",
    "name": "Alice",
    "age": 30,
    "email": "alice@example.com",
    "tags": ["developer", "rust"],
    "profile": {
      "bio": "Software engineer",
      "location": "San Francisco"
    }
  }
]

1 row (7.2ms)
```

**Advantages:**
- Complete data representation (including nested structures)
- Easy to copy and paste
- Can be piped to `jq` or other JSON tools
- Standard format for APIs and data exchange

**Limitations:**
- More verbose than compact format
- Harder to scan visually for large datasets
- Timing information appears after JSON (may interrupt formatting)

**Use Cases:**
- Exporting query results
- Sharing data with other developers
- Processing with JSON tools
- Debugging complex nested data

### Compact Format

Compact format displays results as inline key-value pairs.

**Features:**
- One row per line
- Color-coded output (keys in cyan, values in default)
- Summarizes complex types
- Minimal whitespace

**Example:**

```
kstone> .format compact
kstone> SELECT * FROM items LIMIT 3;
[1] pk="user#123", name="Alice", age=30, email="alice@example.com"
[2] pk="user#456", name="Bob", age=35, email="bob@example.com"
[3] pk="user#789", name="Carol", age=28, email="carol@example.com"

3 rows (11.3ms)
```

**Complex Types:**

```
kstone> SELECT * FROM items WHERE pk = 'user#123';
[1] pk="user#123", name="Alice", tags=[2 items], profile={3 fields}, avatar=<1024 bytes>

1 row (9.1ms)
```

**Advantages:**
- Compact output for large result sets
- Quick scanning of many rows
- Fits more data on screen
- Good for log-like viewing

**Limitations:**
- Not machine-parseable
- Complex types shown as summaries only
- Less detail than JSON format

**Use Cases:**
- Quickly scanning large datasets
- Monitoring or log-like workflows
- Terminal with limited vertical space
- Focusing on specific attributes

### Timing Display

Regardless of format, timing information appears after results (if enabled):

```
3 rows (11.3ms)
```

Format:
- `N row` or `N rows` (count)
- Execution time in milliseconds with 2 decimal places

Toggle with `.timer on` or `.timer off`.

## Interactive Session Examples

Let's walk through several common interactive sessions to demonstrate the shell's capabilities.

### Exploratory Data Analysis

```
kstone> kstone shell analytics.keystone

KeystoneDB Interactive Shell v0.1.0
Database: analytics.keystone

kstone> .schema
Database Schema:
  Path: analytics.keystone
  Storage:
    SST files: 23
    WAL: present
    Total size: 15728640 bytes

kstone> SELECT * FROM items LIMIT 5;
┌────────────────┬──────────┬─────────────────┬────────┐
│ pk             │ event    │ timestamp       │ user   │
├────────────────┼──────────┼─────────────────┼────────┤
│ event#1        │ login    │ 1704000000000   │ alice  │
│ event#2        │ purchase │ 1704003600000   │ bob    │
│ event#3        │ logout   │ 1704007200000   │ alice  │
│ event#4        │ login    │ 1704010800000   │ carol  │
│ event#5        │ view     │ 1704014400000   │ alice  │
└────────────────┴──────────┴─────────────────┴────────┘

5 rows (14.2ms)

kstone> SELECT event, COUNT(*) as count
...>   FROM items
...>   GROUP BY event
...>   ORDER BY count DESC;
┌──────────┬────────┐
│ event    │ count  │
├──────────┼────────┤
│ view     │ 1523   │
│ login    │ 456    │
│ purchase │ 234    │
│ logout   │ 421    │
└──────────┴────────┘

4 rows (45.7ms)

kstone> .format json
kstone> SELECT * FROM items WHERE event = 'purchase' LIMIT 2;
[
  {
    "pk": "event#2",
    "event": "purchase",
    "timestamp": 1704003600000,
    "user": "bob",
    "amount": 49.99,
    "product": "widget"
  },
  {
    "pk": "event#7",
    "event": "purchase",
    "timestamp": 1704025200000,
    "user": "alice",
    "amount": 29.99,
    "product": "gadget"
  }
]

2 rows (8.9ms)

kstone> .exit
```

### Data Migration Session

```
kstone> kstone shell migration-target.keystone

kstone> .format json

kstone> INSERT INTO items VALUE {
...>   'pk': 'user#1000',
...>   'name': 'David',
...>   'email': 'david@example.com',
...>   'migrated_from': 'legacy_db',
...>   'migrated_at': 1704067200
...> };
✓ Item inserted successfully

kstone> SELECT * FROM items WHERE pk = 'user#1000';
[
  {
    "pk": "user#1000",
    "name": "David",
    "email": "david@example.com",
    "migrated_from": "legacy_db",
    "migrated_at": 1704067200
  }
]

1 row (6.2ms)

kstone> UPDATE items
...>   SET verified = true, verification_date = 1704070800
...>   WHERE pk = 'user#1000';
✓ Item updated successfully

{
  "pk": "user#1000",
  "name": "David",
  "email": "david@example.com",
  "migrated_from": "legacy_db",
  "migrated_at": 1704067200,
  "verified": true,
  "verification_date": 1704070800
}

kstone> .exit
```

### Debugging Session

```
kstone> kstone shell production.keystone

kstone> .timer on
kstone> .format table

kstone> SELECT * FROM items WHERE pk = 'user#bug-report';
┌──────────────────┬───────┬──────────┬────────────┐
│ pk               │ name  │ status   │ created_at │
├──────────────────┼───────┼──────────┼────────────┤
│ user#bug-report  │ Test  │ pending  │ 1704000000 │
└──────────────────┴───────┴──────────┴────────────┘

1 row (15.8ms)

kstone> .format json

kstone> SELECT * FROM items WHERE pk = 'user#bug-report';
[
  {
    "pk": "user#bug-report",
    "name": "Test",
    "status": "pending",
    "created_at": 1704000000,
    "metadata": {
      "source": "web",
      "ip": "192.168.1.100",
      "user_agent": "Mozilla/5.0..."
    }
  }
]

1 row (12.1ms)

kstone> DELETE FROM items WHERE pk = 'user#bug-report';
✓ Item deleted successfully

kstone> SELECT * FROM items WHERE pk = 'user#bug-report';
[]

0 rows (4.3ms)

kstone> .exit
```

### Prototyping Session

```
kstone> kstone shell :memory:

KeystoneDB Interactive Shell v0.1.0
Database: :memory:

Note: In-memory mode - data is temporary and will be lost on exit.

kstone> INSERT INTO items VALUE {
...>   'pk': 'product#1',
...>   'name': 'Widget',
...>   'price': 19.99,
...>   'stock': 100,
...>   'categories': ['hardware', 'tools']
...> };
✓ Item inserted successfully

kstone> INSERT INTO items VALUE {
...>   'pk': 'product#2',
...>   'name': 'Gadget',
...>   'price': 29.99,
...>   'stock': 50,
...>   'categories': ['electronics', 'tools']
...> };
✓ Item inserted successfully

kstone> SELECT name, price, stock FROM items;
┌─────────┬────────┬────────┐
│ name    │ price  │ stock  │
├─────────┼────────┼────────┤
│ Widget  │ 19.99  │ 100    │
│ Gadget  │ 29.99  │ 50     │
└─────────┴────────┴────────┘

2 rows (5.1ms)

kstone> UPDATE items SET stock = stock - 1 WHERE pk = 'product#1';
✓ Item updated successfully

kstone> SELECT name, stock FROM items WHERE pk = 'product#1';
┌─────────┬────────┐
│ name    │ stock  │
├─────────┼────────┤
│ Widget  │ 99     │
└─────────┴────────┘

1 row (4.8ms)

kstone> .exit

Thanks for using KeystoneDB!
```

## Summary

The KeystoneDB Interactive Shell provides a rich environment for database exploration and management:

**Key Features:**
- Professional REPL interface with `rustyline`
- Full PartiQL query support (SELECT, INSERT, UPDATE, DELETE)
- Comprehensive meta-commands for shell control
- Intelligent autocomplete for commands and keywords
- Persistent command history across sessions
- Multi-line query support for complex statements
- Three output formats (table, JSON, compact)
- In-memory mode for temporary databases

**Best Practices:**
- Use `.help` to discover features
- Enable `.timer` to monitor query performance
- Switch `.format` based on your task
- Leverage history with arrow keys
- Use multi-line for complex queries
- Try `:memory:` mode for experimentation

**Use Cases:**
- Interactive data exploration
- Query prototyping and testing
- Data debugging and inspection
- Ad-hoc data manipulation
- Learning PartiQL syntax
- Database migration verification

The interactive shell complements the CLI by providing a conversational interface for working with your data. Whether you're debugging production issues, exploring datasets, or prototyping new queries, the shell offers the tools you need for efficient database interaction.

For programmatic access and automation, see Chapter 29 for information on in-memory mode, or refer to the Rust API documentation for building applications with KeystoneDB.

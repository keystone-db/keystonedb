# Chapter 2: Quick Start Guide

## Your First Database in 5 Minutes

This chapter will get you up and running with KeystoneDB in just five minutes. By the end, you'll have created a database, inserted data, run queries, and understood the core concepts through hands-on examples.

## Prerequisites

Before we begin, ensure you have:

1. **Rust toolchain** installed (1.70 or later)
2. **Git** for cloning the repository
3. **Terminal** access to run commands

If you don't have Rust installed, visit [rustup.rs](https://rustup.rs/) and follow the installation instructions. It takes about 2 minutes.

## Step 1: Get KeystoneDB (1 minute)

Clone the repository and build the CLI tool:

```bash
# Clone the repository
git clone https://github.com/yourusername/keystonedb.git
cd keystonedb

# Build in release mode for best performance
cargo build --release

# The CLI binary is now at target/release/kstone
# Optionally, add it to your PATH or create an alias
alias kstone="$(pwd)/target/release/kstone"
```

The build process downloads dependencies and compiles the project. On a modern machine, this typically takes 1-2 minutes.

**Verify the installation**:

```bash
kstone --help
```

You should see output listing available commands:

```
KeystoneDB CLI

Usage: kstone <COMMAND>

Commands:
  create  Create a new database
  put     Put an item
  get     Get an item
  delete  Delete an item
  query   Execute a PartiQL query
  shell   Start interactive shell
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## Step 2: Create Your First Database (30 seconds)

Create a new database directory:

```bash
kstone create myapp.keystone
```

Output:
```
Database created: myapp.keystone
```

Let's see what was created:

```bash
ls -la myapp.keystone/
```

Output:
```
total 8
drwxr-xr-x  3 user  staff    96 Oct  3 10:00 .
drwxr-xr-x  5 user  staff   160 Oct  3 10:00 ..
-rw-r--r--  1 user  staff  4096 Oct  3 10:00 wal.log
```

The database starts with just a Write-Ahead Log (WAL). SST files will be created automatically as you add data.

**What happened?**
- Created a directory named `myapp.keystone`
- Initialized an empty Write-Ahead Log for durability
- Set up internal metadata structures

The `.keystone` extension helps identify database directories, but it's just a convention - any path works.

## Step 3: Insert Your First Item (1 minute)

KeystoneDB uses a document model similar to DynamoDB. Each item has a partition key and optional attributes stored as JSON.

**Insert a user**:

```bash
kstone put myapp.keystone user#alice '{
  "name": "Alice Johnson",
  "email": "alice@example.com",
  "age": 30,
  "active": true
}'
```

Output:
```
Item inserted successfully
```

**What happened?**
- Partition key: `user#alice` (uniquely identifies this item)
- Attributes: `name`, `email`, `age`, `active` (stored with their types)
- The item was written to the WAL and added to an in-memory memtable

**Insert more users**:

```bash
kstone put myapp.keystone user#bob '{
  "name": "Bob Smith",
  "email": "bob@example.com",
  "age": 25,
  "active": true
}'

kstone put myapp.keystone user#charlie '{
  "name": "Charlie Brown",
  "email": "charlie@example.com",
  "age": 35,
  "active": false
}'
```

**Insert items with sort keys** (composite keys):

```bash
# User profile (base record)
kstone put myapp.keystone user#alice profile '{
  "bio": "Software engineer passionate about databases",
  "location": "San Francisco, CA"
}'

# User settings (separate item, same partition)
kstone put myapp.keystone user#alice settings '{
  "theme": "dark",
  "notifications": true,
  "language": "en"
}'
```

Notice how `user#alice` acts as the partition key, and `profile`/`settings` are sort keys. This allows you to efficiently query all items for a specific user.

## Step 4: Retrieve Data (1 minute)

**Get a single item by key**:

```bash
kstone get myapp.keystone user#alice
```

Output (formatted for readability):
```json
{
  "name": "Alice Johnson",
  "email": "alice@example.com",
  "age": 30,
  "active": true
}
```

**Get item with sort key**:

```bash
kstone get myapp.keystone user#alice profile
```

Output:
```json
{
  "bio": "Software engineer passionate about databases",
  "location": "San Francisco, CA"
}
```

**What if an item doesn't exist?**

```bash
kstone get myapp.keystone user#david
```

Output:
```
Item not found
```

KeystoneDB returns a clear message when the item doesn't exist, making debugging easier.

## Step 5: Run Queries with PartiQL (1 minute)

KeystoneDB supports PartiQL, a SQL-like query language that makes querying intuitive.

**Query all users**:

```bash
kstone query myapp.keystone "SELECT * FROM items WHERE pk BEGINS WITH 'user#'"
```

Output (table format):
```
┌──────────────┬─────────────────┬───────────────────────┬─────┬────────┐
│ pk           │ name            │ email                 │ age │ active │
├──────────────┼─────────────────┼───────────────────────┼─────┼────────┤
│ user#alice   │ Alice Johnson   │ alice@example.com     │ 30  │ true   │
│ user#bob     │ Bob Smith       │ bob@example.com       │ 25  │ true   │
│ user#charlie │ Charlie Brown   │ charlie@example.com   │ 35  │ false  │
└──────────────┴─────────────────┴───────────────────────┴─────┴────────┘
3 rows
```

**Query with filtering**:

```bash
kstone query myapp.keystone "SELECT name, email FROM items WHERE pk BEGINS WITH 'user#' AND age > 25"
```

Output:
```
┌───────────────┬───────────────────────┐
│ name          │ email                 │
├───────────────┼───────────────────────┤
│ Alice Johnson │ alice@example.com     │
│ Charlie Brown │ charlie@example.com   │
└───────────────┴───────────────────────┘
2 rows
```

**Query with limit**:

```bash
kstone query myapp.keystone "SELECT * FROM items WHERE pk BEGINS WITH 'user#' LIMIT 2"
```

**Get JSON output** (useful for scripts):

```bash
kstone query myapp.keystone "SELECT * FROM items WHERE pk BEGINS WITH 'user#'" -o json
```

Output:
```json
[
  {
    "pk": "user#alice",
    "name": "Alice Johnson",
    "email": "alice@example.com",
    "age": 30,
    "active": true
  },
  {
    "pk": "user#bob",
    "name": "Bob Smith",
    "email": "bob@example.com",
    "age": 25,
    "active": true
  }
]
```

## Step 6: Update Data (30 seconds)

**Update with PartiQL**:

```bash
kstone query myapp.keystone "UPDATE items SET age = 31, active = true WHERE pk = 'user#alice'"
```

Output:
```
1 row updated
```

**Verify the update**:

```bash
kstone get myapp.keystone user#alice
```

Output:
```json
{
  "name": "Alice Johnson",
  "email": "alice@example.com",
  "age": 31,
  "active": true
}
```

**Increment a value**:

```bash
# First, add a login_count field
kstone query myapp.keystone "UPDATE items SET login_count = 1 WHERE pk = 'user#alice'"

# Increment it
kstone query myapp.keystone "UPDATE items SET login_count = login_count + 1 WHERE pk = 'user#alice'"
```

## Step 7: Delete Data (30 seconds)

**Delete a single item**:

```bash
kstone delete myapp.keystone user#charlie
```

Output:
```
Item deleted successfully
```

**Verify deletion**:

```bash
kstone get myapp.keystone user#charlie
```

Output:
```
Item not found
```

**Delete with PartiQL**:

```bash
kstone query myapp.keystone "DELETE FROM items WHERE pk = 'user#bob'"
```

## Interactive Shell

For a more interactive experience, use the built-in shell:

```bash
kstone shell myapp.keystone
```

Output:
```
KeystoneDB Interactive Shell v0.1.0
Database: myapp.keystone
Type .help for commands, .exit to quit

kstone>
```

Now you can run queries without repeating the database path:

```
kstone> SELECT * FROM items WHERE pk = 'user#alice';
┌─────────────┬───────────────┬───────────────────┬─────┬────────┬─────────────┐
│ pk          │ name          │ email             │ age │ active │ login_count │
├─────────────┼───────────────┼───────────────────┼─────┼────────┼─────────────┤
│ user#alice  │ Alice Johnson │ alice@example.com │ 31  │ true   │ 2           │
└─────────────┴───────────────┴───────────────────┴─────┴────────┴─────────────┘
1 row (2.3ms)

kstone> INSERT INTO items VALUE {'pk': 'user#eve', 'name': 'Eve', 'age': 28};
1 row inserted (1.2ms)

kstone> SELECT COUNT(*) FROM items WHERE pk BEGINS WITH 'user#';
┌─────────┐
│ count   │
├─────────┤
│ 2       │
└─────────┘
1 row (3.1ms)

kstone> .exit
Goodbye!
```

**Shell meta-commands**:
- `.help` - Show all available commands
- `.schema` - Display database schema and statistics
- `.format table|json|compact` - Change output format
- `.timer on|off` - Show/hide query execution time
- `.clear` - Clear the screen
- `.exit` or `.quit` - Exit the shell

## Understanding the Output

When you run queries, you'll see different output formats depending on the command and options:

### Table Format (Default)

Best for human readability:

```
┌─────────────┬───────────────┬───────────────────┬─────┐
│ pk          │ name          │ email             │ age │
├─────────────┼───────────────┼───────────────────┼─────┤
│ user#alice  │ Alice Johnson │ alice@example.com │ 31  │
└─────────────┴───────────────┴───────────────────┴─────┘
1 row (2.3ms)
```

### JSON Format

Best for scripting and integration:

```json
[
  {
    "pk": "user#alice",
    "name": "Alice Johnson",
    "email": "alice@example.com",
    "age": 31
  }
]
```

### JSON Lines Format

Best for streaming and log processing:

```json
{"pk":"user#alice","name":"Alice Johnson","email":"alice@example.com","age":31}
```

### Compact Format

Best for debugging:

```
pk=user#alice name="Alice Johnson" email=alice@example.com age=31
```

## Using the Rust API

While the CLI is great for quick tasks, most applications use the Rust API directly. Here's the same workflow in Rust:

```rust
use kstone_api::{Database, ItemBuilder, Query};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create/open database
    let db = Database::create("myapp.keystone")?;

    // Insert an item
    let alice = ItemBuilder::new()
        .string("name", "Alice Johnson")
        .string("email", "alice@example.com")
        .number("age", 30)
        .bool("active", true)
        .build();

    db.put(b"user#alice", alice)?;

    // Get an item
    if let Some(item) = db.get(b"user#alice")? {
        println!("Found: {:?}", item);
    }

    // Query items
    let query = Query::new(b"user#")
        .sk_begins_with(b"")
        .limit(10);

    let response = db.query(query)?;
    println!("Found {} users", response.items.len());

    // Update an item
    use kstone_api::Update;
    use kstone_core::Value;

    let update = Update::new(b"user#alice")
        .expression("SET age = age + 1")
        .build();

    db.update(update)?;

    // Delete an item
    db.delete(b"user#alice")?;

    Ok(())
}
```

Add KeystoneDB to your `Cargo.toml`:

```toml
[dependencies]
kstone-api = { path = "../path/to/keystonedb/kstone-api" }
kstone-core = { path = "../path/to/keystonedb/kstone-core" }
```

## Common Patterns and Examples

### User Management System

```rust
use kstone_api::{Database, ItemBuilder};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::create("users.keystone")?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs() as i64;

    // Create user
    let user = ItemBuilder::new()
        .string("email", "alice@example.com")
        .string("name", "Alice Johnson")
        .number("created", now)
        .number("login_count", 0)
        .bool("active", true)
        .build();

    db.put(b"user#alice", user)?;

    // Add user profile
    let profile = ItemBuilder::new()
        .string("bio", "Software engineer")
        .string("location", "San Francisco")
        .list("interests", vec!["databases", "rust", "distributed-systems"])
        .build();

    db.put_with_sk(b"user#alice", b"profile", profile)?;

    // Add user settings
    let settings = ItemBuilder::new()
        .string("theme", "dark")
        .bool("notifications", true)
        .string("language", "en")
        .build();

    db.put_with_sk(b"user#alice", b"settings", settings)?;

    Ok(())
}
```

### Session Storage with TTL

```bash
# Create database with TTL
kstone create sessions.keystone --ttl expiresAt

# Insert session that expires in 30 minutes
EXPIRES=$(($(date +%s) + 1800))
kstone put sessions.keystone session#abc123 "{
  \"userId\": \"user#alice\",
  \"ipAddress\": \"192.168.1.1\",
  \"userAgent\": \"Mozilla/5.0...\",
  \"expiresAt\": $EXPIRES
}"

# After 30 minutes, the session is automatically removed
```

In Rust:

```rust
use kstone_api::{Database, TableSchema, ItemBuilder};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create database with TTL on expiresAt attribute
    let schema = TableSchema::new().with_ttl("expiresAt");
    let db = Database::create_with_schema("sessions.keystone", schema)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs() as i64;

    // Session expires in 30 minutes
    let session = ItemBuilder::new()
        .string("userId", "user#alice")
        .string("ipAddress", "192.168.1.1")
        .number("expiresAt", now + 1800)
        .build();

    db.put(b"session#abc123", session)?;

    // Later, when you try to get the session after expiration:
    // get() returns None automatically (lazy deletion)
    if let Some(session) = db.get(b"session#abc123")? {
        println!("Session is still valid");
    } else {
        println!("Session has expired");
    }

    Ok(())
}
```

### Blog Post System

```rust
use kstone_api::{Database, ItemBuilder, Query};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::create("blog.keystone")?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis() as i64;

    // Create a blog post (sort key is timestamp for ordering)
    let post = ItemBuilder::new()
        .string("title", "Getting Started with KeystoneDB")
        .string("author", "alice")
        .string("content", "In this post, we'll explore...")
        .number("published", timestamp)
        .list("tags", vec!["database", "rust", "tutorial"])
        .build();

    db.put_with_sk(
        b"posts#alice",
        format!("post#{}", timestamp).as_bytes(),
        post
    )?;

    // Query all posts by an author (newest first)
    let query = Query::new(b"posts#alice")
        .sk_begins_with(b"post#")
        .forward(false) // Reverse order
        .limit(10);

    let response = db.query(query)?;

    println!("Found {} posts", response.items.len());
    for item in response.items {
        if let Some(title) = item.get("title") {
            println!("- {}", title);
        }
    }

    Ok(())
}
```

### Shopping Cart

```rust
use kstone_api::{Database, ItemBuilder, Update};
use kstone_core::Value;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Database::create("ecommerce.keystone")?;

    // Add item to cart
    let cart_item = ItemBuilder::new()
        .string("productId", "prod#laptop-123")
        .string("name", "Gaming Laptop")
        .number("price", 1299)
        .number("quantity", 1)
        .build();

    db.put_with_sk(b"cart#user#alice", b"item#prod#laptop-123", cart_item)?;

    // Increment quantity
    let update = Update::new(b"cart#user#alice")
        .with_sk(b"item#prod#laptop-123")
        .expression("SET quantity = quantity + :inc")
        .value(":inc", Value::number(1));

    db.update(update)?;

    // Get all items in cart
    let query = Query::new(b"cart#user#alice")
        .sk_begins_with(b"item#");

    let response = db.query(query)?;

    let mut total = 0.0;
    for item in &response.items {
        if let (Some(Value::N(price)), Some(Value::N(qty))) =
            (item.get("price"), item.get("quantity"))
        {
            let price: f64 = price.parse().unwrap_or(0.0);
            let qty: f64 = qty.parse().unwrap_or(0.0);
            total += price * qty;
        }
    }

    println!("Cart total: ${:.2}", total);

    Ok(())
}
```

## Performance Tips

Even in this quick start, you can apply some best practices:

1. **Use composite keys wisely**: Put related items in the same partition for efficient queries
   ```
   user#alice → profile    ✓ (can query all user data)
   user#alice → settings   ✓
   vs.
   profile#alice           ✗ (requires multiple gets)
   settings#alice          ✗
   ```

2. **Batch operations**: Insert multiple items at once
   ```rust
   use kstone_api::BatchWriteRequest;

   let batch = BatchWriteRequest::new()
       .put(b"user#alice", alice)
       .put(b"user#bob", bob)
       .put(b"user#charlie", charlie);

   db.batch_write(batch)?;
   ```

3. **Use TTL for temporary data**: Automatic cleanup saves manual maintenance
   ```rust
   let schema = TableSchema::new().with_ttl("expiresAt");
   ```

4. **Query instead of scan**: Queries are much faster when you know the partition key
   ```rust
   // Fast - queries single partition
   let query = Query::new(b"user#alice").sk_begins_with(b"post#");

   // Slower - scans entire table
   let scan = Scan::new().filter_expression("author = :alice");
   ```

5. **Limit result sets**: Don't fetch more data than you need
   ```rust
   let query = Query::new(b"posts#").limit(20);
   ```

## Troubleshooting

**Database already exists error**:
```bash
kstone create myapp.keystone
Error: Database already exists at myapp.keystone
```

Solution: Use a different name or delete the existing database first:
```bash
rm -rf myapp.keystone
kstone create myapp.keystone
```

**Item not found**:
```bash
kstone get myapp.keystone user#invalid
Item not found
```

Solution: Check the key spelling and ensure the item was inserted.

**JSON parsing error**:
```bash
kstone put myapp.keystone user#test '{invalid json}'
Error: Failed to parse JSON: expected value at line 1 column 2
```

Solution: Ensure JSON is properly formatted with quotes around keys and string values.

**Permission denied**:
```bash
kstone create /root/myapp.keystone
Error: Permission denied (os error 13)
```

Solution: Create the database in a directory where you have write permissions.

## Next Steps

Congratulations! You've successfully:
- Created a KeystoneDB database
- Inserted, retrieved, updated, and deleted items
- Run PartiQL queries
- Used the interactive shell
- Explored common usage patterns

In the next chapter, we'll cover installation and setup in detail, including:
- Building from source
- Installing binary releases
- Development environment setup
- Configuration options
- Performance tuning

Continue to **Chapter 3: Installation & Setup** to learn more about deployment options and advanced configuration.

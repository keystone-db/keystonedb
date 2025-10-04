# Chapter 18: PartiQL Query Language

PartiQL brings SQL compatibility to KeystoneDB, allowing you to query and manipulate items using familiar SQL syntax instead of the native API. Whether you're a SQL veteran or just prefer declarative queries, PartiQL makes KeystoneDB more accessible while maintaining the power of DynamoDB's data model.

## What is PartiQL?

PartiQL (Partitioned Query Language) is an SQL-compatible query language designed for semi-structured and nested data. Originally developed by Amazon for DynamoDB, PartiQL extends SQL to work with flexible schemas and complex data types.

### PartiQL vs SQL

**Similarities:**
- SELECT, INSERT, UPDATE, DELETE statements
- WHERE clauses for filtering
- ORDER BY for sorting (with limitations)
- Familiar operators (=, <, >, AND, OR, NOT)

**Differences:**
- Works with schema-less data (no CREATE TABLE)
- Supports nested attributes (maps and lists)
- No JOINs (DynamoDB is not relational)
- Limited aggregation (COUNT, SUM, AVG not in Phase 4)
- Partition key required for most queries

### Why Use PartiQL?

**Advantages:**
✅ **Familiar syntax**: Leverage existing SQL knowledge
✅ **Readable queries**: Declarative style easier to understand
✅ **Interactive**: Perfect for CLI and exploratory queries
✅ **Cross-platform**: Same syntax as AWS DynamoDB
✅ **Less verbose**: Shorter than equivalent API code

**When to use native API:**
- Complex conditional logic
- Transaction operations (TransactGet/TransactWrite)
- Batch operations
- Fine-grained control over expression context

## Phase 4 Implementation Status

**⚠️ Important:** PartiQL is planned for Phase 4 but **not yet implemented** in KeystoneDB as of Phase 3.4.

**Planned features:**
- Phase 4.1: PartiQL Parser (lexer, AST)
- Phase 4.2: Query Translation (SELECT → Query/Scan)
- Phase 4.3: DML Translation (INSERT → put, UPDATE → update, DELETE → delete)
- Phase 4.4: Expression Mapping (WHERE → condition expressions)
- Phase 4.5: CLI Integration (`kstone query <partiql>`)

**Current status:** This chapter describes the planned design and API. Check CLAUDE.md for implementation status.

## SELECT Statements

SELECT is the most common PartiQL operation, used for querying items.

### Basic SELECT

Query all items in a partition:

```sql
SELECT * FROM items WHERE pk = 'user#123'
```

**Equivalent API:**
```rust
let query = Query::new(b"user#123");
let response = db.query(query)?;
```

### SELECT with Projection

Retrieve specific attributes only:

```sql
SELECT name, email, age FROM items WHERE pk = 'org#acme'
```

**Equivalent API:**
```rust
let query = Query::new(b"org#acme");
let response = db.query(query)?;

for item in response.items {
    let name = item.get("name");
    let email = item.get("email");
    let age = item.get("age");
}
```

**Note:** PartiQL projection is applied client-side after retrieval. All attributes are fetched from storage.

### SELECT with WHERE Clause

Filter by partition key and sort key:

```sql
SELECT * FROM items
WHERE pk = 'user#123' AND sk BETWEEN 'post#2024-01' AND 'post#2024-12'
```

**Equivalent API:**
```rust
let query = Query::new(b"user#123")
    .sk_between(b"post#2024-01", b"post#2024-12");
let response = db.query(query)?;
```

### Sort Key Conditions

PartiQL supports DynamoDB-style sort key conditions:

```sql
-- Equal
SELECT * FROM items WHERE pk = 'user#123' AND sk = 'profile'

-- Greater than
SELECT * FROM items WHERE pk = 'user#123' AND sk > 'post#2024-06'

-- Less than or equal
SELECT * FROM items WHERE pk = 'sensor#456' AND sk <= '2024-12-31'

-- Between
SELECT * FROM items WHERE pk = 'user#789' AND sk BETWEEN 'A' AND 'M'

-- Begins with
SELECT * FROM items WHERE pk = 'org#acme' AND begins_with(sk, 'user#')
```

**Equivalent API:**
```rust
Query::new(b"user#123").sk_eq(b"profile")
Query::new(b"user#123").sk_gt(b"post#2024-06")
Query::new(b"sensor#456").sk_lte(b"2024-12-31")
Query::new(b"user#789").sk_between(b"A", b"M")
Query::new(b"org#acme").sk_begins_with(b"user#")
```

### SELECT with LIMIT

Limit the number of results:

```sql
SELECT * FROM items WHERE pk = 'user#123' LIMIT 10
```

**Equivalent API:**
```rust
let query = Query::new(b"user#123").limit(10);
let response = db.query(query)?;
```

### SELECT with Index

Query using a secondary index:

```sql
-- Local Secondary Index
SELECT * FROM items.email-index
WHERE pk = 'org#acme' AND begins_with(email, 'alice')

-- Global Secondary Index
SELECT * FROM items.status-index
WHERE status = 'active'
```

**Equivalent API:**
```rust
// LSI
let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice");

// GSI
let query = Query::new(b"active")
    .index("status-index");
```

**Index syntax:** `items.<index-name>` specifies which index to use.

### Full Table Scan

Query all items (expensive!):

```sql
SELECT * FROM items
```

**Equivalent API:**
```rust
let scan = Scan::new();
let response = db.scan(scan)?;
```

**⚠️ Warning:** Full table scans are expensive and slow. Always prefer partition key queries.

## INSERT Statements

INSERT creates new items in the database.

### Basic INSERT

```sql
INSERT INTO items VALUE {
    'pk': 'user#123',
    'name': 'Alice',
    'age': 30,
    'active': true
}
```

**Equivalent API:**
```rust
let item = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .bool("active", true)
    .build();

db.put(b"user#123", item)?;
```

### INSERT with Nested Data

PartiQL supports nested maps and lists:

```sql
INSERT INTO items VALUE {
    'pk': 'user#456',
    'name': 'Bob',
    'address': {
        'street': '123 Main St',
        'city': 'Seattle',
        'state': 'WA'
    },
    'tags': ['developer', 'golang', 'rust']
}
```

**Equivalent API:**
```rust
use kstone_core::Value;

let mut address = HashMap::new();
address.insert("street".to_string(), Value::string("123 Main St"));
address.insert("city".to_string(), Value::string("Seattle"));
address.insert("state".to_string(), Value::string("WA"));

let tags = vec![
    Value::string("developer"),
    Value::string("golang"),
    Value::string("rust"),
];

let item = ItemBuilder::new()
    .string("name", "Bob")
    .build();

item.insert("address".to_string(), Value::M(address));
item.insert("tags".to_string(), Value::L(tags));

db.put(b"user#456", item)?;
```

### INSERT with Composite Key

Specify both partition key and sort key:

```sql
INSERT INTO items VALUE {
    'pk': 'org#acme',
    'sk': 'user#alice',
    'name': 'Alice',
    'role': 'admin'
}
```

**Equivalent API:**
```rust
let item = ItemBuilder::new()
    .string("name", "Alice")
    .string("role", "admin")
    .build();

db.put_with_sk(b"org#acme", b"user#alice", item)?;
```

**Note:** PartiQL extracts `pk` and `sk` attributes from the VALUE to construct the key.

## UPDATE Statements

UPDATE modifies existing items using update expressions.

### Basic UPDATE

```sql
UPDATE items
SET name = 'Alice Updated'
WHERE pk = 'user#123'
```

**Equivalent API:**
```rust
let update = Update::new(b"user#123")
    .expression("SET name = :name")
    .value(":name", Value::string("Alice Updated"));

db.update(update)?;
```

### UPDATE Multiple Attributes

```sql
UPDATE items
SET name = 'Alice', age = 31, updated_at = 1704067200000
WHERE pk = 'user#123'
```

**Equivalent API:**
```rust
let update = Update::new(b"user#123")
    .expression("SET name = :name, age = :age, updated_at = :timestamp")
    .value(":name", Value::string("Alice"))
    .value(":age", Value::number(31))
    .value(":timestamp", Value::Ts(1704067200000));

db.update(update)?;
```

### UPDATE with Arithmetic

Increment or decrement numeric values:

```sql
UPDATE items
SET score = score + 50, lives = lives - 1
WHERE pk = 'game#456'
```

**Equivalent API:**
```rust
let update = Update::new(b"game#456")
    .expression("SET score = score + :inc, lives = lives - :dec")
    .value(":inc", Value::number(50))
    .value(":dec", Value::number(1));

db.update(update)?;
```

### UPDATE with REMOVE

Remove attributes from an item:

```sql
UPDATE items
REMOVE temp, verification_code
WHERE pk = 'user#789'
```

**Equivalent API:**
```rust
let update = Update::new(b"user#789")
    .expression("REMOVE temp, verification_code");

db.update(update)?;
```

### UPDATE with ADD

Atomically add to numbers:

```sql
UPDATE items
ADD view_count 1
WHERE pk = 'page#home'
```

**Equivalent API:**
```rust
let update = Update::new(b"page#home")
    .expression("ADD view_count :one")
    .value(":one", Value::number(1));

db.update(update)?;
```

### UPDATE with Condition

Conditional updates using WHERE for conditions:

```sql
UPDATE items
SET balance = balance - 100
WHERE pk = 'account#123' AND balance >= 100
```

**Equivalent API:**
```rust
let update = Update::new(b"account#123")
    .expression("SET balance = balance - :amount")
    .condition("balance >= :amount")
    .value(":amount", Value::number(100));

db.update(update)?;
```

## DELETE Statements

DELETE removes items from the database.

### Basic DELETE

```sql
DELETE FROM items WHERE pk = 'user#123'
```

**Equivalent API:**
```rust
db.delete(b"user#123")?;
```

### DELETE with Composite Key

```sql
DELETE FROM items WHERE pk = 'org#acme' AND sk = 'user#alice'
```

**Equivalent API:**
```rust
db.delete_with_sk(b"org#acme", b"user#alice")?;
```

### DELETE with Condition

Conditional deletion:

```sql
DELETE FROM items
WHERE pk = 'user#789' AND status = 'inactive'
```

**Equivalent API:**
```rust
let context = ExpressionContext::new()
    .with_value(":status", Value::string("inactive"));

db.delete_conditional(b"user#789", "status = :status", context)?;
```

**Note:** PartiQL cannot delete multiple items in one statement. Each DELETE operates on a single item identified by its key.

## PartiQL Expression Syntax

PartiQL supports rich expression syntax for conditions and updates.

### Comparison Operators

```sql
-- Equal
WHERE age = 30

-- Not equal
WHERE status <> 'deleted'

-- Greater than
WHERE score > 100

-- Less than or equal
WHERE price <= 50.00

-- Between
WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31'
```

### Logical Operators

```sql
-- AND
WHERE age >= 18 AND status = 'active'

-- OR
WHERE role = 'admin' OR role = 'moderator'

-- NOT
WHERE NOT deleted
```

### Functions

```sql
-- attribute_exists
WHERE attribute_exists(email)

-- attribute_not_exists
WHERE attribute_not_exists(deleted_at)

-- begins_with
WHERE begins_with(email, 'admin@')

-- Nested attribute
WHERE address.city = 'Seattle'
```

### Nested Attribute Access

Access nested map attributes with dot notation:

```sql
SELECT name, address.city, address.state
FROM items
WHERE pk = 'user#123'
```

**Equivalent API:**
```rust
// Native API doesn't directly support nested paths
// You extract nested values manually:
let item = db.get(b"user#123")?.unwrap();
let address = item.get("address")
    .and_then(|v| v.as_map())
    .unwrap();
let city = address.get("city");
```

### List Access

Access list elements by index:

```sql
SELECT tags[0], tags[1]
FROM items
WHERE pk = 'user#456'
```

**Note:** List indexing may not be supported in Phase 4.1 (future enhancement).

## Query Optimization

PartiQL queries are optimized by translating to efficient native operations:

### Optimization: Partition Key Detection

```sql
-- Good: Has partition key (uses Query)
SELECT * FROM items WHERE pk = 'user#123'
→ Translates to: Query::new(b"user#123")

-- Slow: No partition key (uses Scan)
SELECT * FROM items WHERE age > 30
→ Translates to: Scan::new() + client-side filtering
```

**Rule:** Always include partition key for efficient queries.

### Optimization: Index Selection

```sql
-- Automatically uses index if available
SELECT * FROM items.status-index WHERE status = 'active'
→ Uses GSI: Query::new(b"active").index("status-index")

-- Without index: Full scan
SELECT * FROM items WHERE status = 'active'
→ Scan::new() + client-side filtering
```

**Rule:** Specify index name explicitly for indexed queries.

### Optimization: Projection Pushdown

```sql
-- Projection happens client-side
SELECT name, email FROM items WHERE pk = 'user#123'

-- All attributes fetched, filtered in client
```

**Note:** KeystoneDB fetches all attributes from storage. Projection only reduces network transfer to application.

### Optimization: LIMIT Pushdown

```sql
-- LIMIT pushed to storage layer
SELECT * FROM items WHERE pk = 'user#123' LIMIT 10
→ Query::new(b"user#123").limit(10)

-- Only 10 items read from disk
```

**Rule:** Always use LIMIT for pagination and performance.

## CLI Integration

Use PartiQL in the interactive shell:

```bash
$ kstone shell mydb.keystone

kstone> SELECT * FROM items WHERE pk = 'user#123';
┌─────────────┬───────┬─────┬────────┐
│ pk          │ name  │ age │ active │
├─────────────┼───────┼─────┼────────┤
│ user#123    │ Alice │ 30  │ true   │
└─────────────┴───────┴─────┴────────┘
1 row (12.3ms)

kstone> UPDATE items SET age = 31 WHERE pk = 'user#123';
Updated 1 item (5.2ms)

kstone> SELECT age FROM items WHERE pk = 'user#123';
┌─────┐
│ age │
├─────┤
│ 31  │
└─────┘
1 row (3.1ms)
```

**CLI features:**
- Syntax highlighting
- Auto-completion for keywords
- Multi-line query support (terminate with `;`)
- Result formatting (table, JSON, compact)
- Query timing

## ExecuteStatement API

Execute PartiQL from your application:

```rust
// Planned API (Phase 4.5)
let result = db.execute_statement(
    "SELECT * FROM items WHERE pk = ?",
    vec![Value::string("user#123")]
)?;

for item in result.items {
    println!("{:?}", item);
}
```

### Parameterized Queries

Use placeholders for safety:

```rust
// Planned API
let result = db.execute_statement(
    "SELECT * FROM items WHERE pk = ? AND age > ?",
    vec![
        Value::string("org#acme"),
        Value::number(18),
    ]
)?;
```

**Benefits:**
- SQL injection prevention
- Query plan caching (future)
- Type safety

## BatchExecuteStatement API

Execute multiple PartiQL statements in one call:

```rust
// Planned API (Phase 4.5)
let statements = vec![
    PartiQLStatement::new("SELECT * FROM items WHERE pk = ?")
        .param(Value::string("user#123")),
    PartiQLStatement::new("SELECT * FROM items WHERE pk = ?")
        .param(Value::string("user#456")),
];

let results = db.batch_execute_statement(statements)?;

for result in results {
    println!("Found {} items", result.items.len());
}
```

## PartiQL vs Native API

### When to Use PartiQL

✅ **Use PartiQL for:**
- Interactive queries in CLI
- Ad-hoc data exploration
- Simple CRUD operations
- Readable query logging
- SQL familiarity

**Example:**
```sql
-- Clear and concise
SELECT name, email FROM items WHERE pk = 'user#123' LIMIT 10
```

### When to Use Native API

✅ **Use Native API for:**
- Complex conditional logic
- Transaction operations
- Batch operations
- Performance-critical code
- Fine-grained control

**Example:**
```rust
// Precise control over context and conditions
let request = TransactWriteRequest::new()
    .update_with_condition(
        b"account#source",
        "SET balance = balance - :amount",
        "balance >= :amount"
    )
    .update(b"account#dest", "SET balance = balance + :amount")
    .value(":amount", Value::number(500));

db.transact_write(request)?;
```

### Comparison Table

| Feature | PartiQL | Native API |
|---------|---------|------------|
| **Readability** | High (SQL-like) | Medium (builder pattern) |
| **Verbosity** | Low | High |
| **Type safety** | Runtime | Compile-time |
| **Transactions** | Not supported | Full support |
| **Batch ops** | Limited | Full support |
| **Performance** | Parsing overhead | Direct |
| **Debugging** | String parsing errors | Rust compiler errors |

## Common Patterns

### Pattern 1: Find Item by Key

```sql
-- PartiQL
SELECT * FROM items WHERE pk = 'user#123'

-- API
let item = db.get(b"user#123")?;
```

### Pattern 2: Range Query

```sql
-- PartiQL
SELECT * FROM items
WHERE pk = 'user#123' AND sk BETWEEN 'post#A' AND 'post#Z'

-- API
let query = Query::new(b"user#123")
    .sk_between(b"post#A", b"post#Z");
let response = db.query(query)?;
```

### Pattern 3: Conditional Update

```sql
-- PartiQL
UPDATE items
SET balance = balance - 100
WHERE pk = 'account#123' AND balance >= 100

-- API
let update = Update::new(b"account#123")
    .expression("SET balance = balance - :amount")
    .condition("balance >= :amount")
    .value(":amount", Value::number(100));
db.update(update)?;
```

### Pattern 4: Create Item

```sql
-- PartiQL
INSERT INTO items VALUE {
    'pk': 'user#789',
    'name': 'Charlie',
    'email': 'charlie@example.com'
}

-- API
let item = ItemBuilder::new()
    .string("name", "Charlie")
    .string("email", "charlie@example.com")
    .build();
db.put(b"user#789", item)?;
```

### Pattern 5: Delete Item

```sql
-- PartiQL
DELETE FROM items WHERE pk = 'user#789'

-- API
db.delete(b"user#789")?;
```

## Limitations and Constraints

### Current Limitations (Phase 4 Planned)

1. **No JOINs**: KeystoneDB is not a relational database
2. **No aggregations**: COUNT, SUM, AVG not supported (Phase 4.1)
3. **No GROUP BY**: No grouping operations
4. **Single item DELETE**: Cannot delete multiple items in one statement
5. **No nested UPDATE**: Cannot update nested map/list elements

### PartiQL Constraints

```sql
-- ❌ Not supported: JOIN
SELECT users.name, orders.total
FROM users JOIN orders ON users.pk = orders.user_id

-- ❌ Not supported: Aggregation
SELECT COUNT(*) FROM items WHERE status = 'active'

-- ❌ Not supported: GROUP BY
SELECT status, COUNT(*) FROM items GROUP BY status

-- ❌ Not supported: Multi-item DELETE
DELETE FROM items WHERE status = 'inactive'
```

## Best Practices

### 1. Always Include Partition Key

```sql
-- Good: Has partition key
SELECT * FROM items WHERE pk = 'user#123'

-- Slow: No partition key (full scan)
SELECT * FROM items WHERE age > 30
```

### 2. Use Indexes for Cross-Partition Queries

```sql
-- Good: Uses GSI
SELECT * FROM items.status-index WHERE status = 'active'

-- Slow: Scans all partitions
SELECT * FROM items WHERE status = 'active'
```

### 3. Limit Results for Pagination

```sql
-- Good: Limit results
SELECT * FROM items WHERE pk = 'user#123' LIMIT 10

-- Risky: Unlimited results
SELECT * FROM items WHERE pk = 'user#123'
```

### 4. Use Parameterized Queries

```rust
// Good: Parameterized
db.execute_statement(
    "SELECT * FROM items WHERE pk = ?",
    vec![Value::string(user_input)]
)?;

// Bad: String concatenation (SQL injection risk)
let query = format!("SELECT * FROM items WHERE pk = '{}'", user_input);
db.execute_statement(&query, vec![])?;
```

### 5. Choose Right Tool for the Job

```rust
// Simple query: Use PartiQL
db.execute_statement("SELECT * FROM items WHERE pk = ?", vec![pk])?;

// Complex transaction: Use native API
db.transact_write(TransactWriteRequest::new()...)?;
```

## Summary

PartiQL in KeystoneDB provides:

✅ **SQL compatibility**: Familiar query syntax
✅ **SELECT**: Query items with WHERE, LIMIT, indexes
✅ **INSERT**: Create items with nested data
✅ **UPDATE**: Modify items with SET, REMOVE, ADD
✅ **DELETE**: Remove items by key
✅ **CLI integration**: Interactive query shell

**Key features:**
- Partition key queries (efficient)
- Sort key conditions (equals, between, begins_with, etc.)
- Index queries (LSI, GSI)
- Conditional operations (WHERE clauses)
- Expression syntax (AND, OR, NOT, functions)

**Limitations:**
- No JOINs (not relational)
- No aggregations (COUNT, SUM, AVG)
- No GROUP BY
- Single-item DELETE
- No nested UPDATE

**When to use:**
- Interactive queries in CLI
- Simple CRUD operations
- Ad-hoc data exploration
- Readable code

**When to use native API:**
- Transactions (TransactGet/TransactWrite)
- Batch operations
- Complex conditions
- Performance-critical code

**Best practices:**
- Always include partition key
- Use indexes for cross-partition queries
- Limit results for performance
- Parameterize queries for safety
- Choose right tool (PartiQL vs API)

Master PartiQL to query KeystoneDB with familiar SQL syntax!

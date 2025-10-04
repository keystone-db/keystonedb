# Chapter 12: Update Expressions

Update expressions provide a powerful and efficient way to modify items in KeystoneDB without reading and writing the entire item. Inspired by DynamoDB's update expression syntax, KeystoneDB's update system allows you to perform atomic modifications with minimal data transfer and maximum precision.

## Why Update Expressions?

Traditional item modification follows a read-modify-write pattern:

```rust
// Traditional approach (inefficient)
let mut item = db.get(b"user#123")?.unwrap();
item.insert("age".to_string(), Value::number(31));
db.put(b"user#123", item)?;
```

This approach has several drawbacks:
- **Network overhead**: Fetching and sending the entire item
- **Race conditions**: Another process might modify the item between read and write
- **Complexity**: You need to handle missing items, null values, etc.

Update expressions solve these problems:

```rust
// Modern approach (efficient)
let update = Update::new(b"user#123")
    .expression("SET age = :new_age")
    .value(":new_age", Value::number(31));

db.update(update)?;
```

Benefits:
- **Atomic**: The operation is applied atomically on the server side
- **Efficient**: Only the expression and values are transmitted
- **Type-safe**: The expression parser validates syntax before execution
- **Composable**: Multiple operations can be combined in a single update

## Update Expression Syntax

An update expression consists of one or more action clauses, each specifying a type of modification:

```
SET path = value [, path = value ...]
REMOVE path [, path ...]
ADD path value [, path value ...]
DELETE path value [, path value ...]
```

### Action Types

KeystoneDB supports four update actions:

1. **SET**: Sets an attribute to a new value or performs arithmetic
2. **REMOVE**: Deletes an attribute from the item
3. **ADD**: Atomically increments a number or adds to a set
4. **DELETE**: Removes elements from a set (not fully implemented in Phase 2.4)

Let's explore each action in detail.

## SET Operations

The SET action is the most versatile update operation. It can set attributes to literal values, copy values from other attributes, or perform arithmetic operations.

### Basic SET Operation

Setting an attribute to a new value:

```rust
use kstone_api::{Database, Update};
use kstone_core::Value;

let update = Update::new(b"user#123")
    .expression("SET name = :new_name")
    .value(":new_name", Value::string("Alice"));

let response = db.update(update)?;
println!("Updated item: {:?}", response.item);
```

**Output:**
```
Updated item: {"name": "Alice", "age": "30", ...}
```

### Multiple SET Operations

You can set multiple attributes in a single expression using commas:

```rust
let update = Update::new(b"user#123")
    .expression("SET name = :name, email = :email, updated_at = :timestamp")
    .value(":name", Value::string("Alice"))
    .value(":email", Value::string("alice@example.com"))
    .value(":timestamp", Value::Ts(1704067200000));

let response = db.update(update)?;
```

**Key Points:**
- Each SET clause is separated by a comma
- All SETs are applied atomically
- Value placeholders (`:name`, `:email`, etc.) are resolved from the context

### Arithmetic Expressions

SET supports arithmetic operations for incrementing and decrementing numeric values:

```rust
// Increment score
let update = Update::new(b"game#456")
    .expression("SET score = score + :increment")
    .value(":increment", Value::number(50));

let response = db.update(update)?;
```

**Before:**
```json
{"score": "100"}
```

**After:**
```json
{"score": "150"}
```

### Decrement Operations

Subtracting values works similarly:

```rust
// Decrement lives
let update = Update::new(b"game#456")
    .expression("SET lives = lives - :decrement")
    .value(":decrement", Value::number(1));

let response = db.update(update)?;
```

**Before:**
```json
{"lives": "3"}
```

**After:**
```json
{"lives": "2"}
```

### Complex Arithmetic

You can combine multiple arithmetic operations:

```rust
let update = Update::new(b"account#789")
    .expression("SET balance = balance + :deposit, total_deposits = total_deposits + :deposit")
    .value(":deposit", Value::number(500));

let response = db.update(update)?;
```

This atomically updates both the account balance and the running total of deposits.

## REMOVE Operations

The REMOVE action deletes one or more attributes from an item:

```rust
// Remove single attribute
let update = Update::new(b"user#123")
    .expression("REMOVE temporary_flag");

let response = db.update(update)?;
```

**Before:**
```json
{"name": "Alice", "temporary_flag": true, "age": "30"}
```

**After:**
```json
{"name": "Alice", "age": "30"}
```

### Removing Multiple Attributes

Use commas to remove multiple attributes:

```rust
let update = Update::new(b"user#123")
    .expression("REMOVE temp, verification_code, session_token");

let response = db.update(update)?;
```

This is particularly useful for cleanup operations, such as:
- Removing temporary session data
- Clearing verification codes after use
- Deleting cached values

### Conditional REMOVE

Combine REMOVE with condition expressions for safe deletion:

```rust
let update = Update::new(b"user#123")
    .expression("REMOVE verification_code")
    .condition("attribute_exists(verification_code)");

match db.update(update) {
    Ok(_) => println!("Verification code removed"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("No verification code to remove")
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

## ADD Operations

The ADD action atomically increments numeric values. Unlike SET with arithmetic, ADD creates the attribute if it doesn't exist.

### Basic ADD Operation

```rust
let update = Update::new(b"counter#global")
    .expression("ADD view_count :increment")
    .value(":increment", Value::number(1));

let response = db.update(update)?;
```

**Key Difference from SET:**
- **ADD**: Creates attribute with initial value if it doesn't exist
- **SET with arithmetic**: Requires attribute to exist (fails with error)

### Initializing Counters

ADD is perfect for counters that may not exist yet:

```rust
// First call: creates view_count = 1
db.update(Update::new(b"page#home")
    .expression("ADD view_count :one")
    .value(":one", Value::number(1)))?;

// Second call: increments to view_count = 2
db.update(Update::new(b"page#home")
    .expression("ADD view_count :one")
    .value(":one", Value::number(1)))?;
```

**After first update:**
```json
{"view_count": "1"}
```

**After second update:**
```json
{"view_count": "2"}
```

### Multiple ADD Operations

You can add to multiple counters simultaneously:

```rust
let update = Update::new(b"analytics#daily")
    .expression("ADD page_views :views, unique_visitors :visitors, clicks :clicks")
    .value(":views", Value::number(150))
    .value(":visitors", Value::number(42))
    .value(":clicks", Value::number(73));

let response = db.update(update)?;
```

## Combining Multiple Actions

The real power of update expressions comes from combining different action types in a single atomic operation:

```rust
let update = Update::new(b"user#999")
    .expression("SET last_login = :now, login_count = login_count + :one REMOVE session_token ADD total_logins :one")
    .value(":now", Value::Ts(1704067200000))
    .value(":one", Value::number(1));

let response = db.update(update)?;
```

**This single operation:**
1. Sets the `last_login` timestamp to the current time
2. Increments the `login_count` for this session
3. Removes any existing `session_token` (for security)
4. Adds to the `total_logins` counter (creates if missing)

**Before:**
```json
{
  "name": "Alice",
  "login_count": "42",
  "total_logins": "1337",
  "session_token": "abc123"
}
```

**After:**
```json
{
  "name": "Alice",
  "last_login": 1704067200000,
  "login_count": "43",
  "total_logins": "1338"
}
```

Notice that `session_token` was removed and `last_login` was added.

## Expression Attribute Values

Expression attribute values are placeholders that begin with `:` and are replaced with actual values at runtime:

```rust
let update = Update::new(b"product#123")
    .expression("SET price = :new_price, discount = :discount_pct, updated_by = :user")
    .value(":new_price", Value::number(29.99))
    .value(":discount_pct", Value::number(15))
    .value(":user", Value::string("admin"));

let response = db.update(update)?;
```

**Why use placeholders?**
- **Type safety**: Values are strongly typed (Number, String, Binary, etc.)
- **Reusability**: The same placeholder can be used multiple times in an expression
- **Clarity**: Separates the expression logic from the data

### Reusing Placeholders

The same placeholder can appear multiple times:

```rust
let update = Update::new(b"inventory#456")
    .expression("SET stock = stock - :qty, reserved = reserved + :qty")
    .value(":qty", Value::number(5));

let response = db.update(update)?;
```

Both `stock` and `reserved` use the `:qty` placeholder, ensuring consistency.

## Expression Attribute Names

Expression attribute names are placeholders for attribute names that begin with `#`. They're useful when attribute names:
- Are reserved words (like `name`, `type`, `status`)
- Contain special characters (like hyphens or spaces)
- Need to be computed dynamically

```rust
let update = Update::new(b"user#123")
    .expression("SET #n = :name, #s = :status")
    .name("#n", "name")
    .name("#s", "status")
    .value(":name", Value::string("Alice"))
    .value(":status", Value::string("active"));

let response = db.update(update)?;
```

**Common use cases:**
- `#name` for the `name` attribute (reserved word)
- `#type` for the `type` attribute (Rust keyword)
- `#user-id` for attributes with hyphens

## Practical Examples

### Example 1: User Login Tracking

Track user login activity with a single atomic update:

```rust
use std::time::SystemTime;

let now = SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_millis() as i64;

let update = Update::new(b"user#alice")
    .expression("SET last_login = :now, #s = :active ADD login_count :one")
    .name("#s", "status")
    .value(":now", Value::Ts(now))
    .value(":active", Value::string("online"))
    .value(":one", Value::number(1));

let response = db.update(update)?;
```

### Example 2: Shopping Cart Management

Update a shopping cart when items are added:

```rust
let update = Update::new(b"cart#session123")
    .expression("SET updated_at = :now, total = total + :price ADD item_count :one")
    .value(":now", Value::Ts(1704067200000))
    .value(":price", Value::number(29.99))
    .value(":one", Value::number(1));

let response = db.update(update)?;
```

### Example 3: Inventory Adjustment

Decrement stock and track reservation:

```rust
let update = Update::new(b"product#widget")
    .expression("SET stock = stock - :qty, reserved = reserved + :qty, updated_at = :now")
    .value(":qty", Value::number(3))
    .value(":now", Value::Ts(1704067200000));

let response = db.update(update)?;
```

### Example 4: Session Cleanup

Clean up expired session data:

```rust
let update = Update::new(b"session#xyz")
    .expression("REMOVE token, refresh_token, device_id SET expired = :true")
    .value(":true", Value::Bool(true));

let response = db.update(update)?;
```

## Update Expression Parsing

KeystoneDB uses a recursive descent parser to convert update expression strings into an Abstract Syntax Tree (AST):

**Input:**
```
SET age = :new_age, score = score + :bonus REMOVE temp
```

**Parsed AST:**
```rust
vec![
    UpdateAction::Set("age", UpdateValue::Placeholder(":new_age")),
    UpdateAction::Set("score", UpdateValue::Add("score", Box::new(UpdateValue::Placeholder(":bonus")))),
    UpdateAction::Remove("temp"),
]
```

The parser handles:
- Tokenization (lexical analysis)
- Syntax validation
- Operator precedence
- Placeholder resolution

## Error Handling

Update expressions can fail for various reasons:

### Missing Placeholder

```rust
let update = Update::new(b"user#123")
    .expression("SET age = :new_age");
    // Forgot to add .value(":new_age", ...)

match db.update(update) {
    Err(kstone_core::Error::InvalidExpression(msg)) => {
        println!("Error: {}", msg); // "Placeholder :new_age not found"
    }
    _ => {}
}
```

### Invalid Arithmetic

```rust
// Trying to increment a string
let update = Update::new(b"user#123")
    .expression("SET name = name + :suffix")
    .value(":suffix", Value::string("Smith"));

match db.update(update) {
    Err(kstone_core::Error::InvalidExpression(msg)) => {
        println!("Error: {}", msg); // "Addition requires numbers"
    }
    _ => {}
}
```

### Missing Attribute

```rust
// Trying to increment non-existent attribute with SET
let update = Update::new(b"user#123")
    .expression("SET score = score + :inc")
    .value(":inc", Value::number(10));

match db.update(update) {
    Err(kstone_core::Error::InvalidExpression(msg)) => {
        println!("Error: {}", msg); // "Attribute score not found"
    }
    _ => {}
}
```

**Solution:** Use ADD instead of SET for attributes that may not exist:

```rust
let update = Update::new(b"user#123")
    .expression("ADD score :inc")
    .value(":inc", Value::number(10));

db.update(update)?; // Works even if score doesn't exist
```

## Performance Considerations

### Network Efficiency

Update expressions minimize network overhead:

```rust
// Traditional: ~500 bytes (full item + metadata)
let item = db.get(b"user#123")?.unwrap();
// modify item...
db.put(b"user#123", item)?;

// Update expression: ~50 bytes (just expression + values)
db.update(Update::new(b"user#123")
    .expression("SET age = :age")
    .value(":age", Value::number(31)))?;
```

**Savings:** ~90% reduction in network traffic for simple updates.

### Atomic Operations

Update expressions execute atomically on the server:

```rust
// Two processes incrementing the same counter
// Process A:
db.update(Update::new(b"counter").expression("ADD count :one").value(":one", Value::number(1)))?;

// Process B (simultaneously):
db.update(Update::new(b"counter").expression("ADD count :one").value(":one", Value::number(1)))?;

// Final result: count = 2 (both increments applied)
```

No race conditions or lost updates!

### Reducing Round Trips

Combine multiple operations to reduce round trips:

```rust
// Bad: 3 round trips
db.update(Update::new(b"user#123").expression("SET name = :n").value(":n", Value::string("Alice")))?;
db.update(Update::new(b"user#123").expression("SET age = :a").value(":a", Value::number(30)))?;
db.update(Update::new(b"user#123").expression("ADD login_count :one").value(":one", Value::number(1)))?;

// Good: 1 round trip
db.update(Update::new(b"user#123")
    .expression("SET name = :n, age = :a ADD login_count :one")
    .value(":n", Value::string("Alice"))
    .value(":a", Value::number(30))
    .value(":one", Value::number(1)))?;
```

## Best Practices

### 1. Use Placeholders for All Values

```rust
// Bad: Hardcoded values (error-prone)
let update = Update::new(b"user#123")
    .expression("SET age = 30"); // Won't work - parser expects placeholder

// Good: Use placeholders
let update = Update::new(b"user#123")
    .expression("SET age = :age")
    .value(":age", Value::number(30));
```

### 2. Prefer ADD for Counters

```rust
// If counter might not exist, use ADD
let update = Update::new(b"page#home")
    .expression("ADD views :one")
    .value(":one", Value::number(1));

// If counter is guaranteed to exist, SET is fine
let update = Update::new(b"page#home")
    .expression("SET views = views + :inc")
    .value(":inc", Value::number(1));
```

### 3. Combine Related Updates

```rust
// Update related fields together for consistency
let update = Update::new(b"order#123")
    .expression("SET status = :status, updated_at = :now, updated_by = :user")
    .value(":status", Value::string("shipped"))
    .value(":now", Value::Ts(1704067200000))
    .value(":user", Value::string("admin"));
```

### 4. Use Name Placeholders for Reserved Words

```rust
// Safe for all attribute names
let update = Update::new(b"item#1")
    .expression("SET #n = :name, #t = :type")
    .name("#n", "name")
    .name("#t", "type")
    .value(":name", Value::string("Widget"))
    .value(":type", Value::string("gadget"));
```

### 5. Handle Errors Gracefully

```rust
match db.update(update) {
    Ok(response) => {
        println!("Updated: {:?}", response.item);
    }
    Err(kstone_core::Error::InvalidExpression(msg)) => {
        eprintln!("Expression error: {}", msg);
    }
    Err(kstone_core::Error::ConditionalCheckFailed(msg)) => {
        eprintln!("Condition failed: {}", msg);
    }
    Err(e) => {
        eprintln!("Unexpected error: {}", e);
    }
}
```

## Limitations

### Current Limitations (Phase 2.4)

1. **No nested attributes**: Cannot update `user.address.city` (only top-level attributes)
2. **No list operations**: Cannot append to lists or update list elements by index
3. **No map operations**: Cannot update nested map values
4. **DELETE action incomplete**: Set deletion not fully implemented

### Future Enhancements

Future versions may support:
- **Nested paths**: `SET user.address.city = :city`
- **List append**: `SET items = list_append(items, :new_item)`
- **List element**: `SET items[0].status = :status`
- **Map update**: `SET metadata.tags.color = :color`
- **Set operations**: `DELETE tags :remove_tags`

## Summary

Update expressions in KeystoneDB provide:

✅ **Atomic operations**: All updates applied in a single transaction
✅ **Efficient**: Minimal network overhead (expression + values only)
✅ **Composable**: Combine SET, REMOVE, ADD, DELETE in one call
✅ **Type-safe**: Strong typing for all values
✅ **Flexible**: Arithmetic, placeholders, multiple actions

**Key takeaways:**
- Use **SET** for assigning values and arithmetic
- Use **REMOVE** for deleting attributes
- Use **ADD** for counters that may not exist
- Combine actions for atomic multi-field updates
- Always use placeholders for values (`:value`)
- Use name placeholders for reserved words (`#name`)

Update expressions are essential for building high-performance, concurrent applications with KeystoneDB. Master them to unlock the full potential of the database!

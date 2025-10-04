# Chapter 13: Conditional Operations

Conditional operations are the cornerstone of building safe, concurrent applications with KeystoneDB. They allow you to specify preconditions that must be met before a write operation succeeds, preventing race conditions and ensuring data consistency in multi-user environments.

## The Problem: Race Conditions

Consider a simple scenario where two processes try to create a user account simultaneously:

```rust
// Process A
if db.get(b"user#alice")?.is_none() {
    db.put(b"user#alice", user_item)?; // ⚠️ Race condition!
}

// Process B (running at the same time)
if db.get(b"user#alice")?.is_none() {
    db.put(b"user#alice", different_user_item)?; // ⚠️ Overwrites!
}
```

**The Problem:**
1. Both processes check if the user exists (it doesn't)
2. Both processes proceed to create the user
3. The second write overwrites the first
4. Data loss or corruption occurs

**The Solution: Conditional Put**

```rust
// Process A
match db.put_conditional(b"user#alice", user_item, "attribute_not_exists(name)", context) {
    Ok(_) => println!("User created successfully"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("User already exists")
    }
    Err(e) => eprintln!("Error: {}", e),
}

// Process B (same code)
// One succeeds, one fails with ConditionalCheckFailed - no data loss!
```

## Condition Expression Syntax

Condition expressions are boolean expressions that evaluate to `true` or `false`. A write operation only succeeds if the condition evaluates to `true`.

### Basic Syntax

```
<function>(<path>)
<path> <operator> <value>
<condition> AND <condition>
<condition> OR <condition>
NOT <condition>
(<condition>)
```

### Supported Operators

| Operator | Meaning | Example |
|----------|---------|---------|
| `=` | Equal to | `age = :min_age` |
| `<>` | Not equal to | `status <> :blocked` |
| `<` | Less than | `age < :max_age` |
| `<=` | Less than or equal | `price <= :budget` |
| `>` | Greater than | `score > :threshold` |
| `>=` | Greater than or equal | `balance >= :amount` |

### Supported Functions

| Function | Returns True When | Example |
|----------|-------------------|---------|
| `attribute_exists(path)` | Attribute exists in item | `attribute_exists(email)` |
| `attribute_not_exists(path)` | Attribute does not exist | `attribute_not_exists(deleted_at)` |
| `begins_with(path, value)` | Attribute value starts with prefix | `begins_with(email, :domain)` |

### Logical Operators

| Operator | Meaning | Example |
|----------|---------|---------|
| `AND` | Both conditions must be true | `age >= :min AND active = :true` |
| `OR` | Either condition must be true | `role = :admin OR role = :moderator` |
| `NOT` | Negates the condition | `NOT deleted` |

## Conditional Put: Put-If-Not-Exists

The most common conditional operation is creating an item only if it doesn't already exist.

### Basic Put-If-Not-Exists

```rust
use kstone_core::expression::ExpressionContext;

let item = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .bool("active", true)
    .build();

let context = ExpressionContext::new();

match db.put_conditional(
    b"user#123",
    item,
    "attribute_not_exists(name)",
    context,
) {
    Ok(_) => println!("User created"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("User already exists - skipping creation")
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

**How it works:**
1. Database checks if the `name` attribute exists in the item at `user#123`
2. If the attribute doesn't exist (item is new), the put succeeds
3. If the attribute exists (item already created), the operation fails with `ConditionalCheckFailed`

### Why Check "name" Instead of Item Existence?

DynamoDB-style databases typically check for attribute existence rather than item existence:

```rust
// Good: Standard DynamoDB pattern
db.put_conditional(b"user#123", item, "attribute_not_exists(name)", context)?;

// Also valid: Check for any attribute that should always exist
db.put_conditional(b"user#123", item, "attribute_not_exists(pk)", context)?;
```

This pattern works because:
- New items have no attributes
- Existing items have at least one attribute
- It aligns with DynamoDB's condition expression API

### Idempotent Creation

Combine conditional puts with error handling for idempotent operations:

```rust
fn create_user_idempotent(db: &Database, user_id: &[u8], item: Item) -> Result<()> {
    let context = ExpressionContext::new();

    match db.put_conditional(user_id, item, "attribute_not_exists(name)", context) {
        Ok(_) => {
            println!("User created");
            Ok(())
        }
        Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
            println!("User already exists - no action taken");
            Ok(()) // Treat as success
        }
        Err(e) => Err(e),
    }
}
```

This function can be called multiple times safely - it only creates the user once.

## Conditional Update: Optimistic Locking

Optimistic locking prevents lost updates by ensuring the item hasn't changed since you last read it.

### Basic Optimistic Locking

```rust
// 1. Read the current item
let current_item = db.get(b"user#456")?.unwrap();
let current_age = match current_item.get("age").unwrap() {
    Value::N(n) => n.parse::<i32>().unwrap(),
    _ => panic!("Invalid age"),
};

// 2. Update with condition: age must still equal what we read
let update = Update::new(b"user#456")
    .expression("SET age = :new_age")
    .condition("age = :old_age")
    .value(":new_age", Value::number(current_age + 1))
    .value(":old_age", Value::number(current_age));

match db.update(update) {
    Ok(response) => {
        println!("Age incremented to: {:?}", response.item.get("age"));
    }
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("Age was modified by another process - retry needed");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

**How it works:**
1. Read the current age (e.g., 25)
2. Attempt to update age to 26, but **only if** it's still 25
3. If another process changed the age to 30, the condition fails
4. The application can retry with the new value

### Version Number Pattern

A more robust approach uses explicit version numbers:

```rust
// Item structure with version
let item = ItemBuilder::new()
    .string("name", "Alice")
    .number("age", 30)
    .number("version", 1)
    .build();

db.put(b"user#123", item)?;

// Later: Update with version check
let update = Update::new(b"user#123")
    .expression("SET age = :new_age, version = version + :one")
    .condition("version = :expected_version")
    .value(":new_age", Value::number(31))
    .value(":expected_version", Value::number(1))
    .value(":one", Value::number(1));

match db.update(update) {
    Ok(_) => println!("Updated to version 2"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("Version mismatch - item was modified")
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

**Benefits:**
- Explicit versioning makes concurrent modification obvious
- Easy to implement retry logic
- Clear audit trail of modifications

### Retry Logic with Optimistic Locking

```rust
use std::thread;
use std::time::Duration;

fn update_with_retry(db: &Database, max_retries: u32) -> Result<Item> {
    for attempt in 0..max_retries {
        // Read current item
        let current = db.get(b"counter#global")?.unwrap();
        let version = match current.get("version").unwrap() {
            Value::N(n) => n.parse::<i64>().unwrap(),
            _ => 0,
        };

        // Attempt conditional update
        let update = Update::new(b"counter#global")
            .expression("ADD count :inc SET version = version + :one")
            .condition("version = :expected")
            .value(":inc", Value::number(1))
            .value(":expected", Value::number(version))
            .value(":one", Value::number(1));

        match db.update(update) {
            Ok(response) => return Ok(response.item),
            Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
                if attempt < max_retries - 1 {
                    // Exponential backoff
                    let delay = Duration::from_millis(10 * 2_u64.pow(attempt));
                    thread::sleep(delay);
                    continue;
                }
                return Err(kstone_core::Error::ConditionalCheckFailed(
                    "Max retries exceeded".to_string()
                ));
            }
            Err(e) => return Err(e),
        }
    }

    unreachable!()
}
```

## Conditional Delete

Conditional deletes ensure you only remove items that meet specific criteria.

### Delete Only If Status Is Inactive

```rust
let context = ExpressionContext::new()
    .with_value(":status", Value::string("inactive"));

match db.delete_conditional(b"user#789", "status = :status", context) {
    Ok(_) => println!("Inactive user deleted"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("User is still active - cannot delete")
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### Delete Only If Email Exists

Useful for cleanup operations where you want to delete only if certain data exists:

```rust
let context = ExpressionContext::new();

match db.delete_conditional(b"user#999", "attribute_exists(email)", context) {
    Ok(_) => println!("User with email deleted"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("No email found - skipping delete")
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### Soft Delete Pattern

Implement soft deletes by setting a flag only if not already deleted:

```rust
let update = Update::new(b"user#123")
    .expression("SET deleted_at = :now, deleted_by = :user")
    .condition("attribute_not_exists(deleted_at)")
    .value(":now", Value::Ts(1704067200000))
    .value(":user", Value::string("admin"));

match db.update(update) {
    Ok(_) => println!("User soft-deleted"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("User already deleted")
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

## Complex Conditions

### Multiple Conditions with AND

Combine multiple checks to ensure all conditions are met:

```rust
let update = Update::new(b"account#123")
    .expression("SET balance = balance - :amount")
    .condition("balance >= :amount AND status = :active")
    .value(":amount", Value::number(100))
    .value(":active", Value::string("active"));

match db.update(update) {
    Ok(_) => println!("Withdrawal successful"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("Insufficient balance or inactive account")
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

**This ensures:**
- The account balance is sufficient (`balance >= :amount`)
- The account is active (`status = :active`)

Both conditions must be true, or the withdrawal fails.

### Multiple Conditions with OR

Allow operation if any condition is met:

```rust
let update = Update::new(b"document#456")
    .expression("SET content = :new_content")
    .condition("owner = :user OR role = :admin")
    .value(":new_content", Value::string("Updated content"))
    .value(":user", Value::string("alice"))
    .value(":admin", Value::string("admin"));

db.update(update)?;
```

**This allows the update if:**
- The user is the owner, OR
- The user has the admin role

### Negation with NOT

Invert a condition:

```rust
let update = Update::new(b"item#789")
    .expression("SET archived = :true")
    .condition("NOT archived")
    .value(":true", Value::Bool(true));

match db.update(update) {
    Ok(_) => println!("Item archived"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        println!("Item already archived")
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### Parentheses for Grouping

Control operator precedence with parentheses:

```rust
let update = Update::new(b"subscription#123")
    .expression("SET status = :cancelled")
    .condition("(plan = :basic OR plan = :standard) AND NOT locked")
    .value(":cancelled", Value::string("cancelled"))
    .value(":basic", Value::string("basic"))
    .value(":standard", Value::string("standard"));

db.update(update)?;
```

**This allows cancellation if:**
- The plan is basic OR standard, AND
- The subscription is not locked

## Advanced Patterns

### Compare-and-Swap for Counters

Implement atomic compare-and-swap:

```rust
fn compare_and_swap(db: &Database, key: &[u8], expected: i64, new_value: i64) -> Result<bool> {
    let update = Update::new(key)
        .expression("SET value = :new")
        .condition("value = :expected")
        .value(":new", Value::number(new_value))
        .value(":expected", Value::number(expected));

    match db.update(update) {
        Ok(_) => Ok(true),
        Err(kstone_core::Error::ConditionalCheckFailed(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

// Usage
if compare_and_swap(&db, b"config#flag", 0, 1)? {
    println!("Flag set successfully");
} else {
    println!("Flag was already set by another process");
}
```

### Prevent Duplicate Processing

Ensure an operation is only performed once:

```rust
fn process_order_once(db: &Database, order_id: &[u8]) -> Result<()> {
    // Mark order as processing
    let update = Update::new(order_id)
        .expression("SET processing = :true, processing_started = :now")
        .condition("attribute_not_exists(processing)")
        .value(":true", Value::Bool(true))
        .value(":now", Value::Ts(1704067200000));

    match db.update(update) {
        Ok(_) => {
            // We got the lock - process the order
            process_order(order_id)?;

            // Mark as complete
            let complete = Update::new(order_id)
                .expression("SET processing = :false, processing_complete = :now")
                .value(":false", Value::Bool(false))
                .value(":now", Value::Ts(1704067300000));
            db.update(complete)?;

            Ok(())
        }
        Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
            println!("Order is already being processed");
            Ok(())
        }
        Err(e) => Err(e),
    }
}
```

### Distributed Lock Pattern

Implement a simple distributed lock:

```rust
use std::time::{SystemTime, Duration};

fn acquire_lock(db: &Database, lock_name: &[u8], ttl_seconds: i64) -> Result<bool> {
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let expires_at = now + ttl_seconds;

    let item = ItemBuilder::new()
        .string("holder", "process-123")
        .number("expires_at", expires_at)
        .build();

    let context = ExpressionContext::new()
        .with_value(":now", Value::number(now));

    match db.put_conditional(
        lock_name,
        item,
        "attribute_not_exists(holder) OR expires_at < :now",
        context,
    ) {
        Ok(_) => Ok(true),
        Err(kstone_core::Error::ConditionalCheckFailed(_)) => Ok(false),
        Err(e) => Err(e),
    }
}

fn release_lock(db: &Database, lock_name: &[u8], holder: &str) -> Result<()> {
    let context = ExpressionContext::new()
        .with_value(":holder", Value::string(holder));

    db.delete_conditional(lock_name, "holder = :holder", context)?;
    Ok(())
}

// Usage
if acquire_lock(&db, b"lock#critical-section", 60)? {
    println!("Lock acquired");

    // Perform critical section work
    perform_critical_work()?;

    release_lock(&db, b"lock#critical-section", "process-123")?;
    println!("Lock released");
} else {
    println!("Could not acquire lock - another process holds it");
}
```

## Expression Evaluation

KeystoneDB evaluates condition expressions in a specific order:

1. **Parse**: Convert expression string to AST
2. **Resolve placeholders**: Replace `:value` and `#name` with actual values
3. **Fetch item**: Read the current item from the database
4. **Evaluate**: Execute the expression against the item
5. **Write or fail**: If true, perform write; if false, return `ConditionalCheckFailed`

### Evaluation Example

```rust
// Expression: "age >= :min AND status = :active"
// Context: {":min" -> 18, ":active" -> "active"}
// Item: {"age": 25, "status": "active"}

// Step 1: Parse to AST
And(
    GreaterThanOrEqual(AttributePath("age"), ValuePlaceholder(":min")),
    Equal(AttributePath("status"), ValuePlaceholder(":active"))
)

// Step 2: Resolve placeholders
And(
    GreaterThanOrEqual(25, 18),
    Equal("active", "active")
)

// Step 3: Evaluate
And(true, true) = true

// Result: Condition passed, write succeeds
```

## Comparison Operators

### Numeric Comparisons

Numbers are compared numerically (not lexicographically):

```rust
let update = Update::new(b"product#123")
    .expression("SET on_sale = :true")
    .condition("price >= :min AND price <= :max")
    .value(":true", Value::Bool(true))
    .value(":min", Value::number(10))
    .value(":max", Value::number(100));

db.update(update)?;
```

**Item with `price = 50`:** Condition passes (10 <= 50 <= 100)
**Item with `price = 5`:** Condition fails (5 < 10)

### String Comparisons

Strings are compared lexicographically:

```rust
let update = Update::new(b"user#123")
    .expression("SET category = :premium")
    .condition("last_name >= :start AND last_name < :end")
    .value(":premium", Value::string("premium"))
    .value(":start", Value::string("M"))
    .value(":end", Value::string("N"));

db.update(update)?;
```

**Item with `last_name = "Miller"`:** Condition passes ("M" <= "Miller" < "N")
**Item with `last_name = "Anderson"`:** Condition fails ("Anderson" < "M")

### Binary Comparisons

Binary data is compared byte-by-byte:

```rust
let update = Update::new(b"file#123")
    .expression("SET processed = :true")
    .condition("hash = :expected_hash")
    .value(":true", Value::Bool(true))
    .value(":expected_hash", Value::B(Bytes::from(vec![0xAB, 0xCD])));

db.update(update)?;
```

## Best Practices

### 1. Always Use Conditions for Concurrent Writes

```rust
// Bad: No protection against concurrent modification
db.put(b"user#123", item)?;

// Good: Prevent duplicate creation
db.put_conditional(b"user#123", item, "attribute_not_exists(name)", context)?;
```

### 2. Use Version Numbers for Complex Items

```rust
// Add version field to all items
let item = ItemBuilder::new()
    .string("data", "value")
    .number("version", 1)
    .build();

// Always check version on update
let update = Update::new(b"item#123")
    .expression("SET data = :new_data, version = version + :one")
    .condition("version = :expected_version")
    .value(":new_data", Value::string("new value"))
    .value(":expected_version", Value::number(current_version))
    .value(":one", Value::number(1));
```

### 3. Implement Retry Logic

```rust
fn update_with_exponential_backoff(db: &Database, update: Update) -> Result<UpdateResponse> {
    let max_retries = 5;
    let mut delay = Duration::from_millis(10);

    for attempt in 0..max_retries {
        match db.update(update.clone()) {
            Ok(response) => return Ok(response),
            Err(kstone_core::Error::ConditionalCheckFailed(_)) if attempt < max_retries - 1 => {
                thread::sleep(delay);
                delay *= 2; // Exponential backoff
            }
            Err(e) => return Err(e),
        }
    }

    Err(kstone_core::Error::ConditionalCheckFailed("Max retries exceeded".to_string()))
}
```

### 4. Provide Meaningful Error Messages

```rust
match db.update(update) {
    Ok(_) => println!("Balance updated"),
    Err(kstone_core::Error::ConditionalCheckFailed(_)) => {
        // Re-check to provide specific error
        let current = db.get(b"account#123")?.unwrap();
        if current.get("status") != Some(&Value::string("active")) {
            eprintln!("Account is not active");
        } else {
            eprintln!("Insufficient balance");
        }
    }
    Err(e) => eprintln!("Unexpected error: {}", e),
}
```

### 5. Test Concurrent Scenarios

```rust
#[test]
fn test_concurrent_increment() {
    let db = Database::create_in_memory().unwrap();

    // Create initial item
    let item = ItemBuilder::new().number("count", 0).build();
    db.put(b"counter", item).unwrap();

    // Spawn multiple threads incrementing concurrently
    let handles: Vec<_> = (0..10).map(|_| {
        let db = db.clone();
        thread::spawn(move || {
            let update = Update::new(b"counter")
                .expression("ADD count :one")
                .value(":one", Value::number(1));
            db.update(update).unwrap();
        })
    }).collect();

    // Wait for all increments
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify final count
    let result = db.get(b"counter").unwrap().unwrap();
    match result.get("count").unwrap() {
        Value::N(n) => assert_eq!(n, "10"),
        _ => panic!("Expected number"),
    }
}
```

## Summary

Conditional operations in KeystoneDB provide:

✅ **Optimistic locking**: Prevent lost updates with version checks
✅ **Put-if-not-exists**: Avoid duplicate creation races
✅ **Safe deletes**: Only delete items meeting specific criteria
✅ **Complex conditions**: AND, OR, NOT, parentheses for fine-grained control
✅ **Atomic checks**: Condition + write happen together (no race window)

**Key condition functions:**
- `attribute_exists(path)` - Check if attribute exists
- `attribute_not_exists(path)` - Check if attribute missing
- `begins_with(path, value)` - Check prefix match

**Comparison operators:**
- `=`, `<>`, `<`, `<=`, `>`, `>=` for numbers, strings, binary

**Logical operators:**
- `AND`, `OR`, `NOT`, `()` for complex conditions

**Common patterns:**
- **Create-once**: `attribute_not_exists(name)`
- **Optimistic lock**: `version = :expected`
- **Sufficient balance**: `balance >= :amount`
- **Active account**: `status = :active AND NOT locked`

Master conditional operations to build robust, concurrent-safe applications with KeystoneDB!

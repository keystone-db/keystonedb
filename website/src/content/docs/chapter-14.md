# Chapter 14: Transactions

Transactions in KeystoneDB provide ACID guarantees for complex multi-item operations. Whether you need to read multiple items in a consistent snapshot or update several items atomically, transactions ensure your data remains consistent even in the face of concurrent operations and system failures.

## ACID Guarantees

KeystoneDB transactions provide full ACID properties:

### Atomicity
All operations in a transaction either succeed together or fail together. There's no partial completion - if any operation fails, the entire transaction is rolled back.

```rust
let request = TransactWriteRequest::new()
    .put(b"order#123", order_item)
    .update(b"inventory#456", "SET stock = stock - :qty")
    .update(b"user#789", "SET credits = credits - :amount")
    .value(":qty", Value::number(5))
    .value(":amount", Value::number(100));

// Either all three operations succeed, or none do
match db.transact_write(request) {
    Ok(_) => println!("Order created, inventory updated, credits deducted"),
    Err(_) => println!("Transaction failed - all operations rolled back"),
}
```

### Consistency
Transactions maintain data integrity constraints. Conditional checks ensure that invariants are preserved across the transaction boundary.

```rust
// Ensure account has sufficient balance before transfer
let request = TransactWriteRequest::new()
    .update_with_condition(
        b"account#source",
        "SET balance = balance - :amount",
        "balance >= :amount"  // Consistency check
    )
    .update(b"account#dest", "SET balance = balance + :amount")
    .value(":amount", Value::number(500));

db.transact_write(request)?; // Fails if balance < 500
```

### Isolation
Transactions operate on a consistent snapshot of the data. For reads, all items are read at the same point in time. For writes, all conditions are evaluated before any writes are applied.

```rust
// Read multiple items in a consistent snapshot
let request = TransactGetRequest::new()
    .get(b"user#123")
    .get(b"account#456")
    .get(b"settings#789");

let response = db.transact_get(request)?;
// All items reflect the same point in time
```

### Durability
Once a transaction commits, the changes are permanent and survive system crashes. KeystoneDB's write-ahead log ensures durability.

```rust
db.transact_write(request)?;
// Transaction is now durable - changes persist even if system crashes
```

## TransactGet: Atomic Reads

TransactGet allows you to read multiple items in a single atomic operation, ensuring all items are read from the same consistent snapshot.

### Basic TransactGet

```rust
use kstone_api::TransactGetRequest;

let request = TransactGetRequest::new()
    .get(b"user#1")
    .get(b"user#2")
    .get(b"user#3");

let response = db.transact_get(request)?;

// Process results (in same order as request)
for (i, item_opt) in response.items.iter().enumerate() {
    match item_opt {
        Some(item) => println!("User {}: {:?}", i + 1, item),
        None => println!("User {} not found", i + 1),
    }
}
```

**Key characteristics:**
- Items are returned in the same order as requested
- Missing items return `None` (not an error)
- All items are read atomically (consistent snapshot)
- Maximum efficiency - single round trip to database

### Reading Related Items

TransactGet is perfect for loading related entities:

```rust
// Load user profile, settings, and preferences together
let request = TransactGetRequest::new()
    .get_with_sk(b"user#alice", b"profile")
    .get_with_sk(b"user#alice", b"settings")
    .get_with_sk(b"user#alice", b"preferences");

let response = db.transact_get(request)?;

let profile = response.items[0].as_ref().expect("Profile not found");
let settings = response.items[1].as_ref().expect("Settings not found");
let preferences = response.items[2].as_ref().expect("Preferences not found");

println!("Loaded complete user data for alice");
```

### Handling Missing Items

```rust
let request = TransactGetRequest::new()
    .get(b"user#1")
    .get(b"user#2")
    .get(b"user#3");

let response = db.transact_get(request)?;

let found_items: Vec<&Item> = response.items.iter()
    .filter_map(|opt| opt.as_ref())
    .collect();

println!("Found {} out of 3 items", found_items.len());
```

### Consistency Example

Imagine two processes reading account balances:

```rust
// Process A: TransactGet (atomic read)
let request = TransactGetRequest::new()
    .get(b"account#savings")
    .get(b"account#checking");

let response = db.transact_get(request)?;
// Both balances from the same point in time

// Process B: Individual gets (non-atomic)
let savings = db.get(b"account#savings")?;
// ⚠️ Another process might modify data here
let checking = db.get(b"account#checking")?;
// Balances might be from different points in time!
```

**TransactGet guarantees:** Both balances reflect the same moment, even if another process is actively transferring money between accounts.

## TransactWrite: Atomic Writes

TransactWrite allows you to perform multiple write operations (put, update, delete, condition check) in a single atomic transaction.

### Basic TransactWrite

```rust
use kstone_api::TransactWriteRequest;
use kstone_core::Value;

let mut item1 = ItemBuilder::new()
    .string("name", "Alice")
    .build();

let mut item2 = ItemBuilder::new()
    .string("name", "Bob")
    .build();

let request = TransactWriteRequest::new()
    .put(b"user#1", item1)
    .put(b"user#2", item2);

let response = db.transact_write(request)?;
println!("Committed {} operations", response.committed_count);
```

**Result:** Both users are created atomically - if either put fails, neither is created.

### Mixed Operations

Combine different operation types in a single transaction:

```rust
let request = TransactWriteRequest::new()
    // Create new order
    .put(b"order#123", order_item)
    // Update inventory count
    .update(b"inventory#widget", "SET stock = stock - :qty")
    // Delete old cart
    .delete(b"cart#session456")
    // Verify user account exists
    .condition_check(b"user#alice", "attribute_exists(name)")
    .value(":qty", Value::number(3));

match db.transact_write(request) {
    Ok(response) => {
        println!("Order placed successfully");
        println!("Committed {} operations", response.committed_count);
    }
    Err(kstone_core::Error::TransactionCanceled(msg)) => {
        println!("Transaction failed: {}", msg);
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

**This transaction:**
1. Creates a new order
2. Decrements inventory stock
3. Deletes the shopping cart
4. Verifies the user account exists

All operations succeed together, or all fail together.

### Transaction with Conditions

Add preconditions to ensure data consistency:

```rust
let request = TransactWriteRequest::new()
    // Deduct from source (only if sufficient balance)
    .update_with_condition(
        b"account#alice",
        "SET balance = balance - :amount",
        "balance >= :amount"
    )
    // Add to destination
    .update(
        b"account#bob",
        "SET balance = balance + :amount"
    )
    // Record transaction
    .put(b"transaction#tx123", transaction_record)
    .value(":amount", Value::number(500));

match db.transact_write(request) {
    Ok(_) => println!("Transfer completed successfully"),
    Err(kstone_core::Error::TransactionCanceled(msg)) => {
        println!("Transfer failed: {}", msg);
        // Likely insufficient balance
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

**Guarantees:**
- If Alice has insufficient balance, the **entire** transaction fails
- Bob's balance is never updated if Alice's deduction fails
- The transaction record is only created if both updates succeed

## Transaction Failures and Rollback

Transactions can fail for several reasons, but KeystoneDB guarantees that no partial state is ever persisted.

### Condition Failures

```rust
let request = TransactWriteRequest::new()
    .put(b"item#1", item1)
    .put_with_condition(
        b"item#2",
        item2,
        "attribute_not_exists(name)" // This will fail if item exists
    );

match db.transact_write(request) {
    Ok(_) => println!("Both items created"),
    Err(kstone_core::Error::TransactionCanceled(_)) => {
        println!("Item 2 already exists - item 1 NOT created (rolled back)");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

**Important:** Even though `item#1` was listed first and had no condition, it is **not created** because `item#2`'s condition failed. This is the atomicity guarantee.

### Multiple Condition Failures

```rust
let request = TransactWriteRequest::new()
    .update_with_condition(
        b"account#1",
        "SET balance = balance - :amt",
        "balance >= :amt"
    )
    .update_with_condition(
        b"account#2",
        "SET balance = balance - :amt",
        "balance >= :amt"
    )
    .value(":amt", Value::number(1000));

match db.transact_write(request) {
    Ok(_) => println!("Both accounts debited"),
    Err(kstone_core::Error::TransactionCanceled(msg)) => {
        // Could be either account or both with insufficient balance
        println!("Transaction cancelled: {}", msg);
        // No accounts were modified
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### Retry on Transaction Cancellation

```rust
fn transfer_with_retry(
    db: &Database,
    from: &[u8],
    to: &[u8],
    amount: i64,
    max_retries: u32,
) -> Result<()> {
    for attempt in 0..max_retries {
        let request = TransactWriteRequest::new()
            .update_with_condition(
                from,
                "SET balance = balance - :amt",
                "balance >= :amt"
            )
            .update(to, "SET balance = balance + :amt")
            .value(":amt", Value::number(amount));

        match db.transact_write(request) {
            Ok(_) => return Ok(()),
            Err(kstone_core::Error::TransactionCanceled(_)) if attempt < max_retries - 1 => {
                println!("Transaction cancelled, retrying... (attempt {})", attempt + 1);
                thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(kstone_core::Error::TransactionCanceled("Max retries exceeded".to_string()))
}
```

## Shared Expression Context

All operations in a transaction share the same expression context for values and names:

```rust
let request = TransactWriteRequest::new()
    .update(b"item#1", "SET status = :new_status")
    .update(b"item#2", "SET status = :new_status")
    .update(b"item#3", "SET status = :new_status")
    .value(":new_status", Value::string("active"));
    // All three updates use the same :new_status value

let response = db.transact_write(request)?;
```

**Benefits:**
- DRY: Define values once, use in multiple operations
- Consistency: Ensure same value is used across all operations
- Efficiency: Smaller request payload

### Multiple Values

```rust
let request = TransactWriteRequest::new()
    .update(b"order#123", "SET status = :status, shipped_at = :timestamp")
    .update(b"inventory#456", "SET stock = stock - :qty")
    .update(b"user#alice", "ADD total_orders :one")
    .value(":status", Value::string("shipped"))
    .value(":timestamp", Value::Ts(1704067200000))
    .value(":qty", Value::number(2))
    .value(":one", Value::number(1));

db.transact_write(request)?;
```

## Use Cases for Transactions

### Use Case 1: Money Transfer

Transfer funds between accounts with ACID guarantees:

```rust
fn transfer_funds(db: &Database, from: &[u8], to: &[u8], amount: i64) -> Result<()> {
    let request = TransactWriteRequest::new()
        // Deduct from source account (only if sufficient balance)
        .update_with_condition(
            from,
            "SET balance = balance - :amount, last_modified = :now",
            "balance >= :amount AND status = :active"
        )
        // Add to destination account (must exist and be active)
        .update_with_condition(
            to,
            "SET balance = balance + :amount, last_modified = :now",
            "attribute_exists(balance) AND status = :active"
        )
        .value(":amount", Value::number(amount))
        .value(":now", Value::Ts(current_timestamp()))
        .value(":active", Value::string("active"));

    db.transact_write(request)?;
    Ok(())
}
```

**Guarantees:**
- Source account has sufficient balance
- Both accounts are active
- Transfer is atomic (both succeed or both fail)
- Balances are updated together (no intermediate state)

### Use Case 2: Order Placement

Create order and update inventory atomically:

```rust
fn place_order(db: &Database, order_id: &[u8], items: Vec<OrderItem>) -> Result<()> {
    let mut request = TransactWriteRequest::new();

    // Create order
    let order = ItemBuilder::new()
        .string("status", "pending")
        .number("total", calculate_total(&items))
        .build();
    request = request.put(order_id, order);

    // Update inventory for each item
    for item in &items {
        let product_key = format!("product#{}", item.product_id);
        request = request
            .update_with_condition(
                product_key.as_bytes(),
                "SET stock = stock - :qty",
                "stock >= :qty"
            )
            .value(&format!(":qty_{}", item.product_id), Value::number(item.quantity));
    }

    // Verify customer account exists
    request = request.condition_check(
        b"customer#123",
        "attribute_exists(email) AND status = :active"
    )
    .value(":active", Value::string("active"));

    db.transact_write(request)?;
    Ok(())
}
```

**Benefits:**
- Order is only created if all items are in stock
- Inventory is updated atomically with order creation
- Customer account is verified before order placement

### Use Case 3: Reservation System

Implement seat reservation with transaction guarantees:

```rust
fn reserve_seats(db: &Database, event_id: &[u8], seats: Vec<String>, user_id: &[u8]) -> Result<()> {
    let mut request = TransactWriteRequest::new();

    // Check all seats are available
    for seat in &seats {
        let seat_key = format!("seat#{}-{}",
            String::from_utf8_lossy(event_id),
            seat
        );

        request = request.update_with_condition(
            seat_key.as_bytes(),
            "SET reserved_by = :user, reserved_at = :now",
            "attribute_not_exists(reserved_by)"
        );
    }

    // Create reservation record
    let reservation = ItemBuilder::new()
        .string("user_id", String::from_utf8_lossy(user_id).to_string())
        .string("event_id", String::from_utf8_lossy(event_id).to_string())
        .number("seat_count", seats.len() as i64)
        .build();

    request = request
        .put(b"reservation#123", reservation)
        .value(":user", Value::string(String::from_utf8_lossy(user_id).to_string()))
        .value(":now", Value::Ts(current_timestamp()));

    match db.transact_write(request) {
        Ok(_) => {
            println!("All {} seats reserved successfully", seats.len());
            Ok(())
        }
        Err(kstone_core::Error::TransactionCanceled(_)) => {
            println!("One or more seats already reserved");
            Err(kstone_core::Error::TransactionCanceled(
                "Seats unavailable".to_string()
            ))
        }
        Err(e) => Err(e),
    }
}
```

**Guarantees:**
- All seats are reserved together (or none)
- No partial reservations
- Reservation record matches actual seat assignments

### Use Case 4: User Registration

Create user account with related entities:

```rust
fn register_user(db: &Database, username: &str, email: &str) -> Result<()> {
    let user_key = format!("user#{}", username);

    let request = TransactWriteRequest::new()
        // Create user profile (only if doesn't exist)
        .put_with_condition(
            user_key.as_bytes(),
            ItemBuilder::new()
                .string("username", username)
                .string("email", email)
                .string("status", "active")
                .number("created_at", current_timestamp())
                .build(),
            "attribute_not_exists(username)"
        )
        // Create user settings
        .put(
            format!("settings#{}", username).as_bytes(),
            ItemBuilder::new()
                .bool("email_notifications", true)
                .bool("dark_mode", false)
                .build()
        )
        // Create user preferences
        .put(
            format!("preferences#{}", username).as_bytes(),
            ItemBuilder::new()
                .string("language", "en")
                .string("timezone", "UTC")
                .build()
        )
        // Verify email not already in use
        .condition_check(
            format!("email#{}", email).as_bytes(),
            "attribute_not_exists(user)"
        );

    match db.transact_write(request) {
        Ok(_) => {
            println!("User registered successfully");
            Ok(())
        }
        Err(kstone_core::Error::TransactionCanceled(msg)) => {
            if msg.contains("username") {
                println!("Username already exists");
            } else {
                println!("Email already in use");
            }
            Err(kstone_core::Error::TransactionCanceled(msg))
        }
        Err(e) => Err(e),
    }
}
```

## Condition Check Operations

The `condition_check` operation allows you to verify conditions without performing any writes:

```rust
let request = TransactWriteRequest::new()
    // Perform writes
    .update(b"item#1", "SET status = :new_status")
    .update(b"item#2", "SET status = :new_status")
    // But only if prerequisite condition is met
    .condition_check(b"config#global", "feature_enabled = :true")
    .value(":new_status", Value::string("active"))
    .value(":true", Value::Bool(true));

match db.transact_write(request) {
    Ok(_) => println!("Feature enabled - items updated"),
    Err(kstone_core::Error::TransactionCanceled(_)) => {
        println!("Feature disabled - no changes made")
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

**Use cases for condition_check:**
- Verify global configuration before updates
- Ensure prerequisite entities exist
- Check rate limits or quotas
- Validate feature flags

## Transaction Limitations

### Maximum Operations

KeystoneDB has a practical limit on the number of operations per transaction (typically 25-100 items):

```rust
// Good: Reasonable transaction size
let request = TransactWriteRequest::new()
    .put(b"item#1", item1)
    .put(b"item#2", item2)
    .update(b"item#3", "SET status = :s")
    .value(":s", Value::string("active"));

// Avoid: Too many operations in single transaction
let mut request = TransactWriteRequest::new();
for i in 0..1000 {
    request = request.put(format!("item#{}", i).as_bytes(), item.clone());
}
// This may exceed transaction limits
```

**Best practice:** Keep transactions focused and small. For bulk operations, use multiple transactions or batch writes.

### No Cross-Partition Queries

Transactions operate on individual items, not query results:

```rust
// ❌ Cannot do: "Update all items where status = active"
// Transactions work on specific keys, not query predicates

// ✅ Instead: Query first, then transact on specific items
let query_response = db.query(Query::new(b"org#acme"))?;
let mut request = TransactWriteRequest::new();

for item in query_response.items {
    let key = extract_key(&item);
    request = request.update(key, "SET processed = :true");
}

request = request.value(":true", Value::Bool(true));
db.transact_write(request)?;
```

## Performance Considerations

### Single-Stripe Transactions

For best performance, keep transaction items in the same partition:

```rust
// Good: All items have same partition key (same stripe)
let request = TransactGetRequest::new()
    .get_with_sk(b"user#alice", b"profile")
    .get_with_sk(b"user#alice", b"settings")
    .get_with_sk(b"user#alice", b"preferences");
// Fast: Single stripe read

// Slower: Items in different partitions (different stripes)
let request = TransactGetRequest::new()
    .get(b"user#alice")
    .get(b"user#bob")
    .get(b"user#charlie");
// Slower: Must coordinate across multiple stripes
```

### Write Amplification

Each transaction write creates a single WAL entry, but multiple SST writes on flush:

```rust
// More efficient: Single transaction
db.transact_write(TransactWriteRequest::new()
    .put(b"item#1", item1)
    .put(b"item#2", item2)
    .put(b"item#3", item3))?;
// One WAL entry, coordinated SST writes

// Less efficient: Multiple individual puts
db.put(b"item#1", item1)?;
db.put(b"item#2", item2)?;
db.put(b"item#3", item3)?;
// Three WAL entries, three separate operations
```

## Best Practices

### 1. Use Transactions for Related Writes

```rust
// Good: Related updates in transaction
let request = TransactWriteRequest::new()
    .update(b"order#123", "SET status = :shipped")
    .update(b"inventory#456", "SET stock = stock - :qty")
    .value(":shipped", Value::string("shipped"))
    .value(":qty", Value::number(1));

// Avoid: Separate updates (risk of inconsistency)
db.update(Update::new(b"order#123").expression("SET status = :shipped"))?;
db.update(Update::new(b"inventory#456").expression("SET stock = stock - :qty"))?;
```

### 2. Always Include Conditions for Safety

```rust
// Good: Verify preconditions
let request = TransactWriteRequest::new()
    .update_with_condition(
        b"account#source",
        "SET balance = balance - :amt",
        "balance >= :amt"
    )
    .update(b"account#dest", "SET balance = balance + :amt")
    .value(":amt", Value::number(100));

// Risky: No balance check (could go negative)
let request = TransactWriteRequest::new()
    .update(b"account#source", "SET balance = balance - :amt")
    .update(b"account#dest", "SET balance = balance + :amt")
    .value(":amt", Value::number(100));
```

### 3. Handle Cancellation Gracefully

```rust
match db.transact_write(request) {
    Ok(response) => {
        println!("Transaction committed: {} ops", response.committed_count);
    }
    Err(kstone_core::Error::TransactionCanceled(msg)) => {
        // Expected failure - handle gracefully
        eprintln!("Transaction cancelled: {}", msg);
        // Optionally retry or notify user
    }
    Err(e) => {
        // Unexpected error
        eprintln!("Unexpected error: {}", e);
    }
}
```

### 4. Keep Transactions Small and Focused

```rust
// Good: Small, focused transaction
fn complete_checkout(db: &Database, cart_id: &[u8], order_id: &[u8]) -> Result<()> {
    let request = TransactWriteRequest::new()
        .put(order_id, create_order())
        .delete(cart_id)
        .update(b"user#123", "ADD order_count :one")
        .value(":one", Value::number(1));

    db.transact_write(request)?;
    Ok(())
}

// Avoid: Large, complex transaction
fn do_everything(db: &Database) -> Result<()> {
    let mut request = TransactWriteRequest::new();
    // 50+ operations
    // Complex conditions
    // Cross-partition updates
    // ...
    db.transact_write(request)?; // Higher chance of failure
    Ok(())
}
```

## Summary

Transactions in KeystoneDB provide:

✅ **ACID guarantees**: Atomicity, Consistency, Isolation, Durability
✅ **TransactGet**: Read multiple items in consistent snapshot
✅ **TransactWrite**: Write multiple items atomically with conditions
✅ **All-or-nothing**: Either all operations succeed or all fail
✅ **Shared context**: Reuse expression values across operations

**Key operations:**
- **Put**: Create or replace item (with optional condition)
- **Update**: Modify item with update expression (with optional condition)
- **Delete**: Remove item (with optional condition)
- **ConditionCheck**: Verify condition without writing

**Common patterns:**
- **Money transfer**: Atomic debit + credit
- **Order placement**: Create order + update inventory
- **Seat reservation**: Reserve all or none
- **User registration**: Create user + settings + preferences

**Best practices:**
- Keep transactions small and focused
- Always use conditions for safety
- Handle `TransactionCanceled` gracefully
- Prefer single-partition transactions for performance

Master transactions to build robust, consistent multi-item operations in KeystoneDB!

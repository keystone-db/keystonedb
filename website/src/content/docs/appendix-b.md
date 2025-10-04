# Appendix B: Error Codes & Messages

This appendix provides a comprehensive reference for all error types in KeystoneDB, including error codes, descriptions, common causes, and troubleshooting guidance.

## Error Type Reference

### IO_ERROR

**Code:** `IO_ERROR`
**Rust Type:** `Error::Io(std::io::Error)`
**Retryable:** Yes
**Description:** Operating system I/O operation failed.

**Common Causes:**
- File not found
- Permission denied
- Disk full
- Device not ready
- Network filesystem unavailable

**Example Messages:**
```
IO error: No such file or directory (os error 2)
IO error: Permission denied (os error 13)
IO error: No space left on device (os error 28)
```

**Troubleshooting:**

1. **File not found:**
   ```bash
   # Check if database directory exists
   ls -la /path/to/db.keystone/

   # Create database if it doesn't exist
   kstone create /path/to/db.keystone
   ```

2. **Permission denied:**
   ```bash
   # Check permissions
   ls -l /path/to/db.keystone/

   # Fix permissions
   chmod 755 /path/to/db.keystone/
   chmod 644 /path/to/db.keystone/*
   ```

3. **Disk full:**
   ```bash
   # Check disk space
   df -h /path/to/db.keystone/

   # Free up space or move database
   # Consider enabling max_total_disk_bytes limit
   ```

**Code Example:**
```rust
match db.put(b"key", item) {
    Ok(_) => println!("Success"),
    Err(Error::Io(e)) => {
        eprintln!("I/O error: {}", e);
        match e.kind() {
            std::io::ErrorKind::NotFound => {
                // Database doesn't exist - create it
                let db = Database::create(path)?;
            }
            std::io::ErrorKind::PermissionDenied => {
                // Fix permissions and retry
            }
            _ => {
                // Other I/O errors
                return Err(e.into());
            }
        }
    }
    Err(e) => return Err(e),
}
```

---

### CORRUPTION

**Code:** `CORRUPTION`
**Rust Type:** `Error::Corruption(String)`
**Retryable:** No
**Description:** Data corruption detected in database files.

**Common Causes:**
- Disk hardware failure
- Filesystem corruption
- Incomplete write during crash
- Manual file modification
- Software bugs

**Example Messages:**
```
Corruption detected: Invalid WAL magic
Corruption detected: Checksum mismatch in SST file
Corruption detected: Invalid SST magic
Corruption detected: Non-monotonic sequence numbers
```

**Troubleshooting:**

1. **Invalid magic number:**
   ```bash
   # Check file headers with hexdump
   hexdump -C /path/to/db.keystone/wal.log | head

   # WAL should start with: 57 41 4C 00 (WAL\0)
   # SST should start with: 53 53 54 00 (SST\0)

   # If corrupted, restore from backup or delete and recreate
   ```

2. **Checksum mismatch:**
   ```bash
   # File is corrupted, cannot be recovered
   # Restore from backup if available

   # If WAL is corrupted, SST files may still be valid
   # Try deleting WAL and reopening (loses unflushed data)
   rm /path/to/db.keystone/wal.log
   ```

3. **Prevention:**
   ```bash
   # Enable periodic backups
   # Use filesystem with checksums (ZFS, Btrfs)
   # Monitor disk health (SMART)
   # Use ECC memory for critical deployments
   ```

**Code Example:**
```rust
match Database::open(path) {
    Ok(db) => db,
    Err(Error::Corruption(msg)) => {
        eprintln!("Database corrupted: {}", msg);

        // Try to recover from backup
        if let Ok(backup_db) = Database::open(backup_path) {
            eprintln!("Restored from backup");
            backup_db
        } else {
            eprintln!("No valid backup found");
            return Err(Error::Corruption(msg));
        }
    }
    Err(e) => return Err(e),
}
```

---

### NOT_FOUND

**Code:** `NOT_FOUND`
**Rust Type:** `Error::NotFound(String)`
**Retryable:** No
**Description:** Requested key does not exist in database.

**Example Messages:**
```
Key not found: user#123
Key not found: product#456
```

**Troubleshooting:**

This is not an error condition in most cases - it simply indicates the key doesn't exist. Handle it in application logic:

```rust
match db.get(b"user#123") {
    Ok(Some(item)) => {
        println!("Found user: {:?}", item);
    }
    Ok(None) => {
        println!("User not found - creating new user");
        db.put(b"user#123", default_user)?;
    }
    Err(e) => {
        eprintln!("Database error: {}", e);
        return Err(e);
    }
}
```

---

### INVALID_ARGUMENT

**Code:** `INVALID_ARGUMENT`
**Rust Type:** `Error::InvalidArgument(String)`
**Retryable:** No
**Description:** Invalid parameter provided to database operation.

**Common Causes:**
- Empty key
- Invalid configuration values
- Malformed query parameters
- Invalid expression syntax
- Type mismatch

**Example Messages:**
```
Invalid argument: Key cannot be empty
Invalid argument: max_memtable_records must be greater than 0
Invalid argument: Invalid sort key condition
Invalid argument: Expression syntax error at position 15
```

**Troubleshooting:**

1. **Empty key:**
   ```rust
   // ❌ Invalid
   db.put(b"", item)?;

   // ✅ Valid
   db.put(b"user#123", item)?;
   ```

2. **Invalid configuration:**
   ```rust
   // ❌ Invalid (zero records)
   let config = DatabaseConfig::new()
       .with_max_memtable_records(0);

   // ✅ Valid
   let config = DatabaseConfig::new()
       .with_max_memtable_records(1000);

   // Always validate
   config.validate()?;
   ```

3. **Invalid expression:**
   ```rust
   // ❌ Invalid (syntax error)
   Update::new(b"key")
       .expression("SET age = ")  // Incomplete

   // ✅ Valid
   Update::new(b"key")
       .expression("SET age = :val")
       .value(":val", Value::number(30))
   ```

---

### ALREADY_EXISTS

**Code:** `ALREADY_EXISTS`
**Rust Type:** `Error::AlreadyExists(String)`
**Retryable:** No
**Description:** Database or resource already exists.

**Example Messages:**
```
Database already exists: /path/to/db.keystone
```

**Troubleshooting:**

```rust
// ❌ Fails if database exists
let db = Database::create(path)?;

// ✅ Open existing or create new
let db = if std::path::Path::new(path).exists() {
    Database::open(path)?
} else {
    Database::create(path)?
};

// ✅ Or use a helper
fn open_or_create(path: impl AsRef<Path>) -> Result<Database> {
    match Database::open(&path) {
        Ok(db) => Ok(db),
        Err(Error::Io(e)) if e.kind() == ErrorKind::NotFound => {
            Database::create(path)
        }
        Err(e) => Err(e),
    }
}
```

---

### CHECKSUM_MISMATCH

**Code:** `CHECKSUM_MISMATCH`
**Rust Type:** `Error::ChecksumMismatch`
**Retryable:** No
**Description:** CRC32C checksum verification failed.

**Common Causes:**
- Disk corruption
- Incomplete write during crash
- Hardware failure
- Filesystem bugs

**Troubleshooting:**

```bash
# File is corrupted and cannot be recovered
# Restore from backup or delete corrupted file

# For WAL corruption:
rm /path/to/db.keystone/wal.log
# (Loses unflushed data, but SSTs are intact)

# For SST corruption:
# Identify corrupted file from error message
# Delete it and rely on other SSTs + WAL
rm /path/to/db.keystone/042-15.sst
```

**Prevention:**
- Regular backups
- Filesystem with checksums (ZFS, Btrfs)
- Monitor disk health
- Use UPS for servers

---

### CONDITIONAL_CHECK_FAILED

**Code:** `CONDITIONAL_CHECK_FAILED`
**Rust Type:** `Error::ConditionalCheckFailed(String)`
**Retryable:** No (but can retry with updated condition)
**Description:** Conditional write failed because condition was not met.

**Example Messages:**
```
Conditional check failed: age = :old_age
Conditional check failed: attribute_exists(email)
```

**Troubleshooting:**

This is expected behavior for optimistic locking:

```rust
// Optimistic locking pattern
loop {
    // Read current value
    let current = db.get(b"counter")?.unwrap_or_default();
    let current_value = current.get("count")
        .and_then(|v| v.as_number())
        .and_then(|n| n.parse::<i64>().ok())
        .unwrap_or(0);

    // Try to update with condition
    let result = db.update(b"counter")
        .expression("SET count = :new_val")
        .condition("count = :old_val")
        .value(":new_val", Value::number(current_value + 1))
        .value(":old_val", Value::number(current_value))
        .execute();

    match result {
        Ok(_) => break,  // Success
        Err(Error::ConditionalCheckFailed(_)) => {
            // Concurrent modification - retry
            continue;
        }
        Err(e) => return Err(e),
    }
}
```

---

### TRANSACTION_CANCELED

**Code:** `TRANSACTION_CANCELED`
**Rust Type:** `Error::TransactionCanceled(String)`
**Retryable:** Yes (safe to retry entire transaction)
**Description:** Transaction was aborted because a condition failed.

**Example Messages:**
```
Transaction canceled: balance >= :amount
Transaction canceled: attribute_exists(active)
```

**Troubleshooting:**

```rust
// Retry pattern for transactions
for attempt in 0..3 {
    let result = db.transact_write()
        .update(b"account#1", "SET balance = balance - 100")
        .condition_check(b"account#1", "balance >= 100")
        .update(b"account#2", "SET balance = balance + 100")
        .execute();

    match result {
        Ok(_) => return Ok(()),
        Err(Error::TransactionCanceled(msg)) if attempt < 2 => {
            eprintln!("Transaction failed (attempt {}): {}", attempt + 1, msg);
            std::thread::sleep(Duration::from_millis(100 * (attempt + 1)));
            continue;  // Retry
        }
        Err(e) => return Err(e),
    }
}
```

---

### INVALID_EXPRESSION

**Code:** `INVALID_EXPRESSION`
**Rust Type:** `Error::InvalidExpression(String)`
**Retryable:** No
**Description:** Invalid expression syntax in update or condition.

**Common Causes:**
- Syntax error in expression
- Missing placeholder value
- Invalid attribute path
- Unsupported operation

**Example Messages:**
```
Invalid expression: Syntax error at position 15
Invalid expression: Missing value for placeholder :val
Invalid expression: Invalid attribute path: foo..bar
```

**Troubleshooting:**

```rust
// ❌ Invalid - missing placeholder value
Update::new(b"key")
    .expression("SET age = :val")
    // Missing: .value(":val", ...)

// ✅ Valid
Update::new(b"key")
    .expression("SET age = :val")
    .value(":val", Value::number(30))

// ❌ Invalid - syntax error
Update::new(b"key")
    .expression("SET age = age + ")  // Incomplete

// ✅ Valid
Update::new(b"key")
    .expression("SET age = age + :inc")
    .value(":inc", Value::number(1))

// ❌ Invalid - unsupported operation
Update::new(b"key")
    .expression("SET name = name * 2")  // Can't multiply strings

// ✅ Valid
Update::new(b"key")
    .expression("SET count = count * :mult")
    .value(":mult", Value::number(2))
```

---

### INVALID_QUERY

**Code:** `INVALID_QUERY`
**Rust Type:** `Error::InvalidQuery(String)`
**Retryable:** No
**Description:** Invalid query parameters or PartiQL statement.

**Common Causes:**
- Missing partition key
- Invalid sort key condition
- Unsupported PartiQL syntax
- Type mismatch in comparison

**Example Messages:**
```
Invalid query: Partition key is required
Invalid query: Cannot use > with string sort key
Invalid query: Invalid PartiQL syntax near 'FROM'
```

**Troubleshooting:**

```rust
// ❌ Invalid - missing partition key for query
let query = Query::new()
    .sk_begins_with(b"post#");  // No partition key!

// ✅ Valid
let query = Query::new(b"user#123")
    .sk_begins_with(b"post#");

// ❌ Invalid - conflicting conditions
let query = Query::new(b"user#123")
    .sk_eq(b"exact")
    .sk_begins_with(b"prefix");  // Can't use both!

// ✅ Valid - one condition
let query = Query::new(b"user#123")
    .sk_begins_with(b"prefix");
```

---

### RESOURCE_EXHAUSTED

**Code:** `RESOURCE_EXHAUSTED`
**Rust Type:** `Error::ResourceExhausted(String)`
**Retryable:** Yes (after freeing resources)
**Description:** Resource limit exceeded.

**Common Causes:**
- Disk full
- Maximum database size reached
- Too many open connections
- Memory limit exceeded

**Example Messages:**
```
Resource exhausted: Disk full
Resource exhausted: Maximum database size (100GB) reached
Resource exhausted: Too many concurrent connections
```

**Troubleshooting:**

1. **Disk full:**
   ```bash
   # Free up disk space
   df -h

   # Delete old SST files after compaction
   # Or move database to larger disk
   ```

2. **Database size limit:**
   ```rust
   // Increase limit
   let config = DatabaseConfig::new()
       .with_max_total_disk_bytes(500 * 1024 * 1024 * 1024);  // 500GB

   // Or compact to reclaim space
   db.trigger_compaction()?;
   ```

3. **Too many connections:**
   ```rust
   // Server mode - increase connection limit
   // Or close idle connections
   ```

---

## Error Classification

### Retryable Errors

Safe to retry with exponential backoff:

- `IO_ERROR` (most cases)
- `WAL_FULL`
- `RESOURCE_EXHAUSTED`
- `COMPACTION_ERROR`
- `STRIPE_ERROR`
- `TRANSACTION_CANCELED`

**Retry Pattern:**
```rust
fn retry_operation<T, F>(f: F) -> Result<T>
where
    F: Fn() -> Result<T>,
{
    let max_attempts = 3;
    let mut backoff_ms = 100;

    for attempt in 0..max_attempts {
        match f() {
            Ok(result) => return Ok(result),
            Err(e) if e.is_retryable() && attempt < max_attempts - 1 => {
                std::thread::sleep(Duration::from_millis(backoff_ms));
                backoff_ms *= 2;  // Exponential backoff
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    unreachable!()
}

// Usage
retry_operation(|| {
    db.put(b"key", item.clone())
})?;
```

### Non-Retryable Errors

Indicate programming errors or permanent failures:

- `CORRUPTION`
- `NOT_FOUND`
- `INVALID_ARGUMENT`
- `ALREADY_EXISTS`
- `CHECKSUM_MISMATCH`
- `INVALID_EXPRESSION`
- `CONDITIONAL_CHECK_FAILED`
- `INVALID_QUERY`

Handle these by fixing the application code or data.

## Error Handling Best Practices

### 1. Use is_retryable() Method

```rust
match db.put(b"key", item) {
    Ok(_) => println!("Success"),
    Err(e) if e.is_retryable() => {
        // Retry with backoff
        retry_with_backoff(|| db.put(b"key", item.clone()))?;
    }
    Err(e) => {
        // Permanent error - log and return
        eprintln!("Permanent error: {}", e);
        return Err(e);
    }
}
```

### 2. Add Context to Errors

```rust
db.put(b"user#123", item)
    .map_err(|e| e.with_context("failed to save user profile"))?;
```

### 3. Match Specific Errors

```rust
match db.get(b"key") {
    Ok(Some(item)) => handle_item(item),
    Ok(None) => create_default(),
    Err(Error::Io(e)) if e.kind() == ErrorKind::PermissionDenied => {
        fix_permissions()?;
        retry_get()?
    }
    Err(Error::Corruption(msg)) => {
        restore_from_backup()?
    }
    Err(e) => return Err(e),
}
```

### 4. Log Errors with Code

```rust
match db.put(b"key", item) {
    Ok(_) => {},
    Err(e) => {
        log::error!(
            "Database error [{}]: {}",
            e.code(),  // Stable error code
            e          // Error message
        );
        return Err(e);
    }
}
```

### 5. Convert to Application Errors

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("User not found: {0}")]
    UserNotFound(String),

    #[error("Database error: {0}")]
    Database(#[from] kstone_core::Error),
}

fn get_user(id: &str) -> Result<User, AppError> {
    match db.get(id.as_bytes())? {
        Some(item) => Ok(User::from_item(item)),
        None => Err(AppError::UserNotFound(id.to_string())),
    }
}
```

## Error Monitoring

### Metrics to Track

1. **Error rate by type:**
   ```
   kstone_errors_total{error_code="IO_ERROR"} 5
   kstone_errors_total{error_code="NOT_FOUND"} 123
   kstone_errors_total{error_code="CORRUPTION"} 0
   ```

2. **Retry statistics:**
   ```
   kstone_retries_total{error_code="IO_ERROR"} 15
   kstone_retry_success_total{error_code="IO_ERROR"} 12
   ```

3. **Error latency:**
   ```
   kstone_error_duration_seconds{error_code="CORRUPTION"}
   ```

### Alerting Rules

**Critical Errors (page immediately):**
- Any `CORRUPTION` error
- High rate of `IO_ERROR` (indicates disk failure)
- Any `CHECKSUM_MISMATCH` error

**Warning Errors (investigate):**
- High rate of `RESOURCE_EXHAUSTED`
- Increasing rate of retries
- Sustained `CONDITIONAL_CHECK_FAILED` (contention)

## Summary

- **19 error types** covering all failure modes
- **Stable error codes** for programmatic handling
- **Retryable classification** for automatic recovery
- **Detailed messages** for debugging
- **Context preservation** with error wrapping

For implementation details, see `kstone-core/src/error.rs`.

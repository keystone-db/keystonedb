# Chapter 35: Building Applications with KeystoneDB

This chapter provides practical guidance for building production-ready applications with KeystoneDB. We'll cover best practices, design patterns, optimization techniques, and real-world considerations based on the example applications.

## 35.1 Application Architecture Patterns

### 35.1.1 Embedded Database Architecture

KeystoneDB is an embedded database, meaning it runs in the same process as your application. This architecture provides several advantages:

**Benefits:**
- **Zero network latency**: No network round trips for database operations
- **Simplified deployment**: Single binary deployment with no separate database server
- **Strong consistency**: Direct access to the storage engine
- **Lower resource usage**: No separate database process overhead

**Typical Architecture:**
```
┌─────────────────────────────────────┐
│   Application Process               │
│                                     │
│  ┌──────────────────────────────┐  │
│  │   HTTP/gRPC Server           │  │
│  │   (Axum, Actix, Tonic)       │  │
│  └──────────┬───────────────────┘  │
│             │                       │
│  ┌──────────▼───────────────────┐  │
│  │   Business Logic Layer       │  │
│  │   (Handlers, Services)       │  │
│  └──────────┬───────────────────┘  │
│             │                       │
│  ┌──────────▼───────────────────┐  │
│  │   Database Layer             │  │
│  │   Arc<Database>              │  │
│  └──────────┬───────────────────┘  │
│             │                       │
│  ┌──────────▼───────────────────┐  │
│  │   KeystoneDB Storage Engine  │  │
│  │   (LSM, WAL, SST files)      │  │
│  └──────────────────────────────┘  │
│             │                       │
└─────────────┼───────────────────────┘
              │
              ▼
        ┌──────────┐
        │ Disk I/O │
        └──────────┘
```

### 35.1.2 State Management with Arc

KeystoneDB's `Database` handle can be safely shared across threads using `Arc`:

```rust
use std::sync::Arc;
use kstone_api::Database;
use axum::Router;

#[derive(Clone)]
struct AppState {
    db: Arc<Database>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create database once
    let db = Database::create("myapp.keystone")?;

    // Wrap in Arc for thread-safe sharing
    let state = AppState {
        db: Arc::new(db)
    };

    // Share state across all request handlers
    let app = Router::new()
        .route("/api/items", get(list_items))
        .route("/api/items/:id", get(get_item))
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn get_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Item>, AppError> {
    // Database handle available via shared state
    let key = format!("item#{}", id);
    let item = state.db.get(key.as_bytes())?
        .ok_or(AppError::NotFound)?;

    Ok(Json(item))
}
```

**Key Points:**
- Create `Database` once at application startup
- Wrap in `Arc` for cheap cloning and thread-safe sharing
- Use `#[derive(Clone)]` on your state struct
- Access via extractors in handlers (Axum, Actix, etc.)

### 35.1.3 Multi-Database Pattern

For applications requiring isolation or multi-tenancy:

```rust
use std::collections::HashMap;
use std::sync::RwLock;

struct MultiTenantState {
    databases: Arc<RwLock<HashMap<String, Arc<Database>>>>,
}

impl MultiTenantState {
    fn get_or_create_db(&self, tenant_id: &str) -> anyhow::Result<Arc<Database>> {
        // Check if exists (read lock)
        {
            let dbs = self.databases.read().unwrap();
            if let Some(db) = dbs.get(tenant_id) {
                return Ok(Arc::clone(db));
            }
        }

        // Create new database (write lock)
        let mut dbs = self.databases.write().unwrap();

        // Double-check pattern
        if let Some(db) = dbs.get(tenant_id) {
            return Ok(Arc::clone(db));
        }

        let path = format!("data/{}.keystone", tenant_id);
        let db = Arc::new(Database::create(path)?);
        dbs.insert(tenant_id.to_string(), Arc::clone(&db));

        Ok(db)
    }
}
```

## 35.2 Data Modeling Best Practices

### 35.2.1 Partition Key Design

The partition key determines data distribution and query patterns. Choose partition keys that:

**1. Distribute Load Evenly**
```rust
// ✅ Good: High cardinality, even distribution
let pk = format!("user#{}", user_id);  // Many unique users
let pk = format!("order#{}", order_id); // Many unique orders

// ❌ Bad: Low cardinality, hot partitions
let pk = format!("status#{}", status); // Only a few status values
let pk = format!("country#{}", country); // Only ~200 countries
```

**2. Enable Efficient Queries**
```rust
// ✅ Good: Query all posts by author
let pk = format!("author#{}", author_id);
let sk = format!("post#{}#{}", timestamp, post_id);

// Query implementation
let query = Query::new(pk.as_bytes());
let posts = db.query(query)?;
```

**3. Support Access Patterns**
```rust
// Access pattern: "Get user's recent orders"
// Solution: Use user_id as PK, timestamp in SK

let pk = format!("user#{}", user_id);
let sk = format!("order#{}#{}", timestamp, order_id);

// Efficient query
let query = Query::new(pk.as_bytes())
    .sk_begins_with(b"order#")
    .forward(false)  // Most recent first
    .limit(10);
```

### 35.2.2 Sort Key Strategies

Sort keys provide ordering within a partition:

**Timestamp-Based Ordering**
```rust
// Recent items first (reverse chronological)
let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs();

// Inverted timestamp for reverse ordering
let inverted = u64::MAX - now;
let sk = format!("ts#{:020}", inverted);

// Or use forward=false in query
let query = Query::new(pk.as_bytes())
    .forward(false)
    .limit(20);
```

**Hierarchical Keys**
```rust
// Multi-level hierarchy in sort key
let sk = format!("{}#{}#{}", category, subcategory, item_id);

// Examples:
// "electronics#laptops#item123"
// "electronics#phones#item456"
// "books#fiction#item789"

// Query all items in category
let query = Query::new(pk.as_bytes())
    .sk_begins_with(b"electronics#");
```

**Composite Sort Keys**
```rust
// Combine multiple attributes for sorting
let sk = format!("priority#{}#ts#{}#id#{}",
    priority,    // Sort by priority first
    timestamp,   // Then by time
    item_id      // Finally by ID
);

// High priority items appear first
```

### 35.2.3 Attribute Design

**Use Appropriate Value Types**
```rust
use kstone_api::ItemBuilder;
use kstone_core::Value;

let item = ItemBuilder::new()
    // Strings for text
    .string("email", "user@example.com")
    .string("name", "Alice Smith")

    // Numbers for quantities, IDs, timestamps
    .number("age", 30)
    .number("balance", 100.50)
    .number("created_at", 1704067200)

    // Booleans for flags
    .bool("active", true)
    .bool("verified", false)

    .build();

// Advanced types
let mut item = ItemBuilder::new()
    .string("id", "123")
    .build();

// Timestamps (milliseconds since epoch)
item.insert("expires_at".into(), Value::Ts(1704067200000));

// Vector embeddings
let embedding = vec![0.1, 0.2, 0.3, 0.4];
item.insert("embedding".into(), Value::VecF32(embedding));

// Nested structures (Maps)
let mut address = std::collections::HashMap::new();
address.insert("street".into(), Value::string("123 Main St"));
address.insert("city".into(), Value::string("Boston"));
item.insert("address".into(), Value::M(address));

// Lists
let tags = vec![
    Value::string("rust"),
    Value::string("database"),
];
item.insert("tags".into(), Value::L(tags));
```

## 35.3 Query Optimization

### 35.3.1 Minimize Scans

**Anti-Pattern: Full Table Scans**
```rust
// ❌ Inefficient: Scans entire database
let scan = Scan::new();
let response = db.scan(scan)?;

// Filter in application code
let active_users: Vec<_> = response.items
    .into_iter()
    .filter(|item| {
        matches!(item.get("status"), Some(Value::S(s)) if s == "active")
    })
    .collect();
```

**Better: Use Query with Partition Key**
```rust
// ✅ Efficient: Query single partition
let query = Query::new(format!("org#{}", org_id).as_bytes());
let response = db.query(query)?;
```

**Best: Use Secondary Indexes (Phase 3+)**
```rust
// ✅ Most efficient: Query GSI by status
let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::new("status-index", "status")
    );

let db = Database::create_with_schema(path, schema)?;

// Query by status efficiently
let query = Query::new(b"active")
    .index("status-index");
let active_users = db.query(query)?;
```

### 35.3.2 Pagination Strategies

**Efficient Pagination**
```rust
async fn list_items_paginated(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100); // Cap at 100

    let mut query = Query::new(b"items").limit(limit);

    // Add start key from pagination token
    if let Some(token) = params.next_token {
        let (pk, sk) = decode_pagination_token(&token)?;
        query = query.start_after(&pk, sk.as_deref());
    }

    let response = state.db.query(query)?;

    // Create next token if more results exist
    let next_token = response.last_key
        .map(|(pk, sk)| encode_pagination_token(&pk, sk.as_ref()));

    Ok(Json(PaginatedResponse {
        items: response.items,
        next_token,
        has_more: response.last_key.is_some(),
    }))
}

fn encode_pagination_token(pk: &[u8], sk: Option<&Bytes>) -> String {
    // Base64 encode the continuation key
    use base64::{Engine as _, engine::general_purpose};
    let data = bincode::serialize(&(pk, sk)).unwrap();
    general_purpose::STANDARD.encode(data)
}
```

**Cursor-Based Pagination (Recommended)**
```rust
#[derive(Deserialize)]
struct ListRequest {
    cursor: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
struct ListResponse<T> {
    items: Vec<T>,
    cursor: Option<String>,
    has_more: bool,
}
```

### 35.3.3 Batch Operations

**Batch Reads**
```rust
// Instead of N individual gets
for id in user_ids {
    let item = db.get(format!("user#{}", id).as_bytes())?;
    // Process item
}

// ✅ Use batch_get (up to 100 items)
let request = user_ids.iter()
    .fold(BatchGetRequest::new(), |req, id| {
        req.add_key(format!("user#{}", id).as_bytes())
    });

let response = db.batch_get(request)?;

// Process all items at once
for (key, item) in response.items {
    // Process item
}
```

**Batch Writes**
```rust
// ✅ Batch multiple writes
let mut request = BatchWriteRequest::new();

for item in items_to_create {
    let pk = format!("item#{}", item.id);
    request = request.put(pk.as_bytes(), item.to_keystone_item());
}

for id in items_to_delete {
    let pk = format!("item#{}", id);
    request = request.delete(pk.as_bytes());
}

let response = db.batch_write(request)?;
println!("Processed {} operations", response.processed_count);
```

## 35.4 Error Handling Strategies

### 35.4.1 Application Error Types

Define a custom error type that wraps KeystoneDB errors:

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    Database(kstone_api::KeystoneError),
    NotFound,
    InvalidInput(String),
    Conflict(String),
    Unauthorized,
}

impl From<kstone_api::KeystoneError> for AppError {
    fn from(err: kstone_api::KeystoneError) -> Self {
        AppError::Database(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match self {
            AppError::Database(ref err) => {
                // Map KeystoneDB errors to HTTP status codes
                use kstone_core::Error;
                match err {
                    Error::NotFound(_) => (
                        StatusCode::NOT_FOUND,
                        "NOT_FOUND",
                        err.to_string(),
                    ),
                    Error::InvalidArgument(_) | Error::InvalidExpression(_) => (
                        StatusCode::BAD_REQUEST,
                        "INVALID_REQUEST",
                        err.to_string(),
                    ),
                    Error::ConditionalCheckFailed(_) => (
                        StatusCode::PRECONDITION_FAILED,
                        "CONDITION_FAILED",
                        err.to_string(),
                    ),
                    Error::TransactionCanceled(_) => (
                        StatusCode::CONFLICT,
                        "TRANSACTION_FAILED",
                        err.to_string(),
                    ),
                    Error::ResourceExhausted(_) => (
                        StatusCode::TOO_MANY_REQUESTS,
                        "RATE_LIMIT",
                        err.to_string(),
                    ),
                    _ => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "INTERNAL_ERROR",
                        "An internal error occurred".to_string(),
                    ),
                }
            }
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                "Resource not found".to_string(),
            ),
            AppError::InvalidInput(msg) => (
                StatusCode::BAD_REQUEST,
                "INVALID_INPUT",
                msg,
            ),
            AppError::Conflict(msg) => (
                StatusCode::CONFLICT,
                "CONFLICT",
                msg,
            ),
            AppError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Authentication required".to_string(),
            ),
        };

        let body = Json(json!({
            "error": {
                "code": error_code,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}
```

### 35.4.2 Retry Logic

Implement retry logic for transient errors:

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn with_retry<F, T>(
    mut operation: F,
    max_attempts: u32,
) -> Result<T, AppError>
where
    F: FnMut() -> Result<T, AppError>,
{
    let mut attempts = 0;
    let mut backoff = Duration::from_millis(100);

    loop {
        attempts += 1;

        match operation() {
            Ok(result) => return Ok(result),
            Err(e) => {
                // Check if error is retryable
                let should_retry = match &e {
                    AppError::Database(db_err) => db_err.is_retryable(),
                    _ => false,
                };

                if !should_retry || attempts >= max_attempts {
                    return Err(e);
                }

                // Exponential backoff
                sleep(backoff).await;
                backoff = backoff * 2;
            }
        }
    }
}

// Usage
async fn create_item_with_retry(
    db: Arc<Database>,
    item: Item,
) -> Result<(), AppError> {
    with_retry(|| {
        let key = b"item#123";
        db.put(key, item.clone())?;
        Ok(())
    }, 3).await
}
```

### 35.4.3 Graceful Degradation

Handle database errors gracefully:

```rust
async fn get_user_profile(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<UserProfile>, AppError> {
    let key = format!("user#{}", user_id);

    // Try to get from database
    let profile = match state.db.get(key.as_bytes()) {
        Ok(Some(item)) => UserProfile::from_item(item),
        Ok(None) => return Err(AppError::NotFound),
        Err(e) => {
            // Log error
            tracing::error!("Database error: {}", e);

            // Return cached/default profile if available
            if let Some(cached) = state.cache.get(&user_id) {
                tracing::warn!("Serving cached profile due to DB error");
                cached
            } else {
                return Err(AppError::Database(e));
            }
        }
    };

    Ok(Json(profile))
}
```

## 35.5 Connection and Resource Management

### 35.5.1 Database Lifecycle

Proper initialization and cleanup:

```rust
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize database
    let db = Arc::new(Database::create("app.keystone")?);
    tracing::info!("Database initialized");

    // Create application
    let state = AppState { db: Arc::clone(&db) };
    let app = create_router(state);

    // Start server
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("Server listening on port 3000");

    // Graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Flush database before exit
    db.flush()?;
    tracing::info!("Database flushed, shutting down");

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received");
}
```

### 35.5.2 Resource Limits

Configure appropriate limits:

```rust
use kstone_api::DatabaseConfig;

let config = DatabaseConfig {
    // Memory limits
    memtable_threshold: 1000,  // Flush at 1000 items

    // Compaction settings
    compaction_enabled: true,
    compaction_threshold: 10,   // Compact at 10 SSTs

    // Concurrency
    max_background_jobs: 2,

    // File settings
    block_size: 4096,
    bloom_bits_per_key: 10,
};

let db = Database::create_with_config("app.keystone", config)?;
```

## 35.6 Testing Strategies

### 35.6.1 Unit Testing with Temporary Databases

```rust
use tempfile::TempDir;
use kstone_api::{Database, ItemBuilder};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_user() {
        // Create temporary database
        let temp_dir = TempDir::new().unwrap();
        let db = Database::create(temp_dir.path()).unwrap();

        // Test your logic
        let item = ItemBuilder::new()
            .string("name", "Alice")
            .string("email", "alice@example.com")
            .build();

        db.put(b"user#123", item.clone()).unwrap();

        let retrieved = db.get(b"user#123").unwrap();
        assert_eq!(retrieved, Some(item));

        // Database is automatically cleaned up when temp_dir is dropped
    }

    #[test]
    fn test_user_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::create(temp_dir.path()).unwrap();

        let result = db.get(b"user#nonexistent").unwrap();
        assert!(result.is_none());
    }
}
```

### 35.6.2 Integration Testing

```rust
#[tokio::test]
async fn test_api_create_and_get() {
    use axum_test::TestServer;

    // Create test database
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::create(temp_dir.path()).unwrap());

    // Create test server
    let app = create_router(AppState { db });
    let server = TestServer::new(app).unwrap();

    // Test POST
    let response = server
        .post("/api/users")
        .json(&json!({
            "name": "Alice",
            "email": "alice@example.com"
        }))
        .await;

    assert_eq!(response.status_code(), StatusCode::CREATED);
    let user: User = response.json();

    // Test GET
    let get_response = server
        .get(&format!("/api/users/{}", user.id))
        .await;

    assert_eq!(get_response.status_code(), StatusCode::OK);
    let retrieved_user: User = get_response.json();
    assert_eq!(retrieved_user.id, user.id);
}
```

### 35.6.3 In-Memory Testing

For fast tests without disk I/O:

```rust
#[test]
fn test_fast_in_memory() {
    // Create in-memory database
    let db = Database::create_in_memory().unwrap();

    // Test core operations
    let item = ItemBuilder::new()
        .string("data", "test")
        .build();

    db.put(b"key", item).unwrap();
    assert!(db.get(b"key").unwrap().is_some());

    db.delete(b"key").unwrap();
    assert!(db.get(b"key").unwrap().is_none());

    // No cleanup needed - all in memory
}
```

## 35.7 Performance Best Practices

### 35.7.1 Write Optimization

```rust
// ✅ Batch writes when possible
let mut batch = BatchWriteRequest::new();
for item in items {
    batch = batch.put(item.key(), item.value());
}
db.batch_write(batch)?;

// ✅ Use update expressions for atomic operations
let update = Update::new(b"counter#global")
    .expression("ADD views :inc")
    .value(":inc", Value::number(1));
db.update(update)?;

// ❌ Avoid read-modify-write for counters
let item = db.get(b"counter#global")?.unwrap();
let count: i64 = item.get("views").unwrap().as_number().unwrap();
db.put(b"counter#global",
    ItemBuilder::new().number("views", count + 1).build())?;
```

### 35.7.2 Read Optimization

```rust
// ✅ Use queries instead of scans
let query = Query::new(b"user#123")
    .sk_begins_with(b"order#")
    .limit(10);

// ✅ Limit result set size
let query = Query::new(pk).limit(100);

// ✅ Use indexes for filtering
let query = Query::new(b"active")
    .index("status-index");
```

### 35.7.3 Memory Management

```rust
// Process large result sets in chunks
let mut start_key = None;
let chunk_size = 100;

loop {
    let mut query = Query::new(pk).limit(chunk_size);

    if let Some((pk, sk)) = start_key {
        query = query.start_after(&pk, sk.as_deref());
    }

    let response = db.query(query)?;

    // Process chunk
    for item in response.items {
        process_item(item)?;
    }

    // Check if more results
    match response.last_key {
        Some(key) => start_key = Some(key),
        None => break,
    }
}
```

## 35.8 Production Checklist

Before deploying to production:

**Security**
- [ ] Implement authentication and authorization
- [ ] Validate all user inputs
- [ ] Sanitize data before storage
- [ ] Use encryption for sensitive data
- [ ] Implement rate limiting
- [ ] Add CORS configuration if needed

**Reliability**
- [ ] Implement retry logic for transient errors
- [ ] Add circuit breakers for external dependencies
- [ ] Configure graceful shutdown
- [ ] Set up health checks
- [ ] Implement request timeouts
- [ ] Add proper error logging

**Observability**
- [ ] Add structured logging
- [ ] Implement metrics collection
- [ ] Set up distributed tracing
- [ ] Monitor database health
- [ ] Track operation latencies
- [ ] Alert on error rates

**Performance**
- [ ] Profile critical paths
- [ ] Optimize hot queries
- [ ] Configure appropriate batch sizes
- [ ] Set memory limits
- [ ] Enable compaction
- [ ] Tune flush thresholds

**Operations**
- [ ] Document deployment procedure
- [ ] Set up backup strategy
- [ ] Plan disaster recovery
- [ ] Configure log rotation
- [ ] Set resource limits
- [ ] Document runbook procedures

This chapter provides a solid foundation for building production-ready applications with KeystoneDB. The patterns and practices shown here are battle-tested in real-world applications and will help you build robust, scalable systems.

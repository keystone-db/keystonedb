# Chapter 37: Example Projects

This chapter provides detailed walkthroughs of the example applications included with KeystoneDB. Each example demonstrates progressively more advanced features, providing a learning path from basic CRUD operations to complex multi-user systems with indexes and analytics.

## 37.1 Learning Progression

The examples are designed to build on each other:

| Example | Complexity | Key Features | Lines of Code |
|---------|-----------|--------------|---------------|
| URL Shortener | Beginner | Basic CRUD, TTL, Visit tracking | ~300 |
| Cache Server | Intermediate | In-memory mode, Resource limits | ~400 |
| Todo API | Advanced | Updates, Conditions, Batch ops | ~600 |
| Blog Engine | Expert | Composite keys, Queries, Analytics | ~800 |

**Recommended Learning Path:**
1. Start with **URL Shortener** to understand basic operations
2. Move to **Cache Server** for in-memory usage and configuration
3. Study **Todo API** for advanced features like updates and transactions
4. Master **Blog Engine** for complex data modeling and queries

## 37.2 URL Shortener

### 37.2.1 Overview

A simple URL shortening service that demonstrates the fundamentals of KeystoneDB.

**What You'll Learn:**
- Basic CRUD operations (put, get, delete)
- TTL for automatic expiration
- Visit counter with read-modify-write
- REST API design with Axum
- Health checks and statistics

**Architecture:**
```
┌─────────────────────────────────────────┐
│          HTTP Requests                  │
└─────────────┬───────────────────────────┘
              │
    ┌─────────▼─────────┐
    │   Axum Router     │
    └─────────┬─────────┘
              │
    ┌─────────▼─────────────────────────┐
    │  Request Handlers                 │
    │  • shorten_url                    │
    │  • redirect_url                   │
    │  • get_stats                      │
    │  • delete_url                     │
    └─────────┬─────────────────────────┘
              │
    ┌─────────▼─────────┐
    │   Arc<Database>   │
    └─────────┬─────────┘
              │
    ┌─────────▼─────────┐
    │  KeystoneDB       │
    │  url-shortener/   │
    └───────────────────┘
```

### 37.2.2 Data Model

**Key Schema:**
```
PK: "url#{short_code}"

Attributes:
{
  "long_url": String,      // Original URL
  "short_code": String,    // 6-character code (nanoid)
  "visits": Number,        // Access counter
  "created_at": Number,    // Unix timestamp
  "ttl": Number?           // Optional expiration time
}
```

**Design Rationale:**
- Single partition key (no sort key needed)
- Short code in key for O(1) lookups
- TTL as attribute (not KeystoneDB's built-in TTL) for flexibility
- Visit counter demonstrates read-modify-write pattern

### 37.2.3 Key Code Patterns

#### Creating Short URLs

```rust
async fn shorten_url(
    State(state): State<AppState>,
    Json(request): Json<ShortenRequest>,
) -> Result<Json<ShortenResponse>, AppError> {
    // Generate unique short code
    let short_code = nanoid::nanoid!(6);  // e.g., "abc123"

    // Get current timestamp
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Calculate expiration if TTL provided
    let ttl = request.ttl_seconds.map(|ttl| now + ttl);

    // Build item
    let mut builder = ItemBuilder::new()
        .string("long_url", &request.long_url)
        .string("short_code", &short_code)
        .number("visits", 0)
        .number("created_at", now as i64);

    if let Some(ttl_val) = ttl {
        builder = builder.number("ttl", ttl_val as i64);
    }

    // Store in database
    let key = format!("url#{}", short_code);
    state.db.put(key.as_bytes(), builder.build())?;

    Ok(Json(ShortenResponse {
        short_code: short_code.clone(),
        short_url: format!("http://127.0.0.1:3000/{}", short_code),
        long_url: request.long_url,
        expires_at: ttl,
    }))
}
```

**Pattern Demonstrated:**
- Generating unique identifiers (nanoid)
- Conditional attributes (TTL is optional)
- Builder pattern for clean item creation
- Returning structured responses

#### Redirecting and Tracking Visits

```rust
async fn redirect_url(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Redirect, AppError> {
    let key = format!("url#{}", code);

    // Get item
    let item = state.db.get(key.as_bytes())?
        .ok_or(AppError::NotFound)?;

    // Check expiration
    if let Some(KeystoneValue::N(ttl_str)) = item.get("ttl") {
        let ttl: u64 = ttl_str.parse().unwrap_or(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now > ttl {
            return Err(AppError::Expired);  // HTTP 410 Gone
        }
    }

    // Extract long URL
    let long_url = match item.get("long_url") {
        Some(KeystoneValue::S(url)) => url.clone(),
        _ => return Err(AppError::InvalidData),
    };

    // Increment visit counter (read-modify-write)
    let visits = match item.get("visits") {
        Some(KeystoneValue::N(n)) => n.parse::<i64>().unwrap_or(0) + 1,
        _ => 1,
    };

    // Update item with new visit count
    let updated_item = ItemBuilder::new()
        .string("long_url", &long_url)
        .string("short_code", &code)
        .number("visits", visits)
        .number("created_at", /* ... */)
        .build();

    state.db.put(key.as_bytes(), updated_item)?;

    // Redirect to original URL
    Ok(Redirect::permanent(&long_url))
}
```

**Patterns Demonstrated:**
- Manual TTL checking (application-level)
- Read-modify-write for counters
- Pattern matching on Value types
- HTTP redirects

**Production Note:** In production, use update expressions for atomic counters:
```rust
let update = Update::new(key.as_bytes())
    .expression("ADD visits :inc")
    .value(":inc", Value::number(1));
state.db.update(update)?;
```

### 37.2.4 API Endpoints

```bash
# Create short URL
POST /shorten
{
  "long_url": "https://example.com/very/long/url",
  "ttl_seconds": 3600  // Optional
}

# Redirect (increments counter)
GET /:code

# Get statistics
GET /api/stats/:code

# Delete URL
DELETE /api/delete/:code

# Health check
GET /api/health

# Database stats
GET /api/stats
```

### 37.2.5 Running the Example

```bash
cd examples/url-shortener
cargo run

# In another terminal:
curl -X POST http://localhost:3000/shorten \
  -H "Content-Type: application/json" \
  -d '{"long_url": "https://example.com"}'

# Response:
{
  "short_code": "abc123",
  "short_url": "http://127.0.0.1:3000/abc123",
  "long_url": "https://example.com",
  "expires_at": null
}

# Test redirect
curl -L http://localhost:3000/abc123
```

---

## 37.3 Cache Server

### 37.3.1 Overview

A high-performance in-memory cache server demonstrating KeystoneDB's memory-only mode.

**What You'll Learn:**
- In-memory database mode
- TTL-based expiration (KeystoneDB native)
- Resource limits and configuration
- Prometheus metrics (planned)
- LRU eviction simulation

**Architecture:**
```
┌─────────────────────┐
│   Cache Clients     │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│   HTTP API          │
│   • GET /cache/:key │
│   • PUT /cache/:key │
│   • DELETE          │
└──────────┬──────────┘
           │
┌──────────▼──────────┐
│  MemoryLsmEngine    │
│  (No disk I/O)      │
└─────────────────────┘
```

### 37.3.2 In-Memory Mode

**Creating In-Memory Database:**
```rust
use kstone_api::{Database, TableSchema};

// Enable TTL on expiresAt attribute
let schema = TableSchema::new()
    .with_ttl("expiresAt");

let db = Database::create_in_memory_with_schema(schema)?;
```

**Benefits:**
- Zero disk I/O (pure RAM)
- Faster than disk-backed mode
- Perfect for caching use cases
- Data lost on restart (by design)

### 37.3.3 TTL Implementation

```rust
async fn put_cache(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(request): Json<PutRequest>,
) -> Result<StatusCode, AppError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Calculate expiration time
    let expires_at = now + request.ttl_seconds.unwrap_or(3600);

    // Store with TTL
    let item = ItemBuilder::new()
        .string("value", &request.value)
        .number("expiresAt", expires_at)
        .number("created_at", now)
        .build();

    let cache_key = format!("cache#{}", key);
    state.db.put(cache_key.as_bytes(), item)?;

    Ok(StatusCode::CREATED)
}

async fn get_cache(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<CacheValue>, AppError> {
    let cache_key = format!("cache#{}", key);

    // KeystoneDB automatically filters expired items
    let item = state.db.get(cache_key.as_bytes())?
        .ok_or(AppError::NotFound)?;  // Either not found or expired

    let value = item.get("value")
        .and_then(|v| v.as_string())
        .ok_or(AppError::InvalidData)?;

    Ok(Json(CacheValue {
        value: value.to_string(),
    }))
}
```

**Pattern Demonstrated:**
- Lazy deletion: Expired items filtered on read
- No background cleanup needed
- TTL as schema configuration
- Automatic expiration handling

### 37.3.4 Configuration

```rust
#[derive(Debug, Deserialize)]
struct CacheConfig {
    max_memory_mb: usize,
    default_ttl_seconds: u64,
    eviction_policy: EvictionPolicy,
}

#[derive(Debug, Deserialize)]
enum EvictionPolicy {
    LRU,    // Least Recently Used
    LFU,    // Least Frequently Used
    FIFO,   // First In First Out
}
```

### 37.3.5 Use Cases

**When to Use:**
- Session storage
- Rate limiting counters
- Temporary data
- Fast lookups without persistence

**When NOT to Use:**
- Data must survive restarts
- Need durability guarantees
- Long-term storage

---

## 37.4 Todo API

### 37.4.1 Overview

A comprehensive todo list API demonstrating advanced KeystoneDB features.

**What You'll Learn:**
- Update expressions
- Conditional operations
- Batch operations
- Transactions (simulated)
- Input validation
- Complex state machines (Pending → InProgress → Completed)

**Architecture:**
```
┌──────────────────────────┐
│     REST API             │
│  • POST /todos           │
│  • GET /todos/:id        │
│  • PATCH /todos/:id      │
│  • DELETE /todos/:id     │
│  • POST /todos/:id/complete │
│  • POST /todos/batch     │
└───────────┬──────────────┘
            │
┌───────────▼──────────────┐
│   Business Logic         │
│  • Validation            │
│  • State transitions     │
│  • Authorization         │
└───────────┬──────────────┘
            │
┌───────────▼──────────────┐
│   KeystoneDB             │
│   todo-api.keystone/     │
└──────────────────────────┘
```

### 37.4.2 Data Model

**Key Schema:**
```
PK: "todo#{uuid}"

Attributes:
{
  "id": String,             // UUID v4
  "title": String,          // Required
  "description": String,    // Optional
  "status": String,         // "pending" | "inprogress" | "completed"
  "priority": Number,       // 1-5 (higher = more important)
  "created_at": Number,     // Unix timestamp
  "updated_at": Number,     // Unix timestamp
  "completed_at": Number?   // Set when status = completed
}
```

**Status Flow:**
```
┌─────────┐      ┌─────────────┐      ┌───────────┐
│ Pending │─────>│ InProgress  │─────>│ Completed │
└─────────┘      └─────────────┘      └───────────┘
```

### 37.4.3 Update Expressions

#### Updating Todo Status

```rust
async fn update_todo(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateTodoRequest>,
) -> Result<Json<TodoResponse>, AppError> {
    let key = format!("todo#{}", id);

    // Build update expression dynamically
    let mut expression_parts = Vec::new();
    let mut context = ExpressionContext::new();

    // Always update timestamp
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    expression_parts.push("SET updated_at = :now".to_string());
    context = context.with_value(":now", Value::number(now));

    // Optional fields
    if let Some(title) = request.title {
        expression_parts.push("title = :title".to_string());
        context = context.with_value(":title", Value::string(title));
    }

    if let Some(status) = request.status {
        expression_parts.push("status = :status".to_string());
        context = context.with_value(":status", Value::string(status.as_str()));

        // Set completed_at if completing
        if status == TodoStatus::Completed {
            expression_parts.push("completed_at = :completed".to_string());
            context = context.with_value(":completed", Value::number(now));
        }
    }

    if let Some(priority) = request.priority {
        expression_parts.push("priority = :priority".to_string());
        context = context.with_value(":priority", Value::number(priority));
    }

    // Build final expression
    let expression = expression_parts.join(", ");

    // Execute update
    let update = Update::new(key.as_bytes())
        .expression(&expression)
        .context(context);

    let response = state.db.update(update)?;

    Ok(Json(TodoResponse::from_item(response.item)))
}
```

**Pattern Demonstrated:**
- Dynamic expression building
- Optional field updates
- Automatic timestamp tracking
- State-dependent fields (completed_at)

### 37.4.4 Conditional Completion

Prevent double completion using conditional updates:

```rust
async fn complete_todo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let key = format!("todo#{}", id);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Update only if not already completed
    let update = Update::new(key.as_bytes())
        .expression("SET status = :status, completed_at = :now, updated_at = :now")
        .condition("status <> :completed")  // Prevent double completion
        .value(":status", Value::string("completed"))
        .value(":completed", Value::string("completed"))
        .value(":now", Value::number(now));

    match state.db.update(update) {
        Ok(_) => Ok(StatusCode::OK),
        Err(Error::ConditionalCheckFailed(_)) => {
            Err(AppError::Conflict("Todo already completed".into()))
        }
        Err(e) => Err(AppError::Database(e)),
    }
}
```

**Pattern Demonstrated:**
- Optimistic concurrency control
- Idempotency protection
- Error-specific handling

### 37.4.5 Batch Operations

Execute multiple operations efficiently:

```rust
#[derive(Deserialize)]
struct BatchRequest {
    operations: Vec<BatchOperation>,
}

#[derive(Deserialize)]
#[serde(tag = "operation")]
enum BatchOperation {
    Create { title: String, priority: u8 },
    Update { id: String, status: TodoStatus },
    Delete { id: String },
}

async fn batch_operations(
    State(state): State<AppState>,
    Json(request): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, AppError> {
    let mut results = Vec::new();

    // Process each operation
    for op in request.operations {
        let result = match op {
            BatchOperation::Create { title, priority } => {
                let id = uuid::Uuid::new_v4().to_string();
                let item = create_todo_item(&id, &title, priority);

                state.db.put(format!("todo#{}", id).as_bytes(), item)?;

                BatchResult {
                    operation: "create".into(),
                    success: true,
                    id: Some(id),
                    error: None,
                }
            }
            BatchOperation::Update { id, status } => {
                let update = Update::new(format!("todo#{}", id).as_bytes())
                    .expression("SET status = :status, updated_at = :now")
                    .value(":status", Value::string(status.as_str()))
                    .value(":now", Value::number(now()));

                match state.db.update(update) {
                    Ok(_) => BatchResult {
                        operation: "update".into(),
                        success: true,
                        id: Some(id),
                        error: None,
                    },
                    Err(e) => BatchResult {
                        operation: "update".into(),
                        success: false,
                        id: Some(id),
                        error: Some(e.to_string()),
                    },
                }
            }
            BatchOperation::Delete { id } => {
                state.db.delete(format!("todo#{}", id).as_bytes())?;

                BatchResult {
                    operation: "delete".into(),
                    success: true,
                    id: Some(id),
                    error: None,
                }
            }
        };

        results.push(result);
    }

    Ok(Json(BatchResponse {
        success: true,
        operations_completed: results.len(),
        results,
    }))
}
```

**Pattern Demonstrated:**
- Batch processing with partial failure handling
- Heterogeneous operation types
- Result collection and reporting
- Error isolation (one failure doesn't stop others)

**Future Enhancement:** Use `TransactWriteRequest` for atomic batch operations.

### 37.4.6 Input Validation

```rust
#[derive(Deserialize)]
struct CreateTodoRequest {
    title: String,
    description: Option<String>,
    priority: Option<u8>,
}

impl CreateTodoRequest {
    fn validate(&self) -> Result<(), ValidationError> {
        // Title required and length check
        if self.title.trim().is_empty() {
            return Err(ValidationError::EmptyTitle);
        }

        if self.title.len() > 200 {
            return Err(ValidationError::TitleTooLong);
        }

        // Priority range check
        if let Some(p) = self.priority {
            if !(1..=5).contains(&p) {
                return Err(ValidationError::InvalidPriority);
            }
        }

        Ok(())
    }
}
```

---

## 37.5 Blog Engine

### 37.5.1 Overview

A multi-user blog platform demonstrating the most advanced KeystoneDB features.

**What You'll Learn:**
- Composite keys (PK + SK)
- Query API with sort key conditions
- Global Secondary Indexes (simulated)
- Hierarchical data modeling
- Tag-based search
- View tracking and analytics
- Pagination

**Architecture:**
```
┌────────────────────────────────────────┐
│           Blog Platform                │
│                                        │
│  Posts by Author                       │
│  ┌──────────────────────────────┐     │
│  │ PK: author#alice             │     │
│  │ SK: post#1704067200#uuid1    │     │
│  │ SK: post#1704067201#uuid2    │     │
│  │ SK: post#1704067202#uuid3    │     │
│  └──────────────────────────────┘     │
│                                        │
│  Tags (via scan - GSI planned)         │
│  ┌──────────────────────────────┐     │
│  │ Scan all posts               │     │
│  │ Filter by tag in attributes  │     │
│  └──────────────────────────────┘     │
│                                        │
│  Analytics                             │
│  ┌──────────────────────────────┐     │
│  │ Scan + sort by views         │     │
│  └──────────────────────────────┘     │
└────────────────────────────────────────┘
```

### 37.5.2 Composite Key Design

**Key Schema:**
```
PK: "author#{author_id}"
SK: "post#{timestamp}#{post_id}"

Attributes:
{
  "author_id": String,
  "post_id": String,      // UUID
  "title": String,
  "content": String,
  "tags": String,         // Comma-separated
  "views": Number,
  "created_at": Number,
  "updated_at": Number
}
```

**Design Benefits:**

1. **Automatic Ordering:** Posts naturally sorted by creation time within each author
2. **Efficient Queries:** Get all posts by author in O(log n)
3. **Range Queries:** Query posts within time ranges
4. **Scalability:** Data distributed by author (partition key)

**Sort Key Format:**
```
post#1704067200#550e8400-e29b-41d4-a716-446655440000
     ^^^^^^^^^^  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
     timestamp   UUID (uniqueness)
```

### 37.5.3 Querying Posts by Author

```rust
async fn list_author_posts(
    State(state): State<AppState>,
    Path(author_id): Path<String>,
) -> Result<Json<PostListResponse>, AppError> {
    let pk = format!("author#{}", author_id);

    // Query all posts by this author
    let query = Query::new(pk.as_bytes());
    let response = state.db.query(query)?;

    // Parse items into Post structs
    let posts: Vec<Post> = response.items
        .into_iter()
        .filter_map(|item| Post::from_item(item).ok())
        .collect();

    Ok(Json(PostListResponse {
        posts,
        count: posts.len(),
    }))
}
```

**Pattern Demonstrated:**
- Query returns items in SK order (chronological)
- No need to manually sort
- Efficient single-partition query

### 37.5.4 Creating Posts with Composite Keys

```rust
async fn create_post(
    State(state): State<AppState>,
    Json(request): Json<CreatePostRequest>,
) -> Result<Json<Post>, AppError> {
    let post_id = uuid::Uuid::new_v4().to_string();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Build composite sort key: post#{timestamp}#{uuid}
    let pk = format!("author#{}", request.author_id);
    let sk = format!("post#{}#{}", now, post_id);

    // Create item
    let tags_str = request.tags.join(",");
    let item = ItemBuilder::new()
        .string("author_id", &request.author_id)
        .string("post_id", &post_id)
        .string("title", &request.title)
        .string("content", &request.content)
        .string("tags", &tags_str)
        .number("views", 0)
        .number("created_at", now as i64)
        .number("updated_at", now as i64)
        .build();

    // Store with composite key
    state.db.put_with_sk(pk.as_bytes(), sk.as_bytes(), item.clone())?;

    Ok(Json(Post::from_item(item)?))
}
```

**Pattern Demonstrated:**
- Timestamp in SK for ordering
- UUID for uniqueness
- Denormalized attributes (author_id in item)

### 37.5.5 Retrieving Specific Post

```rust
async fn get_post(
    State(state): State<AppState>,
    Path((author_id, post_id)): Path<(String, String)>,
) -> Result<Json<Post>, AppError> {
    let pk = format!("author#{}", author_id);

    // Query all posts by author
    let query = Query::new(pk.as_bytes());
    let response = state.db.query(query)?;

    // Find post with matching ID
    let post = response.items
        .into_iter()
        .find(|item| {
            item.get("post_id")
                .and_then(|v| v.as_string())
                .map(|id| id == post_id)
                .unwrap_or(false)
        })
        .ok_or(AppError::NotFound)?;

    // Increment view counter
    increment_views(&state.db, &pk, &post_id).await?;

    Ok(Json(Post::from_item(post)?))
}
```

**Note:** This queries the partition and filters in application code. With full SK knowledge, you could use `get_with_sk` directly.

### 37.5.6 Tag-Based Search

Current implementation uses scan (O(n)):

```rust
async fn get_posts_by_tag(
    State(state): State<AppState>,
    Path(tag): Path<String>,
) -> Result<Json<PostListResponse>, AppError> {
    // Scan all items
    let scan = Scan::new();
    let response = state.db.scan(scan)?;

    // Filter by tag in application
    let posts: Vec<Post> = response.items
        .into_iter()
        .filter_map(|item| {
            if let Some(Value::S(tags_str)) = item.get("tags") {
                if tags_str.split(',').any(|t| t.trim() == tag) {
                    return Post::from_item(item).ok();
                }
            }
            None
        })
        .collect();

    Ok(Json(PostListResponse {
        posts,
        count: posts.len(),
    }))
}
```

**Future Enhancement with GSI:**
```rust
// Create GSI on tags (one entry per tag)
let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::new("tags-index", "tag")
    );

// Query would be O(log n):
let query = Query::new(format!("tag#{}", tag).as_bytes())
    .index("tags-index");
let response = db.query(query)?;
```

### 37.5.7 Analytics: Popular Posts

```rust
async fn get_popular_posts(
    State(state): State<AppState>,
) -> Result<Json<PostListResponse>, AppError> {
    // Scan all posts
    let scan = Scan::new();
    let response = state.db.scan(scan)?;

    // Parse and sort by views
    let mut posts: Vec<Post> = response.items
        .into_iter()
        .filter_map(|item| Post::from_item(item).ok())
        .collect();

    // Sort by views (descending)
    posts.sort_by(|a, b| b.views.cmp(&a.views));

    // Return top 10
    posts.truncate(10);

    Ok(Json(PostListResponse {
        posts,
        count: posts.len(),
    }))
}
```

**Pattern Demonstrated:**
- Scan for analytics
- In-memory sorting
- Top-N queries

**Future Enhancement with PartiQL:**
```sql
SELECT * FROM posts
ORDER BY views DESC
LIMIT 10;
```

### 37.5.8 Pagination

```rust
async fn list_posts_paginated(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<PaginatedPostsResponse>, AppError> {
    let limit = params.limit.unwrap_or(20).min(100);

    let mut query = Query::new(b"all_posts").limit(limit);

    // Decode pagination token
    if let Some(token) = params.cursor {
        let (pk, sk) = decode_cursor(&token)?;
        query = query.start_after(&pk, Some(&sk));
    }

    let response = state.db.query(query)?;

    // Create next cursor
    let next_cursor = response.last_key
        .map(|(pk, sk)| encode_cursor(&pk, sk.as_ref()));

    Ok(Json(PaginatedPostsResponse {
        posts: response.items.into_iter().filter_map(Post::from_item).collect(),
        cursor: next_cursor,
        has_more: next_cursor.is_some(),
    }))
}

fn encode_cursor(pk: &[u8], sk: Option<&Bytes>) -> String {
    use base64::{Engine as _, engine::general_purpose};
    let data = (pk, sk);
    general_purpose::STANDARD.encode(bincode::serialize(&data).unwrap())
}

fn decode_cursor(cursor: &str) -> Result<(Bytes, Bytes), AppError> {
    use base64::{Engine as _, engine::general_purpose};
    let data = general_purpose::STANDARD.decode(cursor)
        .map_err(|_| AppError::InvalidInput("Invalid cursor".into()))?;
    let (pk, sk): (Vec<u8>, Option<Vec<u8>>) = bincode::deserialize(&data)
        .map_err(|_| AppError::InvalidInput("Invalid cursor".into()))?;
    Ok((Bytes::from(pk), Bytes::from(sk.unwrap_or_default())))
}
```

**Pattern Demonstrated:**
- Cursor-based pagination
- Base64-encoded continuation tokens
- Opaque cursors (implementation can change)

---

## 37.6 Comparison Table

| Feature | URL Shortener | Cache Server | Todo API | Blog Engine |
|---------|---------------|--------------|----------|-------------|
| **CRUD Operations** | ✅ Basic | ✅ Basic | ✅ Advanced | ✅ Expert |
| **Composite Keys** | ❌ | ❌ | ❌ | ✅ |
| **Query API** | ❌ | ❌ | ❌ | ✅ |
| **Update Expressions** | ❌ | ❌ | ✅ | ✅ |
| **Conditional Ops** | ❌ | ❌ | ✅ | ✅ |
| **Batch Operations** | ❌ | ❌ | ✅ | ❌ |
| **TTL** | ✅ Manual | ✅ Native | ❌ | ❌ |
| **In-Memory Mode** | ❌ | ✅ | ❌ | ❌ |
| **Analytics** | ✅ Simple | ❌ | ✅ Stats | ✅ Complex |
| **Indexes** | ❌ | ❌ | ❌ | ✅ Planned |

---

## 37.7 Next Steps

After mastering these examples:

1. **Build Your Own Application**
   - Choose a use case
   - Design data model
   - Implement access patterns
   - Add observability

2. **Optimize Performance**
   - Profile critical paths
   - Add indexes where needed
   - Use batch operations
   - Implement caching

3. **Add Production Features**
   - Authentication
   - Rate limiting
   - Backup strategies
   - Monitoring

4. **Explore Advanced Features**
   - Streams for change data capture
   - Transactions for ACID guarantees
   - PartiQL for complex queries
   - Vector search for embeddings

Each example builds foundational knowledge. Start simple, experiment, and gradually adopt advanced patterns as your application grows.

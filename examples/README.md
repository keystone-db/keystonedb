# KeystoneDB Examples

This directory contains example applications demonstrating KeystoneDB's features and best practices.

## Available Examples

### 1. URL Shortener (`url-shortener/`)

A simple URL shortening service showcasing basic operations.

**Features:**
- Basic CRUD operations (put/get/delete)
- TTL for automatic expiration
- Visit counter with conditional updates
- REST API with proper error handling
- Health checks and database statistics

**Run it:**
```bash
cd examples/url-shortener
cargo run
```

See [url-shortener/README.md](url-shortener/README.md) for full documentation.

---

### 2. Cache Server (`cache-server/`)

A high-performance in-memory cache server with REST API.

**Features:**
- In-memory mode (no disk persistence)
- TTL-based expiration
- Configurable resource limits
- LRU eviction
- Prometheus metrics

**Run it:**
```bash
cd examples/cache-server
cargo run
```

See [cache-server/README.md](cache-server/README.md) for full documentation.

---

### 3. Todo List API (`todo-api/`)

A complete todo list REST API with advanced features.

**Features:**
- CRUD operations
- Update expressions
- Conditional operations
- Transactions for batch operations
- Global Secondary Index for querying by status
- Health and stats endpoints

**Run it:**
```bash
cd examples/todo-api
cargo run
```

See [todo-api/README.md](todo-api/README.md) for full documentation.

---

### 4. Blog Engine (`blog-engine/`)

A multi-user blog platform with advanced querying.

**Features:**
- Composite keys (author + post ID)
- Query with sort key conditions
- Global Secondary Index for tag-based search
- Local Secondary Index for sorting by popularity
- PartiQL queries for analytics
- Real-time updates via Server-Sent Events
- Pagination

**Run it:**
```bash
cd examples/blog-engine
cargo run
```

See [blog-engine/README.md](blog-engine/README.md) for full documentation.

---

### 5. Language Bindings Examples

KeystoneDB provides bindings for multiple languages, enabling you to use the database from Go, Python, and JavaScript.

#### Go Embedded (`go-embedded/`)

Direct in-process database access via CGO/FFI.

**Features:**
- Create, open, and close databases
- Basic CRUD with partition and sort keys
- Error handling
- Zero network latency

**Run it:**
```bash
cd examples/go-embedded
go run .
```

See [go-embedded/README.md](go-embedded/README.md) and [../BINDINGS.md](../BINDINGS.md#go-embedded) for details.

#### Python Embedded (`python-embedded/`)

Direct in-process database access via PyO3.

**Features:**
- Contact manager CLI
- Full value type support (string, number, boolean, list, map)
- In-memory mode option
- Pythonic API

**Run it:**
```bash
cd examples/python-embedded
pip install -r requirements.txt
python contacts.py
```

See [python-embedded/README.md](python-embedded/README.md) and [../BINDINGS.md](../BINDINGS.md#python-embedded) for details.

#### gRPC Clients (`grpc-client/`)

Remote database access for Go, Python, and JavaScript.

**Features:**
- Multi-language interoperability
- Full KeystoneDB API (Query, Scan, Batch operations)
- Network-based architecture

See [grpc-client/README.md](grpc-client/README.md) and [../BINDINGS.md](../BINDINGS.md#grpc-clients) for details.

---

## What You'll Learn

Each example progressively demonstrates more advanced KeystoneDB features:

| Feature | URL Shortener | Cache Server | Todo API | Blog Engine |
|---------|---------------|--------------|----------|-------------|
| Basic CRUD | ✅ | ✅ | ✅ | ✅ |
| In-memory mode | ❌ | ✅ | ❌ | ❌ |
| TTL | ✅ | ✅ | ❌ | ❌ |
| Composite keys | ❌ | ❌ | ❌ | ✅ |
| Update expressions | ❌ | ❌ | ✅ | ✅ |
| Conditional ops | ✅ | ❌ | ✅ | ✅ |
| Transactions | ❌ | ❌ | ✅ | ✅ |
| GSI | ❌ | ❌ | ✅ | ✅ |
| LSI | ❌ | ❌ | ❌ | ✅ |
| PartiQL | ❌ | ❌ | ❌ | ✅ |
| Streams/CDC | ❌ | ❌ | ❌ | ✅ |
| Config | ❌ | ✅ | ✅ | ✅ |
| Health checks | ✅ | ✅ | ✅ | ✅ |
| Retry logic | ❌ | ✅ | ✅ | ✅ |

## Architecture

All examples follow a similar structure:

```
example-name/
├── Cargo.toml          # Dependencies
├── README.md           # Documentation
├── src/
│   ├── main.rs         # Entry point, router setup
│   ├── models.rs       # Data models
│   ├── handlers.rs     # Request handlers (or handlers/ directory)
│   └── db.rs          # Database operations (optional)
└── examples.http       # HTTP request examples (for some)
```

## Common Patterns

### Error Handling

All examples use a custom `AppError` type that implements `IntoResponse`:

```rust
#[derive(Debug)]
enum AppError {
    Database(kstone_api::KeystoneError),
    NotFound,
    // ...
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Database(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", err)
            ),
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                "Resource not found".to_string()
            ),
        };
        (status, Json(json!({"error": message}))).into_response()
    }
}
```

### Application State

Examples use Arc to share the database across handlers:

```rust
#[derive(Clone)]
struct AppState {
    db: Arc<Database>,
}

let state = AppState { db: Arc::new(db) };

let app = Router::new()
    .route("/", get(handler))
    .with_state(state);
```

### Health Checks

All examples expose health and stats endpoints:

```rust
async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let health = state.db.health();
    Json(HealthResponse {
        status: format!("{:?}", health.status),
        warnings: health.warnings,
        errors: health.errors,
    })
}

async fn db_stats(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let stats = state.db.stats()?;
    Ok(Json(json!({
        "total_keys": stats.total_keys,
        "compaction": {
            "total_compactions": stats.compaction.total_compactions,
            // ...
        }
    })))
}
```

## Running All Examples

```bash
# From the repository root
cargo build --release

# Run each example
./target/release/url-shortener
./target/release/cache-server
./target/release/todo-api
./target/release/blog-engine
```

## Testing

Each example can be tested with curl, httpie, or the provided `.http` files (compatible with VS Code REST Client extension).

Example:
```bash
# Create a short URL
curl -X POST http://localhost:3000/shorten \
  -H "Content-Type: application/json" \
  -d '{"long_url": "https://example.com"}'

# Get statistics
curl http://localhost:3000/api/stats/{code}
```

## Performance

All examples can handle thousands of requests per second. Run benchmarks to see actual numbers on your hardware:

```bash
cd kstone-tests
cargo bench
```

## Production Considerations

These are educational examples. For production use:

1. Add authentication/authorization
2. Implement rate limiting (use `governor` crate)
3. Add request validation
4. Use update expressions for atomic operations
5. Implement proper logging
6. Add monitoring/metrics
7. Configure appropriate resource limits
8. Set up backup procedures
9. Use connection pooling if using multiple databases
10. Implement graceful shutdown

See [../DEPLOYMENT.md](../DEPLOYMENT.md) for production deployment guide.

## Need Help?

- Check [../README.md](../README.md) for KeystoneDB overview
- See [../CLAUDE.md](../CLAUDE.md) for API documentation
- Read [../TROUBLESHOOTING.md](../TROUBLESHOOTING.md) for common issues
- File issues at https://github.com/yourusername/keystonedb/issues

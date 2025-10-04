# Chapter 24: gRPC Server

KeystoneDB's gRPC server transforms the embedded database into a distributed system, allowing remote clients to access the database over the network. This chapter explores the server architecture, Protocol Buffers definitions, configuration options, and operational considerations.

## Overview

The gRPC server (`kstone-server`) provides a network interface to KeystoneDB, enabling:

- **Remote Access**: Clients can access the database from different processes or machines
- **Language Agnostic**: Any language with gRPC support can connect (Python, Go, Java, etc.)
- **Concurrent Clients**: Multiple clients can connect simultaneously with connection limits
- **Production Features**: Rate limiting, connection management, metrics, graceful shutdown

The server is implemented as a separate binary (`kstone-server`) that wraps the embedded `Database` API and exposes it via gRPC.

## Architecture

### Component Structure

The server implementation spans three crates:

1. **kstone-proto**: Protocol Buffers definitions and generated code
2. **kstone-server**: gRPC service implementation and server binary
3. **kstone-client**: Client library for remote access (covered in Chapter 25)

```
┌─────────────────────────────────────────────────┐
│           Client Applications                    │
│  (Rust, Python, Go, Java, etc.)                 │
└───────────────────┬─────────────────────────────┘
                    │ gRPC/HTTP2
                    ▼
┌─────────────────────────────────────────────────┐
│        kstone-server (Tonic Server)             │
│  ┌──────────────────────────────────────────┐   │
│  │    KeystoneService (gRPC Trait Impl)     │   │
│  │  - Request handling                      │   │
│  │  - Type conversions                      │   │
│  │  - Error mapping                         │   │
│  └─────────────┬────────────────────────────┘   │
│                │                                 │
│  ┌─────────────▼────────────────────────────┐   │
│  │    Database API (kstone-api)             │   │
│  │  - Put/Get/Delete                        │   │
│  │  - Query/Scan                            │   │
│  │  - Batch/Transact                        │   │
│  └─────────────┬────────────────────────────┘   │
│                │                                 │
│  ┌─────────────▼────────────────────────────┐   │
│  │    LSM Engine (kstone-core)              │   │
│  │  - Storage engine                        │   │
│  │  - WAL, SST, Compaction                  │   │
│  └──────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
                    │
                    ▼
            ┌───────────────┐
            │  Disk Storage │
            └───────────────┘
```

### Async-Sync Bridge

One of the key architectural challenges is bridging the async gRPC server (Tonic) with the synchronous Database API:

```rust
// gRPC handler is async
async fn put(&self, request: Request<PutRequest>)
    -> Result<Response<PutResponse>, Status>
{
    let db = Arc::clone(&self.db);

    // Bridge to sync API using spawn_blocking
    let result = tokio::task::spawn_blocking(move || {
        db.put(&pk, item)  // Synchronous call
    })
    .await
    .map_err(|e| Status::internal(format!("Task join error: {}", e)))?;

    // Convert result to gRPC response
    match result {
        Ok(_) => Ok(Response::new(PutResponse { success: true, error: None })),
        Err(e) => Err(map_error(e)),
    }
}
```

The `tokio::task::spawn_blocking` function runs the synchronous database operation on a dedicated thread pool, preventing it from blocking the async runtime. This pattern is used for all database operations.

## Protocol Buffers Definition

The gRPC API is defined in `kstone-proto/proto/keystone.proto` using Protocol Buffers version 3.

### Service Definition

The `KeystoneDB` service exposes 11 RPC methods:

```protobuf
service KeystoneDB {
  // Basic operations
  rpc Put(PutRequest) returns (PutResponse);
  rpc Get(GetRequest) returns (GetResponse);
  rpc Delete(DeleteRequest) returns (DeleteResponse);

  // Query and scan
  rpc Query(QueryRequest) returns (QueryResponse);
  rpc Scan(ScanRequest) returns (stream ScanResponse);

  // Batch operations
  rpc BatchGet(BatchGetRequest) returns (BatchGetResponse);
  rpc BatchWrite(BatchWriteRequest) returns (BatchWriteResponse);

  // Transactions
  rpc TransactGet(TransactGetRequest) returns (TransactGetResponse);
  rpc TransactWrite(TransactWriteRequest) returns (TransactWriteResponse);

  // Update
  rpc Update(UpdateRequest) returns (UpdateResponse);

  // PartiQL
  rpc ExecuteStatement(ExecuteStatementRequest) returns (ExecuteStatementResponse);
}
```

All methods are fully implemented in the current version.

### Type System

KeystoneDB's rich type system is mapped to protobuf using `oneof` discriminated unions:

```protobuf
message Value {
  oneof value {
    string string_value = 1;      // String
    string number_value = 2;       // Number (as string for precision)
    bytes binary_value = 3;        // Binary
    bool bool_value = 4;           // Boolean
    NullValue null_value = 5;      // Null
    ListValue list_value = 6;      // List
    MapValue map_value = 7;        // Map
    VectorValue vector_value = 8;  // Vector (f32 array)
    uint64 timestamp_value = 9;    // Timestamp (milliseconds)
  }
}
```

This design supports all KeystoneDB value types while maintaining protobuf's strict type safety.

### Request/Response Patterns

Each operation follows a consistent request/response pattern:

**Put Operation**:
```protobuf
message PutRequest {
  bytes partition_key = 1;
  optional bytes sort_key = 2;
  Item item = 3;
  optional string condition_expression = 4;
  map<string, Value> expression_values = 5;
}

message PutResponse {
  bool success = 1;
  optional string error = 2;
}
```

**Query Operation**:
```protobuf
message QueryRequest {
  bytes partition_key = 1;
  optional SortKeyCondition sort_key_condition = 2;
  optional string filter_expression = 3;
  map<string, Value> expression_values = 4;
  optional string index_name = 5;
  optional uint32 limit = 6;
  optional LastKey exclusive_start_key = 7;
  optional bool scan_forward = 8;
}

message QueryResponse {
  repeated Item items = 1;
  uint32 count = 2;
  uint32 scanned_count = 3;
  optional LastKey last_evaluated_key = 4;
  optional string error = 5;
}
```

The protocol supports all DynamoDB-style operations including conditional expressions, pagination, and secondary indexes.

## Type Conversions

Due to Rust's orphan rules (you can't implement external traits on external types), the server uses conversion functions rather than trait implementations.

### Bidirectional Conversions

The `convert.rs` module provides symmetric conversions:

```rust
// Value conversions
pub fn proto_value_to_ks(value: proto::Value) -> Result<KsValue, Status>
pub fn ks_value_to_proto(value: &KsValue) -> proto::Value

// Item conversions
pub fn proto_item_to_ks(item: proto::Item) -> Result<Item, Status>
pub fn ks_item_to_proto(item: &Item) -> proto::Item

// Key conversions
pub fn proto_key_to_ks(key: proto::Key) -> (Bytes, Option<Bytes>)
pub fn ks_key_to_proto(pk: impl Into<Vec<u8>>, sk: Option<impl Into<Vec<u8>>>) -> proto::Key
```

### Handling Nested Types

Recursive conversion handles nested lists and maps:

```rust
pub fn proto_value_to_ks(value: proto::Value) -> Result<KsValue, Status> {
    let value_enum = value
        .value
        .ok_or_else(|| Status::invalid_argument("Value field is missing"))?;

    match value_enum {
        ProtoValueEnum::StringValue(s) => Ok(KsValue::S(s)),
        ProtoValueEnum::NumberValue(n) => Ok(KsValue::N(n)),
        ProtoValueEnum::BinaryValue(b) => Ok(KsValue::B(Bytes::from(b))),
        ProtoValueEnum::BoolValue(b) => Ok(KsValue::Bool(b)),
        ProtoValueEnum::NullValue(_) => Ok(KsValue::Null),

        // Recursive conversion for lists
        ProtoValueEnum::ListValue(list) => {
            let items: Result<Vec<KsValue>, Status> =
                list.items.into_iter().map(proto_value_to_ks).collect();
            Ok(KsValue::L(items?))
        }

        // Recursive conversion for maps
        ProtoValueEnum::MapValue(map) => {
            let mut kv_map = HashMap::new();
            for (k, v) in map.fields {
                kv_map.insert(k, proto_value_to_ks(v)?);
            }
            Ok(KsValue::M(kv_map))
        }

        ProtoValueEnum::VectorValue(vec) => Ok(KsValue::VecF32(vec.values)),
        ProtoValueEnum::TimestampValue(ts) => Ok(KsValue::Ts(ts as i64)),
    }
}
```

## RPC Method Implementations

### Basic CRUD Operations

The Put operation demonstrates the standard pattern:

```rust
async fn put(
    &self,
    request: Request<proto::PutRequest>,
) -> Result<Response<proto::PutResponse>, Status> {
    // Generate trace ID for request correlation
    let trace_id = Uuid::new_v4().to_string();
    tracing::Span::current().record("trace_id", &trace_id);

    // Start timing for metrics
    let timer = RPC_DURATION_SECONDS.with_label_values(&["put"]).start_timer();

    let req = request.into_inner();

    // Convert protobuf types to KeystoneDB types
    let (pk, sk) = proto_key_to_ks(proto::Key {
        partition_key: req.partition_key.clone(),
        sort_key: req.sort_key.clone(),
    });

    let item = proto_item_to_ks(
        req.item.ok_or_else(|| Status::invalid_argument("Item required"))?
    )?;

    // Execute operation in blocking thread pool
    let db = Arc::clone(&self.db);
    let result = tokio::task::spawn_blocking(move || {
        // Check if this is a conditional put
        if let Some(condition_expr) = req.condition_expression {
            let mut context = kstone_core::expression::ExpressionContext::new();
            for (placeholder, proto_value) in req.expression_values {
                let value = proto_value_to_ks(proto_value)?;
                context = context.with_value(placeholder, value);
            }

            if let Some(sk_bytes) = sk {
                db.put_conditional_with_sk(&pk, &sk_bytes, item, &condition_expr, context)
            } else {
                db.put_conditional(&pk, item, &condition_expr, context)
            }
        } else {
            // Regular put without condition
            if let Some(sk_bytes) = sk {
                db.put_with_sk(&pk, &sk_bytes, item)
            } else {
                db.put(&pk, item)
            }
        }
    })
    .await
    .map_err(|e| Status::internal(format!("Task join error: {}", e)))?;

    // Convert result to gRPC response
    match result {
        Ok(_) => {
            timer.observe_duration();
            RPC_REQUESTS_TOTAL.with_label_values(&["put", "success"]).inc();
            Ok(Response::new(proto::PutResponse {
                success: true,
                error: None,
            }))
        }
        Err(e) => {
            timer.observe_duration();
            RPC_REQUESTS_TOTAL.with_label_values(&["put", "error"]).inc();
            Err(map_error(e))
        }
    }
}
```

### Query Operations

Query operations support all DynamoDB-style conditions:

```rust
async fn query(
    &self,
    request: Request<proto::QueryRequest>,
) -> Result<Response<proto::QueryResponse>, Status> {
    let req = request.into_inner();

    // Build query starting with partition key
    let mut query = kstone_api::Query::new(&req.partition_key);

    // Apply sort key condition if present
    if let Some(sk_cond) = req.sort_key_condition {
        query = apply_sort_key_condition(query, sk_cond)?;
    }

    // Apply limit, pagination, direction, index
    if let Some(limit) = req.limit {
        query = query.limit(limit as usize);
    }

    if let Some(start_key) = req.exclusive_start_key {
        let (pk, sk) = proto_last_key_to_ks(start_key);
        query = query.start_after(&pk, sk.as_deref());
    }

    if let Some(forward) = req.scan_forward {
        query = query.forward(forward);
    }

    if let Some(index_name) = req.index_name {
        query = query.index(index_name);
    }

    // Execute query
    let db = Arc::clone(&self.db);
    let response = tokio::task::spawn_blocking(move || db.query(query))
        .await
        .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
        .map_err(map_error)?;

    // Convert response to protobuf
    Ok(Response::new(proto::QueryResponse {
        items: response.items.iter().map(ks_item_to_proto).collect(),
        count: response.count as u32,
        scanned_count: response.scanned_count as u32,
        last_evaluated_key: ks_last_key_opt_to_proto(response.last_key),
        error: None,
    }))
}
```

### Streaming Scan

Scan uses server-side streaming to handle large result sets:

```rust
type ScanStream = futures::stream::Once<
    futures::future::Ready<Result<proto::ScanResponse, Status>>,
>;

async fn scan(
    &self,
    request: Request<proto::ScanRequest>,
) -> Result<Response<Self::ScanStream>, Status> {
    let req = request.into_inner();

    // Build scan with options
    let mut scan = kstone_api::Scan::new();

    if let Some(limit) = req.limit {
        scan = scan.limit(limit as usize);
    }

    if let Some(start_key) = req.exclusive_start_key {
        let (pk, sk) = proto_last_key_to_ks(start_key);
        scan = scan.start_after(&pk, sk.as_deref());
    }

    // Apply parallel scan segments
    if let (Some(segment), Some(total_segments)) = (req.segment, req.total_segments) {
        scan = scan.segment(segment as usize, total_segments as usize);
    }

    // Execute scan
    let db = Arc::clone(&self.db);
    let response = tokio::task::spawn_blocking(move || db.scan(scan))
        .await
        .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
        .map_err(map_error)?;

    // Convert response to protobuf
    let proto_response = proto::ScanResponse {
        items: response.items.iter().map(ks_item_to_proto).collect(),
        count: response.count as u32,
        scanned_count: response.scanned_count as u32,
        last_evaluated_key: ks_last_key_opt_to_proto(response.last_key),
        error: None,
    };

    // Return as a single-item stream
    let stream = futures::stream::once(futures::future::ready(Ok(proto_response)));
    Ok(Response::new(stream))
}
```

Currently, the scan returns a single-item stream. Future versions could stream results in chunks for better memory efficiency.

### Batch Operations

Batch operations process multiple items in a single RPC:

```rust
async fn batch_write(
    &self,
    request: Request<proto::BatchWriteRequest>,
) -> Result<Response<proto::BatchWriteResponse>, Status> {
    use proto::write_request::Request as WriteRequestEnum;

    let req = request.into_inner();

    // Build batch write request
    let mut batch_request = kstone_api::BatchWriteRequest::new();

    for write_req in req.writes {
        let request_enum = write_req
            .request
            .ok_or_else(|| Status::invalid_argument("Write request is required"))?;

        match request_enum {
            WriteRequestEnum::Put(put_item) => {
                let (pk, sk) = proto_key_to_ks(proto::Key {
                    partition_key: put_item.partition_key,
                    sort_key: put_item.sort_key,
                });
                let item = proto_item_to_ks(
                    put_item.item.ok_or_else(|| Status::invalid_argument("Item required"))?
                )?;

                if let Some(sk_bytes) = sk {
                    batch_request = batch_request.put_with_sk(&pk, &sk_bytes, item);
                } else {
                    batch_request = batch_request.put(&pk, item);
                }
            }
            WriteRequestEnum::Delete(delete_key) => {
                let (pk, sk) = proto_key_to_ks(proto::Key {
                    partition_key: delete_key.partition_key,
                    sort_key: delete_key.sort_key,
                });

                if let Some(sk_bytes) = sk {
                    batch_request = batch_request.delete_with_sk(&pk, &sk_bytes);
                } else {
                    batch_request = batch_request.delete(&pk);
                }
            }
        }
    }

    // Execute batch write
    let db = Arc::clone(&self.db);
    tokio::task::spawn_blocking(move || db.batch_write(batch_request))
        .await
        .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
        .map_err(map_error)?;

    Ok(Response::new(proto::BatchWriteResponse {
        success: true,
        error: None,
    }))
}
```

### Transactional Operations

Transactions maintain ACID guarantees over the network:

```rust
async fn transact_write(
    &self,
    request: Request<proto::TransactWriteRequest>,
) -> Result<Response<proto::TransactWriteResponse>, Status> {
    use proto::transact_write_item::Item as ProtoTxItem;

    let req = request.into_inner();

    // Build transact write request with all operations
    let mut transact_request = kstone_api::TransactWriteRequest::new();

    for item in req.items {
        let proto_item = item
            .item
            .ok_or_else(|| Status::invalid_argument("TransactWriteItem is required"))?;

        match proto_item {
            ProtoTxItem::Put(put) => {
                let key = if let Some(sk) = put.sort_key {
                    kstone_core::Key::with_sk(Bytes::from(put.partition_key), Bytes::from(sk))
                } else {
                    kstone_core::Key::new(Bytes::from(put.partition_key))
                };

                let item = proto_item_to_ks(
                    put.item.ok_or_else(|| Status::invalid_argument("Item required"))?
                )?;

                transact_request.operations.push(kstone_api::TransactWriteOp::Put {
                    key,
                    item,
                    condition: put.condition_expression,
                });
            }
            ProtoTxItem::Update(update) => {
                let key = if let Some(sk) = update.sort_key {
                    kstone_core::Key::with_sk(Bytes::from(update.partition_key), Bytes::from(sk))
                } else {
                    kstone_core::Key::new(Bytes::from(update.partition_key))
                };

                transact_request.operations.push(kstone_api::TransactWriteOp::Update {
                    key,
                    update_expression: update.update_expression,
                    condition: update.condition_expression,
                });
            }
            ProtoTxItem::Delete(delete) => {
                let key = if let Some(sk) = delete.sort_key {
                    kstone_core::Key::with_sk(Bytes::from(delete.partition_key), Bytes::from(sk))
                } else {
                    kstone_core::Key::new(Bytes::from(delete.partition_key))
                };

                transact_request.operations.push(kstone_api::TransactWriteOp::Delete {
                    key,
                    condition: delete.condition_expression,
                });
            }
            ProtoTxItem::ConditionCheck(check) => {
                let key = if let Some(sk) = check.sort_key {
                    kstone_core::Key::with_sk(Bytes::from(check.partition_key), Bytes::from(sk))
                } else {
                    kstone_core::Key::new(Bytes::from(check.partition_key))
                };

                transact_request.operations.push(kstone_api::TransactWriteOp::ConditionCheck {
                    key,
                    condition: check.condition_expression,
                });
            }
        }
    }

    // Execute transactional write
    let db = Arc::clone(&self.db);
    tokio::task::spawn_blocking(move || db.transact_write(transact_request))
        .await
        .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
        .map_err(map_error)?;

    Ok(Response::new(proto::TransactWriteResponse {
        success: true,
        error: None,
    }))
}
```

## Error Handling and Status Codes

The server maps KeystoneDB errors to appropriate gRPC status codes:

```rust
fn map_error(err: KsError) -> Status {
    match err {
        KsError::NotFound(msg) => Status::not_found(msg),
        KsError::InvalidQuery(msg) => Status::invalid_argument(msg),
        KsError::InvalidArgument(msg) => Status::invalid_argument(msg),
        KsError::InvalidExpression(msg) => Status::invalid_argument(msg),
        KsError::ConditionalCheckFailed(msg) => Status::failed_precondition(msg),
        KsError::Io(e) => Status::internal(format!("IO error: {}", e)),
        KsError::Corruption(msg) => Status::data_loss(format!("Data corruption: {}", msg)),
        KsError::ManifestCorruption(msg) => Status::data_loss(format!("Manifest corruption: {}", msg)),
        KsError::TransactionCanceled(msg) => Status::aborted(format!("Transaction canceled: {}", msg)),
        KsError::AlreadyExists(msg) => Status::already_exists(msg),
        KsError::WalFull => Status::resource_exhausted("WAL full"),
        KsError::ChecksumMismatch => Status::data_loss("Checksum mismatch"),
        KsError::Internal(msg) => Status::internal(msg),
        KsError::ResourceExhausted(msg) => Status::resource_exhausted(msg),
    }
}
```

This mapping follows gRPC conventions and allows clients to handle errors appropriately.

## Starting the Server

### Command Line Interface

The server binary accepts several configuration options:

```bash
kstone-server --db-path <PATH> [OPTIONS]

Options:
  -d, --db-path <PATH>                 Path to database directory (required)
  -p, --port <PORT>                    Port to listen on [default: 50051]
      --host <HOST>                    Host to bind to [default: 127.0.0.1]
      --max-connections <N>            Maximum concurrent connections [default: 1000]
      --connection-timeout <SECS>      Connection timeout in seconds [default: 60]
      --shutdown-timeout <SECS>        Graceful shutdown timeout [default: 30]
      --max-rps-per-connection <RPS>   Max requests/second per connection [default: 0]
      --max-rps-global <RPS>           Max total requests/second [default: 0]
```

### Basic Usage

Start a server on the default port:

```bash
# Create database if it doesn't exist
kstone-server --db-path /var/lib/keystone/prod.db
```

Start with custom configuration:

```bash
kstone-server \
  --db-path /var/lib/keystone/prod.db \
  --port 9090 \
  --host 0.0.0.0 \
  --max-connections 5000 \
  --max-rps-global 10000
```

### Logging Configuration

The server uses `tracing` for structured logging. Configure log levels via the `RUST_LOG` environment variable:

```bash
# Info level (default)
RUST_LOG=info kstone-server --db-path data.db

# Debug level for troubleshooting
RUST_LOG=debug kstone-server --db-path data.db

# Trace level for detailed diagnostics
RUST_LOG=trace kstone-server --db-path data.db

# Filter by module
RUST_LOG=kstone_server=debug,kstone_core=info kstone-server --db-path data.db
```

Log output includes structured fields for request correlation:

```
2024-01-15T10:30:45.123Z INFO kstone_server::service: Received put request trace_id="a1b2c3d4" has_sk=true
2024-01-15T10:30:45.125Z INFO kstone_server::service: Put operation completed successfully trace_id="a1b2c3d4"
```

## Connection Management

The server implements connection limits to prevent resource exhaustion.

### Connection Limits

```rust
pub struct ConnectionManager {
    active: Arc<AtomicUsize>,
    max_connections: usize,
    timeout: Duration,
}

impl ConnectionManager {
    pub fn acquire(&self) -> Result<ConnectionGuard, Status> {
        let current = self.active.fetch_add(1, Ordering::SeqCst);

        // Check if we exceeded the limit (0 means unlimited)
        if self.max_connections > 0 && current >= self.max_connections {
            // Rollback the increment
            self.active.fetch_sub(1, Ordering::SeqCst);

            return Err(Status::resource_exhausted(
                format!("Connection limit reached ({}/{})",
                    current, self.max_connections)
            ));
        }

        Ok(ConnectionGuard {
            manager: self.clone(),
        })
    }
}

// RAII guard automatically releases connection when dropped
pub struct ConnectionGuard {
    manager: ConnectionManager,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.manager.active.fetch_sub(1, Ordering::SeqCst);
    }
}
```

When the connection limit is reached, new connections receive a `RESOURCE_EXHAUSTED` status.

### TCP Configuration

The server configures TCP options for optimal performance:

```rust
let server = Server::builder()
    .timeout(Duration::from_secs(args.connection_timeout))
    .tcp_keepalive(Some(Duration::from_secs(30)))  // Detect dead connections
    .tcp_nodelay(true)  // Disable Nagle's algorithm for low latency
    .add_service(KeystoneDbServer::new(service));
```

## Rate Limiting

The server includes token bucket rate limiting to prevent overload.

### Rate Limiter Implementation

```rust
pub struct RateLimiter {
    per_connection: Option<Arc<GovernorRateLimiter<...>>>,
    global: Option<Arc<GovernorRateLimiter<...>>>,
}

impl RateLimiter {
    pub fn new(per_connection_rps: u32, global_rps: u32) -> Self {
        let per_connection = if per_connection_rps > 0 {
            NonZeroU32::new(per_connection_rps).map(|rps| {
                let quota = Quota::per_second(rps);
                Arc::new(GovernorRateLimiter::direct(quota))
            })
        } else {
            None
        };

        let global = if global_rps > 0 {
            NonZeroU32::new(global_rps).map(|rps| {
                let quota = Quota::per_second(rps);
                Arc::new(GovernorRateLimiter::direct(quota))
            })
        } else {
            None
        };

        Self { per_connection, global }
    }

    pub fn check_rate_limit(&self) -> Result<(), Status> {
        // Check per-connection limit first
        if let Some(limiter) = &self.per_connection {
            limiter.check().map_err(|_| {
                Status::resource_exhausted(
                    "Rate limit exceeded: too many requests from this connection"
                )
            })?;
        }

        // Check global limit
        if let Some(limiter) = &self.global {
            limiter.check().map_err(|_| {
                Status::resource_exhausted(
                    "Rate limit exceeded: server at capacity"
                )
            })?;
        }

        Ok(())
    }
}
```

### Rate Limiting Modes

**Per-Connection Limits**: Prevent a single client from overwhelming the server
```bash
kstone-server --db-path data.db --max-rps-per-connection 100
```

**Global Limits**: Cap total server throughput
```bash
kstone-server --db-path data.db --max-rps-global 5000
```

**Combined Limits**: Use both for fine-grained control
```bash
kstone-server --db-path data.db \
  --max-rps-per-connection 100 \
  --max-rps-global 5000
```

When a rate limit is exceeded, clients receive a `RESOURCE_EXHAUSTED` status with a descriptive error message.

## Metrics and Observability

The server exposes Prometheus metrics on port 9090.

### Metrics Endpoints

- **`/metrics`**: Prometheus metrics (text format)
- **`/health`**: Health check endpoint (returns "OK")
- **`/ready`**: Readiness probe (returns "OK")

### Available Metrics

**Request Metrics**:
```
# Total RPC requests by method and status
rpc_requests_total{method="put",status="success"} 1234
rpc_requests_total{method="get",status="success"} 5678
rpc_requests_total{method="query",status="error"} 12

# Request duration histogram
rpc_duration_seconds_bucket{method="put",le="0.001"} 100
rpc_duration_seconds_bucket{method="put",le="0.01"} 450
rpc_duration_seconds_bucket{method="put",le="0.1"} 1200
rpc_duration_seconds_sum{method="put"} 45.67
rpc_duration_seconds_count{method="put"} 1234
```

**Connection Metrics**:
```
# Current active connections
active_connections 42

# Rate limited requests
rate_limited_requests{limit_type="per_connection"} 5
rate_limited_requests{limit_type="global"} 12
```

### Metrics Integration

The metrics are exposed via a separate HTTP server running on port 9090:

```rust
let metrics_app = Router::new()
    .route("/metrics", get(metrics_handler))
    .route("/health", get(health_handler))
    .route("/ready", get(ready_handler));

let metrics_listener = tokio::net::TcpListener::bind("127.0.0.1:9090").await?;
tokio::spawn(async move {
    axum::serve(metrics_listener, metrics_app).await
});
```

## Graceful Shutdown

The server implements graceful shutdown to avoid data loss and connection interruption.

### Shutdown Signal Handling

```rust
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => info!("Received Ctrl+C signal"),
        _ = terminate => info!("Received SIGTERM signal"),
    }

    warn!("Shutting down gracefully...");
}
```

### Shutdown Sequence

1. **Signal Received**: Server catches SIGTERM or SIGINT
2. **Stop Accepting**: New connections are rejected
3. **Drain Connections**: Existing requests complete (up to timeout)
4. **Database Flush**: Ensure all data is persisted
5. **Exit**: Process terminates cleanly

```rust
server
    .serve_with_shutdown(grpc_addr, shutdown_signal())
    .await?;

info!("gRPC server shutdown complete");

// Wait for connection draining
info!("Waiting up to {}s for connections to drain...", args.shutdown_timeout);
tokio::time::sleep(Duration::from_secs(args.shutdown_timeout)).await;

info!("Shutdown complete");
```

### Production Deployment

For production deployments, configure appropriate shutdown timeouts:

```bash
kstone-server \
  --db-path /var/lib/keystone/prod.db \
  --shutdown-timeout 60  # Allow 60 seconds for graceful shutdown
```

In Kubernetes, set `terminationGracePeriodSeconds` to match or exceed the shutdown timeout.

## Performance Considerations

### Thread Pool Sizing

The server uses Tokio's default thread pool for async operations and a blocking thread pool for database operations. For high-throughput deployments:

```bash
# Set Tokio worker threads (default: number of CPU cores)
TOKIO_WORKER_THREADS=16 kstone-server --db-path data.db

# Set blocking thread pool size (default: 512)
# Used for database operations
TOKIO_BLOCKING_THREADS=128 kstone-server --db-path data.db
```

### Memory Usage

Each active connection consumes minimal memory (~10KB), but buffered responses can be significant:

- Query/Scan responses buffer all results in memory
- Large batch operations allocate temporary buffers
- Consider `--max-connections` based on available RAM

### Network Bandwidth

gRPC uses HTTP/2 with binary protobuf encoding, which is efficient but can still consume significant bandwidth:

- Average item size: ~1KB
- Batch of 100 items: ~100KB
- Query with 1000 results: ~1MB

Monitor network bandwidth and consider rate limiting for high-traffic scenarios.

## Security Considerations

### TLS/SSL

The current implementation does not include TLS. For production deployments, use a reverse proxy:

```bash
# Run server on localhost
kstone-server --db-path data.db --host 127.0.0.1 --port 50051

# Nginx reverse proxy with TLS
# nginx.conf:
# upstream keystone {
#   server 127.0.0.1:50051;
# }
# server {
#   listen 443 ssl http2;
#   ssl_certificate /path/to/cert.pem;
#   ssl_certificate_key /path/to/key.pem;
#   location / {
#     grpc_pass grpc://keystone;
#   }
# }
```

### Authentication

The current implementation does not include authentication. Future versions will support:

- JWT-based authentication
- API key validation
- mTLS client certificates

For now, use network-level security (firewalls, VPNs, private networks).

## Troubleshooting

### Connection Refused

**Problem**: Client cannot connect to server

**Solutions**:
- Verify server is running: `lsof -i :50051`
- Check bind address: Use `--host 0.0.0.0` to listen on all interfaces
- Check firewall rules: `sudo ufw allow 50051`

### Resource Exhausted

**Problem**: Clients receiving `RESOURCE_EXHAUSTED` errors

**Solutions**:
- Increase connection limit: `--max-connections 5000`
- Increase rate limits: `--max-rps-global 10000`
- Scale horizontally: Run multiple server instances

### High Latency

**Problem**: Slow response times

**Solutions**:
- Enable debug logging: `RUST_LOG=debug`
- Check database disk I/O: `iostat -x 1`
- Monitor CPU usage: Database operations are CPU-intensive
- Consider in-memory mode for cache workloads
- Increase blocking thread pool: `TOKIO_BLOCKING_THREADS=256`

### Memory Leaks

**Problem**: Server memory usage grows over time

**Solutions**:
- Monitor metrics endpoint: `/metrics`
- Check for unclosed connections
- Review application code for resource leaks
- Restart server periodically in development

## Summary

The KeystoneDB gRPC server provides a production-ready network interface with:

- **Complete API**: All 11 RPC methods fully implemented
- **Type Safety**: Bidirectional protobuf conversions
- **Async Performance**: Tokio-based async runtime with blocking pool
- **Production Features**: Connection limits, rate limiting, graceful shutdown
- **Observability**: Prometheus metrics, structured logging, health checks

The server architecture bridges the async gRPC world with the synchronous database API through careful use of `spawn_blocking`, providing high throughput while maintaining data consistency.

In Chapter 25, we'll explore the client library that connects to this server, completing the distributed system picture.

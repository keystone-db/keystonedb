# Chapter 26: Network Architecture

This chapter provides a deep dive into KeystoneDB's network architecture, exploring the technical details of client-server communication, type conversions, async operations, security, performance optimization, and deployment patterns. Understanding these internals is essential for operating KeystoneDB in production environments.

## Communication Flow

### Request-Response Lifecycle

Every RPC call follows a consistent lifecycle through multiple layers:

```
┌─────────────────────────────────────────────────────────┐
│                    Client Application                    │
└──────────────────────────┬──────────────────────────────┘
                           │ 1. Create request
                           │    (RemoteQuery::new(...))
                           ▼
┌─────────────────────────────────────────────────────────┐
│                   Client Library                         │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 2. Build protobuf request                          │ │
│  │    (proto::QueryRequest { ... })                   │ │
│  └────────────────────────┬───────────────────────────┘ │
└────────────────────────────┼─────────────────────────────┘
                             │ 3. Serialize (Protocol Buffers)
                             ▼
┌─────────────────────────────────────────────────────────┐
│                  Tonic Client (gRPC)                     │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 4. HTTP/2 framing                                  │ │
│  │ 5. Connection management                           │ │
│  │ 6. Send over TCP                                   │ │
│  └────────────────────────┬───────────────────────────┘ │
└────────────────────────────┼─────────────────────────────┘
                             │ Network (TCP/IP)
                             ▼
┌─────────────────────────────────────────────────────────┐
│                  Tonic Server (gRPC)                     │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 7. Receive TCP stream                              │ │
│  │ 8. HTTP/2 deframing                                │ │
│  │ 9. Deserialize request                             │ │
│  └────────────────────────┬───────────────────────────┘ │
└────────────────────────────┼─────────────────────────────┘
                             │ 10. Route to service method
                             ▼
┌─────────────────────────────────────────────────────────┐
│              KeystoneService (Server)                    │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 11. Convert proto → KeystoneDB types               │ │
│  │ 12. Validate request                               │ │
│  └────────────────────────┬───────────────────────────┘ │
└────────────────────────────┼─────────────────────────────┘
                             │ 13. spawn_blocking
                             ▼
┌─────────────────────────────────────────────────────────┐
│               Database API (kstone-api)                  │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 14. Execute query (synchronous)                    │ │
│  │ 15. Return result                                  │ │
│  └────────────────────────┬───────────────────────────┘ │
└────────────────────────────┼─────────────────────────────┘
                             │ 16. await blocking task
                             ▼
┌─────────────────────────────────────────────────────────┐
│              KeystoneService (Server)                    │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 17. Convert KeystoneDB → proto types               │ │
│  │ 18. Build response                                 │ │
│  └────────────────────────┬───────────────────────────┘ │
└────────────────────────────┼─────────────────────────────┘
                             │ 19. Serialize response
                             ▼
┌─────────────────────────────────────────────────────────┐
│                  Tonic Server (gRPC)                     │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 20. HTTP/2 framing                                 │ │
│  │ 21. Send over TCP                                  │ │
│  └────────────────────────┬───────────────────────────┘ │
└────────────────────────────┼─────────────────────────────┘
                             │ Network (TCP/IP)
                             ▼
┌─────────────────────────────────────────────────────────┐
│                  Tonic Client (gRPC)                     │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 22. Receive TCP stream                             │ │
│  │ 23. HTTP/2 deframing                               │ │
│  │ 24. Deserialize response                           │ │
│  └────────────────────────┬───────────────────────────┘ │
└────────────────────────────┼─────────────────────────────┘
                             │ 25. Return to caller
                             ▼
┌─────────────────────────────────────────────────────────┐
│                   Client Library                         │
│  ┌────────────────────────────────────────────────────┐ │
│  │ 26. Convert proto → KeystoneDB types               │ │
│  │ 27. Build response struct                          │ │
│  └────────────────────────┬───────────────────────────┘ │
└────────────────────────────┼─────────────────────────────┘
                             │ 28. Return result
                             ▼
┌─────────────────────────────────────────────────────────┐
│                    Client Application                    │
└─────────────────────────────────────────────────────────┘
```

This 28-step process completes in milliseconds for most operations, thanks to efficient serialization, HTTP/2 multiplexing, and minimal data copying.

### HTTP/2 Multiplexing

gRPC uses HTTP/2, which provides several key benefits:

**Stream Multiplexing**: Multiple requests share a single TCP connection
```
┌──────────────────────────────────────┐
│          TCP Connection              │
│  ┌────────────────────────────────┐  │
│  │ Stream 1: GET user#123         │  │
│  │ Stream 3: PUT user#456         │  │
│  │ Stream 5: QUERY org#acme       │  │
│  │ Stream 7: SCAN ...             │  │
│  └────────────────────────────────┘  │
└──────────────────────────────────────┘
```

**Header Compression**: HPACK compression reduces overhead
- First request: Full headers (~500 bytes)
- Subsequent requests: Header diff (~50 bytes)

**Flow Control**: Prevents fast senders from overwhelming slow receivers
- Stream-level backpressure
- Connection-level flow control windows

**Binary Framing**: Efficient binary protocol vs. text-based HTTP/1.1
- No header parsing overhead
- Deterministic message boundaries

### Connection Establishment

The initial connection involves:

1. **DNS Resolution**: Resolve hostname to IP address
2. **TCP Handshake**: 3-way handshake (SYN, SYN-ACK, ACK)
3. **HTTP/2 Negotiation**: ALPN (Application-Layer Protocol Negotiation)
4. **Connection Preface**: HTTP/2 settings exchange

```rust
// Connection establishment in client
let channel = Channel::from_static("http://localhost:50051")
    .connect()  // Async connection establishment
    .await?;

let client = KeystoneDbClient::new(channel);
```

The `Channel` maintains the connection and automatically reconnects if it fails.

## Type Conversion System

### Orphan Rule Challenge

Rust's orphan rule prevents implementing external traits on external types. Since both `tonic` (protobuf types) and `kstone-core` (KeystoneDB types) are external to `kstone-server` and `kstone-client`, we cannot implement `From` or `Into` traits.

**Solution**: Conversion functions in a dedicated `convert` module.

### Value Conversion Architecture

The type conversion system handles bidirectional transformations:

```rust
// Proto → KeystoneDB
pub fn proto_value_to_ks(value: proto::Value) -> Result<KsValue, Status>

// KeystoneDB → Proto
pub fn ks_value_to_proto(value: &KsValue) -> proto::Value
```

### Detailed Value Conversions

**String Values**:
```rust
// Proto (UTF-8 string) ↔ KeystoneDB (String)
ProtoValueEnum::StringValue(s) => Ok(KsValue::S(s))
KsValue::S(s) => ProtoValueEnum::StringValue(s.clone())
```

**Number Values** (stored as strings for precision):
```rust
// Proto (string) ↔ KeystoneDB (string)
ProtoValueEnum::NumberValue(n) => Ok(KsValue::N(n))
KsValue::N(n) => ProtoValueEnum::NumberValue(n.clone())
```

**Binary Values** (using `bytes::Bytes` for zero-copy):
```rust
// Proto (Vec<u8>) ↔ KeystoneDB (Bytes)
ProtoValueEnum::BinaryValue(b) => Ok(KsValue::B(Bytes::from(b)))
KsValue::B(b) => ProtoValueEnum::BinaryValue(b.to_vec())
```

**Vector Values** (for embeddings):
```rust
// Proto (repeated float) ↔ KeystoneDB (Vec<f32>)
ProtoValueEnum::VectorValue(vec) => Ok(KsValue::VecF32(vec.values))
KsValue::VecF32(vec) => ProtoValueEnum::VectorValue(proto::VectorValue {
    values: vec.clone()
})
```

**Timestamp Values** (milliseconds since epoch):
```rust
// Proto (uint64) ↔ KeystoneDB (i64)
ProtoValueEnum::TimestampValue(ts) => Ok(KsValue::Ts(ts as i64))
KsValue::Ts(ts) => ProtoValueEnum::TimestampValue(*ts as u64)
```

### Recursive Conversions

Lists and maps require recursive conversion:

```rust
// List conversion
ProtoValueEnum::ListValue(list) => {
    let items: Result<Vec<KsValue>, Status> =
        list.items.into_iter()
            .map(proto_value_to_ks)  // Recursive call
            .collect();
    Ok(KsValue::L(items?))
}

// Map conversion
ProtoValueEnum::MapValue(map) => {
    let mut kv_map = HashMap::new();
    for (k, v) in map.fields {
        kv_map.insert(k, proto_value_to_ks(v)?);  // Recursive call
    }
    Ok(KsValue::M(kv_map))
}
```

This handles arbitrarily nested structures:
```rust
// Nested structure example:
{
  "user": {
    "name": "Alice",
    "scores": [95, 87, 92],
    "metadata": {
      "created": 1640995200000,
      "tags": ["vip", "beta"]
    }
  }
}
```

### Item Conversions

Items are maps from String to Value:

```rust
pub type Item = HashMap<String, KsValue>;

// Proto Item → KeystoneDB Item
pub fn proto_item_to_ks(item: proto::Item) -> Result<Item, Status> {
    let mut kv_map = HashMap::new();
    for (k, v) in item.attributes {
        kv_map.insert(k, proto_value_to_ks(v)?);
    }
    Ok(kv_map)
}

// KeystoneDB Item → Proto Item
pub fn ks_item_to_proto(item: &Item) -> proto::Item {
    let mut attributes = HashMap::new();
    for (k, v) in item {
        attributes.insert(k.clone(), ks_value_to_proto(v));
    }
    proto::Item { attributes }
}
```

### Key Conversions

Keys have two forms:

**Tuple Form** (used in server):
```rust
pub fn proto_key_to_ks(key: proto::Key) -> (Bytes, Option<Bytes>) {
    let pk = Bytes::from(key.partition_key);
    let sk = key.sort_key.map(Bytes::from);
    (pk, sk)
}
```

**Core Key Form** (used for transactions):
```rust
pub fn proto_key_to_core_key(key: proto::Key) -> Key {
    if let Some(sk) = key.sort_key {
        Key::with_sk(Bytes::from(key.partition_key), Bytes::from(sk))
    } else {
        Key::new(Bytes::from(key.partition_key))
    }
}
```

### Conversion Performance

Conversions are designed to minimize allocations:

- **Zero-copy where possible**: `Bytes` for binary data
- **Move semantics**: Transfer ownership instead of cloning
- **Lazy evaluation**: Only convert fields that are used

**Performance characteristics**:
- Simple value (string/number): ~50ns
- Nested map (10 fields): ~500ns
- Large list (100 items): ~5µs

For a typical query returning 100 items with 10 fields each, conversion overhead is ~50µs (0.05ms), which is negligible compared to network latency (~1-10ms).

## Async Operations with Tokio

### Runtime Architecture

The server runs on Tokio's multi-threaded runtime:

```
┌────────────────────────────────────────────────────────┐
│                  Tokio Runtime                          │
│  ┌──────────────────────────────────────────────────┐  │
│  │         Worker Thread Pool                       │  │
│  │  ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐         │  │
│  │  │ T1   │  │ T2   │  │ T3   │  │ T4   │  ...    │  │
│  │  └──┬───┘  └──┬───┘  └──┬───┘  └──┬───┘         │  │
│  │     │         │         │         │              │  │
│  │     └─────────┴─────────┴─────────┘              │  │
│  │              Event Loop                           │  │
│  │          (async task scheduler)                   │  │
│  └──────────────────────────────────────────────────┘  │
│                                                         │
│  ┌──────────────────────────────────────────────────┐  │
│  │      Blocking Thread Pool                        │  │
│  │  ┌──────┐  ┌──────┐  ┌──────┐  ┌──────┐         │  │
│  │  │ B1   │  │ B2   │  │ B3   │  │ B4   │  ...    │  │
│  │  └──────┘  └──────┘  └──────┘  └──────┘         │  │
│  │                                                   │  │
│  │     (for spawn_blocking database operations)     │  │
│  └──────────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────────┘
```

**Worker Threads**: Handle async I/O (network, timers, futures)
- Default: Number of CPU cores
- Non-blocking operations only

**Blocking Threads**: Handle synchronous operations (database, file I/O)
- Default: 512 threads
- Automatically managed by Tokio

### Bridging Async and Sync

The key pattern is `spawn_blocking`:

```rust
async fn put(&self, request: Request<PutRequest>)
    -> Result<Response<PutResponse>, Status>
{
    // This function is async and runs on the worker thread pool
    let db = Arc::clone(&self.db);
    let pk = /* ... */;
    let item = /* ... */;

    // spawn_blocking moves execution to the blocking thread pool
    let result = tokio::task::spawn_blocking(move || {
        // This closure runs on a blocking thread
        db.put(&pk, item)  // Synchronous database call
    })
    .await  // Async wait for blocking task to complete
    .map_err(|e| Status::internal(format!("Task join error: {}", e)))?;

    // Back on the worker thread pool
    Ok(Response::new(PutResponse { success: true, error: None }))
}
```

**Why this matters**:
- Database operations can block (disk I/O, locks, etc.)
- Blocking on worker threads prevents handling other requests
- `spawn_blocking` isolates blocking work to dedicated threads
- Worker threads remain free for async I/O

### Async Execution Flow

```
Request arrives
    ↓
Worker Thread: Deserialize request
    ↓
Worker Thread: Convert types
    ↓
Worker Thread: Call spawn_blocking
    ↓
    ├─→ Worker Thread: Yield, handle other requests
    │
    └─→ Blocking Thread: Execute db.put(&pk, item)
            ↓
        Blocking Thread: Acquire lock
            ↓
        Blocking Thread: Write WAL
            ↓
        Blocking Thread: Update memtable
            ↓
        Blocking Thread: Return result
            ↓
Worker Thread: Convert result
    ↓
Worker Thread: Serialize response
    ↓
Worker Thread: Send over network
```

### Concurrency Limits

The server naturally handles concurrent requests:

```rust
// Multiple clients connect
Client 1: PUT user#1   (Blocking Thread 1)
Client 2: GET user#2   (Blocking Thread 2)
Client 3: QUERY org#acme (Blocking Thread 3)
Client 4: SCAN ...     (Blocking Thread 4)

// All executing concurrently on separate blocking threads
// Network I/O multiplexed on worker threads
```

However, the LSM engine uses `RwLock`, so:
- Multiple reads can execute concurrently
- Writes are serialized (one at a time)

This is a future optimization opportunity (multi-version concurrency control).

### Backpressure Handling

When the server is overloaded, Tokio's backpressure mechanisms kick in:

1. **HTTP/2 Flow Control**: Slow consumers receive less data
2. **TCP Backpressure**: Full buffers pause sending
3. **Task Queue**: Blocking thread pool queue grows
4. **Connection Limits**: New connections rejected

The server exposes metrics to monitor these conditions:
```
active_connections 1000
blocking_queue_length 250
```

## TLS/SSL Configuration

### Current State

The current implementation does not include built-in TLS support. This is intentional to:
- Keep the initial implementation simple
- Allow flexibility in deployment architecture
- Defer to battle-tested infrastructure (reverse proxies)

### Reverse Proxy Pattern

The recommended approach for production is to use a reverse proxy:

**Architecture**:
```
┌──────────┐        TLS         ┌──────────┐    Plain HTTP/2    ┌──────────┐
│  Client  │ ←─────────────────→ │  Nginx   │ ←─────────────────→ │  Server  │
└──────────┘                     │  (TLS    │                     │ (localhost│
                                 │ termination)                   │  :50051) │
                                 └──────────┘                     └──────────┘
```

**Nginx Configuration**:
```nginx
# /etc/nginx/nginx.conf

http {
    upstream keystone {
        server 127.0.0.1:50051;
        keepalive 64;
    }

    server {
        listen 443 ssl http2;
        server_name db.example.com;

        # TLS configuration
        ssl_certificate /etc/ssl/certs/keystone.crt;
        ssl_certificate_key /etc/ssl/private/keystone.key;
        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_ciphers HIGH:!aNULL:!MD5;
        ssl_prefer_server_ciphers on;

        # gRPC-specific settings
        grpc_read_timeout 300s;
        grpc_send_timeout 300s;

        location / {
            grpc_pass grpc://keystone;
            grpc_set_header X-Real-IP $remote_addr;
            grpc_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        }
    }
}
```

**Server Configuration** (bind to localhost):
```bash
kstone-server \
  --db-path /var/lib/keystone/prod.db \
  --host 127.0.0.1 \
  --port 50051
```

**Client Configuration** (connect via TLS):
```rust
let client = Client::connect("https://db.example.com").await?;
```

### Future: Native TLS Support

Future versions may include native TLS using Tonic's built-in support:

```rust
// Server with TLS (future)
use tonic::transport::ServerTlsConfig;

let tls_config = ServerTlsConfig::new()
    .identity(Identity::from_pem(cert, key));

let server = Server::builder()
    .tls_config(tls_config)?
    .add_service(KeystoneDbServer::new(service));
```

```rust
// Client with TLS (future)
use tonic::transport::ClientTlsConfig;

let tls_config = ClientTlsConfig::new()
    .ca_certificate(Certificate::from_pem(ca_cert));

let channel = Channel::from_static("https://db.example.com:50051")
    .tls_config(tls_config)?
    .connect()
    .await?;
```

### mTLS (Mutual TLS)

For enhanced security, mTLS requires both server and client certificates:

**Nginx Configuration**:
```nginx
server {
    # ... TLS config ...

    # Require client certificate
    ssl_client_certificate /etc/ssl/certs/ca.crt;
    ssl_verify_client on;

    location / {
        # Pass client cert info to backend
        grpc_set_header X-Client-Cert $ssl_client_cert;
        grpc_pass grpc://keystone;
    }
}
```

This enables authentication based on client certificates, which can be mapped to user identities.

## Network Performance Optimization

### Serialization Performance

Protocol Buffers provides efficient binary serialization:

**Encoding Size** (vs JSON):
```
Item with 10 fields:
  JSON:     ~500 bytes (with whitespace)
  Protobuf: ~200 bytes (60% smaller)

Batch of 100 items:
  JSON:     ~50KB
  Protobuf: ~20KB (60% smaller)
```

**Encoding Speed**:
```
Serialize 1000 items:
  JSON:     ~500µs
  Protobuf: ~150µs (3.3x faster)

Deserialize 1000 items:
  JSON:     ~800µs
  Protobuf: ~200µs (4x faster)
```

### Network Bandwidth

**Factors affecting bandwidth**:

1. **Item Size**: Average item size determines payload
2. **Batch Size**: Larger batches amortize overhead
3. **Compression**: gRPC supports compression (gzip, deflate)
4. **Multiplexing**: Multiple streams share bandwidth

**Example**: Query returning 1000 items
- Average item: 500 bytes
- Total payload: 500KB
- With compression (70%): 150KB
- Network time (1Gbps): ~1.2ms

### Latency Optimization

**Latency Components**:
```
Total Latency = Network RTT + Server Processing + Serialization

Example breakdown:
  Network RTT:        2ms (same datacenter)
  Serialization:      0.1ms
  Server processing:  3ms
  Total:              5.1ms
```

**Optimization Strategies**:

1. **Batching**: Reduce network round trips
```rust
// Instead of N round trips
for key in keys {
    client.get(key).await?;  // N × 5ms = 500ms for 100 keys
}

// 1 round trip with batching
let batch = RemoteBatchGetRequest::new()
    .add_keys(keys);
client.batch_get(batch).await?;  // ~5ms total
```

2. **Parallel Requests**: Issue multiple requests concurrently
```rust
use futures::future::try_join_all;

let futures: Vec<_> = partitions.iter()
    .map(|pk| client.query(RemoteQuery::new(pk)))
    .collect();

let results = try_join_all(futures).await?;  // All queries in parallel
```

3. **Connection Pooling**: Reuse connections
```rust
// Good: Single client instance
let client = Client::connect("http://localhost:50051").await?;
for _ in 0..1000 {
    client.get(b"key").await?;  // Reuses connection
}

// Bad: New connection per request
for _ in 0..1000 {
    let client = Client::connect("http://localhost:50051").await?;
    client.get(b"key").await?;  // 3-way handshake each time
}
```

4. **Compression**: Enable for large payloads
```rust
// Enable gzip compression (future)
let channel = Channel::from_static("http://localhost:50051")
    .accept_compressed(CompressionEncoding::Gzip)
    .send_compressed(CompressionEncoding::Gzip)
    .connect()
    .await?;
```

### TCP Tuning

For high-throughput deployments, tune TCP settings:

**Linux Kernel Parameters** (`/etc/sysctl.conf`):
```bash
# Increase TCP buffer sizes
net.core.rmem_max = 268435456        # 256MB
net.core.wmem_max = 268435456        # 256MB
net.ipv4.tcp_rmem = 4096 87380 134217728
net.ipv4.tcp_wmem = 4096 65536 134217728

# Enable TCP window scaling
net.ipv4.tcp_window_scaling = 1

# Increase max backlog
net.core.netdev_max_backlog = 5000

# Enable TCP Fast Open
net.ipv4.tcp_fastopen = 3
```

Apply with:
```bash
sudo sysctl -p
```

**Server Configuration**:
```rust
let server = Server::builder()
    .tcp_keepalive(Some(Duration::from_secs(30)))  // Detect dead connections
    .tcp_nodelay(true)  // Disable Nagle's algorithm for low latency
    .add_service(KeystoneDbServer::new(service));
```

## Deployment Patterns

### Single Server Deployment

The simplest deployment for development and small-scale production:

```
┌──────────────────────────────────────┐
│         Application Server            │
│  ┌────────────────────────────────┐  │
│  │      kstone-server             │  │
│  │  Port: 50051                   │  │
│  │  DB: /var/lib/keystone/db      │  │
│  └────────────────────────────────┘  │
└──────────────────────────────────────┘
```

**Configuration**:
```bash
kstone-server \
  --db-path /var/lib/keystone/prod.db \
  --host 0.0.0.0 \
  --port 50051 \
  --max-connections 1000
```

**Pros**:
- Simple to deploy and manage
- No coordination required
- Low latency (no network hops)

**Cons**:
- Single point of failure
- Limited scalability
- No redundancy

### Load Balanced Deployment

Multiple read replicas behind a load balancer:

```
                  ┌──────────────┐
                  │ Load Balancer│
                  │  (Nginx/HAProxy)
                  └──────┬───────┘
                         │
         ┌───────────────┼───────────────┐
         │               │               │
         ▼               ▼               ▼
    ┌────────┐      ┌────────┐      ┌────────┐
    │Server 1│      │Server 2│      │Server 3│
    │:50051  │      │:50051  │      │:50051  │
    └────────┘      └────────┘      └────────┘
         │               │               │
         └───────────────┼───────────────┘
                         ▼
                 ┌───────────────┐
                 │  Shared Storage│
                 │  (NFS/EBS)    │
                 └───────────────┘
```

**Note**: This pattern requires external synchronization, which is not currently supported. Use for read-only replicas.

### Docker Deployment

**Dockerfile**:
```dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY . .
RUN cargo build --release --bin kstone-server

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/kstone-server /usr/local/bin/

EXPOSE 50051
EXPOSE 9090

VOLUME ["/var/lib/keystone"]

ENTRYPOINT ["kstone-server"]
CMD ["--db-path", "/var/lib/keystone/db", "--host", "0.0.0.0"]
```

**docker-compose.yml**:
```yaml
version: '3.8'

services:
  keystone:
    build: .
    ports:
      - "50051:50051"  # gRPC
      - "9090:9090"    # Metrics
    volumes:
      - keystone-data:/var/lib/keystone
    environment:
      - RUST_LOG=info
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:9090/health"]
      interval: 30s
      timeout: 10s
      retries: 3

volumes:
  keystone-data:
```

**Build and Run**:
```bash
docker-compose up -d
docker-compose logs -f keystone
```

### Kubernetes Deployment

**Deployment Manifest** (`deployment.yaml`):
```yaml
apiVersion: v1
kind: Service
metadata:
  name: keystone-db
spec:
  selector:
    app: keystone
  ports:
    - name: grpc
      port: 50051
      targetPort: 50051
    - name: metrics
      port: 9090
      targetPort: 9090
  type: ClusterIP

---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: keystone-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: keystone
  template:
    metadata:
      labels:
        app: keystone
    spec:
      containers:
      - name: keystone
        image: keystone-server:latest
        args:
          - "--db-path"
          - "/var/lib/keystone/db"
          - "--host"
          - "0.0.0.0"
          - "--max-connections"
          - "1000"
          - "--shutdown-timeout"
          - "60"
        ports:
        - containerPort: 50051
          name: grpc
        - containerPort: 9090
          name: metrics
        volumeMounts:
        - name: data
          mountPath: /var/lib/keystone
        livenessProbe:
          httpGet:
            path: /health
            port: 9090
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 9090
          initialDelaySeconds: 5
          periodSeconds: 5
        resources:
          requests:
            memory: "1Gi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2000m"
      volumes:
      - name: data
        persistentVolumeClaim:
          claimName: keystone-pvc

---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: keystone-pvc
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 100Gi
```

**Apply**:
```bash
kubectl apply -f deployment.yaml
kubectl get pods -l app=keystone
kubectl logs -f deployment/keystone-server
```

### Cloud Deployment (AWS)

**Elastic Container Service (ECS)**:

1. **Create ECR repository**:
```bash
aws ecr create-repository --repository-name keystone-server
```

2. **Push Docker image**:
```bash
aws ecr get-login-password | docker login --username AWS --password-stdin <account>.dkr.ecr.<region>.amazonaws.com
docker tag keystone-server:latest <account>.dkr.ecr.<region>.amazonaws.com/keystone-server:latest
docker push <account>.dkr.ecr.<region>.amazonaws.com/keystone-server:latest
```

3. **Create ECS task definition** (`task-definition.json`):
```json
{
  "family": "keystone-server",
  "networkMode": "awsvpc",
  "requiresCompatibilities": ["FARGATE"],
  "cpu": "1024",
  "memory": "2048",
  "containerDefinitions": [
    {
      "name": "keystone",
      "image": "<account>.dkr.ecr.<region>.amazonaws.com/keystone-server:latest",
      "portMappings": [
        {"containerPort": 50051, "protocol": "tcp"},
        {"containerPort": 9090, "protocol": "tcp"}
      ],
      "environment": [
        {"name": "RUST_LOG", "value": "info"}
      ],
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group": "/ecs/keystone-server",
          "awslogs-region": "us-east-1",
          "awslogs-stream-prefix": "ecs"
        }
      },
      "mountPoints": [
        {
          "sourceVolume": "keystone-data",
          "containerPath": "/var/lib/keystone"
        }
      ]
    }
  ],
  "volumes": [
    {
      "name": "keystone-data",
      "efsVolumeConfiguration": {
        "fileSystemId": "<efs-id>",
        "transitEncryption": "ENABLED"
      }
    }
  ]
}
```

4. **Create ECS service**:
```bash
aws ecs create-service \
  --cluster keystone-cluster \
  --service-name keystone-server \
  --task-definition keystone-server \
  --desired-count 3 \
  --launch-type FARGATE \
  --network-configuration "awsvpcConfiguration={subnets=[subnet-xxx],securityGroups=[sg-xxx]}"
```

## Monitoring and Observability

### Metrics Collection

**Prometheus Configuration** (`prometheus.yml`):
```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'keystone'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
```

**Key Metrics to Monitor**:

- `rpc_requests_total{method, status}`: Request count by method and status
- `rpc_duration_seconds{method}`: Request latency histogram
- `active_connections`: Current active connections
- `rate_limited_requests{limit_type}`: Rate limiting events

**Grafana Dashboard**:
```json
{
  "panels": [
    {
      "title": "Request Rate",
      "targets": [
        {
          "expr": "rate(rpc_requests_total[1m])"
        }
      ]
    },
    {
      "title": "Request Latency (p50, p95, p99)",
      "targets": [
        {
          "expr": "histogram_quantile(0.50, rate(rpc_duration_seconds_bucket[1m]))"
        },
        {
          "expr": "histogram_quantile(0.95, rate(rpc_duration_seconds_bucket[1m]))"
        },
        {
          "expr": "histogram_quantile(0.99, rate(rpc_duration_seconds_bucket[1m]))"
        }
      ]
    },
    {
      "title": "Active Connections",
      "targets": [
        {
          "expr": "active_connections"
        }
      ]
    }
  ]
}
```

### Distributed Tracing

The server includes trace IDs in structured logs:

```rust
let trace_id = Uuid::new_v4().to_string();
tracing::Span::current().record("trace_id", &trace_id);

info!("Received put request");  // Includes trace_id in output
```

**Log Output**:
```
2024-01-15T10:30:45.123Z INFO kstone_server::service: Received put request trace_id="a1b2c3d4-..."
2024-01-15T10:30:45.125Z INFO kstone_server::service: Put operation completed successfully trace_id="a1b2c3d4-..."
```

For production, integrate with distributed tracing systems like Jaeger or Zipkin.

## Summary

KeystoneDB's network architecture provides:

- **Efficient Communication**: HTTP/2 multiplexing, binary serialization, minimal copying
- **Type Safety**: Bidirectional conversions with compile-time guarantees
- **Async Performance**: Tokio runtime with blocking pool for database operations
- **Security**: TLS via reverse proxy, future mTLS support
- **Scalability**: Connection pooling, batching, parallel operations
- **Production Ready**: Docker/Kubernetes deployment, monitoring, observability

The architecture balances simplicity (no distributed coordination) with power (full gRPC feature set), making KeystoneDB suitable for a wide range of deployment scenarios from single-server applications to multi-region cloud deployments.

The combination of the gRPC server (Chapter 24), client library (Chapter 25), and network architecture (this chapter) provides a complete distributed database solution that's both easy to use and performant at scale.

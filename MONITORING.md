# KeystoneDB Observability & Monitoring

This document describes the observability features built into KeystoneDB and how to use them for monitoring and debugging.

## Overview

KeystoneDB includes comprehensive observability features:

- **Database Statistics API** - Runtime metrics via `db.stats()` (Phase 8+)
- **Health Check API** - Operational status via `db.health()` (Phase 8+)
- **Structured Logging** - Detailed logs with context using the `tracing` crate
- **Prometheus Metrics** - Performance and operational metrics in Prometheus format (server mode)
- **Health Endpoints** - Liveness and readiness endpoints for orchestration systems (server mode)
- **Request Tracing** - Unique trace IDs for correlating logs across request lifecycle (server mode)

## Database Statistics API

KeystoneDB provides a `stats()` method for retrieving runtime metrics (Phase 8+).

### Using stats()

```rust
use kstone_api::Database;

let db = Database::open("/data/mydb.keystone")?;

// Get database statistics
let stats = db.stats()?;

println!("Database Statistics:");
println!("  Total SST files: {}", stats.total_sst_files);
println!("  WAL size: {:?}", stats.wal_size_bytes);
println!("  Memtable size: {:?}", stats.memtable_size_bytes);
println!("  Total disk size: {:?}", stats.total_disk_size_bytes);

// Compaction statistics
println!("\nCompaction Statistics:");
println!("  Total compactions: {}", stats.compaction.total_compactions);
println!("  SSTs merged: {}", stats.compaction.total_ssts_merged);
println!("  SSTs created: {}", stats.compaction.total_ssts_created);
println!("  Bytes read: {}", stats.compaction.total_bytes_read);
println!("  Bytes written: {}", stats.compaction.total_bytes_written);
println!("  Bytes reclaimed: {}", stats.compaction.total_bytes_reclaimed);
println!("  Records deduplicated: {}", stats.compaction.total_records_deduplicated);
println!("  Tombstones removed: {}", stats.compaction.total_tombstones_removed);
println!("  Active compactions: {}", stats.compaction.active_compactions);
```

### DatabaseStats Fields

```rust
pub struct DatabaseStats {
    /// Total number of keys (if available - may be None for large databases)
    pub total_keys: Option<u64>,

    /// Total number of SST files across all stripes
    pub total_sst_files: u64,

    /// Current WAL size in bytes (None if not tracked)
    pub wal_size_bytes: Option<u64>,

    /// Current memtable size in bytes (None if not tracked)
    pub memtable_size_bytes: Option<u64>,

    /// Total disk space used in bytes (None if not tracked)
    pub total_disk_size_bytes: Option<u64>,

    /// Compaction statistics
    pub compaction: CompactionStats,
}
```

### CompactionStats Fields

```rust
pub struct CompactionStats {
    /// Total number of compactions performed
    pub total_compactions: u64,

    /// Total number of SSTs merged
    pub total_ssts_merged: u64,

    /// Total number of SSTs created
    pub total_ssts_created: u64,

    /// Total bytes read during compaction
    pub total_bytes_read: u64,

    /// Total bytes written during compaction
    pub total_bytes_written: u64,

    /// Total bytes reclaimed (space saved)
    pub total_bytes_reclaimed: u64,

    /// Total records deduplicated
    pub total_records_deduplicated: u64,

    /// Total tombstones removed
    pub total_tombstones_removed: u64,

    /// Number of active compactions
    pub active_compactions: u64,
}
```

### Monitoring with stats()

**Check write amplification:**
```rust
let stats = db.stats()?;
let write_amp = stats.compaction.total_bytes_written as f64
    / stats.compaction.total_bytes_read as f64;
println!("Write amplification: {:.2}x", write_amp);

if write_amp > 5.0 {
    println!("WARNING: High write amplification detected");
}
```

**Monitor compaction effectiveness:**
```rust
let stats = db.stats()?;
println!("Space reclaimed: {} bytes", stats.compaction.total_bytes_reclaimed);
println!("Tombstones removed: {}", stats.compaction.total_tombstones_removed);
println!("Records deduplicated: {}", stats.compaction.total_records_deduplicated);
```

**Track compaction activity:**
```rust
let stats = db.stats()?;
println!("Active compactions: {}", stats.compaction.active_compactions);
println!("Total compactions: {}", stats.compaction.total_compactions);
```

## Database Health API

KeystoneDB provides a `health()` method for checking operational status (Phase 8+).

### Using health()

```rust
use kstone_api::{Database, HealthStatus};

let db = Database::open("/data/mydb.keystone")?;

// Check database health
let health = db.health();

match health.status {
    HealthStatus::Healthy => {
        println!("✓ Database is healthy");
    }
    HealthStatus::Degraded => {
        println!("⚠ Database is degraded:");
        for warning in &health.warnings {
            println!("  - {}", warning);
        }
    }
    HealthStatus::Unhealthy => {
        println!("✗ Database is unhealthy:");
        for error in &health.errors {
            println!("  - {}", error);
        }
        // Take corrective action
    }
}
```

### HealthStatus Enum

```rust
pub enum HealthStatus {
    /// Database is fully operational
    Healthy,
    /// Database is operational but has warnings
    Degraded,
    /// Database is not operational
    Unhealthy,
}
```

### DatabaseHealth Struct

```rust
pub struct DatabaseHealth {
    /// Overall health status
    pub status: HealthStatus,
    /// Warning messages (non-fatal issues)
    pub warnings: Vec<String>,
    /// Error messages (fatal issues)
    pub errors: Vec<String>,
}
```

### Health Check Examples

**Example 1: Healthy Database**
```rust
let health = db.health();
assert_eq!(health.status, HealthStatus::Healthy);
assert!(health.warnings.is_empty());
assert!(health.errors.is_empty());
```

**Example 2: Degraded Database**
```rust
let health = db.health();
if health.status == HealthStatus::Degraded {
    // Example warnings:
    // - "High SST count in stripe 42 (15 files)"
    // - "Compaction falling behind in 3 stripes"
    // - "Disk usage above 80%"
}
```

**Example 3: Unhealthy Database**
```rust
let health = db.health();
if health.status == HealthStatus::Unhealthy {
    // Example errors:
    // - "Database directory not accessible"
    // - "Corruption detected in WAL"
    // - "Unable to write to disk (space full)"

    // Log error and alert
    for error in &health.errors {
        log::error!("Database error: {}", error);
    }
}
```

### Monitoring Integration

**Periodic health checks:**
```rust
use std::time::Duration;
use std::thread;

loop {
    let health = db.health();

    if health.status != HealthStatus::Healthy {
        // Send alert
        send_alert(&health);
    }

    thread::sleep(Duration::from_secs(60));
}
```

**Expose health via HTTP:**
```rust
use actix_web::{web, App, HttpResponse, HttpServer};

async fn health_check(db: web::Data<Database>) -> HttpResponse {
    let health = db.health();

    match health.status {
        HealthStatus::Healthy => HttpResponse::Ok().json(health),
        HealthStatus::Degraded => HttpResponse::Ok().json(health),
        HealthStatus::Unhealthy => HttpResponse::ServiceUnavailable().json(health),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db = Database::open("/data/mydb.keystone").unwrap();

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db.clone()))
            .route("/health", web::get().to(health_check))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
```

**Kubernetes liveness probe:**
```yaml
livenessProbe:
  httpGet:
    path: /health
    port: 8080
  initialDelaySeconds: 10
  periodSeconds: 30
```

## Structured Logging

### Configuration

Logs are configured via the `RUST_LOG` environment variable:

```bash
# Info level (default)
RUST_LOG=info cargo run --bin kstone-server -- --db-path ./db

# Debug level for detailed output
RUST_LOG=debug cargo run --bin kstone-server -- --db-path ./db

# Trace level for maximum verbosity
RUST_LOG=trace cargo run --bin kstone-server -- --db-path ./db
```

### Log Format

Logs include:
- **Timestamp** - When the event occurred
- **Level** - INFO, DEBUG, WARN, ERROR
- **Target** - Module that generated the log
- **Thread ID** - Async task identifier
- **File & Line** - Source location
- **Trace ID** - UUID for request correlation
- **Span Context** - Additional structured fields

Example log output:
```
2025-01-15T10:23:45.123Z INFO [kstone_server::service] trace_id="a1b2c3d4-..." has_sk=true Received put request
```

### Request Tracing

Every RPC request is assigned a unique trace ID (UUID v4) that appears in all logs for that request. Use trace IDs to:
- Correlate logs across the request lifecycle
- Debug specific requests
- Track request flow through the system

## Prometheus Metrics

KeystoneDB exposes metrics in Prometheus format on port 9090 at `/metrics`.

### Available Metrics

#### RPC Request Metrics

**`kstone_rpc_requests_total`** (Counter)
- Total number of RPC requests
- Labels: `method` (put, get, delete, etc.), `status` (success, error)

**`kstone_rpc_duration_seconds`** (Histogram)
- RPC request duration distribution
- Labels: `method`
- Buckets: 1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s, 10s

#### Database Operations

**`kstone_db_operations_total`** (Counter)
- Total database operations
- Labels: `operation` (put, get, delete, query, scan), `status` (success, error)

#### Connection Metrics

**`kstone_active_connections`** (Gauge)
- Number of active gRPC connections

#### Error Metrics

**`kstone_errors_total`** (Counter)
- Total errors by type
- Labels: `error_type` (not_found, invalid_argument, condition_failed, etc.)

### Accessing Metrics

Metrics endpoint:
```bash
curl http://localhost:9090/metrics
```

Example output:
```
# HELP kstone_rpc_requests_total Total number of RPC requests
# TYPE kstone_rpc_requests_total counter
kstone_rpc_requests_total{method="put",status="success"} 1234
kstone_rpc_requests_total{method="get",status="success"} 5678

# HELP kstone_rpc_duration_seconds RPC request duration in seconds
# TYPE kstone_rpc_duration_seconds histogram
kstone_rpc_duration_seconds_bucket{method="put",le="0.001"} 100
kstone_rpc_duration_seconds_bucket{method="put",le="0.005"} 450
...
```

### Example Prometheus Queries

**Request rate by method:**
```promql
rate(kstone_rpc_requests_total[5m])
```

**Error rate:**
```promql
rate(kstone_rpc_requests_total{status="error"}[5m])
```

**99th percentile latency:**
```promql
histogram_quantile(0.99, rate(kstone_rpc_duration_seconds_bucket[5m]))
```

**Success rate:**
```promql
sum(rate(kstone_rpc_requests_total{status="success"}[5m]))
/
sum(rate(kstone_rpc_requests_total[5m]))
```

## Health Check Endpoints

### Liveness Probe: `/health`

Indicates if the server process is alive and responsive.

```bash
curl http://localhost:9090/health
# Response: OK
```

**Use case:** Kubernetes liveness probe to restart crashed containers

### Readiness Probe: `/ready`

Indicates if the server is ready to accept traffic.

```bash
curl http://localhost:9090/ready
# Response: OK
```

**Use case:** Kubernetes readiness probe to control traffic routing

## Setting Up Monitoring

### Prometheus Configuration

Add to `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: 'keystonedb'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
```

Start Prometheus:
```bash
docker run -p 9091:9090 \
  -v $(pwd)/prometheus.yml:/etc/prometheus/prometheus.yml \
  prom/prometheus
```

Access Prometheus UI at http://localhost:9091

### Grafana Dashboard

Example dashboard configuration:

**Panel: Request Rate**
- Query: `sum(rate(kstone_rpc_requests_total[5m])) by (method)`
- Visualization: Graph

**Panel: Latency**
- Query: `histogram_quantile(0.95, rate(kstone_rpc_duration_seconds_bucket[5m]))`
- Visualization: Graph

**Panel: Error Rate**
- Query: `rate(kstone_rpc_requests_total{status="error"}[5m])`
- Visualization: Graph

**Panel: Active Connections**
- Query: `kstone_active_connections`
- Visualization: Gauge

### Kubernetes Integration

Example deployment with health checks:

```yaml
apiVersion: v1
kind: Service
metadata:
  name: kstone-server
spec:
  ports:
  - name: grpc
    port: 50051
    targetPort: 50051
  - name: metrics
    port: 9090
    targetPort: 9090
  selector:
    app: kstone-server

---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: kstone-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: kstone-server
  template:
    metadata:
      labels:
        app: kstone-server
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "9090"
        prometheus.io/path: "/metrics"
    spec:
      containers:
      - name: kstone-server
        image: keystonedb/server:latest
        ports:
        - containerPort: 50051
          name: grpc
        - containerPort: 9090
          name: metrics
        livenessProbe:
          httpGet:
            path: /health
            port: 9090
          initialDelaySeconds: 10
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 9090
          initialDelaySeconds: 5
          periodSeconds: 5
        env:
        - name: RUST_LOG
          value: "info"
```

## Debugging with Observability

### Investigating Slow Requests

1. Check latency metrics to identify slow methods:
   ```promql
   histogram_quantile(0.99, rate(kstone_rpc_duration_seconds_bucket[5m])) by (method)
   ```

2. Enable debug logging:
   ```bash
   RUST_LOG=debug cargo run --bin kstone-server
   ```

3. Find trace ID from logs and grep all related log lines:
   ```bash
   grep "trace_id=\"a1b2c3d4-...\"" server.log
   ```

### Investigating Errors

1. Check error rate by type:
   ```promql
   rate(kstone_errors_total[5m]) by (error_type)
   ```

2. Check RPC error rate:
   ```promql
   rate(kstone_rpc_requests_total{status="error"}[5m]) by (method)
   ```

3. Find error logs:
   ```bash
   grep "ERROR" server.log | tail -50
   ```

### Monitoring Performance Degradation

Set up alerts in Prometheus:

**High Error Rate:**
```yaml
- alert: HighErrorRate
  expr: rate(kstone_rpc_requests_total{status="error"}[5m]) > 0.05
  for: 5m
  annotations:
    summary: "High error rate detected"
```

**High Latency:**
```yaml
- alert: HighLatency
  expr: histogram_quantile(0.95, rate(kstone_rpc_duration_seconds_bucket[5m])) > 1.0
  for: 5m
  annotations:
    summary: "95th percentile latency > 1s"
```

## Best Practices

1. **Always use trace IDs** - Include trace ID when reporting issues
2. **Monitor in production** - Set up Prometheus + Grafana for production systems
3. **Set up alerts** - Don't wait for users to report problems
4. **Use structured logging** - Filter and search logs efficiently with `RUST_LOG`
5. **Baseline metrics** - Establish normal performance baselines for your workload
6. **Dashboard for common issues** - Create runbooks linked to dashboard panels

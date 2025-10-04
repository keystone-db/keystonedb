# Chapter 31: Monitoring & Observability

Effective monitoring is essential for maintaining healthy, performant KeystoneDB deployments. This chapter covers KeystoneDB's comprehensive observability features, from built-in statistics APIs to production-grade monitoring with Prometheus and Grafana.

## Database Statistics API

KeystoneDB provides a `stats()` method that returns detailed runtime metrics about database operations, storage, and compaction activity. This API is available in both embedded and server modes.

### Using the Stats API

The stats API provides a snapshot of current database state:

```rust
use kstone_api::Database;

let db = Database::open("/var/lib/keystonedb/data/production.keystone")?;

// Get comprehensive database statistics
let stats = db.stats()?;

println!("=== Database Statistics ===");
println!("Total SST files: {}", stats.total_sst_files);
println!("Total keys: {:?}", stats.total_keys);
println!("WAL size: {:?} bytes", stats.wal_size_bytes);
println!("Memtable size: {:?} bytes", stats.memtable_size_bytes);
println!("Total disk size: {:?} bytes", stats.total_disk_size_bytes);

println!("\n=== Compaction Statistics ===");
let cs = &stats.compaction;
println!("Total compactions: {}", cs.total_compactions);
println!("SSTs merged: {}", cs.total_ssts_merged);
println!("SSTs created: {}", cs.total_ssts_created);
println!("Bytes read: {} MB", cs.total_bytes_read / 1_048_576);
println!("Bytes written: {} MB", cs.total_bytes_written / 1_048_576);
println!("Bytes reclaimed: {} MB", cs.total_bytes_reclaimed / 1_048_576);
println!("Records deduplicated: {}", cs.total_records_deduplicated);
println!("Tombstones removed: {}", cs.total_tombstones_removed);
println!("Active compactions: {}", cs.active_compactions);
```

### DatabaseStats Structure

The `DatabaseStats` struct provides comprehensive metrics:

```rust
pub struct DatabaseStats {
    /// Total number of keys (optional - may be None for large databases)
    pub total_keys: Option<u64>,

    /// Total number of SST files across all stripes
    pub total_sst_files: u64,

    /// Current WAL size in bytes
    pub wal_size_bytes: Option<u64>,

    /// Current memtable size in bytes across all stripes
    pub memtable_size_bytes: Option<u64>,

    /// Total disk space used in bytes
    pub total_disk_size_bytes: Option<u64>,

    /// Compaction statistics
    pub compaction: CompactionStats,
}

pub struct CompactionStats {
    /// Total number of compactions performed
    pub total_compactions: u64,

    /// Total number of SSTs merged during compaction
    pub total_ssts_merged: u64,

    /// Total number of SSTs created by compaction
    pub total_ssts_created: u64,

    /// Total bytes read during compaction
    pub total_bytes_read: u64,

    /// Total bytes written during compaction
    pub total_bytes_written: u64,

    /// Total bytes reclaimed (space savings)
    pub total_bytes_reclaimed: u64,

    /// Total records deduplicated
    pub total_records_deduplicated: u64,

    /// Total tombstones removed
    pub total_tombstones_removed: u64,

    /// Number of currently active compactions
    pub active_compactions: u64,
}
```

### Metrics Analysis

**Write Amplification:**

Write amplification measures how much extra data is written due to compaction:

```rust
let stats = db.stats()?;
let write_amp = stats.compaction.total_bytes_written as f64
    / stats.compaction.total_bytes_read.max(1) as f64;

println!("Write amplification: {:.2}x", write_amp);

if write_amp > 5.0 {
    println!("WARNING: High write amplification detected");
    println!("Consider:");
    println!("  - Increasing memtable threshold");
    println!("  - Reducing compaction frequency");
    println!("  - Using larger items to amortize overhead");
}
```

**Space Efficiency:**

Analyze how effectively compaction is reclaiming space:

```rust
let stats = db.stats()?;
let reclaim_ratio = stats.compaction.total_bytes_reclaimed as f64
    / stats.compaction.total_bytes_read.max(1) as f64;

println!("Space reclaim ratio: {:.1}%", reclaim_ratio * 100.0);
println!("Total space reclaimed: {} MB",
    stats.compaction.total_bytes_reclaimed / 1_048_576);
println!("Tombstones removed: {}", stats.compaction.total_tombstones_removed);
println!("Records deduplicated: {}", stats.compaction.total_records_deduplicated);
```

**Compaction Activity:**

Monitor compaction throughput and activity:

```rust
let stats = db.stats()?;
println!("Compaction Summary:");
println!("  Total compactions: {}", stats.compaction.total_compactions);
println!("  Active compactions: {}", stats.compaction.active_compactions);
println!("  Average SSTs per compaction: {:.1}",
    stats.compaction.total_ssts_merged as f64 /
    stats.compaction.total_compactions.max(1) as f64);
```

## Health Check API

The `health()` method provides operational health status, returning one of three states: Healthy, Degraded, or Unhealthy.

### Using the Health API

```rust
use kstone_api::{Database, HealthStatus};

let db = Database::open("/var/lib/keystonedb/data/production.keystone")?;

// Check database health
let health = db.health();

match health.status {
    HealthStatus::Healthy => {
        println!("✓ Database is fully operational");
    }
    HealthStatus::Degraded => {
        println!("⚠ Database is operational with warnings:");
        for warning in &health.warnings {
            println!("  - {}", warning);
        }
        // Log warnings, send alerts
    }
    HealthStatus::Unhealthy => {
        println!("✗ Database is not operational:");
        for error in &health.errors {
            println!("  - {}", error);
        }
        // Critical alert, page on-call engineer
    }
}
```

### DatabaseHealth Structure

```rust
pub enum HealthStatus {
    /// Database is fully operational
    Healthy,
    /// Database is operational but has warnings
    Degraded,
    /// Database is not operational
    Unhealthy,
}

pub struct DatabaseHealth {
    /// Overall health status
    pub status: HealthStatus,

    /// Warning messages (non-fatal issues)
    pub warnings: Vec<String>,

    /// Error messages (fatal issues)
    pub errors: Vec<String>,
}
```

### Health Check Conditions

**Healthy Status:**
- All operations functioning normally
- Compaction keeping up with writes
- Disk space available
- No corruption detected

**Degraded Status (Warning Conditions):**
- High SST count in one or more stripes (>10 SSTs)
- Compaction falling behind (>15 SSTs in any stripe)
- Disk usage above 80%
- High write amplification (>5x)
- Memory pressure

**Unhealthy Status (Error Conditions):**
- Database directory not accessible
- Corruption detected in WAL or SST files
- Unable to write to disk (space full)
- Critical I/O errors
- Database failed to open

### HTTP Health Endpoint

Expose health status via HTTP for monitoring systems:

```rust
use actix_web::{web, App, HttpResponse, HttpServer};
use kstone_api::Database;

async fn health_check(db: web::Data<Database>) -> HttpResponse {
    let health = db.health();

    let status_code = match health.status {
        HealthStatus::Healthy => 200,
        HealthStatus::Degraded => 200,  // Still serving traffic
        HealthStatus::Unhealthy => 503, // Service unavailable
    };

    HttpResponse::build(status_code.into()).json(serde_json::json!({
        "status": format!("{:?}", health.status),
        "warnings": health.warnings,
        "errors": health.errors,
    }))
}

async fn liveness_check() -> HttpResponse {
    // Simple check: is the process alive?
    HttpResponse::Ok().body("OK")
}

async fn readiness_check(db: web::Data<Database>) -> HttpResponse {
    // Check if ready to accept traffic
    let health = db.health();

    match health.status {
        HealthStatus::Unhealthy => HttpResponse::ServiceUnavailable().body("NOT READY"),
        _ => HttpResponse::Ok().body("READY"),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let db = Database::open("/var/lib/keystonedb/data/production.keystone")
        .expect("Failed to open database");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db.clone()))
            .route("/health", web::get().to(health_check))
            .route("/healthz", web::get().to(liveness_check))
            .route("/ready", web::get().to(readiness_check))
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
```

## Prometheus Metrics

When running in server mode, KeystoneDB exposes Prometheus metrics on port 9090 at `/metrics`.

### Available Metrics

**RPC Request Metrics:**

```
# HELP kstone_rpc_requests_total Total number of RPC requests
# TYPE kstone_rpc_requests_total counter
kstone_rpc_requests_total{method="put",status="success"} 12345
kstone_rpc_requests_total{method="put",status="error"} 23
kstone_rpc_requests_total{method="get",status="success"} 56789
kstone_rpc_requests_total{method="delete",status="success"} 1234
kstone_rpc_requests_total{method="query",status="success"} 8901
```

**RPC Duration Metrics:**

```
# HELP kstone_rpc_duration_seconds RPC request duration
# TYPE kstone_rpc_duration_seconds histogram
kstone_rpc_duration_seconds_bucket{method="put",le="0.001"} 8000
kstone_rpc_duration_seconds_bucket{method="put",le="0.005"} 11000
kstone_rpc_duration_seconds_bucket{method="put",le="0.010"} 11800
kstone_rpc_duration_seconds_bucket{method="put",le="0.050"} 12200
kstone_rpc_duration_seconds_bucket{method="put",le="0.100"} 12300
kstone_rpc_duration_seconds_bucket{method="put",le="0.500"} 12340
kstone_rpc_duration_seconds_bucket{method="put",le="1.000"} 12345
kstone_rpc_duration_seconds_bucket{method="put",le="+Inf"} 12345
kstone_rpc_duration_seconds_sum{method="put"} 45.678
kstone_rpc_duration_seconds_count{method="put"} 12345
```

**Database Operation Metrics:**

```
# HELP kstone_db_operations_total Total database operations
# TYPE kstone_db_operations_total counter
kstone_db_operations_total{operation="put",status="success"} 12345
kstone_db_operations_total{operation="get",status="success"} 56789
kstone_db_operations_total{operation="delete",status="success"} 1234
```

**Connection Metrics:**

```
# HELP kstone_active_connections Number of active gRPC connections
# TYPE kstone_active_connections gauge
kstone_active_connections 42
```

**Error Metrics:**

```
# HELP kstone_errors_total Total errors by type
# TYPE kstone_errors_total counter
kstone_errors_total{error_type="not_found"} 123
kstone_errors_total{error_type="invalid_argument"} 45
kstone_errors_total{error_type="condition_failed"} 12
```

### Accessing Metrics

Metrics are available via HTTP:

```bash
# Fetch metrics
curl http://localhost:9090/metrics

# Filter specific metrics
curl -s http://localhost:9090/metrics | grep kstone_rpc_requests_total

# Format for readability
curl -s http://localhost:9090/metrics | grep -A 5 "kstone_rpc_duration"
```

## Structured Logging with Tracing

KeystoneDB uses the `tracing` crate for structured, contextual logging.

### Log Configuration

Configure log levels via the `RUST_LOG` environment variable:

```bash
# Production (recommended)
export RUST_LOG=info

# Debug mode (verbose)
export RUST_LOG=debug

# Trace mode (very verbose)
export RUST_LOG=trace

# Module-specific levels
export RUST_LOG=kstone_core=debug,kstone_api=info

# Complex filtering
export RUST_LOG=info,kstone_core::compaction=debug,kstone_server=trace
```

### Log Format

Structured logs include:
- **Timestamp**: ISO 8601 format
- **Level**: ERROR, WARN, INFO, DEBUG, TRACE
- **Target**: Module path (e.g., `kstone_core::lsm`)
- **Thread**: Thread name or ID
- **Span Context**: Request trace ID, operation name
- **Message**: Log message
- **Fields**: Structured key-value pairs

Example log output:

```
2025-01-15T14:23:45.123Z INFO [kstone_server::service] trace_id="a1b2c3d4-e5f6-7890-abcd-ef1234567890" method="put" has_sk=false Received put request
2025-01-15T14:23:45.125Z DEBUG [kstone_core::lsm] stripe=42 key_len=10 Routing key to stripe
2025-01-15T14:23:45.127Z DEBUG [kstone_core::wal] lsn=12345 size=256 Appending record to WAL
2025-01-15T14:23:45.129Z INFO [kstone_server::service] trace_id="a1b2c3d4-e5f6-7890-abcd-ef1234567890" duration_ms=6 Put operation completed
```

### Request Tracing

Every RPC request receives a unique trace ID (UUID v4) for correlation:

```rust
use tracing::{info, debug, error};
use uuid::Uuid;

// Generate trace ID at request start
let trace_id = Uuid::new_v4();

// Create span with trace ID
let span = tracing::info_span!("put_request", %trace_id);
let _enter = span.enter();

info!("Received put request");
debug!(key_len = key.len(), "Processing key");

// All logs within this span include trace_id
```

**Using trace IDs for debugging:**

```bash
# Find all logs for a specific request
grep 'trace_id="a1b2c3d4-e5f6-7890-abcd-ef1234567890"' server.log

# Extract trace ID from error
ERROR_TRACE=$(grep "ERROR" server.log | grep -o 'trace_id="[^"]*"' | head -1)

# Get full request trace
grep "$ERROR_TRACE" server.log
```

### Log Analysis Patterns

**Finding errors:**

```bash
# All errors
grep "ERROR" server.log

# Errors with context (5 lines before/after)
grep -B 5 -A 5 "ERROR" server.log

# Specific error types
grep "Corruption detected" server.log
grep "IO error" server.log
```

**Analyzing performance:**

```bash
# Find slow operations (>100ms)
grep "duration_ms" server.log | awk '$NF > 100'

# Compaction frequency
grep "compaction completed" server.log | wc -l

# Flush frequency
grep "memtable flush" server.log | wc -l
```

**Request tracing:**

```bash
# Extract unique trace IDs
grep -o 'trace_id="[^"]*"' server.log | sort -u

# Count requests by method
grep "Received" server.log | grep -o 'method="[^"]*"' | sort | uniq -c
```

## Grafana Dashboards

Create comprehensive dashboards for monitoring KeystoneDB in production.

### Prometheus Configuration

Configure Prometheus to scrape KeystoneDB metrics:

```yaml
# prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'keystonedb'
    static_configs:
      - targets: ['localhost:9090']
    scrape_interval: 15s
    scrape_timeout: 10s
```

Start Prometheus:

```bash
# Using Docker
docker run -d \
    --name prometheus \
    -p 9091:9090 \
    -v $(pwd)/prometheus.yml:/etc/prometheus/prometheus.yml \
    prom/prometheus

# Access UI
open http://localhost:9091
```

### Example Prometheus Queries

**Request rate by method:**

```promql
sum(rate(kstone_rpc_requests_total[5m])) by (method)
```

**Error rate:**

```promql
sum(rate(kstone_rpc_requests_total{status="error"}[5m])) by (method)
```

**Success rate (percentage):**

```promql
sum(rate(kstone_rpc_requests_total{status="success"}[5m]))
/
sum(rate(kstone_rpc_requests_total[5m]))
* 100
```

**P50 latency:**

```promql
histogram_quantile(0.50, rate(kstone_rpc_duration_seconds_bucket[5m]))
```

**P95 latency:**

```promql
histogram_quantile(0.95, rate(kstone_rpc_duration_seconds_bucket[5m]))
```

**P99 latency:**

```promql
histogram_quantile(0.99, rate(kstone_rpc_duration_seconds_bucket[5m]))
```

**Active connections:**

```promql
kstone_active_connections
```

### Grafana Dashboard Configuration

Install Grafana:

```bash
# Using Docker
docker run -d \
    --name grafana \
    -p 3000:3000 \
    grafana/grafana

# Access UI (default credentials: admin/admin)
open http://localhost:3000
```

**Add Prometheus data source:**

1. Navigate to Configuration → Data Sources
2. Add Prometheus data source
3. URL: `http://prometheus:9090` (or `http://localhost:9091`)
4. Save & Test

**Create dashboard panels:**

**Panel 1: Request Rate**
- Visualization: Graph
- Query: `sum(rate(kstone_rpc_requests_total[5m])) by (method)`
- Legend: `{{method}}`
- Y-axis: Requests/sec

**Panel 2: Latency (P50, P95, P99)**
- Visualization: Graph
- Queries:
  - P50: `histogram_quantile(0.50, rate(kstone_rpc_duration_seconds_bucket[5m]))`
  - P95: `histogram_quantile(0.95, rate(kstone_rpc_duration_seconds_bucket[5m]))`
  - P99: `histogram_quantile(0.99, rate(kstone_rpc_duration_seconds_bucket[5m]))`
- Y-axis: Seconds

**Panel 3: Error Rate**
- Visualization: Graph
- Query: `sum(rate(kstone_rpc_requests_total{status="error"}[5m])) by (method)`
- Y-axis: Errors/sec
- Alert threshold: > 0.05 (5% error rate)

**Panel 4: Active Connections**
- Visualization: Gauge
- Query: `kstone_active_connections`
- Thresholds: Green (< 500), Yellow (500-800), Red (> 800)

**Panel 5: Success Rate**
- Visualization: Stat
- Query: `sum(rate(kstone_rpc_requests_total{status="success"}[5m])) / sum(rate(kstone_rpc_requests_total[5m])) * 100`
- Unit: Percent (0-100)
- Thresholds: Red (< 95%), Yellow (95-99%), Green (> 99%)

### Setting Up Alerts

Configure alerts in Prometheus for critical conditions:

```yaml
# prometheus-alerts.yml
groups:
  - name: keystonedb
    interval: 30s
    rules:
      - alert: HighErrorRate
        expr: rate(kstone_rpc_requests_total{status="error"}[5m]) > 0.05
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High error rate detected"
          description: "Error rate is {{ $value }} errors/sec"

      - alert: HighLatency
        expr: histogram_quantile(0.95, rate(kstone_rpc_duration_seconds_bucket[5m])) > 1.0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High latency detected"
          description: "P95 latency is {{ $value }}s"

      - alert: DatabaseUnhealthy
        expr: up{job="keystonedb"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "KeystoneDB is down"
          description: "Database instance is unreachable"
```

## Kubernetes Integration

Deploy monitoring alongside KeystoneDB in Kubernetes:

```yaml
# monitoring-stack.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: prometheus-config
data:
  prometheus.yml: |
    global:
      scrape_interval: 15s
    scrape_configs:
      - job_name: 'keystonedb'
        kubernetes_sd_configs:
          - role: pod
        relabel_configs:
          - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_scrape]
            action: keep
            regex: true
          - source_labels: [__meta_kubernetes_pod_annotation_prometheus_io_port]
            action: replace
            target_label: __address__
            regex: ([^:]+)(?::\d+)?;(\d+)
            replacement: $1:$2

---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: prometheus
spec:
  replicas: 1
  selector:
    matchLabels:
      app: prometheus
  template:
    metadata:
      labels:
        app: prometheus
    spec:
      containers:
      - name: prometheus
        image: prom/prometheus:latest
        ports:
        - containerPort: 9090
        volumeMounts:
        - name: config
          mountPath: /etc/prometheus
      volumes:
      - name: config
        configMap:
          name: prometheus-config
```

Annotate KeystoneDB pods for auto-discovery:

```yaml
metadata:
  annotations:
    prometheus.io/scrape: "true"
    prometheus.io/port: "9090"
    prometheus.io/path: "/metrics"
```

This comprehensive monitoring setup ensures you have complete visibility into KeystoneDB's performance, health, and operational status.

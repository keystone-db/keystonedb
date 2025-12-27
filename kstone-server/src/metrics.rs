/// Prometheus metrics for KeystoneDB server
///
/// This module defines and manages all metrics exposed by the server.
/// Metrics are collected automatically by instrumented RPC handlers and
/// exposed at the /metrics endpoint in Prometheus format.

use once_cell::sync::Lazy;
use prometheus::{
    opts, histogram_opts, HistogramVec, IntCounterVec, IntGauge, Registry, TextEncoder, Encoder,
};

/// Global Prometheus registry
pub static REGISTRY: Lazy<Registry> = Lazy::new(Registry::new);

/// Total number of RPC requests by method and status
///
/// Labels:
/// - method: RPC method name (put, get, delete, query, etc.)
/// - status: success or error
pub static RPC_REQUESTS_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    IntCounterVec::new(
        opts!(
            "kstone_rpc_requests_total",
            "Total number of RPC requests"
        ),
        &["method", "status"],
    )
    .unwrap_or_else(|_| {
        // Fallback to a metric with a different name if creation fails
        IntCounterVec::new(
            opts!("kstone_rpc_requests_total_fallback", "Total RPC requests"),
            &["method", "status"],
        )
        .expect("Failed to create fallback RPC_REQUESTS_TOTAL metric")
    })
});

/// RPC request duration in seconds
///
/// Labels:
/// - method: RPC method name
///
/// Buckets: 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0 seconds
pub static RPC_DURATION_SECONDS: Lazy<HistogramVec> = Lazy::new(|| {
    HistogramVec::new(
        histogram_opts!(
            "kstone_rpc_duration_seconds",
            "RPC request duration in seconds",
            vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]
        ),
        &["method"],
    )
    .unwrap_or_else(|_| {
        HistogramVec::new(
            histogram_opts!(
                "kstone_rpc_duration_seconds_fallback",
                "RPC request duration"
            ),
            &["method"],
        )
        .expect("Failed to create fallback RPC_DURATION_SECONDS metric")
    })
});

/// Number of active gRPC connections
pub static ACTIVE_CONNECTIONS: Lazy<IntGauge> = Lazy::new(|| {
    IntGauge::new(
        "kstone_active_connections",
        "Number of active gRPC connections",
    )
    .unwrap_or_else(|_| {
        IntGauge::new(
            "kstone_active_connections_fallback",
            "Active connections",
        )
        .expect("Failed to create fallback ACTIVE_CONNECTIONS metric")
    })
});

/// Total number of database operations by operation type and status
///
/// Labels:
/// - operation: put, get, delete, query, scan, update, etc.
/// - status: success or error
pub static DB_OPERATIONS_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    IntCounterVec::new(
        opts!(
            "kstone_db_operations_total",
            "Total number of database operations"
        ),
        &["operation", "status"],
    )
    .unwrap_or_else(|_| {
        IntCounterVec::new(
            opts!("kstone_db_operations_total_fallback", "Database operations"),
            &["operation", "status"],
        )
        .expect("Failed to create fallback DB_OPERATIONS_TOTAL metric")
    })
});

/// Total number of errors by error type
///
/// Labels:
/// - error_type: not_found, invalid_argument, condition_failed, etc.
pub static ERRORS_TOTAL: Lazy<IntCounterVec> = Lazy::new(|| {
    IntCounterVec::new(
        opts!(
            "kstone_errors_total",
            "Total number of errors by type"
        ),
        &["error_type"],
    )
    .unwrap_or_else(|_| {
        IntCounterVec::new(
            opts!("kstone_errors_total_fallback", "Total errors"),
            &["error_type"],
        )
        .expect("Failed to create fallback ERRORS_TOTAL metric")
    })
});

/// Total number of rate-limited requests
///
/// Labels:
/// - limit_type: per_connection or global
pub static RATE_LIMITED_REQUESTS: Lazy<IntCounterVec> = Lazy::new(|| {
    IntCounterVec::new(
        opts!(
            "kstone_rate_limited_requests_total",
            "Total number of rate-limited requests"
        ),
        &["limit_type"],
    )
    .unwrap_or_else(|_| {
        IntCounterVec::new(
            opts!("kstone_rate_limited_requests_total_fallback", "Rate limited requests"),
            &["limit_type"],
        )
        .expect("Failed to create fallback RATE_LIMITED_REQUESTS metric")
    })
});

/// Register all metrics with the global registry
///
/// Returns an error if any metric fails to register (e.g., duplicate registration).
/// This should only be called once during server startup.
pub fn register_metrics() -> Result<(), Box<dyn std::error::Error>> {
    REGISTRY
        .register(Box::new(RPC_REQUESTS_TOTAL.clone()))
        .map_err(|e| format!("Failed to register RPC_REQUESTS_TOTAL metric: {}", e))?;

    REGISTRY
        .register(Box::new(RPC_DURATION_SECONDS.clone()))
        .map_err(|e| format!("Failed to register RPC_DURATION_SECONDS metric: {}", e))?;

    REGISTRY
        .register(Box::new(ACTIVE_CONNECTIONS.clone()))
        .map_err(|e| format!("Failed to register ACTIVE_CONNECTIONS metric: {}", e))?;

    REGISTRY
        .register(Box::new(DB_OPERATIONS_TOTAL.clone()))
        .map_err(|e| format!("Failed to register DB_OPERATIONS_TOTAL metric: {}", e))?;

    REGISTRY
        .register(Box::new(ERRORS_TOTAL.clone()))
        .map_err(|e| format!("Failed to register ERRORS_TOTAL metric: {}", e))?;

    REGISTRY
        .register(Box::new(RATE_LIMITED_REQUESTS.clone()))
        .map_err(|e| format!("Failed to register RATE_LIMITED_REQUESTS metric: {}", e))?;

    Ok(())
}

/// Encode metrics in Prometheus text format
pub fn encode_metrics() -> Result<String, Box<dyn std::error::Error>> {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

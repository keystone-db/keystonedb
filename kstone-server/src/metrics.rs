/// Prometheus metrics for KeystoneDB server
///
/// This module defines and manages all metrics exposed by the server.
/// Metrics are collected automatically by instrumented RPC handlers and
/// exposed at the /metrics endpoint in Prometheus format.

use lazy_static::lazy_static;
use prometheus::{
    opts, histogram_opts, register_histogram_vec, register_int_counter_vec, register_int_gauge,
    HistogramVec, IntCounterVec, IntGauge, Registry, TextEncoder, Encoder,
};

lazy_static! {
    /// Global Prometheus registry
    pub static ref REGISTRY: Registry = Registry::new();

    /// Total number of RPC requests by method and status
    ///
    /// Labels:
    /// - method: RPC method name (put, get, delete, query, etc.)
    /// - status: success or error
    pub static ref RPC_REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        opts!(
            "kstone_rpc_requests_total",
            "Total number of RPC requests"
        ),
        &["method", "status"]
    )
    .expect("Failed to create RPC_REQUESTS_TOTAL metric - metric name may be invalid or already registered");

    /// RPC request duration in seconds
    ///
    /// Labels:
    /// - method: RPC method name
    ///
    /// Buckets: 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0 seconds
    pub static ref RPC_DURATION_SECONDS: HistogramVec = register_histogram_vec!(
        histogram_opts!(
            "kstone_rpc_duration_seconds",
            "RPC request duration in seconds",
            vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0]
        ),
        &["method"]
    )
    .expect("Failed to create RPC_DURATION_SECONDS metric - metric name may be invalid or already registered");

    /// Number of active gRPC connections
    pub static ref ACTIVE_CONNECTIONS: IntGauge = register_int_gauge!(
        opts!(
            "kstone_active_connections",
            "Number of active gRPC connections"
        )
    )
    .expect("Failed to create ACTIVE_CONNECTIONS metric - metric name may be invalid or already registered");

    /// Total number of database operations by operation type and status
    ///
    /// Labels:
    /// - operation: put, get, delete, query, scan, update, etc.
    /// - status: success or error
    pub static ref DB_OPERATIONS_TOTAL: IntCounterVec = register_int_counter_vec!(
        opts!(
            "kstone_db_operations_total",
            "Total number of database operations"
        ),
        &["operation", "status"]
    )
    .expect("Failed to create DB_OPERATIONS_TOTAL metric - metric name may be invalid or already registered");

    /// Total number of errors by error type
    ///
    /// Labels:
    /// - error_type: not_found, invalid_argument, condition_failed, etc.
    pub static ref ERRORS_TOTAL: IntCounterVec = register_int_counter_vec!(
        opts!(
            "kstone_errors_total",
            "Total number of errors by type"
        ),
        &["error_type"]
    )
    .expect("Failed to create ERRORS_TOTAL metric - metric name may be invalid or already registered");

    /// Total number of rate-limited requests
    ///
    /// Labels:
    /// - limit_type: per_connection or global
    pub static ref RATE_LIMITED_REQUESTS: IntCounterVec = register_int_counter_vec!(
        opts!(
            "kstone_rate_limited_requests_total",
            "Total number of rate-limited requests"
        ),
        &["limit_type"]
    )
    .expect("Failed to create RATE_LIMITED_REQUESTS metric - metric name may be invalid or already registered");
}

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

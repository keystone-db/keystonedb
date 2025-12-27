/// KeystoneDB gRPC Server Binary
///
/// Starts a gRPC server that exposes the KeystoneDB API over the network.

use axum::{routing::get, Router};
use clap::Parser;
use kstone_api::Database;
use kstone_server::{ConnectionManager, KeystoneDbServer, KeystoneService, RateLimiter, metrics};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tonic::transport::Server;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "kstone-server")]
#[command(about = "KeystoneDB gRPC Server", long_about = None)]
struct Args {
    /// Path to the database directory
    #[arg(short, long, value_name = "PATH")]
    db_path: PathBuf,

    /// Port to listen on
    #[arg(short, long, default_value = "50051")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Maximum number of concurrent connections (0 = unlimited)
    #[arg(long, default_value = "1000")]
    max_connections: usize,

    /// Connection timeout in seconds
    #[arg(long, default_value = "60")]
    connection_timeout: u64,

    /// Graceful shutdown timeout in seconds
    #[arg(long, default_value = "30")]
    shutdown_timeout: u64,

    /// Max requests per second per connection (0 = unlimited)
    #[arg(long, default_value = "0")]
    max_rps_per_connection: u32,

    /// Max total requests per second (0 = unlimited)
    #[arg(long, default_value = "0")]
    max_rps_global: u32,

    /// Path to TLS certificate file (enables TLS)
    #[arg(long, value_name = "FILE")]
    tls_cert: Option<PathBuf>,

    /// Path to TLS private key file (required if --tls-cert is set)
    #[arg(long, value_name = "FILE")]
    tls_key: Option<PathBuf>,

    /// Path to TLS CA certificate file (enables mTLS client verification)
    #[arg(long, value_name = "FILE")]
    tls_ca: Option<PathBuf>,
}

async fn metrics_handler() -> String {
    metrics::encode_metrics().unwrap_or_else(|e| {
        tracing::error!("Failed to encode metrics: {}", e);
        String::from("# Error encoding metrics\n")
    })
}

async fn health_handler() -> &'static str {
    "OK"
}

async fn ready_handler() -> &'static str {
    "OK"
}

/// Configure TLS settings if certificates are provided
fn configure_tls(
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
    tls_ca: Option<PathBuf>,
) -> Result<Option<tonic::transport::ServerTlsConfig>, Box<dyn std::error::Error>> {
    // If no TLS cert provided, return None (no TLS)
    let cert_path = match tls_cert {
        Some(path) => path,
        None => {
            if tls_key.is_some() || tls_ca.is_some() {
                return Err("--tls-key and --tls-ca require --tls-cert to be set".into());
            }
            return Ok(None);
        }
    };

    // TLS cert provided, so TLS key is required
    let key_path = tls_key.ok_or("--tls-cert requires --tls-key to be set")?;

    // Validate that cert and key files exist
    if !cert_path.exists() {
        return Err(format!("TLS certificate file not found: {:?}", cert_path).into());
    }
    if !key_path.exists() {
        return Err(format!("TLS key file not found: {:?}", key_path).into());
    }

    // Read certificate and key files
    let cert = std::fs::read_to_string(&cert_path)
        .map_err(|e| format!("Failed to read TLS certificate from {:?}: {}", cert_path, e))?;
    let key = std::fs::read_to_string(&key_path)
        .map_err(|e| format!("Failed to read TLS key from {:?}: {}", key_path, e))?;

    // Create TLS identity from cert and key
    let identity = tonic::transport::Identity::from_pem(cert, key);

    // Build TLS config
    let mut tls_config = tonic::transport::ServerTlsConfig::new().identity(identity);

    // If CA certificate is provided, enable mTLS (client certificate verification)
    if let Some(ca_path) = tls_ca {
        if !ca_path.exists() {
            return Err(format!("TLS CA certificate file not found: {:?}", ca_path).into());
        }

        let ca_cert = std::fs::read_to_string(&ca_path)
            .map_err(|e| format!("Failed to read TLS CA certificate from {:?}: {}", ca_path, e))?;

        let ca = tonic::transport::Certificate::from_pem(ca_cert);
        tls_config = tls_config.client_ca_root(ca);

        info!("mTLS enabled: client certificate verification required");
    } else {
        info!("TLS enabled: server certificate only (no client verification)");
    }

    Ok(Some(tls_config))
}

/// Wait for shutdown signal (SIGTERM, SIGINT, or Ctrl+C)
///
/// This function will gracefully handle signal registration failures by logging
/// errors instead of panicking. If signal handlers fail to register, the server
/// will continue running but won't respond to signals (requiring manual termination).
async fn shutdown_signal() {
    let ctrl_c = async {
        match signal::ctrl_c().await {
            Ok(()) => (),
            Err(err) => {
                tracing::error!(
                    "Failed to register Ctrl+C signal handler: {}. Server will not respond to Ctrl+C.",
                    err
                );
                // Wait forever since we can't handle the signal
                std::future::pending::<()>().await
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => {
                tracing::error!(
                    "Failed to register SIGTERM signal handler: {}. Server will not respond to SIGTERM.",
                    err
                );
                // Wait forever since we can't handle the signal
                std::future::pending::<()>().await
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal");
        }
        _ = terminate => {
            info!("Received SIGTERM signal");
        }
    }

    warn!("Shutting down gracefully...");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with environment filter
    // Default to info level, can override with RUST_LOG env var
    // Example: RUST_LOG=debug cargo run --bin kstone-server
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .init();

    // Initialize Prometheus metrics
    metrics::register_metrics()
        .map_err(|e| format!("Failed to initialize Prometheus metrics: {}", e))?;
    info!("Initialized Prometheus metrics");

    // Parse command line arguments
    let args = Args::parse();

    // Create connection manager
    // Note: Full integration with tonic would require custom middleware layers
    // For now, we demonstrate the infrastructure and use TCP-level settings
    let _conn_manager = ConnectionManager::new(
        args.max_connections,
        Duration::from_secs(args.connection_timeout),
    );
    info!(
        "Connection manager initialized: max_connections={}, timeout={}s",
        if args.max_connections == 0 { "unlimited".to_string() } else { args.max_connections.to_string() },
        args.connection_timeout
    );

    // Create rate limiter
    let rate_limiter = Arc::new(RateLimiter::new(args.max_rps_per_connection, args.max_rps_global));
    if rate_limiter.is_enabled() {
        info!(
            "Rate limiting enabled: per_connection={} rps, global={} rps",
            if args.max_rps_per_connection == 0 { "unlimited".to_string() } else { args.max_rps_per_connection.to_string() },
            if args.max_rps_global == 0 { "unlimited".to_string() } else { args.max_rps_global.to_string() }
        );
    } else {
        info!("Rate limiting disabled");
    }

    // Open or create database
    info!("Opening database at {:?}", args.db_path);
    let db = if args.db_path.exists() {
        Database::open(&args.db_path)?
    } else {
        info!("Database not found, creating new database");
        Database::create(&args.db_path)?
    };

    // Configure TLS if certificates are provided
    let tls_config = configure_tls(args.tls_cert, args.tls_key, args.tls_ca)?;

    // Create gRPC service
    let service = KeystoneService::new(db, Arc::clone(&rate_limiter));
    let grpc_addr = format!("{}:{}", args.host, args.port).parse()?;

    if tls_config.is_some() {
        info!("Starting KeystoneDB gRPC server on {} with TLS", grpc_addr);
    } else {
        info!("Starting KeystoneDB gRPC server on {} (no TLS)", grpc_addr);
    }

    // Create HTTP server for metrics and health checks
    let metrics_app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_handler))
        .route("/ready", get(ready_handler));
    let metrics_addr = format!("{}:9090", args.host);

    info!("Starting HTTP server on {} with /metrics, /health, /ready endpoints", metrics_addr);

    // Spawn metrics server as background task
    let metrics_listener = tokio::net::TcpListener::bind(&metrics_addr).await?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(metrics_listener, metrics_app).await {
            tracing::error!("Metrics server error: {}", e);
        }
    });

    // Configure server with connection settings and optional TLS
    let mut server_builder = Server::builder()
        .timeout(Duration::from_secs(args.connection_timeout))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .tcp_nodelay(true);

    // Apply TLS configuration if provided
    if let Some(tls) = tls_config {
        server_builder = server_builder.tls_config(tls)?;
    }

    let server = server_builder.add_service(KeystoneDbServer::new(service));

    // Start gRPC server with graceful shutdown
    info!(
        "Server configured: timeout={}s, tcp_keepalive=30s, tcp_nodelay=true, shutdown_timeout={}s",
        args.connection_timeout, args.shutdown_timeout
    );

    info!("Server ready - listening for connections");

    // Serve with graceful shutdown on signal
    server
        .serve_with_shutdown(grpc_addr, shutdown_signal())
        .await?;

    info!("gRPC server shutdown complete");

    // Give shutdown timeout for connection draining
    info!("Waiting up to {}s for connections to drain...", args.shutdown_timeout);
    tokio::time::sleep(Duration::from_secs(args.shutdown_timeout)).await;

    info!("Shutdown complete");
    Ok(())
}

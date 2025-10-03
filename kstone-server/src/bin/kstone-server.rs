/// KeystoneDB gRPC Server Binary
///
/// Starts a gRPC server that exposes the KeystoneDB API over the network.

use axum::{routing::get, Router};
use clap::Parser;
use kstone_api::Database;
use kstone_server::{ConnectionManager, KeystoneDbServer, KeystoneService, metrics};
use std::path::PathBuf;
use std::time::Duration;
use tonic::transport::Server;
use tracing::info;
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
    metrics::register_metrics();
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

    // Open or create database
    info!("Opening database at {:?}", args.db_path);
    let db = if args.db_path.exists() {
        Database::open(&args.db_path)?
    } else {
        info!("Database not found, creating new database");
        Database::create(&args.db_path)?
    };

    // Create gRPC service
    let service = KeystoneService::new(db);
    let grpc_addr = format!("{}:{}", args.host, args.port).parse()?;

    info!("Starting KeystoneDB gRPC server on {}", grpc_addr);

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

    // Configure server with connection settings
    let server = Server::builder()
        .timeout(Duration::from_secs(args.connection_timeout))
        .tcp_keepalive(Some(Duration::from_secs(30)))
        .tcp_nodelay(true)
        .add_service(KeystoneDbServer::new(service));

    // Start gRPC server (blocks until shutdown)
    info!(
        "Server configured: timeout={}s, tcp_keepalive=30s, tcp_nodelay=true",
        args.connection_timeout
    );

    server.serve(grpc_addr).await?;

    Ok(())
}

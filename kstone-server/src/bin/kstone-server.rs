/// KeystoneDB gRPC Server Binary
///
/// Starts a gRPC server that exposes the KeystoneDB API over the network.

use axum::{routing::get, Router};
use clap::Parser;
use kstone_api::Database;
use kstone_server::{KeystoneDbServer, KeystoneService, metrics};
use std::path::PathBuf;
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

    // Start gRPC server (blocks until shutdown)
    Server::builder()
        .add_service(KeystoneDbServer::new(service))
        .serve(grpc_addr)
        .await?;

    Ok(())
}

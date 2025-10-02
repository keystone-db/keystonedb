/// KeystoneDB gRPC Server Binary
///
/// Starts a gRPC server that exposes the KeystoneDB API over the network.

use clap::Parser;
use kstone_api::Database;
use kstone_server::{KeystoneDbServer, KeystoneService};
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
    let addr = format!("{}:{}", args.host, args.port).parse()?;

    info!("Starting KeystoneDB server on {}", addr);

    // Start server
    Server::builder()
        .add_service(KeystoneDbServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}

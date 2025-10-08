/// KeystoneDB Notebook Interface
///
/// An embedded notebook interface for interactive database exploration,
/// similar to Jupyter but built directly into the KeystoneDB CLI.

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use kstone_api::Database;

mod server;
mod handlers;
mod websocket;
mod storage;
mod assets;

pub use server::NotebookServer;
pub use storage::{Notebook, Cell, CellType};

/// Configuration for the notebook server
#[derive(Debug, Clone)]
pub struct NotebookConfig {
    pub host: String,
    pub port: u16,
    pub read_only: bool,
    pub auto_open_browser: bool,
}

impl Default for NotebookConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            read_only: false,
            auto_open_browser: true,
        }
    }
}

/// Launch the notebook interface for a database
pub async fn launch_notebook(
    db_path: &Path,
    config: NotebookConfig,
) -> Result<()> {
    // Open or create the database
    let db = if db_path.to_str() == Some(":memory:") {
        tracing::info!("Creating in-memory database for notebook");
        Database::create_in_memory()?
    } else if db_path.exists() {
        tracing::info!("Opening existing database: {}", db_path.display());
        Database::open(db_path)?
    } else {
        tracing::info!("Creating new database: {}", db_path.display());
        Database::create(db_path)?
    };

    let db = Arc::new(db);

    // Initialize notebook storage in the database
    storage::init_notebook_storage(&db)?;

    // Create and start the server
    let server = NotebookServer::new(db, config.clone());

    let url = format!("http://{}:{}", config.host, config.port);

    println!("🚀 Starting KeystoneDB Notebook");
    println!("📂 Database: {}", db_path.display());
    println!("🌐 Interface: {}", url);

    if config.auto_open_browser {
        // Try to open browser
        if let Err(e) = open::that(&url) {
            tracing::warn!("Failed to open browser: {}", e);
            println!("⚠️  Please open your browser and navigate to: {}", url);
        } else {
            println!("✅ Opening browser...");
        }
    }

    println!("\nPress Ctrl+C to stop the notebook server\n");

    // Start the server
    server.serve().await?;

    Ok(())
}

/// Open browser helper
mod open {
    use std::process::Command;

    pub fn that(url: &str) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(target_os = "macos")]
        {
            Command::new("open").arg(url).spawn()?;
        }

        #[cfg(target_os = "windows")]
        {
            Command::new("cmd").args(["/C", "start", url]).spawn()?;
        }

        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open").arg(url).spawn()?;
        }

        Ok(())
    }
}
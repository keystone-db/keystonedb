use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use kstone_api::{Database, KeystoneValue, ExecuteStatementResponse};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

mod shell;
mod table;
mod notebook;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OutputFormat {
    /// Table format (default)
    Table,
    /// Pretty JSON
    Json,
    /// JSON Lines (one item per line)
    Jsonl,
    /// CSV format
    Csv,
}

#[derive(Parser)]
#[command(name = "kstone")]
#[command(about = "KeystoneDB CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new database
    Create {
        /// Database file path
        path: PathBuf,
    },
    /// Put an item
    Put {
        /// Database file path
        path: PathBuf,
        /// Partition key
        key: String,
        /// Item as JSON
        item: String,
    },
    /// Get an item
    Get {
        /// Database file path
        path: PathBuf,
        /// Partition key
        key: String,
    },
    /// Delete an item
    Delete {
        /// Database file path
        path: PathBuf,
        /// Partition key
        key: String,
    },
    /// Execute a PartiQL query
    Query {
        /// Database file path
        path: PathBuf,
        /// PartiQL SQL statement
        sql: String,
        /// Maximum number of items to return
        #[arg(short, long)]
        limit: Option<usize>,
        /// Output format (table, json, jsonl, csv)
        #[arg(short, long, value_enum, default_value = "table")]
        output: OutputFormat,
    },
    /// Start interactive shell
    Shell {
        /// Database file path (optional, defaults to :memory:)
        path: Option<PathBuf>,
    },
    /// Launch notebook interface
    Notebook {
        /// Database file path
        path: PathBuf,
        /// Port to serve on
        #[arg(short, long, default_value = "8080")]
        port: u16,
        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Don't automatically open browser
        #[arg(long)]
        no_browser: bool,
    },
    /// Cloud sync operations
    Sync {
        #[command(subcommand)]
        command: SyncCommands,
    },
}

#[derive(Subcommand)]
enum SyncCommands {
    /// Initialize sync metadata
    Init {
        /// Database file path
        path: PathBuf,
    },
    /// Add a sync endpoint
    AddEndpoint {
        /// Database file path
        path: PathBuf,
        /// Endpoint type (dynamodb, http, keystone, filesystem)
        #[arg(short = 't', long)]
        endpoint_type: String,
        /// Endpoint URL or path
        url: String,
        /// AWS region (for DynamoDB)
        #[arg(long)]
        region: Option<String>,
        /// Table name (for DynamoDB)
        #[arg(long)]
        table: Option<String>,
    },
    /// Start a sync operation
    Start {
        /// Database file path
        path: PathBuf,
        /// Endpoint URL to sync with
        endpoint: String,
        /// Conflict resolution strategy (last-writer-wins, first-writer-wins, vector-clock, manual)
        #[arg(short = 's', long, default_value = "last-writer-wins")]
        strategy: String,
        /// Enable continuous sync
        #[arg(long)]
        continuous: bool,
        /// Sync interval in seconds (for continuous sync)
        #[arg(long, default_value = "30")]
        interval: u64,
    },
    /// Check sync status
    Status {
        /// Database file path
        path: PathBuf,
    },
    /// Show sync history
    History {
        /// Database file path
        path: PathBuf,
        /// Number of entries to show
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Create { path } => {
            Database::create(&path).context("Failed to create database")?;
            println!("Database created: {}", path.display());
        }

        Commands::Put { path, key, item } => {
            let db = Database::open(&path).context("Failed to open database")?;

            // Parse JSON item
            let json: serde_json::Value =
                serde_json::from_str(&item).context("Invalid JSON")?;

            let item = json_to_item(&json)?;

            db.put(key.as_bytes(), item)
                .context("Failed to put item")?;
            println!("Item stored");
        }

        Commands::Get { path, key } => {
            let db = Database::open(&path).context("Failed to open database")?;

            match db.get(key.as_bytes()).context("Failed to get item")? {
                Some(item) => {
                    let json = item_to_json(&item);
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                None => {
                    println!("Item not found");
                }
            }
        }

        Commands::Delete { path, key } => {
            let db = Database::open(&path).context("Failed to open database")?;
            db.delete(key.as_bytes())
                .context("Failed to delete item")?;
            println!("Item deleted");
        }

        Commands::Query { path, sql, limit, output } => {
            let db = Database::open(&path).context("Failed to open database")?;

            // Append LIMIT clause if provided
            let sql_with_limit = if let Some(limit_val) = limit {
                format!("{} LIMIT {}", sql, limit_val)
            } else {
                sql
            };

            match db.execute_statement(&sql_with_limit).context("Failed to execute statement")? {
                ExecuteStatementResponse::Select {
                    items,
                    count,
                    scanned_count,
                    last_key,
                } => {
                    // Format and print results based on output format
                    match output {
                        OutputFormat::Table => {
                            let table = table::format_items_table(&items);
                            println!("{}", table);
                            println!();
                            println!("Count: {}, Scanned: {}", count, scanned_count);

                            if last_key.is_some() {
                                println!("(More results available - use LIMIT/OFFSET for pagination)");
                            }
                        }
                        OutputFormat::Json => {
                            // Pretty JSON array
                            let json_items: Vec<_> = items.iter()
                                .map(item_to_json)
                                .collect();
                            println!("{}", serde_json::to_string_pretty(&json_items)?);
                        }
                        OutputFormat::Jsonl => {
                            // JSON Lines - one item per line
                            for item in &items {
                                let json = item_to_json(item);
                                println!("{}", serde_json::to_string(&json)?);
                            }
                        }
                        OutputFormat::Csv => {
                            // CSV format
                            format_csv(&items)?;
                        }
                    }
                }
                ExecuteStatementResponse::Insert { success } => {
                    if success {
                        println!("✓ Item inserted successfully");
                    } else {
                        println!("✗ Insert failed");
                    }
                }
                ExecuteStatementResponse::Update { item } => {
                    println!("✓ Item updated successfully");
                    println!();
                    let json = item_to_json(&item);
                    println!("{}", serde_json::to_string_pretty(&json)?);
                }
                ExecuteStatementResponse::Delete { success } => {
                    if success {
                        println!("✓ Item deleted successfully");
                    } else {
                        println!("✗ Delete failed");
                    }
                }
            }
        }

        Commands::Shell { path } => {
            let mut shell = shell::Shell::new(path.as_deref())?;
            shell.run()?;
        }

        Commands::Notebook { path, port, host, no_browser } => {
            let config = notebook::NotebookConfig {
                host,
                port,
                read_only: false,
                auto_open_browser: !no_browser,
            };

            // Use tokio runtime for the notebook server
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(notebook::launch_notebook(&path, config))?;
        }

        Commands::Sync { command } => {
            handle_sync_command(command)?;
        }
    }

    Ok(())
}

fn handle_sync_command(command: SyncCommands) -> Result<()> {
    use kstone_sync::{
        CloudSyncBuilder, SyncEndpoint, ConflictStrategy,
        SyncMetadataStore, EndpointInfo,
    };
    use std::time::Duration;

    match command {
        SyncCommands::Init { path } => {
            let db = Database::open(&path).context("Failed to open database")?;

            // Initialize sync metadata
            let metadata_store = SyncMetadataStore::new(Arc::new(db));
            metadata_store.initialize().context("Failed to initialize sync metadata")?;

            println!("✓ Sync metadata initialized for database: {}", path.display());
        }

        SyncCommands::AddEndpoint {
            path,
            endpoint_type,
            url,
            region,
            table,
        } => {
            let db = Database::open(&path).context("Failed to open database")?;
            let metadata_store = SyncMetadataStore::new(Arc::new(db));

            // Create endpoint based on type
            let endpoint = match endpoint_type.as_str() {
                "dynamodb" => {
                    let region = region.ok_or_else(|| anyhow::anyhow!("Region required for DynamoDB endpoint"))?;
                    let table = table.ok_or_else(|| anyhow::anyhow!("Table name required for DynamoDB endpoint"))?;
                    SyncEndpoint::DynamoDB {
                        region,
                        table_name: table,
                        credentials: None, // Will use default AWS credentials
                    }
                }
                "http" => SyncEndpoint::Http {
                    url: url.clone(),
                    auth: None,
                },
                "keystone" => SyncEndpoint::Keystone {
                    url: url.clone(),
                    auth_token: None,
                },
                "filesystem" => SyncEndpoint::FileSystem {
                    path: url.clone(),
                },
                _ => return Err(anyhow::anyhow!("Unknown endpoint type: {}", endpoint_type)),
            };

            // Load existing metadata
            let mut metadata = metadata_store
                .load_metadata()?
                .ok_or_else(|| anyhow::anyhow!("Sync metadata not initialized. Run 'sync init' first."))?;

            // Add endpoint
            let endpoint_info = EndpointInfo {
                id: endpoint.endpoint_id(),
                url: url.clone(),
                endpoint_type: endpoint.endpoint_type().to_string(),
                active: true,
                last_clock: None,
            };

            metadata.add_endpoint(endpoint_info.clone());
            metadata_store.save_metadata(&metadata)?;
            metadata_store.save_endpoint(&endpoint_info)?;

            println!("✓ Added {} endpoint: {}", endpoint_type, url);
        }

        SyncCommands::Start {
            path,
            endpoint,
            strategy,
            continuous,
            interval,
        } => {
            let _db = Database::open(&path).context("Failed to open database")?;

            // Parse conflict strategy
            let conflict_strategy = match strategy.as_str() {
                "last-writer-wins" => ConflictStrategy::LastWriterWins,
                "first-writer-wins" => ConflictStrategy::FirstWriterWins,
                "vector-clock" => ConflictStrategy::VectorClock,
                "manual" => ConflictStrategy::Manual,
                _ => return Err(anyhow::anyhow!("Unknown conflict strategy: {}", strategy)),
            };

            // Parse endpoint URL to determine type
            let sync_endpoint = if endpoint.starts_with("dynamodb://") {
                // Parse: dynamodb://region/table
                let parts: Vec<&str> = endpoint.trim_start_matches("dynamodb://").split('/').collect();
                if parts.len() != 2 {
                    return Err(anyhow::anyhow!("Invalid DynamoDB URL format. Use: dynamodb://region/table"));
                }
                SyncEndpoint::DynamoDB {
                    region: parts[0].to_string(),
                    table_name: parts[1].to_string(),
                    credentials: None,
                }
            } else if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                SyncEndpoint::Http {
                    url: endpoint.clone(),
                    auth: None,
                }
            } else if endpoint.starts_with("keystone://") {
                SyncEndpoint::Keystone {
                    url: endpoint.trim_start_matches("keystone://").to_string(),
                    auth_token: None,
                }
            } else {
                // Assume filesystem path
                SyncEndpoint::FileSystem {
                    path: endpoint.clone(),
                }
            };

            // Build sync engine
            let sync_interval = if continuous {
                Some(Duration::from_secs(interval))
            } else {
                None
            };

            let mut sync_engine = CloudSyncBuilder::new()
                .with_endpoint(sync_endpoint)
                .with_conflict_strategy(conflict_strategy)
                .with_sync_interval(sync_interval.unwrap_or(Duration::from_secs(30)))
                .build()
                .context("Failed to create sync engine")?;

            if continuous {
                println!("✓ Starting continuous sync with {} (interval: {}s)", endpoint, interval);
                println!("Press Ctrl+C to stop...");

                // Use tokio runtime for async sync
                let runtime = tokio::runtime::Runtime::new()?;
                runtime.block_on(async {
                    sync_engine.start().await?;
                    // Keep running until interrupted
                    tokio::signal::ctrl_c().await?;
                    sync_engine.stop().await?;
                    Ok::<(), anyhow::Error>(())
                })?;
            } else {
                println!("✓ Starting one-time sync with {}", endpoint);

                // Use tokio runtime for async sync
                let runtime = tokio::runtime::Runtime::new()?;
                runtime.block_on(async {
                    sync_engine.sync_once().await
                })?;

                println!("✓ Sync completed successfully");
            }
        }

        SyncCommands::Status { path } => {
            let db = Database::open(&path).context("Failed to open database")?;
            let metadata_store = SyncMetadataStore::new(Arc::new(db));

            // Load metadata
            let metadata = metadata_store
                .load_metadata()?
                .ok_or_else(|| anyhow::anyhow!("Sync metadata not initialized"))?;

            println!("Sync Status for: {}", path.display());
            println!("─────────────────────────────────────────");
            println!("Local Endpoint: {}", metadata.local_endpoint.0);
            println!("Remote Endpoints: {}", metadata.remote_endpoints.len());

            for endpoint in &metadata.remote_endpoints {
                let status = if endpoint.active { "active" } else { "inactive" };
                println!("  - {} ({}) [{}]", endpoint.url, endpoint.endpoint_type, status);

                if let Some(last_sync) = metadata.get_last_sync_time(&endpoint.id) {
                    let duration = chrono::Utc::now().timestamp_millis() - last_sync;
                    let seconds = duration / 1000;
                    let minutes = seconds / 60;
                    let hours = minutes / 60;

                    let time_str = if hours > 0 {
                        format!("{}h ago", hours)
                    } else if minutes > 0 {
                        format!("{}m ago", minutes)
                    } else {
                        format!("{}s ago", seconds)
                    };

                    println!("    Last sync: {}", time_str);
                }
            }

            println!("\nSync Statistics:");
            println!("  Total syncs: {}", metadata.stats.total_syncs);
            println!("  Successful: {}", metadata.stats.successful_syncs);
            println!("  Failed: {}", metadata.stats.failed_syncs);
            println!("  Conflicts detected: {}", metadata.stats.conflicts_detected);
            println!("  Conflicts resolved: {}", metadata.stats.conflicts_resolved);
        }

        SyncCommands::History { path, limit } => {
            let db = Database::open(&path).context("Failed to open database")?;
            let metadata_store = SyncMetadataStore::new(Arc::new(db));

            // Load metadata
            let metadata = metadata_store
                .load_metadata()?
                .ok_or_else(|| anyhow::anyhow!("Sync metadata not initialized"))?;

            println!("Sync History for: {}", path.display());
            println!("─────────────────────────────────────────");

            // In a real implementation, we'd store sync history records
            // For now, just show basic info from metadata
            println!("Total syncs performed: {}", metadata.stats.total_syncs);
            println!("Successful syncs: {}", metadata.stats.successful_syncs);
            println!("Failed syncs: {}", metadata.stats.failed_syncs);

            if metadata.stats.total_syncs > 0 {
                let success_rate = (metadata.stats.successful_syncs as f64 / metadata.stats.total_syncs as f64) * 100.0;
                println!("Success rate: {:.1}%", success_rate);
            }

            println!("\nNote: Detailed sync history will be available in a future version.");
            println!("Showing last {} entries (when available)", limit);
        }
    }

    Ok(())
}

fn json_to_item(json: &serde_json::Value) -> Result<HashMap<String, KeystoneValue>> {
    let obj = json
        .as_object()
        .context("Item must be a JSON object")?;

    let mut item = HashMap::new();

    for (key, value) in obj {
        let ks_value = match value {
            serde_json::Value::String(s) => KeystoneValue::string(s.clone()),
            serde_json::Value::Number(n) => KeystoneValue::number(n),
            serde_json::Value::Bool(b) => KeystoneValue::Bool(*b),
            serde_json::Value::Null => KeystoneValue::Null,
            serde_json::Value::Array(arr) => {
                let mut list = Vec::new();
                for item in arr {
                    list.push(json_value_to_keystone(item)?);
                }
                KeystoneValue::L(list)
            }
            serde_json::Value::Object(_) => {
                let nested = json_to_item(value)?;
                KeystoneValue::M(nested)
            }
        };
        item.insert(key.clone(), ks_value);
    }

    Ok(item)
}

fn json_value_to_keystone(value: &serde_json::Value) -> Result<KeystoneValue> {
    Ok(match value {
        serde_json::Value::String(s) => KeystoneValue::string(s.clone()),
        serde_json::Value::Number(n) => KeystoneValue::number(n),
        serde_json::Value::Bool(b) => KeystoneValue::Bool(*b),
        serde_json::Value::Null => KeystoneValue::Null,
        serde_json::Value::Array(arr) => {
            let mut list = Vec::new();
            for item in arr {
                list.push(json_value_to_keystone(item)?);
            }
            KeystoneValue::L(list)
        }
        serde_json::Value::Object(_) => {
            let nested = json_to_item(value)?;
            KeystoneValue::M(nested)
        }
    })
}

fn item_to_json(item: &HashMap<String, KeystoneValue>) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    for (key, value) in item {
        obj.insert(key.clone(), keystone_value_to_json(value));
    }

    serde_json::Value::Object(obj)
}

fn keystone_value_to_json(value: &KeystoneValue) -> serde_json::Value {
    match value {
        KeystoneValue::S(s) => serde_json::Value::String(s.clone()),
        KeystoneValue::N(n) => {
            // Try to parse as number
            if let Ok(i) = n.parse::<i64>() {
                serde_json::Value::Number(i.into())
            } else if let Ok(f) = n.parse::<f64>() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or_else(|| serde_json::Value::String(n.clone()))
            } else {
                serde_json::Value::String(n.clone())
            }
        }
        KeystoneValue::Bool(b) => serde_json::Value::Bool(*b),
        KeystoneValue::Null => serde_json::Value::Null,
        KeystoneValue::L(list) => {
            let arr: Vec<_> = list.iter().map(keystone_value_to_json).collect();
            serde_json::Value::Array(arr)
        }
        KeystoneValue::M(map) => item_to_json(map),
        KeystoneValue::B(bytes) => {
            // Encode binary as base64
            serde_json::Value::String(base64_encode(bytes))
        }
        KeystoneValue::VecF32(vec) => {
            // Encode f32 vector as JSON array
            let arr: Vec<_> = vec.iter()
                .map(|&f| serde_json::Number::from_f64(f as f64)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null))
                .collect();
            serde_json::Value::Array(arr)
        }
        KeystoneValue::Ts(ts) => {
            // Encode timestamp as number
            serde_json::Value::Number((*ts).into())
        }
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    use std::io::Write;
    let mut buf = Vec::new();
    let mut encoder = base64::write::EncoderWriter::new(&mut buf, &base64::engine::general_purpose::STANDARD);
    encoder.write_all(bytes).unwrap();
    drop(encoder);
    String::from_utf8(buf).unwrap()
}

fn format_csv(items: &[HashMap<String, KeystoneValue>]) -> Result<()> {
    use std::collections::HashSet;

    if items.is_empty() {
        return Ok(());
    }

    // Collect all unique attribute names
    let mut all_keys = HashSet::new();
    for item in items {
        for key in item.keys() {
            all_keys.insert(key.clone());
        }
    }

    // Sort keys for consistent column order
    let mut sorted_keys: Vec<_> = all_keys.into_iter().collect();
    sorted_keys.sort();

    // Print header
    println!("{}", sorted_keys.join(","));

    // Print rows
    for item in items {
        let row: Vec<String> = sorted_keys.iter().map(|key| {
            item.get(key)
                .map(|value| format_csv_value(value))
                .unwrap_or_else(|| String::new())
        }).collect();
        println!("{}", row.join(","));
    }

    Ok(())
}

fn format_csv_value(value: &KeystoneValue) -> String {
    match value {
        KeystoneValue::S(s) => escape_csv(s),
        KeystoneValue::N(n) => n.clone(),
        KeystoneValue::Bool(b) => b.to_string(),
        KeystoneValue::Null => String::new(),
        KeystoneValue::L(list) => {
            let items: Vec<String> = list.iter()
                .map(format_csv_value)
                .collect();
            escape_csv(&format!("[{}]", items.join(",")))
        }
        KeystoneValue::M(_) => escape_csv("[object]"),
        KeystoneValue::B(_) => escape_csv("[binary]"),
        KeystoneValue::VecF32(_) => escape_csv("[vector]"),
        KeystoneValue::Ts(ts) => ts.to_string(),
    }
}

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

mod base64 {
    pub mod write {
        use std::io::{self, Write};

        pub struct EncoderWriter<'a, W: Write> {
            writer: &'a mut W,
            engine: &'a super::super::base64::engine::Engine,
        }

        impl<'a, W: Write> EncoderWriter<'a, W> {
            pub fn new(writer: &'a mut W, engine: &'a super::super::base64::engine::Engine) -> Self {
                Self { writer, engine }
            }
        }

        impl<W: Write> Write for EncoderWriter<'_, W> {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let encoded = (self.engine.encode)(buf);
                self.writer.write_all(encoded.as_bytes())?;
                Ok(buf.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                self.writer.flush()
            }
        }
    }

    pub mod engine {
        pub mod general_purpose {
            use super::Engine;

            pub const STANDARD: Engine = Engine {
                encode: |data| {
                    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
                    let mut result = String::new();
                    let mut i = 0;
                    while i + 2 < data.len() {
                        let b1 = data[i];
                        let b2 = data[i + 1];
                        let b3 = data[i + 2];
                        result.push(CHARS[(b1 >> 2) as usize] as char);
                        result.push(CHARS[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize] as char);
                        result.push(CHARS[(((b2 & 0x0f) << 2) | (b3 >> 6)) as usize] as char);
                        result.push(CHARS[(b3 & 0x3f) as usize] as char);
                        i += 3;
                    }
                    if i < data.len() {
                        let b1 = data[i];
                        result.push(CHARS[(b1 >> 2) as usize] as char);
                        if i + 1 < data.len() {
                            let b2 = data[i + 1];
                            result.push(CHARS[(((b1 & 0x03) << 4) | (b2 >> 4)) as usize] as char);
                            result.push(CHARS[((b2 & 0x0f) << 2) as usize] as char);
                            result.push('=');
                        } else {
                            result.push(CHARS[((b1 & 0x03) << 4) as usize] as char);
                            result.push('=');
                            result.push('=');
                        }
                    }
                    result
                },
            };
        }

        pub struct Engine {
            pub encode: fn(&[u8]) -> String,
        }
    }
}

/// Format query response as table
pub fn format_response_table(response: &ExecuteStatementResponse) -> Result<()> {
    use colored::Colorize;

    match response {
        ExecuteStatementResponse::Select { items, .. } => {
            if items.is_empty() {
                println!("{}", "No items found".yellow());
            } else {
                let table = table::format_items_table(items);
                println!("{}", table);
            }
        }
        ExecuteStatementResponse::Insert { .. } => {
            println!("{}", "✓ Item inserted successfully".green());
        }
        ExecuteStatementResponse::Update { item, .. } => {
            println!("{}", "✓ Item updated successfully".green());
            let json = item_to_json(item);
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        ExecuteStatementResponse::Delete { .. } => {
            println!("{}", "✓ Item deleted successfully".green());
        }
    }
    Ok(())
}

/// Format query response as JSON
pub fn format_response_json(response: &ExecuteStatementResponse) -> Result<()> {
    use colored::Colorize;

    match response {
        ExecuteStatementResponse::Select { items, .. } => {
            if items.is_empty() {
                println!("{}", r#"{"items": [], "count": 0}"#.dimmed());
            } else {
                let json_items: Vec<_> = items.iter()
                    .map(item_to_json)
                    .collect();
                println!("{}", serde_json::to_string_pretty(&json_items)?);
            }
        }
        ExecuteStatementResponse::Insert { .. } => {
            println!("{}", r#"{"success": true, "operation": "INSERT"}"#.green());
        }
        ExecuteStatementResponse::Update { item, .. } => {
            let json = item_to_json(item);
            let result = serde_json::json!({
                "success": true,
                "operation": "UPDATE",
                "item": json
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        ExecuteStatementResponse::Delete { .. } => {
            println!("{}", r#"{"success": true, "operation": "DELETE"}"#.green());
        }
    }
    Ok(())
}

/// Format query response in compact mode
pub fn format_response_compact(response: &ExecuteStatementResponse) -> Result<()> {
    use colored::Colorize;

    match response {
        ExecuteStatementResponse::Select { items, .. } => {
            for (idx, item) in items.iter().enumerate() {
                // Row header with index
                print!("{} ", format!("[{}]", idx + 1).dimmed());

                // Print key-value pairs inline
                let pairs: Vec<String> = item.iter()
                    .map(|(k, v)| format!("{}={}", k.cyan(), value_to_compact_string(v)))
                    .collect();

                println!("{}", pairs.join(", "));
            }
        }
        ExecuteStatementResponse::Insert { .. } => {
            println!("{}", "✓ INSERT completed".green());
        }
        ExecuteStatementResponse::Update { item, .. } => {
            println!("{}", "✓ UPDATE completed".green());
            // Show updated item inline
            let pairs: Vec<String> = item.iter()
                .map(|(k, v)| format!("{}={}", k.cyan(), value_to_compact_string(v)))
                .collect();
            println!("  {}", pairs.join(", "));
        }
        ExecuteStatementResponse::Delete { .. } => {
            println!("{}", "✓ DELETE completed".green());
        }
    }
    Ok(())
}

/// Convert a value to a compact string representation
fn value_to_compact_string(value: &KeystoneValue) -> String {
    use KeystoneValue::*;
    match value {
        S(s) => format!("\"{}\"", s),
        N(n) => n.to_string(),
        B(b) => format!("<{} bytes>", b.len()),
        Bool(b) => b.to_string(),
        Null => "null".to_string(),
        Ts(ts) => format!("@{}", ts),
        L(list) => format!("[{} items]", list.len()),
        M(map) => format!("{{{} fields}}", map.len()),
        VecF32(vec) => format!("<vector[{}]>", vec.len()),
    }
}

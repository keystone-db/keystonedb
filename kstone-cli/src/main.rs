use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use kstone_api::{Database, KeystoneValue, ExecuteStatementResponse};
use std::collections::HashMap;
use std::path::PathBuf;

mod table;

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

/// Interactive REPL shell for KeystoneDB
///
/// Provides a user-friendly interactive interface with line editing,
/// history, autocomplete, and meta-commands.

use anyhow::{Context, Result};
use colored::Colorize;
use kstone_api::Database;
use rustyline::error::ReadlineError;
use rustyline::{
    completion::{Completer, Pair},
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
    Helper,
};
use std::path::Path;

/// Autocomplete helper for PartiQL and meta-commands
#[derive(Clone)]
struct KeystoneCompleter {
    meta_commands: Vec<String>,
    partiql_keywords: Vec<String>,
}

impl KeystoneCompleter {
    fn new() -> Self {
        Self {
            meta_commands: vec![
                ".help".to_string(),
                ".exit".to_string(),
                ".quit".to_string(),
                ".schema".to_string(),
                ".indexes".to_string(),
                ".format".to_string(),
                ".timer".to_string(),
                ".clear".to_string(),
            ],
            partiql_keywords: vec![
                "SELECT".to_string(),
                "FROM".to_string(),
                "WHERE".to_string(),
                "INSERT".to_string(),
                "INTO".to_string(),
                "VALUE".to_string(),
                "UPDATE".to_string(),
                "SET".to_string(),
                "DELETE".to_string(),
                "AND".to_string(),
                "OR".to_string(),
                "NOT".to_string(),
                "items".to_string(), // table name
            ],
        }
    }

    fn complete_meta(&self, line: &str) -> Vec<Pair> {
        self.meta_commands
            .iter()
            .filter(|cmd| cmd.starts_with(line))
            .map(|cmd| Pair {
                display: cmd.clone(),
                replacement: cmd.clone(),
            })
            .collect()
    }

    fn complete_keyword(&self, word: &str) -> Vec<Pair> {
        let word_upper = word.to_uppercase();
        self.partiql_keywords
            .iter()
            .filter(|kw| kw.to_uppercase().starts_with(&word_upper))
            .map(|kw| Pair {
                display: kw.clone(),
                replacement: kw.clone(),
            })
            .collect()
    }
}

impl Completer for KeystoneCompleter {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let line_prefix = &line[..pos];

        // Complete meta-commands if line starts with '.'
        if line_prefix.starts_with('.') {
            let candidates = self.complete_meta(line_prefix);
            return Ok((0, candidates));
        }

        // Complete PartiQL keywords
        // Find the last word before cursor
        if let Some(last_space) = line_prefix.rfind(|c: char| c.is_whitespace()) {
            let word_start = last_space + 1;
            let word = &line_prefix[word_start..];
            let candidates = self.complete_keyword(word);
            return Ok((word_start, candidates));
        }

        // First word completion
        let candidates = self.complete_keyword(line_prefix);
        Ok((0, candidates))
    }
}

impl Hinter for KeystoneCompleter {
    type Hint = String;
}

impl Highlighter for KeystoneCompleter {}

impl Validator for KeystoneCompleter {}

impl Helper for KeystoneCompleter {}

/// Interactive shell session state
pub struct Shell {
    /// Database instance
    db: Database,
    /// Database path for display
    db_path: String,
    /// Line editor with history and autocomplete
    editor: rustyline::Editor<KeystoneCompleter, rustyline::history::FileHistory>,
    /// Current output format
    format: OutputFormat,
    /// Show query timing
    show_timing: bool,
}

/// Output format for query results
#[derive(Clone, Copy, Debug)]
pub enum OutputFormat {
    Table,
    Json,
    Compact,
}

impl Shell {
    /// Create a new shell session
    pub fn new(db_path: Option<&Path>) -> Result<Self> {
        // Determine if we should use in-memory mode
        let (db, display_path) = match db_path {
            // No path provided - use in-memory
            None => {
                let db = Database::create_in_memory()
                    .context("Failed to create in-memory database")?;
                (db, ":memory:".to_string())
            }
            // Path provided - check if it's the special :memory: string
            Some(path) => {
                let path_str = path.to_string_lossy();
                if path_str == ":memory:" {
                    let db = Database::create_in_memory()
                        .context("Failed to create in-memory database")?;
                    (db, ":memory:".to_string())
                } else {
                    let db = Database::open(path)
                        .context(format!("Failed to open database at {:?}", path))?;
                    (db, path.display().to_string())
                }
            }
        };

        // Create editor with custom completer
        let completer = KeystoneCompleter::new();
        let mut editor = rustyline::Editor::new()
            .context("Failed to initialize line editor")?;
        editor.set_helper(Some(completer));

        // Load history from file
        let history_path = dirs::home_dir()
            .map(|p| p.join(".keystone_history"))
            .unwrap_or_else(|| ".keystone_history".into());

        if history_path.exists() {
            let _ = editor.load_history(&history_path);
        }

        Ok(Self {
            db,
            db_path: display_path,
            editor,
            format: OutputFormat::Table,
            show_timing: true,
        })
    }

    /// Run the interactive REPL
    pub fn run(&mut self) -> Result<()> {
        self.print_welcome();

        let mut buffer = String::new();
        let mut in_multiline = false;

        loop {
            let prompt = if in_multiline {
                format!("{}    ", "...>".dimmed())
            } else {
                format!("{} ", "kstone>".green().bold())
            };

            match self.editor.readline(&prompt) {
                Ok(line) => {
                    let line = line.trim();

                    // Skip empty lines in single-line mode
                    if line.is_empty() && !in_multiline {
                        continue;
                    }

                    // Handle exit in single-line mode
                    if !in_multiline && (line == ".exit" || line == ".quit") {
                        break;
                    }

                    // Accumulate input
                    if !buffer.is_empty() {
                        buffer.push(' ');
                    }
                    buffer.push_str(line);

                    // Check if query is complete
                    let complete = if buffer.starts_with('.') {
                        // Meta-commands are always single line
                        true
                    } else {
                        // PartiQL queries end with semicolon
                        buffer.trim_end().ends_with(';')
                    };

                    if complete {
                        // Execute the complete query
                        let query = buffer.trim().to_string();
                        buffer.clear();
                        in_multiline = false;

                        if let Err(e) = self.execute(&query) {
                            eprintln!("{} {}", "Error:".red().bold(), e);
                        }
                    } else {
                        // Continue accumulating
                        in_multiline = true;
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    // Ctrl+C - cancel current input and start fresh
                    println!("^C");
                    buffer.clear();
                    in_multiline = false;
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    // Ctrl+D - exit
                    break;
                }
                Err(err) => {
                    eprintln!("Error reading line: {}", err);
                    break;
                }
            }
        }

        self.print_goodbye();
        self.save_history()?;

        Ok(())
    }

    /// Execute a command or query
    fn execute(&mut self, input: &str) -> Result<()> {
        // Check if it's a meta-command
        if input.starts_with('.') {
            self.execute_meta_command(input)?;
        } else {
            // It's a PartiQL query
            self.execute_query(input)?;
        }

        Ok(())
    }

    /// Execute a meta-command (dot-command)
    fn execute_meta_command(&mut self, command: &str) -> Result<()> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let cmd = parts.first().unwrap_or(&"");

        match *cmd {
            ".help" => self.show_help(),
            ".exit" | ".quit" => {
                // Handled in main loop
                Ok(())
            }
            ".schema" => self.show_schema(),
            ".indexes" => self.show_indexes(),
            ".format" => {
                if parts.len() < 2 {
                    println!("Usage: .format <table|json|compact>");
                    println!("Current format: {:?}", self.format);
                } else {
                    self.set_format(parts[1])?;
                }
                Ok(())
            }
            ".timer" => {
                if parts.len() < 2 {
                    println!("Usage: .timer <on|off>");
                    println!("Current: {}", if self.show_timing { "on" } else { "off" });
                } else {
                    self.set_timer(parts[1])?;
                }
                Ok(())
            }
            ".clear" => {
                print!("\x1B[2J\x1B[1;1H");
                Ok(())
            }
            _ => {
                println!("{} {}", "Unknown command:".yellow(), cmd);
                println!("Type .help for available commands");
                Ok(())
            }
        }
    }

    /// Execute a PartiQL query
    fn execute_query(&mut self, sql: &str) -> Result<()> {
        let start = std::time::Instant::now();

        let response = self.db.execute_statement(sql)
            .context("Query execution failed")?;

        let elapsed = start.elapsed();

        // Display results based on format
        match self.format {
            OutputFormat::Table => {
                crate::format_response_table(&response)?;
            }
            OutputFormat::Json => {
                crate::format_response_json(&response)?;
            }
            OutputFormat::Compact => {
                crate::format_response_compact(&response)?;
            }
        }

        // Show timing if enabled
        if self.show_timing {
            let count = match response {
                kstone_api::ExecuteStatementResponse::Select { items, .. } => items.len(),
                kstone_api::ExecuteStatementResponse::Insert { .. } => 1,
                kstone_api::ExecuteStatementResponse::Update { .. } => 1,
                kstone_api::ExecuteStatementResponse::Delete { .. } => 1,
            };

            println!(
                "\n{} ({:.2}ms)",
                format!("{} row{}", count, if count == 1 { "" } else { "s" }).dimmed(),
                elapsed.as_secs_f64() * 1000.0
            );
        }

        Ok(())
    }

    /// Show help message
    fn show_help(&self) -> Result<()> {
        println!("\n{}", "Available Commands:".bold());
        println!("\n  {}", "Meta-commands:".cyan());
        println!("    .help              Show this help message");
        println!("    .exit, .quit       Exit the shell");
        println!("    .schema            Display database schema");
        println!("    .indexes           List all indexes (LSI/GSI)");
        println!("    .format <type>     Set output format (table|json|compact)");
        println!("    .timer <on|off>    Toggle query timing display");
        println!("    .clear             Clear the screen");

        println!("\n  {}", "SQL Queries:".cyan());
        println!("    SELECT * FROM items WHERE pk = 'key';");
        println!("    INSERT INTO items VALUE {{'pk': 'key', 'name': 'Alice'}};");
        println!("    UPDATE items SET age = 30 WHERE pk = 'key';");
        println!("    DELETE FROM items WHERE pk = 'key';");
        println!("\n  {}", "Multi-line Queries:".cyan());
        println!("    Queries without a semicolon will continue on the next line.");
        println!("    Use Ctrl+C to cancel a multi-line query.");

        println!("\n  {}", "Keyboard Shortcuts:".cyan());
        println!("    Ctrl+C             Cancel current input");
        println!("    Ctrl+D             Exit shell");
        println!("    Up/Down Arrow      Navigate command history");
        println!("    Tab                Autocomplete commands and keywords");
        println!();

        Ok(())
    }

    /// Show database schema
    fn show_schema(&self) -> Result<()> {
        println!("\n{}", "Database Schema:".bold());
        println!("  {}: {}", "Path".cyan(), self.db_path);
        println!();

        // Check if in-memory mode
        if self.db_path == ":memory:" {
            println!("  {}", "Mode:".cyan());
            println!("    In-memory database (no disk persistence)");
            println!("    All data will be lost when shell exits");
            println!("    Full PartiQL support available");
            println!();
        } else {
            // Show database files
            let db_path = std::path::Path::new(&self.db_path);
            if db_path.exists() {
                let mut sst_count = 0;
                let mut wal_exists = false;
                let mut total_size: u64 = 0;

                if let Ok(entries) = std::fs::read_dir(db_path) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Ok(metadata) = entry.metadata() {
                            total_size += metadata.len();

                            if let Some(name) = path.file_name() {
                                let name_str = name.to_string_lossy();
                                if name_str.ends_with(".sst") {
                                    sst_count += 1;
                                } else if name_str == "wal.log" {
                                    wal_exists = true;
                                }
                            }
                        }
                    }
                }

                println!("  {}", "Storage:".cyan());
                println!("    SST files: {}", sst_count);
                println!("    WAL: {}", if wal_exists { "present" } else { "missing" });
                println!("    Total size: {} bytes", total_size);
                println!();
            }
        }

        // Future features note
        println!("  {}", "Note:".yellow());
        println!("    Full schema inspection (table schema, partition/sort keys)");
        println!("    will be available in Phase 3+");
        println!();

        Ok(())
    }

    /// Show indexes
    fn show_indexes(&self) -> Result<()> {
        println!("\n{}", "Indexes:".bold());
        println!();

        println!("  {}", "Current Phase:".cyan());
        println!("    KeystoneDB is currently in Phase 0 (Walking Skeleton)");
        println!("    Indexes are not yet implemented.");
        println!();

        println!("  {}", "Coming in Phase 3:".cyan());
        println!("    • Local Secondary Indexes (LSI) - Alternate sort keys");
        println!("    • Global Secondary Indexes (GSI) - Alternate partition keys");
        println!("    • Full-text search indexes");
        println!("    • Vector similarity indexes");
        println!();

        Ok(())
    }

    /// Set output format
    fn set_format(&mut self, format: &str) -> Result<()> {
        self.format = match format.to_lowercase().as_str() {
            "table" => OutputFormat::Table,
            "json" => OutputFormat::Json,
            "compact" => OutputFormat::Compact,
            _ => {
                println!("{} {}. Use: table, json, or compact", "Invalid format:".red(), format);
                return Ok(());
            }
        };

        println!("Output format set to: {:?}", self.format);
        Ok(())
    }

    /// Set timer on/off
    fn set_timer(&mut self, value: &str) -> Result<()> {
        self.show_timing = match value.to_lowercase().as_str() {
            "on" | "true" | "1" => true,
            "off" | "false" | "0" => false,
            _ => {
                println!("{} {}. Use: on or off", "Invalid value:".red(), value);
                return Ok(());
            }
        };

        println!("Timer {}", if self.show_timing { "enabled" } else { "disabled" });
        Ok(())
    }

    /// Print welcome banner
    fn print_welcome(&self) {
        println!();
        println!("{}", "╔═══════════════════════════════════════════════════════╗".cyan());
        println!("{}", "║                                                       ║".cyan());
        println!("{}", "║         KeystoneDB Interactive Shell v0.1.0           ║".cyan().bold());
        println!("{}", "║                                                       ║".cyan());
        println!("{}", format!("║  Database: {:<43} ║", self.truncate_path(&self.db_path, 43)).cyan());
        println!("{}", "║                                                       ║".cyan());
        println!("{}", "║  Quick Start:                                         ║".cyan());
        println!("{}", "║    .help           - Show all commands                ║".cyan());
        println!("{}", "║    .format <type>  - Change output (table|json|compact)║".cyan());
        println!("{}", "║    .exit           - Exit shell                       ║".cyan());
        println!("{}", "║                                                       ║".cyan());
        println!("{}", "╚═══════════════════════════════════════════════════════╝".cyan());
        println!();

        // Show tip based on database mode
        if self.db_path == ":memory:" {
            println!("  {} In-memory mode - data is temporary and will be lost on exit.", "Note:".yellow().bold());
        } else {
            println!("  {} Multi-line queries supported. End with {} to execute.", "Tip:".yellow().bold(), ";".bold());
        }
        println!();
    }

    /// Truncate path to fit in welcome banner
    fn truncate_path(&self, path: &str, max_len: usize) -> String {
        if path.len() <= max_len {
            path.to_string()
        } else {
            let start = &path[..15];
            let end_start = path.len() - (max_len - 18);
            format!("{}...{}", start, &path[end_start..])
        }
    }

    /// Print goodbye message
    fn print_goodbye(&self) {
        println!();
        println!("{}", "Thanks for using KeystoneDB!".green().bold());
        println!("{}", "  Session saved. Your command history has been preserved.".dimmed());
        println!();
    }

    /// Save command history
    fn save_history(&mut self) -> Result<()> {
        let history_path = dirs::home_dir()
            .map(|p| p.join(".keystone_history"))
            .unwrap_or_else(|| ".keystone_history".into());

        self.editor.save_history(&history_path)
            .context("Failed to save command history")?;

        Ok(())
    }
}

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
                _ => 0,
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

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // KeystoneCompleter Tests
    // ========================================

    #[test]
    fn test_completer_meta_command_completion() {
        let completer = KeystoneCompleter::new();

        // Complete .help
        let candidates = completer.complete_meta(".he");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, ".help");

        // Complete .exit
        let candidates = completer.complete_meta(".e");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, ".exit");

        // Complete all commands starting with .
        let candidates = completer.complete_meta(".");
        assert!(candidates.len() >= 8); // All meta commands

        // No match
        let candidates = completer.complete_meta(".xyz");
        assert_eq!(candidates.len(), 0);
    }

    #[test]
    fn test_completer_keyword_completion() {
        let completer = KeystoneCompleter::new();

        // Complete SELECT (case insensitive)
        let candidates = completer.complete_keyword("sel");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, "SELECT");

        // Complete SELECT with uppercase
        let candidates = completer.complete_keyword("SEL");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, "SELECT");

        // Complete FROM
        let candidates = completer.complete_keyword("fr");
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, "FROM");

        // Complete with no matches
        let candidates = completer.complete_keyword("xyz");
        assert_eq!(candidates.len(), 0);
    }

    #[test]
    fn test_completer_full_complete_meta_command() {
        let completer = KeystoneCompleter::new();
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Complete meta-command at start of line
        let (start, candidates) = completer.complete(".he", 3, &ctx).unwrap();
        assert_eq!(start, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, ".help");

        // Complete partial meta-command
        let (start, candidates) = completer.complete(".form", 5, &ctx).unwrap();
        assert_eq!(start, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, ".format");
    }

    #[test]
    fn test_completer_full_complete_partiql_keyword() {
        let completer = KeystoneCompleter::new();
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Complete first word
        let (start, candidates) = completer.complete("SEL", 3, &ctx).unwrap();
        assert_eq!(start, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, "SELECT");

        // Complete after whitespace
        let (start, candidates) = completer.complete("SELECT * FR", 11, &ctx).unwrap();
        assert_eq!(start, 9); // Start of "FR"
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, "FROM");

        // Complete in middle of query
        let (start, candidates) = completer.complete("SELECT * FROM items WH", 22, &ctx).unwrap();
        assert_eq!(start, 20); // Start of "WH"
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, "WHERE");
    }

    #[test]
    fn test_completer_complete_with_multiple_matches() {
        let completer = KeystoneCompleter::new();
        let history = rustyline::history::DefaultHistory::new();
        let ctx = rustyline::Context::new(&history);

        // Multiple keywords starting with 'IN'
        let (start, candidates) = completer.complete("IN", 2, &ctx).unwrap();
        assert_eq!(start, 0);
        assert!(candidates.len() >= 2); // INSERT, INTO
        let displays: Vec<_> = candidates.iter().map(|c| c.display.as_str()).collect();
        assert!(displays.contains(&"INSERT"));
        assert!(displays.contains(&"INTO"));
    }

    // ========================================
    // Format and Timer Tests
    // ========================================

    #[test]
    fn test_set_format_valid() {
        let mut shell = create_test_shell();

        // Set to table format
        shell.set_format("table").unwrap();
        assert!(matches!(shell.format, OutputFormat::Table));

        // Set to json format
        shell.set_format("json").unwrap();
        assert!(matches!(shell.format, OutputFormat::Json));

        // Set to compact format
        shell.set_format("compact").unwrap();
        assert!(matches!(shell.format, OutputFormat::Compact));

        // Case insensitive
        shell.set_format("TABLE").unwrap();
        assert!(matches!(shell.format, OutputFormat::Table));
    }

    #[test]
    fn test_set_format_invalid() {
        let mut shell = create_test_shell();

        // Invalid format should not change the format (starts as Table)
        shell.set_format("invalid").unwrap();
        assert!(matches!(shell.format, OutputFormat::Table));

        // Another invalid format
        shell.set_format("xml").unwrap();
        assert!(matches!(shell.format, OutputFormat::Table));
    }

    #[test]
    fn test_set_timer_valid() {
        let mut shell = create_test_shell();

        // Enable timer
        shell.set_timer("on").unwrap();
        assert!(shell.show_timing);

        // Disable timer
        shell.set_timer("off").unwrap();
        assert!(!shell.show_timing);

        // Alternative values for on
        shell.set_timer("true").unwrap();
        assert!(shell.show_timing);

        shell.set_timer("1").unwrap();
        assert!(shell.show_timing);

        // Alternative values for off
        shell.set_timer("false").unwrap();
        assert!(!shell.show_timing);

        shell.set_timer("0").unwrap();
        assert!(!shell.show_timing);

        // Case insensitive
        shell.set_timer("ON").unwrap();
        assert!(shell.show_timing);
    }

    #[test]
    fn test_set_timer_invalid() {
        let mut shell = create_test_shell();
        shell.show_timing = true;

        // Invalid value should not change the timer
        shell.set_timer("invalid").unwrap();
        assert!(shell.show_timing);

        // Another invalid value
        shell.set_timer("yes").unwrap();
        assert!(shell.show_timing);
    }

    // ========================================
    // Query Detection Tests
    // ========================================

    #[test]
    fn test_is_meta_command() {
        // Meta-commands start with '.'
        assert!(".help".starts_with('.'));
        assert!(".exit".starts_with('.'));
        assert!(".format table".starts_with('.'));
        assert!(".timer on".starts_with('.'));

        // PartiQL queries don't start with '.'
        assert!(!"SELECT * FROM items;".starts_with('.'));
        assert!(!"INSERT INTO items VALUE {};".starts_with('.'));
    }

    #[test]
    fn test_is_query_complete() {
        // Meta-commands are always complete (single line)
        assert!(".help".starts_with('.'));

        // PartiQL queries are complete when they end with semicolon
        assert!("SELECT * FROM items;".trim_end().ends_with(';'));
        assert!("INSERT INTO items VALUE {};".trim_end().ends_with(';'));
        assert!("UPDATE items SET x = 1;".trim_end().ends_with(';'));
        assert!("DELETE FROM items WHERE pk = 'key';".trim_end().ends_with(';'));

        // Incomplete queries (no semicolon)
        assert!(!"SELECT * FROM items".trim_end().ends_with(';'));
        assert!(!"INSERT INTO items VALUE {}".trim_end().ends_with(';'));
    }

    #[test]
    fn test_multiline_query_detection() {
        // Simulate multi-line query accumulation
        let mut buffer = String::new();

        // First line - incomplete
        buffer.push_str("SELECT * FROM items");
        assert!(!buffer.trim_end().ends_with(';'));
        assert!(!buffer.starts_with('.'));

        // Second line - still incomplete
        buffer.push(' ');
        buffer.push_str("WHERE pk = 'user#123'");
        assert!(!buffer.trim_end().ends_with(';'));

        // Third line - now complete
        buffer.push(' ');
        buffer.push_str("AND sk = 'profile';");
        assert!(buffer.trim_end().ends_with(';'));
    }

    // ========================================
    // Helper Method Tests
    // ========================================

    #[test]
    fn test_truncate_path_short() {
        let shell = create_test_shell();

        // Short path - should not be truncated
        let short_path = "/path/to/db";
        let truncated = shell.truncate_path(short_path, 43);
        assert_eq!(truncated, short_path);
    }

    #[test]
    fn test_truncate_path_long() {
        let shell = create_test_shell();

        // Long path - should be truncated
        let long_path = "/very/long/path/to/some/deeply/nested/directory/structure/database.keystone";
        let truncated = shell.truncate_path(long_path, 43);

        // Should be exactly 43 characters or less
        assert!(truncated.len() <= 43);

        // Should contain "..." in the middle
        assert!(truncated.contains("..."));

        // Should start with beginning of path
        assert!(truncated.starts_with("/very/long/path"));

        // Should end with end of path
        assert!(truncated.ends_with(".keystone"));
    }

    #[test]
    fn test_truncate_path_exact_length() {
        let shell = create_test_shell();

        // Path exactly at max length - should not be truncated
        let exact_path = "a".repeat(43);
        let truncated = shell.truncate_path(&exact_path, 43);
        assert_eq!(truncated, exact_path);
    }

    // ========================================
    // Meta-Command Execution Tests
    // ========================================

    #[test]
    fn test_execute_meta_command_help() {
        let mut shell = create_test_shell();

        // .help should execute without error
        let result = shell.execute_meta_command(".help");
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_meta_command_exit() {
        let mut shell = create_test_shell();

        // .exit and .quit should execute without error
        let result = shell.execute_meta_command(".exit");
        assert!(result.is_ok());

        let result = shell.execute_meta_command(".quit");
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_meta_command_schema() {
        let mut shell = create_test_shell();

        // .schema should execute without error
        let result = shell.execute_meta_command(".schema");
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_meta_command_indexes() {
        let mut shell = create_test_shell();

        // .indexes should execute without error
        let result = shell.execute_meta_command(".indexes");
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_meta_command_format() {
        let mut shell = create_test_shell();

        // .format with argument
        let result = shell.execute_meta_command(".format json");
        assert!(result.is_ok());
        assert!(matches!(shell.format, OutputFormat::Json));

        // .format without argument (should show usage)
        let result = shell.execute_meta_command(".format");
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_meta_command_timer() {
        let mut shell = create_test_shell();

        // .timer with argument
        let result = shell.execute_meta_command(".timer off");
        assert!(result.is_ok());
        assert!(!shell.show_timing);

        // .timer without argument (should show usage)
        let result = shell.execute_meta_command(".timer");
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_meta_command_clear() {
        let mut shell = create_test_shell();

        // .clear should execute without error
        let result = shell.execute_meta_command(".clear");
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_meta_command_unknown() {
        let mut shell = create_test_shell();

        // Unknown command should not error, but should print message
        let result = shell.execute_meta_command(".unknown");
        assert!(result.is_ok());

        let result = shell.execute_meta_command(".notacommand");
        assert!(result.is_ok());
    }

    // ========================================
    // Output Format Tests
    // ========================================

    #[test]
    fn test_output_format_debug() {
        // Ensure OutputFormat implements Debug for printing
        let table = OutputFormat::Table;
        let json = OutputFormat::Json;
        let compact = OutputFormat::Compact;

        assert_eq!(format!("{:?}", table), "Table");
        assert_eq!(format!("{:?}", json), "Json");
        assert_eq!(format!("{:?}", compact), "Compact");
    }

    #[test]
    fn test_output_format_copy() {
        // Ensure OutputFormat can be copied
        let format1 = OutputFormat::Table;
        let format2 = format1;

        assert!(matches!(format1, OutputFormat::Table));
        assert!(matches!(format2, OutputFormat::Table));
    }

    // ========================================
    // Test Helpers
    // ========================================

    /// Create a test shell instance with in-memory database
    fn create_test_shell() -> Shell {
        Shell::new(Some(Path::new(":memory:"))).expect("Failed to create test shell")
    }
}

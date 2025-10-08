/// Notebook storage in KeystoneDB
///
/// Notebooks are stored as special items in the database with a reserved prefix.

use anyhow::Result;
use kstone_api::{Database, ItemBuilder, KeystoneValue};
use kstone_core::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

const NOTEBOOK_PREFIX: &str = "_notebook#";
const NOTEBOOK_LIST_KEY: &str = "_notebooks#list";

/// A notebook containing multiple cells
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notebook {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub cells: Vec<Cell>,
    pub metadata: HashMap<String, String>,
}

/// A single cell in a notebook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub id: String,
    #[serde(rename = "type")]
    pub cell_type: CellType,
    pub content: String,
    pub execution_count: Option<u32>,
    pub outputs: Vec<CellOutput>,
    pub metadata: HashMap<String, String>,
}

/// Type of notebook cell
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CellType {
    Query,
    Markdown,
    Chart,
    Schema,
}

/// Output from cell execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CellOutput {
    Result {
        rows: Vec<HashMap<String, serde_json::Value>>,
        schema: Vec<ColumnSchema>,
        execution_time_ms: u64,
        row_count: usize,
    },
    Error {
        message: String,
    },
    Chart {
        spec: serde_json::Value,
    },
    Markdown {
        html: String,
    },
}

/// Column schema information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    pub type_name: String,
}

impl Notebook {
    /// Create a new empty notebook
    pub fn new(title: String) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            created_at: now,
            updated_at: now,
            cells: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add a cell to the notebook
    pub fn add_cell(&mut self, cell: Cell) {
        self.cells.push(cell);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Remove a cell by ID
    pub fn remove_cell(&mut self, cell_id: &str) {
        self.cells.retain(|c| c.id != cell_id);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Update a cell
    pub fn update_cell(&mut self, cell_id: &str, content: String) {
        if let Some(cell) = self.cells.iter_mut().find(|c| c.id == cell_id) {
            cell.content = content;
            self.updated_at = chrono::Utc::now().timestamp();
        }
    }
}

impl Cell {
    /// Create a new cell
    pub fn new(cell_type: CellType, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            cell_type,
            content,
            execution_count: None,
            outputs: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Clear outputs from the cell
    pub fn clear_outputs(&mut self) {
        self.outputs.clear();
    }

    /// Add an output to the cell
    pub fn add_output(&mut self, output: CellOutput) {
        self.outputs.push(output);
    }
}

/// Initialize notebook storage tables in the database
pub fn init_notebook_storage(db: &Database) -> Result<()> {
    // Create a special item to track notebooks
    let mut list_item = ItemBuilder::new()
        .string("type", "notebook_list")
        .number("count", 0)
        .build();

    // Manually add the list field
    list_item.insert("notebooks".to_string(), KeystoneValue::L(vec![]));

    // Only create if it doesn't exist
    if db.get(NOTEBOOK_LIST_KEY.as_bytes())?.is_none() {
        db.put(NOTEBOOK_LIST_KEY.as_bytes(), list_item)?;
    }

    Ok(())
}

/// Save a notebook to the database
pub fn save_notebook(db: &Database, notebook: &Notebook) -> Result<()> {
    let key = format!("{}{}", NOTEBOOK_PREFIX, notebook.id);
    let notebook_json = serde_json::to_string(notebook)?;

    let item = ItemBuilder::new()
        .string("type", "notebook")
        .string("id", &notebook.id)
        .string("title", &notebook.title)
        .string("content", notebook_json)
        .number("created_at", notebook.created_at)
        .number("updated_at", notebook.updated_at)
        .build();

    db.put(key.as_bytes(), item)?;

    // Update the notebook list
    update_notebook_list(db, &notebook.id)?;

    Ok(())
}

/// Load a notebook from the database
pub fn load_notebook(db: &Database, notebook_id: &str) -> Result<Option<Notebook>> {
    let key = format!("{}{}", NOTEBOOK_PREFIX, notebook_id);

    if let Some(item) = db.get(key.as_bytes())? {
        if let Some(KeystoneValue::S(content)) = item.get("content") {
            let notebook: Notebook = serde_json::from_str(content)?;
            return Ok(Some(notebook));
        }
    }

    Ok(None)
}

/// List all notebooks
pub fn list_notebooks(db: &Database) -> Result<Vec<NotebookMetadata>> {
    let mut notebooks = Vec::new();

    // Get the notebook list
    if let Some(list_item) = db.get(NOTEBOOK_LIST_KEY.as_bytes())? {
        if let Some(KeystoneValue::L(notebook_ids)) = list_item.get("notebooks") {
            for id_value in notebook_ids {
                if let Value::S(id) = id_value {
                    // Load notebook metadata
                    let key = format!("{}{}", NOTEBOOK_PREFIX, id);
                    if let Some(item) = db.get(key.as_bytes())? {
                        let metadata = NotebookMetadata {
                            id: id.clone(),
                            title: item.get("title")
                                .and_then(|v| v.as_string())
                                .unwrap_or("Untitled").to_string(),
                            created_at: item.get("created_at")
                                .and_then(|v| match v {
                                    KeystoneValue::N(n) => n.parse::<i64>().ok(),
                                    _ => None,
                                })
                                .unwrap_or(0),
                            updated_at: item.get("updated_at")
                                .and_then(|v| match v {
                                    KeystoneValue::N(n) => n.parse::<i64>().ok(),
                                    _ => None,
                                })
                                .unwrap_or(0),
                        };
                        notebooks.push(metadata);
                    }
                }
            }
        }
    }

    Ok(notebooks)
}

/// Delete a notebook
pub fn delete_notebook(db: &Database, notebook_id: &str) -> Result<()> {
    let key = format!("{}{}", NOTEBOOK_PREFIX, notebook_id);
    db.delete(key.as_bytes())?;

    // Remove from notebook list
    remove_from_notebook_list(db, notebook_id)?;

    Ok(())
}

/// Notebook metadata for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMetadata {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Update the notebook list with a new notebook ID
fn update_notebook_list(db: &Database, notebook_id: &str) -> Result<()> {
    if let Some(mut item) = db.get(NOTEBOOK_LIST_KEY.as_bytes())? {
        if let Some(KeystoneValue::L(notebooks)) = item.get_mut("notebooks") {
            // Check if already in list
            let id_exists = notebooks.iter().any(|v| {
                if let Value::S(id) = v {
                    id == notebook_id
                } else {
                    false
                }
            });

            if !id_exists {
                notebooks.push(Value::S(notebook_id.to_string()));

                // Update count
                let count = notebooks.len() as i64;
                item.insert("count".to_string(), KeystoneValue::N(count.to_string()));

                db.put(NOTEBOOK_LIST_KEY.as_bytes(), item)?;
            }
        }
    }

    Ok(())
}

/// Remove a notebook ID from the list
fn remove_from_notebook_list(db: &Database, notebook_id: &str) -> Result<()> {
    if let Some(mut item) = db.get(NOTEBOOK_LIST_KEY.as_bytes())? {
        if let Some(KeystoneValue::L(notebooks)) = item.get_mut("notebooks") {
            notebooks.retain(|v| {
                if let Value::S(id) = v {
                    id != notebook_id
                } else {
                    true
                }
            });

            // Update count
            let count = notebooks.len() as i64;
            item.insert("count".to_string(), KeystoneValue::N(count.to_string()));

            db.put(NOTEBOOK_LIST_KEY.as_bytes(), item)?;
        }
    }

    Ok(())
}

// Add chrono for timestamps
use chrono;
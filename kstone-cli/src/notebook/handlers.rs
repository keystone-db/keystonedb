/// HTTP request handlers for the notebook API

use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

use super::{
    server::AppState,
    storage::{
        self, Cell, CellOutput, CellType, ColumnSchema, Notebook, NotebookMetadata,
    },
};
use kstone_api::{ItemBuilder, KeystoneValue};
use kstone_core::Value;

/// List all notebooks
pub async fn list_notebooks(
    State(state): State<AppState>,
) -> Result<Json<Vec<NotebookMetadata>>, AppError> {
    let notebooks = storage::list_notebooks(&state.db)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(notebooks))
}

/// Create notebook request
#[derive(Debug, Deserialize)]
pub struct CreateNotebookRequest {
    pub title: String,
}

/// Create a new notebook
pub async fn create_notebook(
    State(state): State<AppState>,
    Json(req): Json<CreateNotebookRequest>,
) -> Result<Json<Notebook>, AppError> {
    let notebook = Notebook::new(req.title);

    storage::save_notebook(&state.db, &notebook)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(notebook))
}

/// Get a notebook by ID
pub async fn get_notebook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Notebook>, AppError> {
    let notebook = storage::load_notebook(&state.db, &id)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Notebook {} not found", id)))?;

    Ok(Json(notebook))
}

/// Update notebook request
#[derive(Debug, Deserialize)]
pub struct UpdateNotebookRequest {
    pub title: Option<String>,
    pub cells: Option<Vec<Cell>>,
}

/// Update a notebook
pub async fn update_notebook(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateNotebookRequest>,
) -> Result<Json<Notebook>, AppError> {
    let mut notebook = storage::load_notebook(&state.db, &id)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Notebook {} not found", id)))?;

    if let Some(title) = req.title {
        notebook.title = title;
    }

    if let Some(cells) = req.cells {
        notebook.cells = cells;
    }

    notebook.updated_at = chrono::Utc::now().timestamp();

    storage::save_notebook(&state.db, &notebook)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(notebook))
}

/// Delete a notebook
pub async fn delete_notebook(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    storage::delete_notebook(&state.db, &id)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Add cell request
#[derive(Debug, Deserialize)]
pub struct AddCellRequest {
    pub cell_type: CellType,
    pub content: String,
}

/// Add a cell to a notebook
pub async fn add_cell(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<AddCellRequest>,
) -> Result<Json<Cell>, AppError> {
    let mut notebook = storage::load_notebook(&state.db, &id)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Notebook {} not found", id)))?;

    let cell = Cell::new(req.cell_type, req.content);
    notebook.add_cell(cell.clone());

    storage::save_notebook(&state.db, &notebook)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(cell))
}

/// Update cell request
#[derive(Debug, Deserialize)]
pub struct UpdateCellRequest {
    pub content: String,
}

/// Update a cell in a notebook
pub async fn update_cell(
    State(state): State<AppState>,
    Path((notebook_id, cell_id)): Path<(String, String)>,
    Json(req): Json<UpdateCellRequest>,
) -> Result<StatusCode, AppError> {
    let mut notebook = storage::load_notebook(&state.db, &notebook_id)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Notebook {} not found", notebook_id)))?;

    notebook.update_cell(&cell_id, req.content);

    storage::save_notebook(&state.db, &notebook)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::OK)
}

/// Delete a cell from a notebook
pub async fn delete_cell(
    State(state): State<AppState>,
    Path((notebook_id, cell_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let mut notebook = storage::load_notebook(&state.db, &notebook_id)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Notebook {} not found", notebook_id)))?;

    notebook.remove_cell(&cell_id);

    storage::save_notebook(&state.db, &notebook)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Execute a cell (returns result)
pub async fn execute_cell(
    State(state): State<AppState>,
    Path((notebook_id, cell_id)): Path<(String, String)>,
) -> Result<Json<CellOutput>, AppError> {
    let mut notebook = storage::load_notebook(&state.db, &notebook_id)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound(format!("Notebook {} not found", notebook_id)))?;

    let cell = notebook.cells
        .iter_mut()
        .find(|c| c.id == cell_id)
        .ok_or(AppError::NotFound(format!("Cell {} not found", cell_id)))?;

    // Execute the cell based on its type
    let output = match &cell.cell_type {
        CellType::Query => {
            // Execute PartiQL query
            let start = std::time::Instant::now();

            match state.db.execute_statement(&cell.content) {
                Ok(response) => {
                    let execution_time_ms = start.elapsed().as_millis() as u64;

                    // Handle different response types
                    match response {
                        kstone_api::ExecuteStatementResponse::Select { items, .. } => {
                            // Convert items to JSON-compatible format
                            let rows: Vec<HashMap<String, serde_json::Value>> = items
                                .iter()
                                .map(|item| {
                                    let mut row = HashMap::new();
                                    for (key, value) in item {
                                        row.insert(key.clone(), keystone_value_to_json(value));
                                    }
                                    row
                                })
                                .collect();

                            // Extract schema from first row
                            let schema = if let Some(first) = items.first() {
                                first.keys()
                                    .map(|k| ColumnSchema {
                                        name: k.clone(),
                                        type_name: "STRING".to_string(), // Simplified for now
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };

                            CellOutput::Result {
                                rows: rows.clone(),
                                schema,
                                execution_time_ms,
                                row_count: rows.len(),
                            }
                        }
                        kstone_api::ExecuteStatementResponse::Insert { success } => {
                            if success {
                                CellOutput::Result {
                                    rows: vec![],
                                    schema: vec![],
                                    execution_time_ms,
                                    row_count: 0,
                                }
                            } else {
                                CellOutput::Error {
                                    message: "Insert failed".to_string(),
                                }
                            }
                        }
                        kstone_api::ExecuteStatementResponse::Update { .. } => {
                            CellOutput::Result {
                                rows: vec![],
                                schema: vec![],
                                execution_time_ms,
                                row_count: 1,
                            }
                        }
                        kstone_api::ExecuteStatementResponse::Delete { success } => {
                            if success {
                                CellOutput::Result {
                                    rows: vec![],
                                    schema: vec![],
                                    execution_time_ms,
                                    row_count: 0,
                                }
                            } else {
                                CellOutput::Error {
                                    message: "Delete failed".to_string(),
                                }
                            }
                        }
                    }
                }
                Err(e) => CellOutput::Error {
                    message: e.to_string(),
                },
            }
        }
        CellType::Markdown => {
            // Process markdown to HTML (simplified for now)
            CellOutput::Markdown {
                html: format!("<div>{}</div>", html_escape::encode_text(&cell.content)),
            }
        }
        CellType::Chart => {
            // Return chart spec as-is (client will render)
            match serde_json::from_str(&cell.content) {
                Ok(spec) => CellOutput::Chart { spec },
                Err(e) => CellOutput::Error {
                    message: format!("Invalid chart spec: {}", e),
                },
            }
        }
        CellType::Schema => {
            // Get database schema information
            // This is a placeholder - would need to implement schema introspection
            CellOutput::Markdown {
                html: "<p>Schema information not yet implemented</p>".to_string(),
            }
        }
    };

    // Update execution count for query cells
    if matches!(cell.cell_type, CellType::Query) {
        cell.execution_count = Some(cell.execution_count.unwrap_or(0) + 1);
    }

    // Clear old outputs and add new one
    cell.clear_outputs();
    cell.add_output(output.clone());

    // Save the notebook with updated cell
    storage::save_notebook(&state.db, &notebook)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(output))
}

/// Database info response
#[derive(Debug, Serialize)]
pub struct DatabaseInfo {
    pub path: String,
    pub sst_count: usize,
    pub wal_exists: bool,
    pub total_size: u64,
}

/// Get database information
pub async fn database_info(
    State(state): State<AppState>,
) -> Result<Json<DatabaseInfo>, AppError> {
    // This is a placeholder - would need to add methods to Database API
    let info = DatabaseInfo {
        path: "database.keystone".to_string(),
        sst_count: 0,
        wal_exists: true,
        total_size: 0,
    };

    Ok(Json(info))
}

/// Database schema response
#[derive(Debug, Serialize)]
pub struct DatabaseSchema {
    pub indexes: Vec<String>,
    pub ttl_enabled: bool,
    pub stream_enabled: bool,
}

/// Get database schema
pub async fn database_schema(
    State(_state): State<AppState>,
) -> Result<Json<DatabaseSchema>, AppError> {
    // This is a placeholder - would need to add methods to Database API
    let schema = DatabaseSchema {
        indexes: vec![],
        ttl_enabled: false,
        stream_enabled: false,
    };

    Ok(Json(schema))
}

/// Convert KeystoneValue to JSON
fn keystone_value_to_json(value: &KeystoneValue) -> serde_json::Value {
    match value {
        KeystoneValue::S(s) => json!(s),
        KeystoneValue::N(n) => {
            // Try to parse as number, otherwise as string
            n.parse::<f64>()
                .map(|v| json!(v))
                .unwrap_or_else(|_| json!(n))
        }
        KeystoneValue::B(bytes) => {
            use base64::Engine;
            json!(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        KeystoneValue::Bool(b) => json!(b),
        KeystoneValue::Null => json!(null),
        KeystoneValue::L(list) => {
            json!(list.iter().map(|v| value_to_json(v)).collect::<Vec<_>>())
        }
        KeystoneValue::M(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), keystone_value_to_json(v));
            }
            json!(obj)
        }
        KeystoneValue::VecF32(vec) => json!(vec),
        KeystoneValue::Ts(ts) => json!(ts),
    }
}

/// Convert Value to JSON (for lists)
fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::S(s) => json!(s),
        Value::N(n) => {
            n.parse::<f64>()
                .map(|v| json!(v))
                .unwrap_or_else(|_| json!(n))
        }
        Value::B(bytes) => {
            use base64::Engine;
            json!(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        Value::Bool(b) => json!(b),
        Value::Null => json!(null),
        Value::L(list) => {
            json!(list.iter().map(|v| value_to_json(v)).collect::<Vec<_>>())
        }
        Value::M(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), keystone_value_to_json(v));
            }
            json!(obj)
        }
        Value::VecF32(vec) => json!(vec),
        Value::Ts(ts) => json!(ts),
    }
}

/// Application error type
#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = json!({
            "error": message,
        });

        (status, Json(body)).into_response()
    }
}


// Add HTML escaping
mod html_escape {
    pub fn encode_text(text: &str) -> String {
        text.chars()
            .map(|c| match c {
                '<' => "&lt;".to_string(),
                '>' => "&gt;".to_string(),
                '&' => "&amp;".to_string(),
                '"' => "&quot;".to_string(),
                '\'' => "&#39;".to_string(),
                _ => c.to_string(),
            })
            .collect()
    }
}
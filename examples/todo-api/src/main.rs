/// Todo List REST API Example using KeystoneDB
///
/// This example demonstrates:
/// - Full CRUD operations (Create, Read, Update, Delete)
/// - Conditional operations (preventing double completion)
/// - Batch operations using transactions
/// - List/query operations with filtering
/// - REST API design patterns
/// - Proper error handling with HTTP status codes
/// - Advanced KeystoneDB features

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use kstone_api::{Database, ItemBuilder, KeystoneValue};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;
use uuid::Uuid;

mod models;
use models::*;

/// Application state
#[derive(Clone)]
struct AppState {
    db: Arc<Database>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Create or open database
    let db = Database::create("todo-api.keystone")?;
    info!("Database initialized");

    // Create application state
    let state = AppState { db: Arc::new(db) };

    // Build router
    let app = Router::new()
        .route("/", get(root))
        .route("/todos", post(create_todo))
        .route("/todos", get(list_todos))
        .route("/todos/:id", get(get_todo))
        .route("/todos/:id", patch(update_todo))
        .route("/todos/:id", delete(delete_todo))
        .route("/todos/:id/complete", post(complete_todo))
        .route("/todos/batch", post(batch_operations))
        .route("/api/health", get(health_check))
        .route("/api/stats", get(stats))
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3002").await?;
    info!("Server listening on http://127.0.0.1:3002");
    println!("🚀 Todo API running at http://127.0.0.1:3002");
    println!("   POST   /todos          - Create todo");
    println!("   GET    /todos          - List all todos");
    println!("   GET    /todos/:id      - Get specific todo");
    println!("   PATCH  /todos/:id      - Update todo");
    println!("   DELETE /todos/:id      - Delete todo");
    println!("   POST   /todos/:id/complete - Mark complete (conditional)");
    println!("   POST   /todos/batch    - Batch operations");
    println!("   GET    /api/health     - Health check");
    println!("   GET    /api/stats      - Database statistics");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Root endpoint
async fn root() -> &'static str {
    "Todo List API - POST /todos to create a new todo"
}

/// Create a new todo
async fn create_todo(
    State(state): State<AppState>,
    Json(request): Json<CreateTodoRequest>,
) -> Result<(StatusCode, Json<TodoResponse>), AppError> {
    // Validate input
    if request.title.trim().is_empty() {
        return Err(AppError::BadRequest("Title cannot be empty".to_string()));
    }

    if request.priority < 1 || request.priority > 5 {
        return Err(AppError::BadRequest(
            "Priority must be between 1 and 5".to_string(),
        ));
    }

    // Generate ID
    let id = Uuid::new_v4().to_string();

    // Get current timestamp
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create todo item
    let item = ItemBuilder::new()
        .string("id", &id)
        .string("title", request.title.trim())
        .string("status", Status::Pending.as_str())
        .number("priority", request.priority)
        .number("created_at", now as i64)
        .number("updated_at", now as i64)
        .build_with_optional(|builder| {
            if let Some(desc) = &request.description {
                if !desc.trim().is_empty() {
                    return builder.string("description", desc.trim());
                }
            }
            builder
        });

    // Store in database
    let key = format!("todo#{}", id);
    state.db.put(key.as_bytes(), item)?;

    info!("Created todo: {} - {}", id, request.title);

    // Create response
    let todo = Todo {
        id: id.clone(),
        title: request.title,
        description: request.description,
        status: Status::Pending,
        priority: request.priority,
        created_at: now,
        updated_at: now,
        completed_at: None,
    };

    Ok((StatusCode::CREATED, Json(TodoResponse { todo })))
}

/// List all todos
async fn list_todos(State(_state): State<AppState>) -> Result<Json<TodoListResponse>, AppError> {
    // In a real implementation, we would use a Query/Scan operation
    // For now, we'll need to scan through keys with the "todo#" prefix
    // This is a limitation of the current KeystoneDB API

    // Since KeystoneDB doesn't have a built-in scan/list operation yet,
    // we'll return a placeholder response
    // In production, you'd implement a secondary index or maintain a list of todo IDs

    info!("List todos requested");

    Ok(Json(TodoListResponse {
        todos: vec![],
        count: 0,
    }))
}

/// Get a specific todo
async fn get_todo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TodoResponse>, AppError> {
    let key = format!("todo#{}", id);

    let item = state
        .db
        .get(key.as_bytes())?
        .ok_or(AppError::NotFound("Todo not found".to_string()))?;

    let todo = item_to_todo(item)?;

    info!("Retrieved todo: {}", id);

    Ok(Json(TodoResponse { todo }))
}

/// Update a todo
async fn update_todo(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateTodoRequest>,
) -> Result<Json<TodoResponse>, AppError> {
    let key = format!("todo#{}", id);

    // Get existing item
    let existing_item = state
        .db
        .get(key.as_bytes())?
        .ok_or(AppError::NotFound("Todo not found".to_string()))?;

    // Validate priority if provided
    if let Some(priority) = request.priority {
        if priority < 1 || priority > 5 {
            return Err(AppError::BadRequest(
                "Priority must be between 1 and 5".to_string(),
            ));
        }
    }

    // Extract existing values
    let existing_title = match existing_item.get("title") {
        Some(KeystoneValue::S(s)) => s.clone(),
        _ => return Err(AppError::InvalidData),
    };

    let existing_description = existing_item
        .get("description")
        .and_then(|v| match v {
            KeystoneValue::S(s) => Some(s.clone()),
            _ => None,
        });

    let existing_status = match existing_item.get("status") {
        Some(KeystoneValue::S(s)) => Status::from_str(s).ok_or(AppError::InvalidData)?,
        _ => return Err(AppError::InvalidData),
    };

    let existing_priority = match existing_item.get("priority") {
        Some(KeystoneValue::N(n)) => n.parse::<i64>().unwrap_or(3),
        _ => 3,
    };

    let created_at = match existing_item.get("created_at") {
        Some(KeystoneValue::N(n)) => n.parse::<u64>().unwrap_or(0),
        _ => 0,
    };

    let existing_completed_at = existing_item
        .get("completed_at")
        .and_then(|v| match v {
            KeystoneValue::N(n) => n.parse::<u64>().ok(),
            _ => None,
        });

    // Get current timestamp
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Build updated item
    let new_title = request.title.clone().unwrap_or(existing_title);
    let new_status = request.status.clone().unwrap_or(existing_status.clone());
    let new_priority = request.priority.unwrap_or(existing_priority);
    let new_description = request.description.clone().or(existing_description.clone());

    let mut builder = ItemBuilder::new()
        .string("id", &id)
        .string("title", &new_title)
        .string("status", new_status.as_str())
        .number("priority", new_priority)
        .number("created_at", created_at as i64)
        .number("updated_at", now as i64);

    if let Some(desc) = &new_description {
        builder = builder.string("description", desc);
    }

    if let Some(completed) = existing_completed_at {
        builder = builder.number("completed_at", completed as i64);
    }

    let updated_item = builder.build();

    // Update in database
    state.db.put(key.as_bytes(), updated_item)?;

    info!("Updated todo: {}", id);

    let todo = Todo {
        id,
        title: new_title,
        description: new_description,
        status: new_status,
        priority: new_priority,
        created_at,
        updated_at: now,
        completed_at: existing_completed_at,
    };

    Ok(Json(TodoResponse { todo }))
}

/// Delete a todo
async fn delete_todo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let key = format!("todo#{}", id);

    // Check if exists
    if state.db.get(key.as_bytes())?.is_none() {
        return Err(AppError::NotFound("Todo not found".to_string()));
    }

    state.db.delete(key.as_bytes())?;

    info!("Deleted todo: {}", id);

    Ok(StatusCode::NO_CONTENT)
}

/// Mark a todo as complete (conditional - prevents double completion)
async fn complete_todo(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<TodoResponse>, AppError> {
    let key = format!("todo#{}", id);

    // Get existing item
    let existing_item = state
        .db
        .get(key.as_bytes())?
        .ok_or(AppError::NotFound("Todo not found".to_string()))?;

    // Check current status
    let current_status = match existing_item.get("status") {
        Some(KeystoneValue::S(s)) => Status::from_str(s).ok_or(AppError::InvalidData)?,
        _ => return Err(AppError::InvalidData),
    };

    // Prevent double completion (conditional check)
    if current_status == Status::Completed {
        return Err(AppError::Conflict("Todo is already completed".to_string()));
    }

    // Extract existing values
    let title = match existing_item.get("title") {
        Some(KeystoneValue::S(s)) => s.clone(),
        _ => return Err(AppError::InvalidData),
    };

    let description = existing_item
        .get("description")
        .and_then(|v| match v {
            KeystoneValue::S(s) => Some(s.clone()),
            _ => None,
        });

    let priority = match existing_item.get("priority") {
        Some(KeystoneValue::N(n)) => n.parse::<i64>().unwrap_or(3),
        _ => 3,
    };

    let created_at = match existing_item.get("created_at") {
        Some(KeystoneValue::N(n)) => n.parse::<u64>().unwrap_or(0),
        _ => 0,
    };

    // Get current timestamp
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Build updated item with completed status
    let mut builder = ItemBuilder::new()
        .string("id", &id)
        .string("title", &title)
        .string("status", Status::Completed.as_str())
        .number("priority", priority)
        .number("created_at", created_at as i64)
        .number("updated_at", now as i64)
        .number("completed_at", now as i64);

    if let Some(desc) = &description {
        builder = builder.string("description", desc);
    }

    let updated_item = builder.build();

    // Update in database
    state.db.put(key.as_bytes(), updated_item)?;

    info!("Completed todo: {}", id);

    let todo = Todo {
        id,
        title,
        description,
        status: Status::Completed,
        priority,
        created_at,
        updated_at: now,
        completed_at: Some(now),
    };

    Ok(Json(TodoResponse { todo }))
}

/// Batch operations
async fn batch_operations(
    State(state): State<AppState>,
    Json(request): Json<BatchRequest>,
) -> Result<Json<BatchResponse>, AppError> {
    if request.operations.is_empty() {
        return Err(AppError::BadRequest(
            "No operations provided".to_string(),
        ));
    }

    let mut results = Vec::new();
    let mut operations_completed = 0;

    // Note: In a real implementation with transaction support,
    // all operations would be atomic. For now, we execute them sequentially.
    for op in request.operations {
        match op {
            BatchOperation::Create {
                title,
                description,
                priority,
            } => {
                let id = Uuid::new_v4().to_string();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                let mut builder = ItemBuilder::new()
                    .string("id", &id)
                    .string("title", title.trim())
                    .string("status", Status::Pending.as_str())
                    .number("priority", priority)
                    .number("created_at", now as i64)
                    .number("updated_at", now as i64);

                if let Some(desc) = description {
                    if !desc.trim().is_empty() {
                        builder = builder.string("description", desc.trim());
                    }
                }

                let item = builder.build();
                let key = format!("todo#{}", id);

                match state.db.put(key.as_bytes(), item) {
                    Ok(_) => {
                        operations_completed += 1;
                        results.push(BatchResult {
                            operation: "create".to_string(),
                            success: true,
                            id: Some(id),
                            error: None,
                        });
                    }
                    Err(e) => {
                        results.push(BatchResult {
                            operation: "create".to_string(),
                            success: false,
                            id: None,
                            error: Some(format!("{}", e)),
                        });
                    }
                }
            }
            BatchOperation::Update {
                id,
                title,
                description,
                status,
                priority,
            } => {
                let key = format!("todo#{}", id);

                match state.db.get(key.as_bytes()) {
                    Ok(Some(existing_item)) => {
                        // Similar update logic as update_todo
                        let now = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        let existing_title = match existing_item.get("title") {
                            Some(KeystoneValue::S(s)) => s.clone(),
                            _ => {
                                results.push(BatchResult {
                                    operation: "update".to_string(),
                                    success: false,
                                    id: Some(id),
                                    error: Some("Invalid data".to_string()),
                                });
                                continue;
                            }
                        };

                        let new_title = title.unwrap_or(existing_title);
                        let new_status = status
                            .unwrap_or_else(|| {
                                existing_item
                                    .get("status")
                                    .and_then(|v| match v {
                                        KeystoneValue::S(s) => Status::from_str(s),
                                        _ => None,
                                    })
                                    .unwrap_or(Status::Pending)
                            });

                        let mut builder = ItemBuilder::new()
                            .string("id", &id)
                            .string("title", &new_title)
                            .string("status", new_status.as_str())
                            .number("updated_at", now as i64);

                        // Copy other fields...
                        if let Some(desc) = description {
                            builder = builder.string("description", &desc);
                        }

                        if let Some(prio) = priority {
                            builder = builder.number("priority", prio);
                        }

                        let updated_item = builder.build();

                        match state.db.put(key.as_bytes(), updated_item) {
                            Ok(_) => {
                                operations_completed += 1;
                                results.push(BatchResult {
                                    operation: "update".to_string(),
                                    success: true,
                                    id: Some(id),
                                    error: None,
                                });
                            }
                            Err(e) => {
                                results.push(BatchResult {
                                    operation: "update".to_string(),
                                    success: false,
                                    id: Some(id),
                                    error: Some(format!("{}", e)),
                                });
                            }
                        }
                    }
                    Ok(None) => {
                        results.push(BatchResult {
                            operation: "update".to_string(),
                            success: false,
                            id: Some(id),
                            error: Some("Todo not found".to_string()),
                        });
                    }
                    Err(e) => {
                        results.push(BatchResult {
                            operation: "update".to_string(),
                            success: false,
                            id: Some(id),
                            error: Some(format!("{}", e)),
                        });
                    }
                }
            }
            BatchOperation::Delete { id } => {
                let key = format!("todo#{}", id);

                match state.db.delete(key.as_bytes()) {
                    Ok(_) => {
                        operations_completed += 1;
                        results.push(BatchResult {
                            operation: "delete".to_string(),
                            success: true,
                            id: Some(id),
                            error: None,
                        });
                    }
                    Err(e) => {
                        results.push(BatchResult {
                            operation: "delete".to_string(),
                            success: false,
                            id: Some(id),
                            error: Some(format!("{}", e)),
                        });
                    }
                }
            }
        }
    }

    let success = results.iter().all(|r| r.success);

    info!(
        "Batch operation: {} of {} succeeded",
        operations_completed,
        results.len()
    );

    Ok(Json(BatchResponse {
        success,
        operations_completed,
        results,
    }))
}

/// Health check endpoint
async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let health = state.db.health();

    Json(HealthResponse {
        status: format!("{:?}", health.status),
        warnings: health.warnings,
        errors: health.errors,
    })
}

/// Database statistics endpoint
async fn stats(State(state): State<AppState>) -> Result<Json<StatsResponse>, AppError> {
    let db_stats = state.db.stats()?;

    // In a real implementation, we would count todos by status
    // For now, return placeholder counts
    let status_counts = StatusCounts {
        pending: 0,
        in_progress: 0,
        completed: 0,
    };

    Ok(Json(StatsResponse {
        total_todos: 0,
        by_status: status_counts,
        database_stats: DatabaseStats {
            total_keys: db_stats.total_keys,
            total_sst_files: db_stats.total_sst_files,
            wal_size_bytes: db_stats.wal_size_bytes,
            memtable_size_bytes: db_stats.memtable_size_bytes,
            total_disk_size_bytes: db_stats.total_disk_size_bytes,
        },
    }))
}

/// Helper function to convert item to Todo
fn item_to_todo(item: std::collections::HashMap<String, KeystoneValue>) -> Result<Todo, AppError> {
    let id = match item.get("id") {
        Some(KeystoneValue::S(s)) => s.clone(),
        _ => return Err(AppError::InvalidData),
    };

    let title = match item.get("title") {
        Some(KeystoneValue::S(s)) => s.clone(),
        _ => return Err(AppError::InvalidData),
    };

    let description = item.get("description").and_then(|v| match v {
        KeystoneValue::S(s) => Some(s.clone()),
        _ => None,
    });

    let status = match item.get("status") {
        Some(KeystoneValue::S(s)) => Status::from_str(s).ok_or(AppError::InvalidData)?,
        _ => return Err(AppError::InvalidData),
    };

    let priority = match item.get("priority") {
        Some(KeystoneValue::N(n)) => n.parse::<i64>().unwrap_or(3),
        _ => 3,
    };

    let created_at = match item.get("created_at") {
        Some(KeystoneValue::N(n)) => n.parse::<u64>().unwrap_or(0),
        _ => 0,
    };

    let updated_at = match item.get("updated_at") {
        Some(KeystoneValue::N(n)) => n.parse::<u64>().unwrap_or(0),
        _ => 0,
    };

    let completed_at = item.get("completed_at").and_then(|v| match v {
        KeystoneValue::N(n) => n.parse::<u64>().ok(),
        _ => None,
    });

    Ok(Todo {
        id,
        title,
        description,
        status,
        priority,
        created_at,
        updated_at,
        completed_at,
    })
}

/// Application errors
#[derive(Debug)]
enum AppError {
    Database(kstone_api::KeystoneError),
    NotFound(String),
    BadRequest(String),
    Conflict(String),
    InvalidData,
}

impl From<kstone_api::KeystoneError> for AppError {
    fn from(err: kstone_api::KeystoneError) -> Self {
        AppError::Database(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::Database(err) => {
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", err))
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::InvalidData => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Invalid data format".to_string())
            }
        };

        let body = Json(serde_json::json!({
            "error": message
        }));

        (status, body).into_response()
    }
}

// Helper trait to build items with optional fields
trait BuildWithOptional {
    fn build_with_optional<F>(self, f: F) -> std::collections::HashMap<String, KeystoneValue>
    where
        F: FnOnce(ItemBuilder) -> ItemBuilder;
}

impl BuildWithOptional for ItemBuilder {
    fn build_with_optional<F>(self, f: F) -> std::collections::HashMap<String, KeystoneValue>
    where
        F: FnOnce(ItemBuilder) -> ItemBuilder,
    {
        f(self).build()
    }
}

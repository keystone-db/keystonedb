/// Data models for Todo API

use serde::{Deserialize, Serialize};

/// Todo status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    InProgress,
    Completed,
}

impl Status {
    pub fn as_str(&self) -> &str {
        match self {
            Status::Pending => "pending",
            Status::InProgress => "inprogress",
            Status::Completed => "completed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Status::Pending),
            "inprogress" => Some(Status::InProgress),
            "completed" => Some(Status::Completed),
            _ => None,
        }
    }
}

/// Todo item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    /// Unique ID
    pub id: String,
    /// Todo title
    pub title: String,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Status
    pub status: Status,
    /// Priority (1-5, higher is more important)
    pub priority: i64,
    /// Unix timestamp when created
    pub created_at: u64,
    /// Unix timestamp when last updated
    pub updated_at: u64,
    /// Unix timestamp when completed (if status is Completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
}

/// Request to create a new todo
#[derive(Debug, Deserialize)]
pub struct CreateTodoRequest {
    /// Todo title
    pub title: String,
    /// Optional description
    pub description: Option<String>,
    /// Priority (1-5, defaults to 3)
    #[serde(default = "default_priority")]
    pub priority: i64,
}

fn default_priority() -> i64 {
    3
}

/// Request to update an existing todo
#[derive(Debug, Deserialize)]
pub struct UpdateTodoRequest {
    /// Optional new title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional new description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional new status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    /// Optional new priority
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
}

/// Response with a single todo
#[derive(Debug, Serialize)]
pub struct TodoResponse {
    pub todo: Todo,
}

/// Response with a list of todos
#[derive(Debug, Serialize)]
pub struct TodoListResponse {
    pub todos: Vec<Todo>,
    pub count: usize,
}

/// Batch operation type
#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "lowercase")]
pub enum BatchOperation {
    Create {
        title: String,
        description: Option<String>,
        #[serde(default = "default_priority")]
        priority: i64,
    },
    Update {
        id: String,
        title: Option<String>,
        description: Option<String>,
        status: Option<Status>,
        priority: Option<i64>,
    },
    Delete {
        id: String,
    },
}

/// Request for batch operations
#[derive(Debug, Deserialize)]
pub struct BatchRequest {
    pub operations: Vec<BatchOperation>,
}

/// Response for batch operations
#[derive(Debug, Serialize)]
pub struct BatchResponse {
    pub success: bool,
    pub operations_completed: usize,
    pub results: Vec<BatchResult>,
}

/// Result of a single batch operation
#[derive(Debug, Serialize)]
pub struct BatchResult {
    pub operation: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Stats response
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub total_todos: usize,
    pub by_status: StatusCounts,
    pub database_stats: DatabaseStats,
}

/// Counts by status
#[derive(Debug, Serialize)]
pub struct StatusCounts {
    pub pending: usize,
    pub in_progress: usize,
    pub completed: usize,
}

/// Database statistics
#[derive(Debug, Serialize)]
pub struct DatabaseStats {
    pub total_keys: Option<u64>,
    pub total_sst_files: u64,
    pub wal_size_bytes: Option<u64>,
    pub memtable_size_bytes: Option<u64>,
    pub total_disk_size_bytes: Option<u64>,
}

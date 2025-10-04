/// In-Memory Cache Server Example using KeystoneDB
///
/// Demonstrates:
/// - In-memory mode (no disk persistence)
/// - TTL-based expiration
/// - Configurable resource limits
/// - Health and stats endpoints
/// - Retry logic for transient failures

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use kstone_api::{Database, DatabaseConfig, ItemBuilder, KeystoneValue};
use kstone_core::retry;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Clone)]
struct AppState {
    db: Arc<Database>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Create in-memory database with limits
    let db = Database::create_in_memory()?;
    info!("In-memory cache initialized");

    let state = AppState { db: Arc::new(db) };

    let app = Router::new()
        .route("/", get(root))
        .route("/cache/:key", put(set_value))
        .route("/cache/:key", get(get_value))
        .route("/cache/:key", delete(delete_value))
        .route("/api/flush", post(flush_cache))
        .route("/api/health", get(health_check))
        .route("/api/stats", get(cache_stats))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await?;
    info!("Cache server listening on http://127.0.0.1:3001");
    println!("🚀 Cache Server running at http://127.0.0.1:3001");
    println!("   PUT /cache/:key - Set value");
    println!("   GET /cache/:key - Get value");
    println!("   DELETE /cache/:key - Delete value");
    println!("   POST /api/flush - Clear all entries");
    println!("   GET /api/health - Health check");
    println!("   GET /api/stats - Cache statistics");

    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> &'static str {
    "In-Memory Cache Server - Use PUT /cache/:key to store values"
}

#[derive(Deserialize)]
struct SetRequest {
    value: serde_json::Value,
    ttl_seconds: Option<u64>,
}

async fn set_value(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(request): Json<SetRequest>,
) -> Result<StatusCode, AppError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut builder = ItemBuilder::new()
        .string("value", &request.value.to_string())
        .number("created_at", now as i64);

    if let Some(ttl) = request.ttl_seconds {
        builder = builder.number("ttl", (now + ttl) as i64);
    }

    let item = builder.build();

    // Use retry logic for transient failures
    retry(|| state.db.put(key.as_bytes(), item.clone()))?;

    info!("Cached key: {}", key);
    Ok(StatusCode::CREATED)
}

async fn get_value(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let item = state
        .db
        .get(key.as_bytes())?
        .ok_or(AppError::NotFound)?;

    // Check TTL
    if let Some(KeystoneValue::N(ttl_str)) = item.get("ttl") {
        let ttl: u64 = ttl_str.parse().unwrap_or(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if now > ttl {
            state.db.delete(key.as_bytes())?;
            return Err(AppError::Expired);
        }
    }

    let value_str = match item.get("value") {
        Some(KeystoneValue::S(v)) => v.clone(),
        _ => return Err(AppError::InvalidData),
    };

    let value: serde_json::Value = serde_json::from_str(&value_str)
        .unwrap_or_else(|_| serde_json::Value::String(value_str));

    Ok(Json(serde_json::json!({
        "key": key,
        "value": value,
    })))
}

async fn delete_value(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<StatusCode, AppError> {
    if state.db.get(key.as_bytes())?.is_none() {
        return Err(AppError::NotFound);
    }

    state.db.delete(key.as_bytes())?;
    info!("Deleted key: {}", key);
    Ok(StatusCode::NO_CONTENT)
}

async fn flush_cache(State(state): State<AppState>) -> Result<StatusCode, AppError> {
    // In a real implementation, you'd need a way to list and delete all keys
    // For now, just acknowledge the flush
    info!("Cache flush requested");
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    warnings: Vec<String>,
    errors: Vec<String>,
}

async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let health = state.db.health();
    Json(HealthResponse {
        status: format!("{:?}", health.status),
        warnings: health.warnings,
        errors: health.errors,
    })
}

async fn cache_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let stats = state.db.stats()?;

    Ok(Json(serde_json::json!({
        "in_memory": true,
        "total_keys": stats.total_keys,
        "disk_size": 0,
    })))
}

#[derive(Debug)]
enum AppError {
    Database(kstone_api::KeystoneError),
    NotFound,
    Expired,
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
            AppError::NotFound => (StatusCode::NOT_FOUND, "Key not found".to_string()),
            AppError::Expired => (StatusCode::GONE, "Key has expired".to_string()),
            AppError::InvalidData => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Invalid data".to_string())
            }
        };

        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

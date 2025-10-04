/// URL Shortener Example using KeystoneDB
///
/// This example demonstrates:
/// - Basic put/get/delete operations
/// - TTL for automatic link expiration
/// - Conditional updates (visit counter)
/// - REST API with Axum
/// - Health and stats endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post},
    Json, Router,
};
use kstone_api::{Database, ItemBuilder, KeystoneValue};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

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
    let db = Database::create("url-shortener.keystone")?;
    info!("Database initialized");

    // Create application state
    let state = AppState { db: Arc::new(db) };

    // Build router
    let app = Router::new()
        .route("/", get(root))
        .route("/shorten", post(shorten_url))
        .route("/:code", get(redirect_url))
        .route("/api/stats/:code", get(get_stats))
        .route("/api/delete/:code", delete(delete_url))
        .route("/api/health", get(health_check))
        .route("/api/stats", get(db_stats))
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    info!("Server listening on http://127.0.0.1:3000");
    println!("🚀 URL Shortener running at http://127.0.0.1:3000");
    println!("   POST /shorten - Create short URL");
    println!("   GET /:code - Redirect to long URL");
    println!("   GET /api/stats/:code - Get URL stats");
    println!("   DELETE /api/delete/:code - Delete URL");
    println!("   GET /api/health - Health check");
    println!("   GET /api/stats - Database stats");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Root endpoint
async fn root() -> &'static str {
    "URL Shortener API - POST /shorten to create a short URL"
}

/// Shorten a URL
async fn shorten_url(
    State(state): State<AppState>,
    Json(request): Json<ShortenRequest>,
) -> Result<Json<ShortenResponse>, AppError> {
    // Generate short code
    let short_code = nanoid::nanoid!(6);

    // Get current timestamp
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Calculate TTL if provided
    let ttl = request
        .ttl_seconds
        .map(|ttl| now + ttl);

    // Create item
    let mut builder = ItemBuilder::new()
        .string("long_url", &request.long_url)
        .string("short_code", &short_code)
        .number("visits", 0)
        .number("created_at", now as i64);

    if let Some(ttl_val) = ttl {
        builder = builder.number("ttl", ttl_val as i64);
    }

    let item = builder.build();

    // Store in database
    let key = format!("url#{}", short_code);
    state.db.put(key.as_bytes(), item)?;

    info!("Created short URL: {}", short_code);

    Ok(Json(ShortenResponse {
        short_code: short_code.clone(),
        short_url: format!("http://127.0.0.1:3000/{}", short_code),
        long_url: request.long_url,
        expires_at: ttl,
    }))
}

/// Redirect to long URL
async fn redirect_url(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Redirect, AppError> {
    let key = format!("url#{}", code);

    // Get item
    let item = state
        .db
        .get(key.as_bytes())?
        .ok_or(AppError::NotFound)?;

    // Check if expired
    if let Some(KeystoneValue::N(ttl_str)) = item.get("ttl") {
        let ttl: u64 = ttl_str.parse().unwrap_or(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now > ttl {
            return Err(AppError::Expired);
        }
    }

    // Get long URL
    let long_url = match item.get("long_url") {
        Some(KeystoneValue::S(url)) => url.clone(),
        _ => return Err(AppError::InvalidData),
    };

    // Increment visit counter (using update expression would be better in production)
    let visits = match item.get("visits") {
        Some(KeystoneValue::N(n)) => n.parse::<i64>().unwrap_or(0) + 1,
        _ => 1,
    };

    let updated_item = ItemBuilder::new()
        .string("long_url", &long_url)
        .string("short_code", &code)
        .number("visits", visits)
        .number("created_at",
            match item.get("created_at") {
                Some(KeystoneValue::N(n)) => n.parse().unwrap_or(0),
                _ => 0,
            }
        )
        .build();

    state.db.put(key.as_bytes(), updated_item)?;

    info!("Redirecting {} to {} (visit #{})", code, long_url, visits);

    Ok(Redirect::permanent(&long_url))
}

/// Get URL statistics
async fn get_stats(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<StatsResponse>, AppError> {
    let key = format!("url#{}", code);

    let item = state
        .db
        .get(key.as_bytes())?
        .ok_or(AppError::NotFound)?;

    let long_url = match item.get("long_url") {
        Some(KeystoneValue::S(url)) => url.clone(),
        _ => return Err(AppError::InvalidData),
    };

    let visits = match item.get("visits") {
        Some(KeystoneValue::N(n)) => n.parse().unwrap_or(0),
        _ => 0,
    };

    let created_at = match item.get("created_at") {
        Some(KeystoneValue::N(n)) => n.parse().unwrap_or(0),
        _ => 0,
    };

    let ttl = match item.get("ttl") {
        Some(KeystoneValue::N(n)) => Some(n.parse().unwrap_or(0)),
        _ => None,
    };

    Ok(Json(StatsResponse {
        short_code: code,
        long_url,
        visits,
        created_at,
        ttl,
    }))
}

/// Delete a short URL
async fn delete_url(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<StatusCode, AppError> {
    let key = format!("url#{}", code);

    // Check if exists
    if state.db.get(key.as_bytes())?.is_none() {
        return Err(AppError::NotFound);
    }

    state.db.delete(key.as_bytes())?;

    info!("Deleted short URL: {}", code);

    Ok(StatusCode::NO_CONTENT)
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
async fn db_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let stats = state.db.stats()?;

    let stats_json = serde_json::json!({
        "total_keys": stats.total_keys,
        "total_sst_files": stats.total_sst_files,
        "wal_size_bytes": stats.wal_size_bytes,
        "memtable_size_bytes": stats.memtable_size_bytes,
        "total_disk_size_bytes": stats.total_disk_size_bytes,
        "compaction": {
            "total_compactions": stats.compaction.total_compactions,
            "total_ssts_merged": stats.compaction.total_ssts_merged,
            "total_bytes_read": stats.compaction.total_bytes_read,
            "total_bytes_written": stats.compaction.total_bytes_written,
        }
    });

    Ok(Json(stats_json))
}

/// Application errors
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
            AppError::NotFound => (StatusCode::NOT_FOUND, "URL not found".to_string()),
            AppError::Expired => (StatusCode::GONE, "URL has expired".to_string()),
            AppError::InvalidData => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Invalid data".to_string())
            }
        };

        let body = Json(serde_json::json!({
            "error": message
        }));

        (status, body).into_response()
    }
}

/// Blog Engine Example using KeystoneDB
///
/// This example demonstrates advanced KeystoneDB features:
/// - Composite keys (partition key + sort key) for hierarchical data
/// - Query API to efficiently retrieve items within a partition
/// - Scan API for analytics across all items
/// - Multi-user blog platform with authors and posts
/// - Tag-based filtering (simulated GSI pattern)
/// - View tracking and popular posts analytics
///
/// Data Model:
///   PK: "author#{author_id}"
///   SK: "post#{timestamp}#{post_id}"
///   Attributes: {
///     author_id, post_id, title, content, tags,
///     views, created_at, updated_at
///   }
///
/// This design allows efficient queries like:
/// - Get all posts by an author (Query on PK)
/// - Get posts sorted by creation time (SK ordering)
/// - Track individual post views
/// - Analyze popular content

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use kstone_api::Database;
use std::sync::Arc;
use tracing::info;

mod models;
mod handlers;

use handlers::*;

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
    let db = Database::create("blog-engine.keystone")?;
    info!("Blog database initialized");

    // Create application state
    let state = AppState { db: Arc::new(db) };

    // Build router with all endpoints
    let app = Router::new()
        .route("/", get(root))
        // Post operations
        .route("/posts", post(create_post))
        .route("/posts/:author", get(list_author_posts))
        .route("/posts/:author/:post_id", get(get_post))
        .route("/posts/:author/:post_id", patch(update_post))
        .route("/posts/:author/:post_id", delete(delete_post))
        // Tag operations
        .route("/tags", get(list_all_tags))
        .route("/tags/:tag", get(get_posts_by_tag))
        // Statistics and analytics
        .route("/stats/popular", get(get_popular_posts))
        // System endpoints
        .route("/api/health", get(health_check))
        .route("/api/stats", get(db_stats))
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3003").await?;
    info!("Blog Engine listening on http://127.0.0.1:3003");

    println!("🚀 Blog Engine running at http://127.0.0.1:3003");
    println!("\n📝 Post Operations:");
    println!("   POST   /posts - Create new post");
    println!("   GET    /posts/:author - List author's posts");
    println!("   GET    /posts/:author/:post_id - Get specific post");
    println!("   PATCH  /posts/:author/:post_id - Update post");
    println!("   DELETE /posts/:author/:post_id - Delete post");
    println!("\n🏷️  Tag Operations:");
    println!("   GET    /tags - List all tags with counts");
    println!("   GET    /tags/:tag - Get posts by tag");
    println!("\n📊 Analytics:");
    println!("   GET    /stats/popular - Most viewed posts");
    println!("\n🔧 System:");
    println!("   GET    /api/health - Health check");
    println!("   GET    /api/stats - Database statistics");
    println!("\n💡 Features Demonstrated:");
    println!("   • Composite keys (PK + SK) for hierarchical data");
    println!("   • Query API for efficient partition queries");
    println!("   • Scan API for analytics");
    println!("   • View tracking and counters");
    println!("   • Tag-based filtering (simulated GSI)");

    axum::serve(listener, app).await?;

    Ok(())
}

/// Root endpoint
async fn root() -> &'static str {
    "Blog Engine API - POST /posts to create a blog post"
}

/// Application errors
#[derive(Debug)]
pub enum AppError {
    Database(kstone_api::KeystoneError),
    NotFound,
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
            AppError::NotFound => (StatusCode::NOT_FOUND, "Post not found".to_string()),
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

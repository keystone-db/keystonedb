use axum::{extract::State, Json};
use kstone_api::{KeystoneValue, Scan};
use tracing::info;

use crate::{models::*, AppError, AppState};

/// Get the most popular posts (by view count)
///
/// GET /stats/popular
///
/// Returns the top 10 most viewed posts.
/// In a real implementation, this could use PartiQL:
/// SELECT * FROM posts ORDER BY views DESC LIMIT 10
pub async fn get_popular_posts(
    State(state): State<AppState>,
) -> Result<Json<PostListResponse>, AppError> {
    // Scan all posts
    let scan = Scan::new();
    let response = state.db.scan(scan)?;

    let mut posts = Vec::new();

    for item in response.items {
        if let Some(KeystoneValue::S(post_id)) = item.get("post_id") {
            if let Some(post) = PostResponse::from_item(&item, post_id.clone()) {
                posts.push(post);
            }
        }
    }

    // Sort by views descending (most viewed first)
    posts.sort_by(|a, b| b.views.cmp(&a.views));

    // Take top 10
    posts.truncate(10);

    info!("Retrieved {} popular posts", posts.len());

    Ok(Json(PostListResponse {
        count: posts.len(),
        posts,
    }))
}

/// Health check endpoint
///
/// GET /api/health
pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let health = state.db.health();

    Json(HealthResponse {
        status: format!("{:?}", health.status),
        warnings: health.warnings,
        errors: health.errors,
    })
}

/// Database statistics endpoint
///
/// GET /api/stats
pub async fn db_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let stats = state.db.stats()?;

    // Also count total posts
    let scan = Scan::new();
    let response = state.db.scan(scan)?;
    let total_posts = response.items.len();

    let stats_json = serde_json::json!({
        "total_posts": total_posts,
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

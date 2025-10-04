use axum::{
    extract::{Path, State},
    Json,
};
use kstone_api::{KeystoneValue, Scan};
use std::collections::HashMap;
use tracing::info;

use crate::{models::*, AppError, AppState};

/// Get all posts with a specific tag
///
/// GET /tags/:tag
///
/// Note: In a real implementation with GSI (Global Secondary Index),
/// we would create an index on the tags attribute for efficient querying.
/// For now, we scan all items and filter by tag.
pub async fn get_posts_by_tag(
    State(state): State<AppState>,
    Path(tag): Path<String>,
) -> Result<Json<PostListResponse>, AppError> {
    // Use scan to get all posts (in production, this would use a GSI)
    let scan = Scan::new();
    let response = state.db.scan(scan)?;

    let mut posts = Vec::new();

    for item in response.items {
        // Check if this item has the requested tag
        if let Some(KeystoneValue::S(tags_str)) = item.get("tags") {
            let tags: Vec<&str> = tags_str.split(',').map(|s| s.trim()).collect();

            if tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
                if let Some(KeystoneValue::S(post_id)) = item.get("post_id") {
                    if let Some(post) = PostResponse::from_item(&item, post_id.clone()) {
                        posts.push(post);
                    }
                }
            }
        }
    }

    // Sort by created_at descending (newest first)
    posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    info!("Found {} posts with tag '{}'", posts.len(), tag);

    Ok(Json(PostListResponse {
        count: posts.len(),
        posts,
    }))
}

/// Get all tags and their post counts
///
/// GET /tags
///
/// Returns a list of all tags with the number of posts for each tag.
pub async fn list_all_tags(
    State(state): State<AppState>,
) -> Result<Json<Vec<TagInfo>>, AppError> {
    // Scan all posts to collect tags
    let scan = Scan::new();
    let response = state.db.scan(scan)?;

    let mut tag_counts: HashMap<String, usize> = HashMap::new();

    for item in response.items {
        if let Some(KeystoneValue::S(tags_str)) = item.get("tags") {
            if !tags_str.is_empty() {
                for tag in tags_str.split(',') {
                    let tag = tag.trim().to_string();
                    if !tag.is_empty() {
                        *tag_counts.entry(tag).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    // Convert to TagInfo and sort by count descending
    let mut tags: Vec<TagInfo> = tag_counts
        .into_iter()
        .map(|(tag, post_count)| TagInfo { tag, post_count })
        .collect();

    tags.sort_by(|a, b| b.post_count.cmp(&a.post_count));

    info!("Found {} unique tags", tags.len());

    Ok(Json(tags))
}

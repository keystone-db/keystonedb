use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use kstone_api::{ItemBuilder, KeystoneValue, Query};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;
use uuid::Uuid;

use crate::{models::*, AppError, AppState};

/// Create a new blog post
///
/// POST /posts
/// Body: CreatePostRequest
pub async fn create_post(
    State(state): State<AppState>,
    Json(request): Json<CreatePostRequest>,
) -> Result<Json<PostResponse>, AppError> {
    // Generate unique post ID
    let post_id = Uuid::new_v4().to_string();

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Create composite key
    // PK: author#{author_id}
    // SK: post#{timestamp}#{post_id}
    let pk = format!("author#{}", request.author_id);
    let sk = format!("post#{}#{}", now, post_id);

    // Join tags into comma-separated string for simplicity
    let tags_str = request.tags.join(",");

    // Build item
    let item = ItemBuilder::new()
        .string("author_id", &request.author_id)
        .string("post_id", &post_id)
        .string("title", &request.title)
        .string("content", &request.content)
        .string("tags", &tags_str)
        .number("views", 0)
        .number("created_at", now as i64)
        .number("updated_at", now as i64)
        .build();

    // Store with composite key
    state.db.put_with_sk(pk.as_bytes(), sk.as_bytes(), item)?;

    info!(
        "Created post {} by author {} with {} tags",
        post_id,
        request.author_id,
        request.tags.len()
    );

    Ok(Json(PostResponse {
        post_id,
        author_id: request.author_id,
        title: request.title,
        content: request.content,
        tags: request.tags,
        views: 0,
        created_at: now,
        updated_at: now,
    }))
}

/// Get a specific post
///
/// GET /posts/:author/:post_id
pub async fn get_post(
    State(state): State<AppState>,
    Path((author_id, post_id)): Path<(String, String)>,
) -> Result<Json<PostResponse>, AppError> {
    let pk = format!("author#{}", author_id);

    // Query all posts for this author to find the one with matching post_id
    let query = Query::new(pk.as_bytes());
    let response = state.db.query(query)?;

    // Find the post with matching post_id
    for item in response.items {
        if let Some(KeystoneValue::S(id)) = item.get("post_id") {
            if id == &post_id {
                // Found the post - increment view counter
                let views = match item.get("views") {
                    Some(KeystoneValue::N(n)) => n.parse::<i64>().unwrap_or(0) + 1,
                    _ => 1,
                };

                // Extract created_at to rebuild the SK
                let created_at = match item.get("created_at") {
                    Some(KeystoneValue::N(n)) => n.parse::<u64>().unwrap_or(0),
                    _ => 0,
                };

                let sk = format!("post#{}#{}", created_at, post_id);

                // Update view count
                let updated_item = ItemBuilder::new()
                    .string("author_id", &author_id)
                    .string("post_id", &post_id)
                    .string(
                        "title",
                        match item.get("title") {
                            Some(KeystoneValue::S(s)) => s,
                            _ => "",
                        },
                    )
                    .string(
                        "content",
                        match item.get("content") {
                            Some(KeystoneValue::S(s)) => s,
                            _ => "",
                        },
                    )
                    .string(
                        "tags",
                        match item.get("tags") {
                            Some(KeystoneValue::S(s)) => s,
                            _ => "",
                        },
                    )
                    .number("views", views)
                    .number("created_at", created_at as i64)
                    .number(
                        "updated_at",
                        match item.get("updated_at") {
                            Some(KeystoneValue::N(n)) => n.parse().unwrap_or(0),
                            _ => 0,
                        },
                    )
                    .build();

                state
                    .db
                    .put_with_sk(pk.as_bytes(), sk.as_bytes(), updated_item)?;

                // Return post with incremented views
                if let Some(post) = PostResponse::from_item(&item, post_id.clone()) {
                    let mut post = post;
                    post.views = views;
                    return Ok(Json(post));
                }
            }
        }
    }

    Err(AppError::NotFound)
}

/// List all posts by an author
///
/// GET /posts/:author
pub async fn list_author_posts(
    State(state): State<AppState>,
    Path(author_id): Path<String>,
) -> Result<Json<PostListResponse>, AppError> {
    let pk = format!("author#{}", author_id);

    // Query all posts for this author using composite key query
    let query = Query::new(pk.as_bytes());
    let response = state.db.query(query)?;

    let mut posts = Vec::new();

    for item in response.items {
        if let Some(KeystoneValue::S(post_id)) = item.get("post_id") {
            if let Some(post) = PostResponse::from_item(&item, post_id.clone()) {
                posts.push(post);
            }
        }
    }

    // Sort by created_at descending (newest first)
    posts.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    info!("Retrieved {} posts for author {}", posts.len(), author_id);

    Ok(Json(PostListResponse {
        count: posts.len(),
        posts,
    }))
}

/// Update a post
///
/// PATCH /posts/:author/:post_id
pub async fn update_post(
    State(state): State<AppState>,
    Path((author_id, post_id)): Path<(String, String)>,
    Json(request): Json<UpdatePostRequest>,
) -> Result<Json<PostResponse>, AppError> {
    let pk = format!("author#{}", author_id);

    // Query to find the post
    let query = Query::new(pk.as_bytes());
    let response = state.db.query(query)?;

    // Find the post with matching post_id
    for item in response.items {
        if let Some(KeystoneValue::S(id)) = item.get("post_id") {
            if id == &post_id {
                // Found the post - extract created_at to rebuild SK
                let created_at = match item.get("created_at") {
                    Some(KeystoneValue::N(n)) => n.parse::<u64>().unwrap_or(0),
                    _ => 0,
                };

                let sk = format!("post#{}#{}", created_at, post_id);

                // Update fields
                let title = request.title.unwrap_or_else(|| {
                    match item.get("title") {
                        Some(KeystoneValue::S(s)) => s.clone(),
                        _ => String::new(),
                    }
                });

                let content = request.content.unwrap_or_else(|| {
                    match item.get("content") {
                        Some(KeystoneValue::S(s)) => s.clone(),
                        _ => String::new(),
                    }
                });

                let tags_str = if let Some(tags) = request.tags {
                    tags.join(",")
                } else {
                    match item.get("tags") {
                        Some(KeystoneValue::S(s)) => s.clone(),
                        _ => String::new(),
                    }
                };

                let tags: Vec<String> = if tags_str.is_empty() {
                    Vec::new()
                } else {
                    tags_str.split(',').map(|s| s.trim().to_string()).collect()
                };

                let views = match item.get("views") {
                    Some(KeystoneValue::N(n)) => n.parse().unwrap_or(0),
                    _ => 0,
                };

                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                // Build updated item
                let updated_item = ItemBuilder::new()
                    .string("author_id", &author_id)
                    .string("post_id", &post_id)
                    .string("title", &title)
                    .string("content", &content)
                    .string("tags", &tags_str)
                    .number("views", views)
                    .number("created_at", created_at as i64)
                    .number("updated_at", now as i64)
                    .build();

                state
                    .db
                    .put_with_sk(pk.as_bytes(), sk.as_bytes(), updated_item)?;

                info!("Updated post {} by author {}", post_id, author_id);

                return Ok(Json(PostResponse {
                    post_id,
                    author_id,
                    title,
                    content,
                    tags,
                    views,
                    created_at,
                    updated_at: now,
                }));
            }
        }
    }

    Err(AppError::NotFound)
}

/// Delete a post
///
/// DELETE /posts/:author/:post_id
pub async fn delete_post(
    State(state): State<AppState>,
    Path((author_id, post_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let pk = format!("author#{}", author_id);

    // Query to find the post
    let query = Query::new(pk.as_bytes());
    let response = state.db.query(query)?;

    // Find the post with matching post_id
    for item in response.items {
        if let Some(KeystoneValue::S(id)) = item.get("post_id") {
            if id == &post_id {
                // Found the post - extract created_at to rebuild SK
                let created_at = match item.get("created_at") {
                    Some(KeystoneValue::N(n)) => n.parse::<u64>().unwrap_or(0),
                    _ => 0,
                };

                let sk = format!("post#{}#{}", created_at, post_id);

                state.db.delete_with_sk(pk.as_bytes(), sk.as_bytes())?;

                info!("Deleted post {} by author {}", post_id, author_id);

                return Ok(StatusCode::NO_CONTENT);
            }
        }
    }

    Err(AppError::NotFound)
}

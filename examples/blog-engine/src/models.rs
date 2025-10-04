use serde::{Deserialize, Serialize};

/// Request to create a new blog post
#[derive(Debug, Deserialize)]
pub struct CreatePostRequest {
    pub author_id: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Request to update an existing post
#[derive(Debug, Deserialize)]
pub struct UpdatePostRequest {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Blog post response
#[derive(Debug, Serialize, Deserialize)]
pub struct PostResponse {
    pub post_id: String,
    pub author_id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub views: i64,
    pub created_at: u64,
    pub updated_at: u64,
}

impl PostResponse {
    pub fn from_item(
        item: &std::collections::HashMap<String, kstone_api::KeystoneValue>,
        post_id: String,
    ) -> Option<Self> {
        use kstone_api::KeystoneValue;

        let author_id = match item.get("author_id") {
            Some(KeystoneValue::S(s)) => s.clone(),
            _ => return None,
        };

        let title = match item.get("title") {
            Some(KeystoneValue::S(s)) => s.clone(),
            _ => return None,
        };

        let content = match item.get("content") {
            Some(KeystoneValue::S(s)) => s.clone(),
            _ => return None,
        };

        let tags_str = match item.get("tags") {
            Some(KeystoneValue::S(s)) => s.clone(),
            _ => String::new(),
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

        let created_at = match item.get("created_at") {
            Some(KeystoneValue::N(n)) => n.parse().unwrap_or(0),
            _ => 0,
        };

        let updated_at = match item.get("updated_at") {
            Some(KeystoneValue::N(n)) => n.parse().unwrap_or(0),
            _ => 0,
        };

        Some(PostResponse {
            post_id,
            author_id,
            title,
            content,
            tags,
            views,
            created_at,
            updated_at,
        })
    }
}

/// List of posts response
#[derive(Debug, Serialize)]
pub struct PostListResponse {
    pub posts: Vec<PostResponse>,
    pub count: usize,
}

/// Tag information
#[derive(Debug, Serialize)]
pub struct TagInfo {
    pub tag: String,
    pub post_count: usize,
}

/// Health check response
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

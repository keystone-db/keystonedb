/// Data models for URL Shortener

use serde::{Deserialize, Serialize};

/// Request to shorten a URL
#[derive(Debug, Deserialize)]
pub struct ShortenRequest {
    /// The long URL to shorten
    pub long_url: String,
    /// Optional TTL in seconds (how long the short URL should be valid)
    pub ttl_seconds: Option<u64>,
}

/// Response after shortening a URL
#[derive(Debug, Serialize)]
pub struct ShortenResponse {
    /// The generated short code
    pub short_code: String,
    /// The full short URL
    pub short_url: String,
    /// The original long URL
    pub long_url: String,
    /// Unix timestamp when the URL expires (if TTL was set)
    pub expires_at: Option<u64>,
}

/// Statistics for a short URL
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    /// The short code
    pub short_code: String,
    /// The long URL
    pub long_url: String,
    /// Number of times this URL has been accessed
    pub visits: i64,
    /// Unix timestamp when the URL was created
    pub created_at: u64,
    /// Unix timestamp when the URL expires (if TTL was set)
    pub ttl: Option<u64>,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Health status: Healthy, Degraded, or Unhealthy
    pub status: String,
    /// Warning messages
    pub warnings: Vec<String>,
    /// Error messages
    pub errors: Vec<String>,
}

/// Authentication interceptor for gRPC requests
///
/// Provides simple API key authentication using Bearer tokens in the Authorization header.

use tonic::{Request, Status};
use tracing::{debug, warn};

/// Authentication state for the server
#[derive(Clone)]
pub struct AuthInterceptor {
    /// Expected API key (if authentication is enabled)
    api_key: Option<String>,
}

impl AuthInterceptor {
    /// Create a new authentication interceptor
    ///
    /// If `api_key` is None, authentication is disabled (dev mode).
    /// If `api_key` is Some, all requests must include "authorization: Bearer <key>" header.
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }

    /// Check if authentication is enabled
    pub fn is_enabled(&self) -> bool {
        self.api_key.is_some()
    }

    /// Intercept and validate incoming requests
    ///
    /// Returns Ok if authentication passes or is disabled.
    /// Returns Err(Status::UNAUTHENTICATED) if authentication fails.
    pub fn intercept<T>(&self, request: Request<T>) -> Result<Request<T>, Status> {
        // If no API key is configured, allow all requests (dev mode)
        let expected_key = match &self.api_key {
            None => {
                debug!("Authentication disabled - allowing request");
                return Ok(request);
            }
            Some(key) => key,
        };

        // Extract authorization header
        let auth_header = match request.metadata().get("authorization") {
            Some(header) => header,
            None => {
                warn!("Request rejected: missing authorization header");
                return Err(Status::unauthenticated("Missing authorization header"));
            }
        };

        // Parse Bearer token
        let auth_str = match auth_header.to_str() {
            Ok(s) => s,
            Err(_) => {
                warn!("Request rejected: invalid authorization header encoding");
                return Err(Status::unauthenticated("Invalid authorization header encoding"));
            }
        };

        // Check for "Bearer " prefix
        if !auth_str.starts_with("Bearer ") {
            warn!("Request rejected: authorization header must use Bearer scheme");
            return Err(Status::unauthenticated(
                "Authorization header must use Bearer scheme (format: 'Bearer <key>')",
            ));
        }

        // Extract and validate API key
        let provided_key = &auth_str[7..]; // Skip "Bearer "

        if provided_key != expected_key {
            warn!("Request rejected: invalid API key");
            return Err(Status::unauthenticated("Invalid API key"));
        }

        debug!("Authentication successful");
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tonic::metadata::MetadataValue;

    #[test]
    fn test_auth_disabled() {
        let interceptor = AuthInterceptor::new(None);
        assert!(!interceptor.is_enabled());

        let request = Request::new(());
        let result = interceptor.intercept(request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_auth_enabled_valid_key() {
        let interceptor = AuthInterceptor::new(Some("test-key-123".to_string()));
        assert!(interceptor.is_enabled());

        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::from_static("Bearer test-key-123"),
        );

        let result = interceptor.intercept(request);
        assert!(result.is_ok());
    }

    #[test]
    fn test_auth_enabled_invalid_key() {
        let interceptor = AuthInterceptor::new(Some("test-key-123".to_string()));

        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::from_static("Bearer wrong-key"),
        );

        let result = interceptor.intercept(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_auth_enabled_missing_header() {
        let interceptor = AuthInterceptor::new(Some("test-key-123".to_string()));

        let request = Request::new(());
        let result = interceptor.intercept(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_auth_enabled_wrong_scheme() {
        let interceptor = AuthInterceptor::new(Some("test-key-123".to_string()));

        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::from_static("Basic test-key-123"),
        );

        let result = interceptor.intercept(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }

    #[test]
    fn test_auth_enabled_no_scheme() {
        let interceptor = AuthInterceptor::new(Some("test-key-123".to_string()));

        let mut request = Request::new(());
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::from_static("test-key-123"),
        );

        let result = interceptor.intercept(request);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
    }
}

/// Rate limiting for gRPC requests
///
/// Implements token bucket rate limiting to prevent server overload

use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorRateLimiter,
};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;
use tonic::Status;
use tracing::{debug, warn};

use crate::metrics::RATE_LIMITED_REQUESTS;

/// Rate limiter for controlling request rate
#[derive(Clone)]
pub struct RateLimiter {
    /// Per-connection rate limiter (requests per second)
    per_connection: Option<Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,
    /// Global rate limiter (total requests per second)
    global: Option<Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    /// * `per_connection_rps` - Max requests per second per connection (0 = unlimited)
    /// * `global_rps` - Max total requests per second (0 = unlimited)
    pub fn new(per_connection_rps: u32, global_rps: u32) -> Self {
        let per_connection = if per_connection_rps > 0 {
            NonZeroU32::new(per_connection_rps).map(|rps| {
                let quota = Quota::per_second(rps);
                Arc::new(GovernorRateLimiter::direct(quota))
            })
        } else {
            None
        };

        let global = if global_rps > 0 {
            NonZeroU32::new(global_rps).map(|rps| {
                let quota = Quota::per_second(rps);
                Arc::new(GovernorRateLimiter::direct(quota))
            })
        } else {
            None
        };

        Self {
            per_connection,
            global,
        }
    }

    /// Check if a request should be allowed
    ///
    /// Returns Ok if allowed, Err(Status) if rate limited
    pub fn check_rate_limit(&self) -> Result<(), Status> {
        // Check per-connection limit first
        if let Some(limiter) = &self.per_connection {
            match limiter.check() {
                Ok(_) => debug!("Per-connection rate limit check passed"),
                Err(_) => {
                    warn!("Per-connection rate limit exceeded");
                    RATE_LIMITED_REQUESTS.with_label_values(&["per_connection"]).inc();
                    return Err(Status::resource_exhausted(
                        "Rate limit exceeded: too many requests from this connection"
                    ));
                }
            }
        }

        // Check global limit
        if let Some(limiter) = &self.global {
            match limiter.check() {
                Ok(_) => debug!("Global rate limit check passed"),
                Err(_) => {
                    warn!("Global rate limit exceeded");
                    RATE_LIMITED_REQUESTS.with_label_values(&["global"]).inc();
                    return Err(Status::resource_exhausted(
                        "Rate limit exceeded: server at capacity"
                    ));
                }
            }
        }

        Ok(())
    }

    /// Get time until next request would be allowed (for per-connection limit)
    pub fn time_until_ready(&self) -> Option<Duration> {
        // Note: This is a simplified version that returns None for now
        // Full implementation would require tracking the last check time
        self.per_connection.as_ref()?;
        None
    }

    /// Check if rate limiting is enabled
    pub fn is_enabled(&self) -> bool {
        self.per_connection.is_some() || self.global.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_rate_limiter_unlimited() {
        let limiter = RateLimiter::new(0, 0);

        // Should allow unlimited requests
        for _ in 0..100 {
            assert!(limiter.check_rate_limit().is_ok());
        }

        assert!(!limiter.is_enabled());
    }

    #[test]
    fn test_rate_limiter_per_connection() {
        // Allow 10 requests per second
        let limiter = RateLimiter::new(10, 0);

        // First 10 should succeed
        for i in 0..10 {
            assert!(
                limiter.check_rate_limit().is_ok(),
                "Request {} should succeed",
                i
            );
        }

        // 11th should fail
        assert!(limiter.check_rate_limit().is_err());

        assert!(limiter.is_enabled());
    }

    #[test]
    fn test_rate_limiter_global() {
        // Allow 5 requests per second globally
        let limiter = RateLimiter::new(0, 5);

        // First 5 should succeed
        for i in 0..5 {
            assert!(
                limiter.check_rate_limit().is_ok(),
                "Request {} should succeed",
                i
            );
        }

        // 6th should fail
        assert!(limiter.check_rate_limit().is_err());
    }

    #[test]
    fn test_rate_limiter_refill() {
        // Allow 2 requests per second
        let limiter = RateLimiter::new(2, 0);

        // Use both tokens
        assert!(limiter.check_rate_limit().is_ok());
        assert!(limiter.check_rate_limit().is_ok());

        // Should be rate limited now
        assert!(limiter.check_rate_limit().is_err());

        // Wait for refill (1 second + margin)
        thread::sleep(Duration::from_millis(1100));

        // Should work again
        assert!(limiter.check_rate_limit().is_ok());
    }

    #[test]
    fn test_rate_limiter_with_both_limits() {
        // Test with both per-connection and global limits
        let limiter = RateLimiter::new(10, 5);

        // First 5 should succeed (limited by global)
        for i in 0..5 {
            assert!(
                limiter.check_rate_limit().is_ok(),
                "Request {} should succeed",
                i
            );
        }

        // 6th should fail (global limit hit first)
        assert!(limiter.check_rate_limit().is_err());
    }
}

use crate::{Error, Result};
use std::time::Duration;

/// Configuration for retry behavior with exponential backoff.
///
/// Defines how many times to retry an operation and how long to wait
/// between attempts, using exponential backoff with configurable parameters.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (not including the initial attempt)
    pub max_attempts: u32,

    /// Initial backoff duration in milliseconds
    pub initial_backoff_ms: u64,

    /// Maximum backoff duration in milliseconds
    pub max_backoff_ms: u64,

    /// Multiplier applied to backoff after each retry
    pub backoff_multiplier: f64,
}

impl RetryPolicy {
    /// Creates a new retry policy with the specified parameters.
    pub fn new(
        max_attempts: u32,
        initial_backoff_ms: u64,
        max_backoff_ms: u64,
        backoff_multiplier: f64,
    ) -> Self {
        Self {
            max_attempts,
            initial_backoff_ms,
            max_backoff_ms,
            backoff_multiplier,
        }
    }

    /// Returns a policy with no retries.
    pub fn no_retry() -> Self {
        Self {
            max_attempts: 0,
            initial_backoff_ms: 0,
            max_backoff_ms: 0,
            backoff_multiplier: 1.0,
        }
    }

    /// Returns a policy optimized for quick transient failures.
    pub fn fast() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 10,
            max_backoff_ms: 100,
            backoff_multiplier: 2.0,
        }
    }

    /// Returns a policy for longer-running retry scenarios.
    pub fn standard() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            backoff_multiplier: 2.0,
        }
    }

    /// Calculates the backoff duration for a given attempt number (0-indexed).
    pub fn backoff_duration(&self, attempt: u32) -> Duration {
        let backoff_ms = (self.initial_backoff_ms as f64
            * self.backoff_multiplier.powi(attempt as i32))
            .min(self.max_backoff_ms as f64) as u64;
        Duration::from_millis(backoff_ms)
    }
}

impl Default for RetryPolicy {
    /// Returns a sensible default retry policy (same as `standard()`).
    fn default() -> Self {
        Self::standard()
    }
}

/// Retries an operation according to the specified policy.
///
/// Only retries if the error is retryable (as determined by `Error::is_retryable()`).
/// Uses exponential backoff between retry attempts.
///
/// # Examples
///
/// ```no_run
/// use kstone_core::retry::{retry_with_policy, RetryPolicy};
/// use kstone_core::{Error, Result};
///
/// fn flaky_operation() -> Result<String> {
///     // Simulate a transient failure
///     Err(Error::Io(std::io::Error::new(
///         std::io::ErrorKind::TimedOut,
///         "timeout"
///     )))
/// }
///
/// let policy = RetryPolicy::fast();
/// let result = retry_with_policy(&policy, || flaky_operation());
/// ```
pub fn retry_with_policy<F, T>(
    policy: &RetryPolicy,
    mut operation: F,
) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    let mut last_error = None;

    // Initial attempt (attempt 0)
    match operation() {
        Ok(result) => return Ok(result),
        Err(e) => {
            if !e.is_retryable() {
                // Non-retryable error, fail immediately
                return Err(e);
            }
            last_error = Some(e);
        }
    }

    // Retry attempts
    for attempt in 0..policy.max_attempts {
        let backoff = policy.backoff_duration(attempt);
        std::thread::sleep(backoff);

        match operation() {
            Ok(result) => return Ok(result),
            Err(e) => {
                if !e.is_retryable() {
                    // Non-retryable error, fail immediately
                    return Err(e);
                }
                last_error = Some(e);
            }
        }
    }

    // All retries exhausted, return last error
    Err(last_error.unwrap_or_else(|| {
        Error::Internal("retry exhausted without error".to_string())
    }))
}

/// Retries an operation with the default policy.
///
/// This is a convenience function that uses `RetryPolicy::default()`.
///
/// # Examples
///
/// ```no_run
/// use kstone_core::retry::retry;
/// use kstone_core::Result;
///
/// fn my_operation() -> Result<()> {
///     Ok(())
/// }
///
/// let result = retry(|| my_operation());
/// ```
pub fn retry<F, T>(operation: F) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    retry_with_policy(&RetryPolicy::default(), operation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_retry_policy_default() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 5);
        assert_eq!(policy.initial_backoff_ms, 100);
        assert_eq!(policy.max_backoff_ms, 5000);
        assert_eq!(policy.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_retry_policy_fast() {
        let policy = RetryPolicy::fast();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.initial_backoff_ms, 10);
    }

    #[test]
    fn test_retry_policy_no_retry() {
        let policy = RetryPolicy::no_retry();
        assert_eq!(policy.max_attempts, 0);
    }

    #[test]
    fn test_backoff_duration_exponential() {
        let policy = RetryPolicy::new(5, 100, 10000, 2.0);

        assert_eq!(policy.backoff_duration(0).as_millis(), 100);
        assert_eq!(policy.backoff_duration(1).as_millis(), 200);
        assert_eq!(policy.backoff_duration(2).as_millis(), 400);
        assert_eq!(policy.backoff_duration(3).as_millis(), 800);
    }

    #[test]
    fn test_backoff_duration_respects_max() {
        let policy = RetryPolicy::new(10, 100, 500, 2.0);

        // Should cap at max_backoff_ms
        assert_eq!(policy.backoff_duration(5).as_millis(), 500);
        assert_eq!(policy.backoff_duration(10).as_millis(), 500);
    }

    #[test]
    fn test_retry_succeeds_immediately() {
        let policy = RetryPolicy::fast();
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_policy(&policy, || {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;
            Ok::<i32, Error>(42)
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(*counter.lock().unwrap(), 1); // Only called once
    }

    #[test]
    fn test_retry_succeeds_after_failures() {
        let policy = RetryPolicy::fast();
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_policy(&policy, || {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;

            if *count < 3 {
                // Fail first 2 attempts with retryable error
                Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "timeout"
                )))
            } else {
                Ok::<i32, Error>(42)
            }
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(*counter.lock().unwrap(), 3); // Called 3 times total
    }

    #[test]
    fn test_retry_fails_after_max_attempts() {
        let policy = RetryPolicy::new(2, 1, 10, 1.5); // Only 2 retries
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_policy(&policy, || {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;

            // Always fail with retryable error
            Err::<i32, Error>(Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "timeout"
            )))
        });

        assert!(result.is_err());
        // Initial attempt + 2 retries = 3 calls
        assert_eq!(*counter.lock().unwrap(), 3);
    }

    #[test]
    fn test_retry_does_not_retry_non_retryable_error() {
        let policy = RetryPolicy::fast();
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_policy(&policy, || {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;

            // Fail with non-retryable error
            Err::<i32, Error>(Error::InvalidArgument("bad input".to_string()))
        });

        assert!(result.is_err());
        assert_eq!(*counter.lock().unwrap(), 1); // Only called once, no retries

        match result {
            Err(Error::InvalidArgument(_)) => (),
            _ => panic!("Expected InvalidArgument error"),
        }
    }

    #[test]
    fn test_retry_no_retry_policy() {
        let policy = RetryPolicy::no_retry();
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        let result = retry_with_policy(&policy, || {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;

            // Fail with retryable error
            Err::<i32, Error>(Error::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "timeout"
            )))
        });

        assert!(result.is_err());
        assert_eq!(*counter.lock().unwrap(), 1); // Only initial attempt, no retries
    }

    #[test]
    fn test_retry_helper_function() {
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = counter.clone();

        let result = retry(|| {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;

            if *count < 2 {
                Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "timeout"
                )))
            } else {
                Ok::<i32, Error>(100)
            }
        });

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);
    }
}

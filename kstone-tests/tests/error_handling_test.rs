use kstone_core::{Error, Result, retry::{RetryPolicy, retry_with_policy}};
use std::sync::{Arc, Mutex};
use std::io;

/// Test error codes are correct and stable
#[test]
fn test_error_codes() {
    assert_eq!(
        Error::Io(io::Error::new(io::ErrorKind::NotFound, "test")).code(),
        "IO_ERROR"
    );
    assert_eq!(
        Error::Corruption("test".to_string()).code(),
        "CORRUPTION"
    );
    assert_eq!(
        Error::NotFound("key".to_string()).code(),
        "NOT_FOUND"
    );
    assert_eq!(
        Error::InvalidArgument("arg".to_string()).code(),
        "INVALID_ARGUMENT"
    );
    assert_eq!(
        Error::AlreadyExists("path".to_string()).code(),
        "ALREADY_EXISTS"
    );
    assert_eq!(Error::WalFull.code(), "WAL_FULL");
    assert_eq!(Error::ChecksumMismatch.code(), "CHECKSUM_MISMATCH");
    assert_eq!(
        Error::Internal("msg".to_string()).code(),
        "INTERNAL_ERROR"
    );
    assert_eq!(
        Error::EncryptionError("msg".to_string()).code(),
        "ENCRYPTION_ERROR"
    );
    assert_eq!(
        Error::CompressionError("msg".to_string()).code(),
        "COMPRESSION_ERROR"
    );
    assert_eq!(
        Error::ManifestCorruption("msg".to_string()).code(),
        "MANIFEST_CORRUPTION"
    );
    assert_eq!(
        Error::CompactionError("msg".to_string()).code(),
        "COMPACTION_ERROR"
    );
    assert_eq!(
        Error::StripeError("msg".to_string()).code(),
        "STRIPE_ERROR"
    );
    assert_eq!(
        Error::InvalidExpression("expr".to_string()).code(),
        "INVALID_EXPRESSION"
    );
    assert_eq!(
        Error::ConditionalCheckFailed("cond".to_string()).code(),
        "CONDITIONAL_CHECK_FAILED"
    );
    assert_eq!(
        Error::TransactionCanceled("tx".to_string()).code(),
        "TRANSACTION_CANCELED"
    );
    assert_eq!(
        Error::InvalidQuery("query".to_string()).code(),
        "INVALID_QUERY"
    );
    assert_eq!(
        Error::ResourceExhausted("resource".to_string()).code(),
        "RESOURCE_EXHAUSTED"
    );
}

/// Test is_retryable classification
#[test]
fn test_error_retryability() {
    // Retryable errors (transient)
    assert!(Error::Io(io::Error::new(io::ErrorKind::TimedOut, "test")).is_retryable());
    assert!(Error::WalFull.is_retryable());
    assert!(Error::ResourceExhausted("memory".to_string()).is_retryable());
    assert!(Error::CompactionError("failed".to_string()).is_retryable());
    assert!(Error::StripeError("locked".to_string()).is_retryable());

    // Non-retryable errors (logical/permanent)
    assert!(!Error::Corruption("data".to_string()).is_retryable());
    assert!(!Error::NotFound("key".to_string()).is_retryable());
    assert!(!Error::InvalidArgument("bad".to_string()).is_retryable());
    assert!(!Error::AlreadyExists("path".to_string()).is_retryable());
    assert!(!Error::ChecksumMismatch.is_retryable());
    assert!(!Error::Internal("error".to_string()).is_retryable());
    assert!(!Error::EncryptionError("key".to_string()).is_retryable());
    assert!(!Error::CompressionError("bad".to_string()).is_retryable());
    assert!(!Error::ManifestCorruption("corrupt".to_string()).is_retryable());
    assert!(!Error::InvalidExpression("syntax".to_string()).is_retryable());
    assert!(!Error::ConditionalCheckFailed("failed".to_string()).is_retryable());
    assert!(!Error::TransactionCanceled("aborted".to_string()).is_retryable());
    assert!(!Error::InvalidQuery("sql".to_string()).is_retryable());
}

/// Test with_context() method
#[test]
fn test_error_with_context() {
    let original = Error::Io(io::Error::new(io::ErrorKind::NotFound, "file.txt"));
    let with_context = original.with_context("failed to open database");

    match with_context {
        Error::Internal(msg) => {
            assert!(msg.contains("failed to open database"));
            assert!(msg.contains("file.txt"));
        }
        _ => panic!("Expected Internal error"),
    }
}

/// Test context propagation through multiple layers
#[test]
fn test_error_context_propagation() {
    fn layer3() -> Result<()> {
        Err(Error::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "access denied"
        )))
    }

    fn layer2() -> Result<()> {
        layer3().map_err(|e| e.with_context("layer2: write failed"))
    }

    fn layer1() -> Result<()> {
        layer2().map_err(|e| e.with_context("layer1: operation failed"))
    }

    let result = layer1();
    assert!(result.is_err());

    match result.unwrap_err() {
        Error::Internal(msg) => {
            assert!(msg.contains("layer1: operation failed"));
            assert!(msg.contains("layer2: write failed"));
            assert!(msg.contains("access denied"));
        }
        _ => panic!("Expected Internal error with context"),
    }
}

/// Test retry logic with simulated transient failures
#[test]
fn test_retry_with_transient_failures() {
    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    let policy = RetryPolicy::new(3, 1, 10, 1.5);

    let result = retry_with_policy(&policy, || {
        let mut count = counter_clone.lock().unwrap();
        *count += 1;

        if *count < 3 {
            // Fail first 2 attempts with retryable error
            Err(Error::Io(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "connection reset"
            )))
        } else {
            // Succeed on 3rd attempt
            Ok::<String, Error>("success".to_string())
        }
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
    assert_eq!(*counter.lock().unwrap(), 3);
}

/// Test retry stops on non-retryable error
#[test]
fn test_retry_stops_on_non_retryable_error() {
    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    let policy = RetryPolicy::new(5, 1, 10, 1.5);

    let result = retry_with_policy(&policy, || {
        let mut count = counter_clone.lock().unwrap();
        *count += 1;

        // Always fail with non-retryable error
        Err::<String, Error>(Error::InvalidArgument("bad input".to_string()))
    });

    assert!(result.is_err());
    // Should only be called once (no retries for non-retryable errors)
    assert_eq!(*counter.lock().unwrap(), 1);

    match result.unwrap_err() {
        Error::InvalidArgument(_) => (),
        _ => panic!("Expected InvalidArgument error"),
    }
}

/// Test retry exhausts max attempts
#[test]
fn test_retry_exhausts_attempts() {
    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    let policy = RetryPolicy::new(2, 1, 5, 1.5); // Only 2 retries

    let result = retry_with_policy(&policy, || {
        let mut count = counter_clone.lock().unwrap();
        *count += 1;

        // Always fail with retryable error
        Err::<String, Error>(Error::WalFull)
    });

    assert!(result.is_err());
    // Initial attempt + 2 retries = 3 total calls
    assert_eq!(*counter.lock().unwrap(), 3);

    match result.unwrap_err() {
        Error::WalFull => (),
        _ => panic!("Expected WalFull error"),
    }
}

/// Test mixed retryable and non-retryable errors
#[test]
fn test_retry_with_mixed_errors() {
    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    let policy = RetryPolicy::new(5, 1, 10, 1.5);

    let result = retry_with_policy(&policy, || {
        let mut count = counter_clone.lock().unwrap();
        *count += 1;

        match *count {
            1 => Err(Error::ResourceExhausted("memory".to_string())), // Retryable
            2 => Err(Error::Io(io::Error::new(io::ErrorKind::TimedOut, "timeout"))), // Retryable
            3 => Err(Error::NotFound("key".to_string())), // Non-retryable - should stop here
            _ => Ok::<String, Error>("should not reach".to_string()),
        }
    });

    assert!(result.is_err());
    // Should stop at attempt 3 (non-retryable error)
    assert_eq!(*counter.lock().unwrap(), 3);

    match result.unwrap_err() {
        Error::NotFound(_) => (),
        _ => panic!("Expected NotFound error"),
    }
}

/// Test retry with different policies
#[test]
fn test_retry_policies() {
    // Test fast policy
    let fast = RetryPolicy::fast();
    assert_eq!(fast.max_attempts, 3);
    assert_eq!(fast.initial_backoff_ms, 10);
    assert_eq!(fast.max_backoff_ms, 100);

    // Test standard policy (default)
    let standard = RetryPolicy::standard();
    assert_eq!(standard.max_attempts, 5);
    assert_eq!(standard.initial_backoff_ms, 100);
    assert_eq!(standard.max_backoff_ms, 5000);

    // Test no retry policy
    let no_retry = RetryPolicy::no_retry();
    assert_eq!(no_retry.max_attempts, 0);
}

/// Test exponential backoff via retry behavior
#[test]
fn test_exponential_backoff_behavior() {
    // We test backoff indirectly by verifying policy parameters
    let policy = RetryPolicy::new(5, 100, 10000, 2.0);

    assert_eq!(policy.initial_backoff_ms, 100);
    assert_eq!(policy.max_backoff_ms, 10000);
    assert_eq!(policy.backoff_multiplier, 2.0);

    // Backoff calculation is tested implicitly through retry_with_policy behavior
    // The internal backoff_duration method is private, which is correct design
}

/// Test retry with resource exhausted error
#[test]
fn test_retry_resource_exhausted() {
    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    let policy = RetryPolicy::fast();

    let result = retry_with_policy(&policy, || {
        let mut count = counter_clone.lock().unwrap();
        *count += 1;

        if *count < 2 {
            Err(Error::ResourceExhausted("connection pool full".to_string()))
        } else {
            Ok::<i32, Error>(42)
        }
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(*counter.lock().unwrap(), 2);
}

/// Test error code stability (important for clients)
#[test]
fn test_error_code_stability() {
    // These codes must never change as they are part of the public API
    let expected_codes = vec![
        ("IO_ERROR", Error::Io(io::Error::new(io::ErrorKind::Other, "test"))),
        ("CORRUPTION", Error::Corruption("test".into())),
        ("NOT_FOUND", Error::NotFound("test".into())),
        ("INVALID_ARGUMENT", Error::InvalidArgument("test".into())),
        ("ALREADY_EXISTS", Error::AlreadyExists("test".into())),
        ("WAL_FULL", Error::WalFull),
        ("CHECKSUM_MISMATCH", Error::ChecksumMismatch),
        ("INTERNAL_ERROR", Error::Internal("test".into())),
        ("ENCRYPTION_ERROR", Error::EncryptionError("test".into())),
        ("COMPRESSION_ERROR", Error::CompressionError("test".into())),
        ("MANIFEST_CORRUPTION", Error::ManifestCorruption("test".into())),
        ("COMPACTION_ERROR", Error::CompactionError("test".into())),
        ("STRIPE_ERROR", Error::StripeError("test".into())),
        ("INVALID_EXPRESSION", Error::InvalidExpression("test".into())),
        ("CONDITIONAL_CHECK_FAILED", Error::ConditionalCheckFailed("test".into())),
        ("TRANSACTION_CANCELED", Error::TransactionCanceled("test".into())),
        ("INVALID_QUERY", Error::InvalidQuery("test".into())),
        ("RESOURCE_EXHAUSTED", Error::ResourceExhausted("test".into())),
    ];

    for (expected_code, error) in expected_codes {
        assert_eq!(
            error.code(),
            expected_code,
            "Error code mismatch for {:?}",
            error
        );
    }
}

/// Test that retryable errors are properly identified
#[test]
fn test_retryable_error_coverage() {
    // All retryable errors
    let retryable = vec![
        Error::Io(io::Error::new(io::ErrorKind::Other, "test")),
        Error::WalFull,
        Error::ResourceExhausted("test".into()),
        Error::CompactionError("test".into()),
        Error::StripeError("test".into()),
    ];

    for error in retryable {
        assert!(
            error.is_retryable(),
            "Error should be retryable: {:?}",
            error
        );
    }

    // All non-retryable errors
    let non_retryable = vec![
        Error::Corruption("test".into()),
        Error::NotFound("test".into()),
        Error::InvalidArgument("test".into()),
        Error::AlreadyExists("test".into()),
        Error::ChecksumMismatch,
        Error::Internal("test".into()),
        Error::EncryptionError("test".into()),
        Error::CompressionError("test".into()),
        Error::ManifestCorruption("test".into()),
        Error::InvalidExpression("test".into()),
        Error::ConditionalCheckFailed("test".into()),
        Error::TransactionCanceled("test".into()),
        Error::InvalidQuery("test".into()),
    ];

    for error in non_retryable {
        assert!(
            !error.is_retryable(),
            "Error should not be retryable: {:?}",
            error
        );
    }
}

/// Test retry with successful first attempt
#[test]
fn test_retry_immediate_success() {
    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    let policy = RetryPolicy::standard();

    let result = retry_with_policy(&policy, || {
        let mut count = counter_clone.lock().unwrap();
        *count += 1;
        Ok::<String, Error>("immediate success".to_string())
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "immediate success");
    // Should only be called once (no retries needed)
    assert_eq!(*counter.lock().unwrap(), 1);
}

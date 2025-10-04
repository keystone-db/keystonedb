use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Corruption detected: {0}")]
    Corruption(String),

    #[error("Key not found: {0}")]
    NotFound(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Database already exists: {0}")]
    AlreadyExists(String),

    #[error("WAL full")]
    WalFull,

    #[error("Checksum mismatch")]
    ChecksumMismatch,

    #[error("Internal error: {0}")]
    Internal(String),

    // Phase 1 additions
    #[error("Encryption error: {0}")]
    EncryptionError(String),

    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("Manifest corruption: {0}")]
    ManifestCorruption(String),

    #[error("Compaction error: {0}")]
    CompactionError(String),

    #[error("Stripe error: {0}")]
    StripeError(String),

    // Phase 2.3 additions
    #[error("Invalid expression: {0}")]
    InvalidExpression(String),

    // Phase 2.5 additions
    #[error("Conditional check failed: {0}")]
    ConditionalCheckFailed(String),

    // Phase 2.7 additions
    #[error("Transaction canceled: {0}")]
    TransactionCanceled(String),

    // Phase 4 additions
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    // Phase 8 additions
    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),
}

impl Error {
    /// Returns a stable error code for this error variant.
    /// These codes are stable and can be used by clients for error classification.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io(_) => "IO_ERROR",
            Error::Corruption(_) => "CORRUPTION",
            Error::NotFound(_) => "NOT_FOUND",
            Error::InvalidArgument(_) => "INVALID_ARGUMENT",
            Error::AlreadyExists(_) => "ALREADY_EXISTS",
            Error::WalFull => "WAL_FULL",
            Error::ChecksumMismatch => "CHECKSUM_MISMATCH",
            Error::Internal(_) => "INTERNAL_ERROR",
            Error::EncryptionError(_) => "ENCRYPTION_ERROR",
            Error::CompressionError(_) => "COMPRESSION_ERROR",
            Error::ManifestCorruption(_) => "MANIFEST_CORRUPTION",
            Error::CompactionError(_) => "COMPACTION_ERROR",
            Error::StripeError(_) => "STRIPE_ERROR",
            Error::InvalidExpression(_) => "INVALID_EXPRESSION",
            Error::ConditionalCheckFailed(_) => "CONDITIONAL_CHECK_FAILED",
            Error::TransactionCanceled(_) => "TRANSACTION_CANCELED",
            Error::InvalidQuery(_) => "INVALID_QUERY",
            Error::ResourceExhausted(_) => "RESOURCE_EXHAUSTED",
        }
    }

    /// Returns true if this error is potentially retryable.
    ///
    /// Transient errors like IO errors are retryable, while logical errors
    /// like InvalidArgument or ConditionalCheckFailed are not.
    pub fn is_retryable(&self) -> bool {
        match self {
            // Retryable errors (transient)
            Error::Io(_) => true,
            Error::WalFull => true,
            Error::ResourceExhausted(_) => true,
            Error::CompactionError(_) => true,
            Error::StripeError(_) => true,

            // Non-retryable errors (logical/permanent)
            Error::Corruption(_) => false,
            Error::NotFound(_) => false,
            Error::InvalidArgument(_) => false,
            Error::AlreadyExists(_) => false,
            Error::ChecksumMismatch => false,
            Error::Internal(_) => false,
            Error::EncryptionError(_) => false,
            Error::CompressionError(_) => false,
            Error::ManifestCorruption(_) => false,
            Error::InvalidExpression(_) => false,
            Error::ConditionalCheckFailed(_) => false,
            Error::TransactionCanceled(_) => false,
            Error::InvalidQuery(_) => false,
        }
    }

    /// Adds context to an error by wrapping it in an Internal error.
    ///
    /// This is useful for adding operation context to errors that propagate
    /// from lower layers.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use kstone_core::Error;
    ///
    /// fn write_data() -> Result<(), Error> {
    ///     // Some operation that might fail
    ///     Err(Error::Io(std::io::Error::new(
    ///         std::io::ErrorKind::NotFound,
    ///         "file not found"
    ///     )))
    /// }
    ///
    /// fn save_record() -> Result<(), Error> {
    ///     write_data().map_err(|e| e.with_context("failed to save record"))
    /// }
    /// ```
    pub fn with_context(self, context: &str) -> Error {
        Error::Internal(format!("{}: {}", context, self))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

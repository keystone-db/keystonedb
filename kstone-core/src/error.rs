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
}

pub type Result<T> = std::result::Result<T, Error>;

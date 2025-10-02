/// Error types for the KeystoneDB client
use thiserror::Error;
use tonic::Status;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Condition check failed: {0}")]
    ConditionCheckFailed(String),

    #[error("Connection error: {0}")]
    ConnectionError(String),

    #[error("Server unavailable: {0}")]
    Unavailable(String),

    #[error("Request timeout: {0}")]
    Timeout(String),

    #[error("Internal server error: {0}")]
    InternalError(String),

    #[error("Data corruption: {0}")]
    DataCorruption(String),

    #[error("Transaction aborted: {0}")]
    TransactionAborted(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Resource exhausted: {0}")]
    ResourceExhausted(String),

    #[error("Unimplemented: {0}")]
    Unimplemented(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// Convert gRPC Status to ClientError
impl From<Status> for ClientError {
    fn from(status: Status) -> Self {
        let msg = status.message().to_string();

        match status.code() {
            tonic::Code::NotFound => ClientError::NotFound(msg),
            tonic::Code::InvalidArgument => ClientError::InvalidArgument(msg),
            tonic::Code::FailedPrecondition => ClientError::ConditionCheckFailed(msg),
            tonic::Code::Unavailable => ClientError::Unavailable(msg),
            tonic::Code::DeadlineExceeded => ClientError::Timeout(msg),
            tonic::Code::Internal => ClientError::InternalError(msg),
            tonic::Code::DataLoss => ClientError::DataCorruption(msg),
            tonic::Code::Aborted => ClientError::TransactionAborted(msg),
            tonic::Code::AlreadyExists => ClientError::AlreadyExists(msg),
            tonic::Code::ResourceExhausted => ClientError::ResourceExhausted(msg),
            tonic::Code::Unimplemented => ClientError::Unimplemented(msg),
            tonic::Code::PermissionDenied => ClientError::PermissionDenied(msg),
            _ => ClientError::Unknown(msg),
        }
    }
}

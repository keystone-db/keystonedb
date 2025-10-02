/// KeystoneDB gRPC Client Library
///
/// This crate provides a Rust client for connecting to KeystoneDB gRPC servers.

pub mod error;
pub mod client;
pub mod convert;
pub mod query;
pub mod scan;
pub mod batch;
pub mod transaction;
pub mod update;
pub mod partiql;

// Re-export key types
pub use client::Client;
pub use error::{ClientError, Result};
pub use kstone_core::{Item, Value};
pub use query::{RemoteQuery, RemoteQueryResponse};
pub use scan::{RemoteScan, RemoteScanResponse};
pub use batch::{RemoteBatchGetRequest, RemoteBatchGetResponse, RemoteBatchWriteRequest, RemoteBatchWriteResponse};
pub use transaction::{RemoteTransactGetRequest, RemoteTransactGetResponse, RemoteTransactWriteRequest};
pub use update::{RemoteUpdate, RemoteUpdateResponse};
pub use partiql::RemoteExecuteStatementResponse;

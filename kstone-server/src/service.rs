/// gRPC service implementation for KeystoneDB
///
/// This module implements the KeystoneDb gRPC service trait, wiring the
/// protocol buffer interface to the KeystoneDB Database API.

use bytes::Bytes;
use kstone_api::Database;
use kstone_core::Error as KsError;
use kstone_proto::{self as proto, keystone_db_server::KeystoneDb};
use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::convert::*;

/// KeystoneDB gRPC service implementation
pub struct KeystoneService {
    db: Arc<Database>,
}

impl KeystoneService {
    /// Create a new KeystoneService wrapping a Database
    pub fn new(db: Database) -> Self {
        Self { db: Arc::new(db) }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Map KeystoneDB errors to gRPC Status
fn map_error(err: KsError) -> Status {
    match err {
        KsError::NotFound(msg) => Status::not_found(msg),
        KsError::InvalidQuery(msg) => Status::invalid_argument(msg),
        KsError::InvalidArgument(msg) => Status::invalid_argument(msg),
        KsError::InvalidExpression(msg) => Status::invalid_argument(msg),
        KsError::ConditionalCheckFailed(msg) => Status::failed_precondition(msg),
        KsError::Io(e) => Status::internal(format!("IO error: {}", e)),
        KsError::Corruption(msg) => Status::data_loss(format!("Data corruption: {}", msg)),
        KsError::ManifestCorruption(msg) => Status::data_loss(format!("Manifest corruption: {}", msg)),
        KsError::TransactionCanceled(msg) => Status::aborted(format!("Transaction canceled: {}", msg)),
        KsError::AlreadyExists(msg) => Status::already_exists(msg),
        KsError::WalFull => Status::resource_exhausted("WAL full"),
        KsError::ChecksumMismatch => Status::data_loss("Checksum mismatch"),
        KsError::Internal(msg) => Status::internal(msg),
        KsError::EncryptionError(msg) => Status::internal(format!("Encryption error: {}", msg)),
        KsError::CompressionError(msg) => Status::internal(format!("Compression error: {}", msg)),
        KsError::CompactionError(msg) => Status::internal(format!("Compaction error: {}", msg)),
        KsError::StripeError(msg) => Status::internal(format!("Stripe error: {}", msg)),
    }
}

/// Convert proto Value to bytes for use as key
fn value_to_key_bytes(value: proto::Value) -> Result<Bytes, Status> {
    use proto::value::Value as ProtoValueEnum;

    let value_enum = value
        .value
        .ok_or_else(|| Status::invalid_argument("Value field is missing"))?;

    match value_enum {
        ProtoValueEnum::BinaryValue(b) => Ok(Bytes::from(b)),
        ProtoValueEnum::StringValue(s) => Ok(Bytes::from(s.into_bytes())),
        ProtoValueEnum::NumberValue(n) => Ok(Bytes::from(n.into_bytes())),
        _ => Err(Status::invalid_argument(
            "Sort key must be binary, string, or number",
        )),
    }
}

/// Apply sort key condition to query builder
fn apply_sort_key_condition(
    query: kstone_api::Query,
    sk_cond: proto::SortKeyCondition,
) -> Result<kstone_api::Query, Status> {
    use proto::sort_key_condition::Condition;

    let condition = sk_cond
        .condition
        .ok_or_else(|| Status::invalid_argument("Sort key condition is required"))?;

    match condition {
        Condition::EqualTo(v) => {
            let sk = value_to_key_bytes(v)?;
            Ok(query.sk_eq(&sk))
        }
        Condition::LessThan(v) => {
            let sk = value_to_key_bytes(v)?;
            Ok(query.sk_lt(&sk))
        }
        Condition::LessThanOrEqual(v) => {
            let sk = value_to_key_bytes(v)?;
            Ok(query.sk_lte(&sk))
        }
        Condition::GreaterThan(v) => {
            let sk = value_to_key_bytes(v)?;
            Ok(query.sk_gt(&sk))
        }
        Condition::GreaterThanOrEqual(v) => {
            let sk = value_to_key_bytes(v)?;
            Ok(query.sk_gte(&sk))
        }
        Condition::BeginsWith(v) => {
            let sk = value_to_key_bytes(v)?;
            Ok(query.sk_begins_with(&sk))
        }
        Condition::Between(between) => {
            let sk1 = value_to_key_bytes(
                between
                    .lower
                    .ok_or_else(|| Status::invalid_argument("Between lower value required"))?,
            )?;
            let sk2 = value_to_key_bytes(
                between
                    .upper
                    .ok_or_else(|| Status::invalid_argument("Between upper value required"))?,
            )?;
            Ok(query.sk_between(&sk1, &sk2))
        }
    }
}

// ============================================================================
// gRPC Service Implementation
// ============================================================================

#[tonic::async_trait]
impl KeystoneDb for KeystoneService {
    /// Put an item into the database
    async fn put(
        &self,
        request: Request<proto::PutRequest>,
    ) -> Result<Response<proto::PutResponse>, Status> {
        let req = request.into_inner();

        // Convert key
        let (pk, sk) = proto_key_to_ks(proto::Key {
            partition_key: req.partition_key,
            sort_key: req.sort_key,
        });

        // Convert item
        let item = proto_item_to_ks(
            req.item
                .ok_or_else(|| Status::invalid_argument("Item required"))?,
        )?;

        // TODO: Support condition_expression (needs expression context)
        if req.condition_expression.is_some() {
            return Err(Status::unimplemented(
                "Conditional put not yet supported in server",
            ));
        }

        // Execute put operation (blocking DB call in spawn_blocking)
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            if let Some(sk_bytes) = sk {
                db.put_with_sk(&pk, &sk_bytes, item)?;
            } else {
                db.put(&pk, item)?;
            }
            Ok::<_, KsError>(())
        })
        .await
        .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
        .map_err(map_error)?;

        Ok(Response::new(proto::PutResponse {
            success: true,
            error: None,
        }))
    }

    /// Get an item from the database
    async fn get(
        &self,
        request: Request<proto::GetRequest>,
    ) -> Result<Response<proto::GetResponse>, Status> {
        let req = request.into_inner();

        // Convert key
        let (pk, sk) = proto_key_to_ks(proto::Key {
            partition_key: req.partition_key,
            sort_key: req.sort_key,
        });

        // Execute get operation
        let db = Arc::clone(&self.db);
        let item_opt = tokio::task::spawn_blocking(move || {
            if let Some(sk_bytes) = sk {
                db.get_with_sk(&pk, &sk_bytes)
            } else {
                db.get(&pk)
            }
        })
        .await
        .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
        .map_err(map_error)?;

        Ok(Response::new(proto::GetResponse {
            item: item_opt.map(|item| ks_item_to_proto(&item)),
            error: None,
        }))
    }

    /// Delete an item from the database
    async fn delete(
        &self,
        request: Request<proto::DeleteRequest>,
    ) -> Result<Response<proto::DeleteResponse>, Status> {
        let req = request.into_inner();

        // Convert key
        let (pk, sk) = proto_key_to_ks(proto::Key {
            partition_key: req.partition_key,
            sort_key: req.sort_key,
        });

        // TODO: Support condition_expression (needs expression context)
        if req.condition_expression.is_some() {
            return Err(Status::unimplemented(
                "Conditional delete not yet supported in server",
            ));
        }

        // Execute delete operation
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            if let Some(sk_bytes) = sk {
                db.delete_with_sk(&pk, &sk_bytes)?;
            } else {
                db.delete(&pk)?;
            }
            Ok::<_, KsError>(())
        })
        .await
        .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
        .map_err(map_error)?;

        Ok(Response::new(proto::DeleteResponse {
            success: true,
            error: None,
        }))
    }

    // TODO: Implement remaining methods
    // - query
    // - scan
    // - batch_get
    // - batch_write
    // - transact_get
    // - transact_write
    // - update
    // - execute_statement

    /// Query items by partition key
    async fn query(
        &self,
        request: Request<proto::QueryRequest>,
    ) -> Result<Response<proto::QueryResponse>, Status> {
        let req = request.into_inner();

        // Build query starting with partition key
        let mut query = kstone_api::Query::new(&req.partition_key);

        // Apply sort key condition if present
        if let Some(sk_cond) = req.sort_key_condition {
            query = apply_sort_key_condition(query, sk_cond)?;
        }

        // Apply limit
        if let Some(limit) = req.limit {
            query = query.limit(limit as usize);
        }

        // Apply exclusive start key for pagination
        if let Some(start_key) = req.exclusive_start_key {
            let (pk, sk) = proto_last_key_to_ks(start_key);
            query = query.start_after(&pk, sk.as_deref());
        }

        // Apply scan direction
        if let Some(forward) = req.scan_forward {
            query = query.forward(forward);
        }

        // Apply index name
        if let Some(index_name) = req.index_name {
            query = query.index(index_name);
        }

        // TODO: Support filter_expression (needs expression context)
        if req.filter_expression.is_some() {
            return Err(Status::unimplemented(
                "Filter expressions not yet supported in server",
            ));
        }

        // Execute query
        let db = Arc::clone(&self.db);
        let response = tokio::task::spawn_blocking(move || db.query(query))
            .await
            .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
            .map_err(map_error)?;

        // Convert response to protobuf
        Ok(Response::new(proto::QueryResponse {
            items: response.items.iter().map(ks_item_to_proto).collect(),
            count: response.count as u32,
            scanned_count: response.scanned_count as u32,
            last_evaluated_key: ks_last_key_opt_to_proto(response.last_key),
            error: None,
        }))
    }

    /// Scan items (streaming response)
    type ScanStream = futures::stream::Once<
        futures::future::Ready<Result<proto::ScanResponse, Status>>,
    >;

    async fn scan(
        &self,
        request: Request<proto::ScanRequest>,
    ) -> Result<Response<Self::ScanStream>, Status> {
        let req = request.into_inner();

        // Build scan starting with defaults
        let mut scan = kstone_api::Scan::new();

        // Apply limit
        if let Some(limit) = req.limit {
            scan = scan.limit(limit as usize);
        }

        // Apply exclusive start key for pagination
        if let Some(start_key) = req.exclusive_start_key {
            let (pk, sk) = proto_last_key_to_ks(start_key);
            scan = scan.start_after(&pk, sk.as_deref());
        }

        // Apply parallel scan segments
        if let (Some(segment), Some(total_segments)) = (req.segment, req.total_segments) {
            scan = scan.segment(segment as usize, total_segments as usize);
        }

        // TODO: Support filter_expression (needs expression context)
        if req.filter_expression.is_some() {
            return Err(Status::unimplemented(
                "Filter expressions not yet supported in server",
            ));
        }

        // TODO: Support index_name for GSI/LSI
        if req.index_name.is_some() {
            return Err(Status::unimplemented(
                "Index scans not yet supported in server",
            ));
        }

        // Execute scan
        let db = Arc::clone(&self.db);
        let response = tokio::task::spawn_blocking(move || db.scan(scan))
            .await
            .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
            .map_err(map_error)?;

        // Convert response to protobuf
        let proto_response = proto::ScanResponse {
            items: response.items.iter().map(ks_item_to_proto).collect(),
            count: response.count as u32,
            scanned_count: response.scanned_count as u32,
            last_evaluated_key: ks_last_key_opt_to_proto(response.last_key),
            error: None,
        };

        // Return as a single-item stream
        let stream = futures::stream::once(futures::future::ready(Ok(proto_response)));
        Ok(Response::new(stream))
    }

    /// Batch get multiple items
    async fn batch_get(
        &self,
        request: Request<proto::BatchGetRequest>,
    ) -> Result<Response<proto::BatchGetResponse>, Status> {
        let req = request.into_inner();

        // Convert protobuf keys to core Keys
        let mut batch_request = kstone_api::BatchGetRequest::new();
        for proto_key in req.keys {
            let (pk, sk) = proto_key_to_ks(proto_key);
            if let Some(sk_bytes) = sk {
                batch_request = batch_request.add_key_with_sk(&pk, &sk_bytes);
            } else {
                batch_request = batch_request.add_key(&pk);
            }
        }

        // Execute batch get
        let db = Arc::clone(&self.db);
        let response = tokio::task::spawn_blocking(move || db.batch_get(batch_request))
            .await
            .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
            .map_err(map_error)?;

        // Convert items to protobuf
        let items: Vec<proto::Item> = response
            .items
            .values()
            .map(ks_item_to_proto)
            .collect();

        Ok(Response::new(proto::BatchGetResponse {
            items,
            count: response.items.len() as u32,
            error: None,
        }))
    }

    /// Batch write multiple items
    async fn batch_write(
        &self,
        request: Request<proto::BatchWriteRequest>,
    ) -> Result<Response<proto::BatchWriteResponse>, Status> {
        use proto::write_request::Request as WriteRequestEnum;

        let req = request.into_inner();

        // Build batch write request
        let mut batch_request = kstone_api::BatchWriteRequest::new();

        for write_req in req.writes {
            let request_enum = write_req
                .request
                .ok_or_else(|| Status::invalid_argument("Write request is required"))?;

            match request_enum {
                WriteRequestEnum::Put(put_item) => {
                    let (pk, sk) = proto_key_to_ks(proto::Key {
                        partition_key: put_item.partition_key,
                        sort_key: put_item.sort_key,
                    });
                    let item = proto_item_to_ks(
                        put_item
                            .item
                            .ok_or_else(|| Status::invalid_argument("Item required for put"))?,
                    )?;

                    if let Some(sk_bytes) = sk {
                        batch_request = batch_request.put_with_sk(&pk, &sk_bytes, item);
                    } else {
                        batch_request = batch_request.put(&pk, item);
                    }
                }
                WriteRequestEnum::Delete(delete_key) => {
                    let (pk, sk) = proto_key_to_ks(proto::Key {
                        partition_key: delete_key.partition_key,
                        sort_key: delete_key.sort_key,
                    });

                    if let Some(sk_bytes) = sk {
                        batch_request = batch_request.delete_with_sk(&pk, &sk_bytes);
                    } else {
                        batch_request = batch_request.delete(&pk);
                    }
                }
            }
        }

        // Execute batch write
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.batch_write(batch_request))
            .await
            .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
            .map_err(map_error)?;

        Ok(Response::new(proto::BatchWriteResponse {
            success: true,
            error: None,
        }))
    }

    /// Transactional get
    async fn transact_get(
        &self,
        _request: Request<proto::TransactGetRequest>,
    ) -> Result<Response<proto::TransactGetResponse>, Status> {
        Err(Status::unimplemented("TransactGet not yet implemented"))
    }

    /// Transactional write
    async fn transact_write(
        &self,
        _request: Request<proto::TransactWriteRequest>,
    ) -> Result<Response<proto::TransactWriteResponse>, Status> {
        Err(Status::unimplemented("TransactWrite not yet implemented"))
    }

    /// Update an item
    async fn update(
        &self,
        _request: Request<proto::UpdateRequest>,
    ) -> Result<Response<proto::UpdateResponse>, Status> {
        Err(Status::unimplemented("Update not yet implemented"))
    }

    /// Execute a PartiQL statement
    async fn execute_statement(
        &self,
        _request: Request<proto::ExecuteStatementRequest>,
    ) -> Result<Response<proto::ExecuteStatementResponse>, Status> {
        Err(Status::unimplemented("ExecuteStatement not yet implemented"))
    }
}

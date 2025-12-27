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
use tracing::{error, info, instrument};
use uuid::Uuid;

use crate::convert::*;
use crate::metrics::{RPC_REQUESTS_TOTAL, RPC_DURATION_SECONDS};
use crate::rate_limit::RateLimiter;

/// KeystoneDB gRPC service implementation
pub struct KeystoneService {
    db: Arc<Database>,
    rate_limiter: Arc<RateLimiter>,
}

impl KeystoneService {
    /// Create a new KeystoneService wrapping a Database
    pub fn new(db: Database, rate_limiter: Arc<RateLimiter>) -> Self {
        Self {
            db: Arc::new(db),
            rate_limiter,
        }
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
        KsError::ResourceExhausted(msg) => Status::resource_exhausted(format!("Resource exhausted: {}", msg)),
        _ => Status::internal(format!("Unknown error: {}", err)),
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
    #[instrument(skip(self, request), fields(trace_id, has_sk, has_condition))]
    async fn put(
        &self,
        request: Request<proto::PutRequest>,
    ) -> Result<Response<proto::PutResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

        // Start timing
        let timer = RPC_DURATION_SECONDS.with_label_values(&["put"]).start_timer();

        info!("Received put request");
        let req = request.into_inner();

        // Convert key
        let (pk, sk) = proto_key_to_ks(proto::Key {
            partition_key: req.partition_key.clone(),
            sort_key: req.sort_key.clone(),
        });

        tracing::Span::current().record("has_sk", sk.is_some());
        tracing::Span::current().record("has_condition", req.condition_expression.is_some());

        // Convert item
        let item = proto_item_to_ks(
            req.item
                .ok_or_else(|| Status::invalid_argument("Item required"))?,
        )?;

        // Execute put operation (blocking DB call in spawn_blocking)
        let db = Arc::clone(&self.db);
        let result = tokio::task::spawn_blocking(move || {
            // Check if this is a conditional put
            if let Some(condition_expr) = req.condition_expression {
                // Build expression context from expression_values
                let mut context = kstone_core::expression::ExpressionContext::new();
                for (placeholder, proto_value) in req.expression_values {
                    let value = proto_value_to_ks(proto_value)
                        .map_err(|_| KsError::InvalidExpression(format!("Invalid expression value for {}", placeholder)))?;
                    context = context.with_value(placeholder, value);
                }

                if let Some(sk_bytes) = sk {
                    db.put_conditional_with_sk(&pk, &sk_bytes, item, &condition_expr, context)?;
                } else {
                    db.put_conditional(&pk, item, &condition_expr, context)?;
                }
            } else {
                // Regular put without condition
                if let Some(sk_bytes) = sk {
                    db.put_with_sk(&pk, &sk_bytes, item)?;
                } else {
                    db.put(&pk, item)?;
                }
            }
            Ok::<_, KsError>(())
        })
        .await
        .map_err(|e| Status::internal(format!("Task join error: {}", e)))?;

        match result {
            Ok(_) => {
                timer.observe_duration();
                RPC_REQUESTS_TOTAL.with_label_values(&["put", "success"]).inc();
                info!("Put operation completed successfully");
                Ok(Response::new(proto::PutResponse {
                    success: true,
                    error: None,
                }))
            }
            Err(e) => {
                timer.observe_duration();
                RPC_REQUESTS_TOTAL.with_label_values(&["put", "error"]).inc();
                error!(?e, "Put operation failed");
                Err(map_error(e))
            }
        }
    }

    /// Get an item from the database
    #[instrument(skip(self, request), fields(trace_id, has_sk, found))]
    async fn get(
        &self,
        request: Request<proto::GetRequest>,
    ) -> Result<Response<proto::GetResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

        info!("Received get request");
        let req = request.into_inner();

        // Convert key
        let (pk, sk) = proto_key_to_ks(proto::Key {
            partition_key: req.partition_key,
            sort_key: req.sort_key,
        });

        tracing::Span::current().record("has_sk", sk.is_some());

        // Execute get operation
        let db = Arc::clone(&self.db);
        let result = tokio::task::spawn_blocking(move || {
            if let Some(sk_bytes) = sk {
                db.get_with_sk(&pk, &sk_bytes)
            } else {
                db.get(&pk)
            }
        })
        .await
        .map_err(|e| Status::internal(format!("Task join error: {}", e)))?;

        match result {
            Ok(item_opt) => {
                tracing::Span::current().record("found", item_opt.is_some());
                info!("Get operation completed");
                Ok(Response::new(proto::GetResponse {
                    item: item_opt.map(|item| ks_item_to_proto(&item)),
                    error: None,
                }))
            }
            Err(e) => {
                error!(?e, "Get operation failed");
                Err(map_error(e))
            }
        }
    }

    /// Delete an item from the database
    #[instrument(skip(self, request), fields(trace_id))]
    async fn delete(
        &self,
        request: Request<proto::DeleteRequest>,
    ) -> Result<Response<proto::DeleteResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

        let req = request.into_inner();

        // Convert key
        let (pk, sk) = proto_key_to_ks(proto::Key {
            partition_key: req.partition_key,
            sort_key: req.sort_key,
        });

        // Execute delete operation
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || {
            // Check if this is a conditional delete
            if let Some(condition_expr) = req.condition_expression {
                // Build expression context from expression_values
                let mut context = kstone_core::expression::ExpressionContext::new();
                for (placeholder, proto_value) in req.expression_values {
                    let value = proto_value_to_ks(proto_value)
                        .map_err(|_| KsError::InvalidExpression(format!("Invalid expression value for {}", placeholder)))?;
                    context = context.with_value(placeholder, value);
                }

                if let Some(sk_bytes) = sk {
                    db.delete_conditional_with_sk(&pk, &sk_bytes, &condition_expr, context)?;
                } else {
                    db.delete_conditional(&pk, &condition_expr, context)?;
                }
            } else {
                // Regular delete without condition
                if let Some(sk_bytes) = sk {
                    db.delete_with_sk(&pk, &sk_bytes)?;
                } else {
                    db.delete(&pk)?;
                }
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
    #[instrument(skip(self, request), fields(trace_id))]
    async fn query(
        &self,
        request: Request<proto::QueryRequest>,
    ) -> Result<Response<proto::QueryResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

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

    #[instrument(skip(self, request), fields(trace_id))]
    async fn scan(
        &self,
        request: Request<proto::ScanRequest>,
    ) -> Result<Response<Self::ScanStream>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

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
    #[instrument(skip(self, request), fields(trace_id))]
    async fn batch_get(
        &self,
        request: Request<proto::BatchGetRequest>,
    ) -> Result<Response<proto::BatchGetResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

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
    #[instrument(skip(self, request), fields(trace_id))]
    async fn batch_write(
        &self,
        request: Request<proto::BatchWriteRequest>,
    ) -> Result<Response<proto::BatchWriteResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

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
    #[instrument(skip(self, request), fields(trace_id))]
    async fn transact_get(
        &self,
        request: Request<proto::TransactGetRequest>,
    ) -> Result<Response<proto::TransactGetResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

        let req = request.into_inner();

        // Build transact get request with all keys
        let mut transact_request = kstone_api::TransactGetRequest::new();
        for proto_key in req.keys {
            let core_key = proto_key_to_core_key(proto_key);
            if let Some(sk) = &core_key.sk {
                transact_request = transact_request.get_with_sk(&core_key.pk, sk);
            } else {
                transact_request = transact_request.get(&core_key.pk);
            }
        }

        // Execute transactional get
        let db = Arc::clone(&self.db);
        let response = tokio::task::spawn_blocking(move || db.transact_get(transact_request))
            .await
            .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
            .map_err(map_error)?;

        // Convert response items to protobuf
        let items: Vec<proto::TransactGetItem> = response
            .items
            .iter()
            .map(|item_opt| proto::TransactGetItem {
                item: item_opt.as_ref().map(ks_item_to_proto),
            })
            .collect();

        Ok(Response::new(proto::TransactGetResponse {
            items,
            error: None,
        }))
    }

    /// Transactional write
    #[instrument(skip(self, request), fields(trace_id))]
    async fn transact_write(
        &self,
        request: Request<proto::TransactWriteRequest>,
    ) -> Result<Response<proto::TransactWriteResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

        use proto::transact_write_item::Item as ProtoTxItem;

        let req = request.into_inner();

        // Build transact write request with all operations
        let mut transact_request = kstone_api::TransactWriteRequest::new();

        for item in req.items {
            let proto_item = item
                .item
                .ok_or_else(|| Status::invalid_argument("TransactWriteItem is required"))?;

            match proto_item {
                ProtoTxItem::Put(put) => {
                    let key = if let Some(sk) = put.sort_key {
                        kstone_core::Key::with_sk(
                            Bytes::from(put.partition_key),
                            Bytes::from(sk),
                        )
                    } else {
                        kstone_core::Key::new(Bytes::from(put.partition_key))
                    };

                    let item = proto_item_to_ks(
                        put.item
                            .ok_or_else(|| Status::invalid_argument("Item required for put"))?,
                    )?;

                    transact_request = transact_request.add_operation(kstone_api::TransactWriteOp::Put {
                        key,
                        item,
                        condition: put.condition_expression,
                    });
                }
                ProtoTxItem::Update(update) => {
                    let key = if let Some(sk) = update.sort_key {
                        kstone_core::Key::with_sk(
                            Bytes::from(update.partition_key),
                            Bytes::from(sk),
                        )
                    } else {
                        kstone_core::Key::new(Bytes::from(update.partition_key))
                    };

                    transact_request = transact_request.add_operation(kstone_api::TransactWriteOp::Update {
                        key,
                        update_expression: update.update_expression,
                        condition: update.condition_expression,
                    });
                }
                ProtoTxItem::Delete(delete) => {
                    let key = if let Some(sk) = delete.sort_key {
                        kstone_core::Key::with_sk(
                            Bytes::from(delete.partition_key),
                            Bytes::from(sk),
                        )
                    } else {
                        kstone_core::Key::new(Bytes::from(delete.partition_key))
                    };

                    transact_request = transact_request.add_operation(kstone_api::TransactWriteOp::Delete {
                        key,
                        condition: delete.condition_expression,
                    });
                }
                ProtoTxItem::ConditionCheck(check) => {
                    let key = if let Some(sk) = check.sort_key {
                        kstone_core::Key::with_sk(
                            Bytes::from(check.partition_key),
                            Bytes::from(sk),
                        )
                    } else {
                        kstone_core::Key::new(Bytes::from(check.partition_key))
                    };

                    transact_request = transact_request.add_operation(kstone_api::TransactWriteOp::ConditionCheck {
                        key,
                        condition: check.condition_expression,
                    });
                }
            }
        }

        // Execute transactional write
        let db = Arc::clone(&self.db);
        tokio::task::spawn_blocking(move || db.transact_write(transact_request))
            .await
            .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
            .map_err(map_error)?;

        Ok(Response::new(proto::TransactWriteResponse {
            success: true,
            error: None,
        }))
    }

    /// Update an item
    #[instrument(skip(self, request), fields(trace_id))]
    async fn update(
        &self,
        request: Request<proto::UpdateRequest>,
    ) -> Result<Response<proto::UpdateResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

        let req = request.into_inner();

        // Build update operation
        let mut update = if let Some(sk) = req.sort_key {
            kstone_api::Update::with_sk(&req.partition_key, &sk)
        } else {
            kstone_api::Update::new(&req.partition_key)
        };

        // Set update expression
        update = update.expression(req.update_expression);

        // Set condition if present
        if let Some(condition) = req.condition_expression {
            update = update.condition(condition);
        }

        // Add expression values
        for (placeholder, proto_value) in req.expression_values {
            let value = proto_value_to_ks(proto_value)?;
            update = update.value(placeholder, value);
        }

        // Execute update
        let db = Arc::clone(&self.db);
        let response = tokio::task::spawn_blocking(move || db.update(update))
            .await
            .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
            .map_err(map_error)?;

        Ok(Response::new(proto::UpdateResponse {
            item: Some(ks_item_to_proto(&response.item)),
            error: None,
        }))
    }

    /// Execute a PartiQL statement
    #[instrument(skip(self, request), fields(trace_id))]
    async fn execute_statement(
        &self,
        request: Request<proto::ExecuteStatementRequest>,
    ) -> Result<Response<proto::ExecuteStatementResponse>, Status> {
        // Generate trace ID for request correlation
        let trace_id = Uuid::new_v4().to_string();
        tracing::Span::current().record("trace_id", &trace_id);

        // Check rate limit
        self.rate_limiter.check_rate_limit()?;

        use proto::execute_statement_response::Response as ProtoStmtResponse;

        let req = request.into_inner();

        // Execute the statement
        let db = Arc::clone(&self.db);
        let statement = req.statement;
        let response = tokio::task::spawn_blocking(move || db.execute_statement(&statement))
            .await
            .map_err(|e| Status::internal(format!("Task join error: {}", e)))?
            .map_err(map_error)?;

        // Convert response based on statement type
        let proto_response = match response {
            kstone_api::ExecuteStatementResponse::Select {
                items,
                count,
                scanned_count,
                last_key,
            } => ProtoStmtResponse::Select(proto::SelectResult {
                items: items.iter().map(ks_item_to_proto).collect(),
                count: count as u32,
                scanned_count: scanned_count as u32,
                last_key: ks_last_key_opt_to_proto(last_key),
            }),
            kstone_api::ExecuteStatementResponse::Insert { success } => {
                ProtoStmtResponse::Insert(proto::InsertResult { success })
            }
            kstone_api::ExecuteStatementResponse::Update { item } => {
                ProtoStmtResponse::Update(proto::UpdateResult {
                    item: Some(ks_item_to_proto(&item)),
                })
            }
            kstone_api::ExecuteStatementResponse::Delete { success } => {
                ProtoStmtResponse::Delete(proto::DeleteResult { success })
            }
            // Handle future response variants
            _ => {
                return Err(Status::unimplemented("Unsupported statement response type"));
            }
        };

        Ok(Response::new(proto::ExecuteStatementResponse {
            response: Some(proto_response),
            error: None,
        }))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use kstone_proto::{self as proto, value::Value as ProtoValueEnum};
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tonic::Request;

    /// Test helper to create a KeystoneService with a temporary database
    fn create_test_service() -> (KeystoneService, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::create(temp_dir.path()).unwrap();

        // Use unlimited rate limiter for tests
        let rate_limiter = Arc::new(RateLimiter::new(0, 0));

        let service = KeystoneService::new(db, rate_limiter);
        (service, temp_dir)
    }

    /// Helper to create a proto Value from a string
    fn proto_string(s: &str) -> proto::Value {
        proto::Value {
            value: Some(ProtoValueEnum::StringValue(s.to_string())),
        }
    }

    /// Helper to create a proto Value from a number
    fn proto_number(n: &str) -> proto::Value {
        proto::Value {
            value: Some(ProtoValueEnum::NumberValue(n.to_string())),
        }
    }

    /// Helper to create a proto Value from a bool
    fn proto_bool(b: bool) -> proto::Value {
        proto::Value {
            value: Some(ProtoValueEnum::BoolValue(b)),
        }
    }

    /// Helper to create a proto Item
    fn proto_item(attributes: HashMap<String, proto::Value>) -> proto::Item {
        proto::Item { attributes }
    }

    // ============================================================================
    // Basic CRUD Tests
    // ============================================================================

    #[tokio::test]
    async fn test_put_and_get_basic() {
        let (service, _dir) = create_test_service();

        // Create an item
        let mut attributes = HashMap::new();
        attributes.insert("name".to_string(), proto_string("Alice"));
        attributes.insert("age".to_string(), proto_number("30"));
        attributes.insert("active".to_string(), proto_bool(true));

        // Put the item
        let put_req = Request::new(proto::PutRequest {
            partition_key: b"user#123".to_vec(),
            sort_key: None,
            item: Some(proto_item(attributes.clone())),
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        let put_response = service.put(put_req).await.unwrap();
        assert!(put_response.into_inner().success);

        // Get the item back
        let get_req = Request::new(proto::GetRequest {
            partition_key: b"user#123".to_vec(),
            sort_key: None,
        });

        let get_response = service.get(get_req).await.unwrap();
        let item = get_response.into_inner().item.unwrap();

        // Verify attributes
        assert_eq!(item.attributes.len(), 3);
        assert!(item.attributes.contains_key("name"));
        assert!(item.attributes.contains_key("age"));
        assert!(item.attributes.contains_key("active"));
    }

    #[tokio::test]
    async fn test_put_and_get_with_sort_key() {
        let (service, _dir) = create_test_service();

        let mut attributes = HashMap::new();
        attributes.insert("data".to_string(), proto_string("test data"));

        // Put with sort key
        let put_req = Request::new(proto::PutRequest {
            partition_key: b"user#456".to_vec(),
            sort_key: Some(b"profile".to_vec()),
            item: Some(proto_item(attributes)),
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        service.put(put_req).await.unwrap();

        // Get with sort key
        let get_req = Request::new(proto::GetRequest {
            partition_key: b"user#456".to_vec(),
            sort_key: Some(b"profile".to_vec()),
        });

        let get_response = service.get(get_req).await.unwrap();
        let item = get_response.into_inner().item.unwrap();

        assert_eq!(item.attributes.len(), 1);
        assert!(item.attributes.contains_key("data"));
    }

    #[tokio::test]
    async fn test_get_not_found() {
        let (service, _dir) = create_test_service();

        let get_req = Request::new(proto::GetRequest {
            partition_key: b"nonexistent".to_vec(),
            sort_key: None,
        });

        let get_response = service.get(get_req).await.unwrap();
        let inner = get_response.into_inner();

        // Item should be None when not found
        assert!(inner.item.is_none());
    }

    #[tokio::test]
    async fn test_delete_basic() {
        let (service, _dir) = create_test_service();

        // First put an item
        let mut attributes = HashMap::new();
        attributes.insert("temp".to_string(), proto_string("data"));

        let put_req = Request::new(proto::PutRequest {
            partition_key: b"temp#1".to_vec(),
            sort_key: None,
            item: Some(proto_item(attributes)),
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        service.put(put_req).await.unwrap();

        // Delete the item
        let delete_req = Request::new(proto::DeleteRequest {
            partition_key: b"temp#1".to_vec(),
            sort_key: None,
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        let delete_response = service.delete(delete_req).await.unwrap();
        assert!(delete_response.into_inner().success);

        // Verify it's gone
        let get_req = Request::new(proto::GetRequest {
            partition_key: b"temp#1".to_vec(),
            sort_key: None,
        });

        let get_response = service.get(get_req).await.unwrap();
        assert!(get_response.into_inner().item.is_none());
    }

    #[tokio::test]
    async fn test_delete_with_sort_key() {
        let (service, _dir) = create_test_service();

        // Put item with sort key
        let mut attributes = HashMap::new();
        attributes.insert("data".to_string(), proto_string("value"));

        let put_req = Request::new(proto::PutRequest {
            partition_key: b"user#789".to_vec(),
            sort_key: Some(b"settings".to_vec()),
            item: Some(proto_item(attributes)),
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        service.put(put_req).await.unwrap();

        // Delete with sort key
        let delete_req = Request::new(proto::DeleteRequest {
            partition_key: b"user#789".to_vec(),
            sort_key: Some(b"settings".to_vec()),
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        service.delete(delete_req).await.unwrap();

        // Verify deletion
        let get_req = Request::new(proto::GetRequest {
            partition_key: b"user#789".to_vec(),
            sort_key: Some(b"settings".to_vec()),
        });

        let get_response = service.get(get_req).await.unwrap();
        assert!(get_response.into_inner().item.is_none());
    }

    // ============================================================================
    // Query Tests
    // ============================================================================

    #[tokio::test]
    async fn test_query_basic() {
        let (service, _dir) = create_test_service();

        // Put multiple items with same PK, different SKs
        for i in 1..=3 {
            let mut attributes = HashMap::new();
            attributes.insert("index".to_string(), proto_number(&i.to_string()));

            let put_req = Request::new(proto::PutRequest {
                partition_key: b"org#acme".to_vec(),
                sort_key: Some(format!("user#{:03}", i).into_bytes()),
                item: Some(proto_item(attributes)),
                condition_expression: None,
                expression_values: HashMap::new(),
            });

            service.put(put_req).await.unwrap();
        }

        // Query all items with this partition key
        let query_req = Request::new(proto::QueryRequest {
            partition_key: b"org#acme".to_vec(),
            sort_key_condition: None,
            limit: None,
            exclusive_start_key: None,
            scan_forward: None,
            index_name: None,
            filter_expression: None,
            expression_values: HashMap::new(),
        });

        let query_response = service.query(query_req).await.unwrap();
        let response = query_response.into_inner();

        assert_eq!(response.count, 3);
        assert_eq!(response.items.len(), 3);
    }

    #[tokio::test]
    async fn test_query_with_begins_with() {
        let (service, _dir) = create_test_service();

        // Put items with different prefixes
        let items = vec![
            ("org#xyz", "user#alice"),
            ("org#xyz", "user#bob"),
            ("org#xyz", "admin#charlie"),
        ];

        for (pk, sk) in items {
            let mut attributes = HashMap::new();
            attributes.insert("name".to_string(), proto_string(sk));

            let put_req = Request::new(proto::PutRequest {
                partition_key: pk.as_bytes().to_vec(),
                sort_key: Some(sk.as_bytes().to_vec()),
                item: Some(proto_item(attributes)),
                condition_expression: None,
                expression_values: HashMap::new(),
            });

            service.put(put_req).await.unwrap();
        }

        // Query with begins_with condition
        let sk_condition = proto::SortKeyCondition {
            condition: Some(proto::sort_key_condition::Condition::BeginsWith(
                proto_string("user#"),
            )),
        };

        let query_req = Request::new(proto::QueryRequest {
            partition_key: b"org#xyz".to_vec(),
            sort_key_condition: Some(sk_condition),
            limit: None,
            exclusive_start_key: None,
            scan_forward: None,
            index_name: None,
            filter_expression: None,
            expression_values: HashMap::new(),
        });

        let query_response = service.query(query_req).await.unwrap();
        let response = query_response.into_inner();

        // Should only return user# items, not admin#
        assert_eq!(response.count, 2);
    }

    #[tokio::test]
    async fn test_query_with_limit() {
        let (service, _dir) = create_test_service();

        // Put 5 items
        for i in 1..=5 {
            let mut attributes = HashMap::new();
            attributes.insert("index".to_string(), proto_number(&i.to_string()));

            let put_req = Request::new(proto::PutRequest {
                partition_key: b"batch#test".to_vec(),
                sort_key: Some(format!("item#{:03}", i).into_bytes()),
                item: Some(proto_item(attributes)),
                condition_expression: None,
                expression_values: HashMap::new(),
            });

            service.put(put_req).await.unwrap();
        }

        // Query with limit
        let query_req = Request::new(proto::QueryRequest {
            partition_key: b"batch#test".to_vec(),
            sort_key_condition: None,
            limit: Some(3),
            exclusive_start_key: None,
            scan_forward: None,
            index_name: None,
            filter_expression: None,
            expression_values: HashMap::new(),
        });

        let query_response = service.query(query_req).await.unwrap();
        let response = query_response.into_inner();

        assert_eq!(response.count, 3);
        assert!(response.last_evaluated_key.is_some());
    }

    #[tokio::test]
    async fn test_query_with_between() {
        let (service, _dir) = create_test_service();

        // Put items with numeric sort keys
        for i in 1..=10 {
            let mut attributes = HashMap::new();
            attributes.insert("value".to_string(), proto_number(&i.to_string()));

            let put_req = Request::new(proto::PutRequest {
                partition_key: b"range#test".to_vec(),
                sort_key: Some(format!("{:03}", i).into_bytes()),
                item: Some(proto_item(attributes)),
                condition_expression: None,
                expression_values: HashMap::new(),
            });

            service.put(put_req).await.unwrap();
        }

        // Query with between condition (003 to 007)
        let sk_condition = proto::SortKeyCondition {
            condition: Some(proto::sort_key_condition::Condition::Between(
                proto::BetweenCondition {
                    lower: Some(proto_string("003")),
                    upper: Some(proto_string("007")),
                },
            )),
        };

        let query_req = Request::new(proto::QueryRequest {
            partition_key: b"range#test".to_vec(),
            sort_key_condition: Some(sk_condition),
            limit: None,
            exclusive_start_key: None,
            scan_forward: None,
            index_name: None,
            filter_expression: None,
            expression_values: HashMap::new(),
        });

        let query_response = service.query(query_req).await.unwrap();
        let response = query_response.into_inner();

        // Should return items 3, 4, 5, 6, 7 (5 items)
        assert_eq!(response.count, 5);
    }

    // ============================================================================
    // Batch Operation Tests
    // ============================================================================

    #[tokio::test]
    async fn test_batch_get() {
        let (service, _dir) = create_test_service();

        // Put multiple items
        for i in 1..=3 {
            let mut attributes = HashMap::new();
            attributes.insert("id".to_string(), proto_number(&i.to_string()));

            let put_req = Request::new(proto::PutRequest {
                partition_key: format!("item#{}", i).into_bytes(),
                sort_key: None,
                item: Some(proto_item(attributes)),
                condition_expression: None,
                expression_values: HashMap::new(),
            });

            service.put(put_req).await.unwrap();
        }

        // Batch get
        let keys = vec![
            proto::Key {
                partition_key: b"item#1".to_vec(),
                sort_key: None,
            },
            proto::Key {
                partition_key: b"item#2".to_vec(),
                sort_key: None,
            },
            proto::Key {
                partition_key: b"item#3".to_vec(),
                sort_key: None,
            },
        ];

        let batch_get_req = Request::new(proto::BatchGetRequest { keys });

        let batch_response = service.batch_get(batch_get_req).await.unwrap();
        let response = batch_response.into_inner();

        assert_eq!(response.count, 3);
        assert_eq!(response.items.len(), 3);
    }

    #[tokio::test]
    async fn test_batch_get_with_missing_items() {
        let (service, _dir) = create_test_service();

        // Put only one item
        let mut attributes = HashMap::new();
        attributes.insert("data".to_string(), proto_string("exists"));

        let put_req = Request::new(proto::PutRequest {
            partition_key: b"exists#1".to_vec(),
            sort_key: None,
            item: Some(proto_item(attributes)),
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        service.put(put_req).await.unwrap();

        // Try to get multiple items (only one exists)
        let keys = vec![
            proto::Key {
                partition_key: b"exists#1".to_vec(),
                sort_key: None,
            },
            proto::Key {
                partition_key: b"missing#1".to_vec(),
                sort_key: None,
            },
        ];

        let batch_get_req = Request::new(proto::BatchGetRequest { keys });

        let batch_response = service.batch_get(batch_get_req).await.unwrap();
        let response = batch_response.into_inner();

        // Should only return the one that exists
        assert_eq!(response.count, 1);
        assert_eq!(response.items.len(), 1);
    }

    #[tokio::test]
    async fn test_batch_write_puts() {
        let (service, _dir) = create_test_service();

        // Create batch write with multiple puts
        let writes = vec![
            proto::WriteRequest {
                request: Some(proto::write_request::Request::Put(proto::PutItem {
                    partition_key: b"batch#1".to_vec(),
                    sort_key: None,
                    item: Some(proto_item({
                        let mut attrs = HashMap::new();
                        attrs.insert("name".to_string(), proto_string("item1"));
                        attrs
                    })),
                })),
            },
            proto::WriteRequest {
                request: Some(proto::write_request::Request::Put(proto::PutItem {
                    partition_key: b"batch#2".to_vec(),
                    sort_key: None,
                    item: Some(proto_item({
                        let mut attrs = HashMap::new();
                        attrs.insert("name".to_string(), proto_string("item2"));
                        attrs
                    })),
                })),
            },
        ];

        let batch_write_req = Request::new(proto::BatchWriteRequest { writes });

        let batch_response = service.batch_write(batch_write_req).await.unwrap();
        assert!(batch_response.into_inner().success);

        // Verify items were created
        let get_req = Request::new(proto::GetRequest {
            partition_key: b"batch#1".to_vec(),
            sort_key: None,
        });

        let get_response = service.get(get_req).await.unwrap();
        assert!(get_response.into_inner().item.is_some());
    }

    #[tokio::test]
    async fn test_batch_write_mixed_operations() {
        let (service, _dir) = create_test_service();

        // First put an item to delete later
        let mut attributes = HashMap::new();
        attributes.insert("temp".to_string(), proto_string("delete_me"));

        let put_req = Request::new(proto::PutRequest {
            partition_key: b"to_delete".to_vec(),
            sort_key: None,
            item: Some(proto_item(attributes)),
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        service.put(put_req).await.unwrap();

        // Batch write with put and delete
        let writes = vec![
            proto::WriteRequest {
                request: Some(proto::write_request::Request::Put(proto::PutItem {
                    partition_key: b"new_item".to_vec(),
                    sort_key: None,
                    item: Some(proto_item({
                        let mut attrs = HashMap::new();
                        attrs.insert("status".to_string(), proto_string("created"));
                        attrs
                    })),
                })),
            },
            proto::WriteRequest {
                request: Some(proto::write_request::Request::Delete(proto::DeleteKey {
                    partition_key: b"to_delete".to_vec(),
                    sort_key: None,
                })),
            },
        ];

        let batch_write_req = Request::new(proto::BatchWriteRequest { writes });

        let batch_response = service.batch_write(batch_write_req).await.unwrap();
        assert!(batch_response.into_inner().success);

        // Verify new item exists
        let get_req = Request::new(proto::GetRequest {
            partition_key: b"new_item".to_vec(),
            sort_key: None,
        });

        let get_response = service.get(get_req).await.unwrap();
        assert!(get_response.into_inner().item.is_some());

        // Verify deleted item is gone
        let get_req = Request::new(proto::GetRequest {
            partition_key: b"to_delete".to_vec(),
            sort_key: None,
        });

        let get_response = service.get(get_req).await.unwrap();
        assert!(get_response.into_inner().item.is_none());
    }

    // ============================================================================
    // Error Handling Tests
    // ============================================================================

    #[tokio::test]
    async fn test_put_without_item() {
        let (service, _dir) = create_test_service();

        let put_req = Request::new(proto::PutRequest {
            partition_key: b"test".to_vec(),
            sort_key: None,
            item: None, // Missing item
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        let result = service.put(put_req).await;
        assert!(result.is_err());

        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
        assert!(status.message().contains("Item required"));
    }

    #[tokio::test]
    async fn test_conditional_put_failure() {
        let (service, _dir) = create_test_service();

        // First put an item
        let mut attributes = HashMap::new();
        attributes.insert("name".to_string(), proto_string("Alice"));

        let put_req = Request::new(proto::PutRequest {
            partition_key: b"user#conditional".to_vec(),
            sort_key: None,
            item: Some(proto_item(attributes.clone())),
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        service.put(put_req).await.unwrap();

        // Try to put again with attribute_not_exists condition (should fail)
        let mut expression_values = HashMap::new();
        expression_values.insert(":name".to_string(), proto_string("Bob"));

        let conditional_put_req = Request::new(proto::PutRequest {
            partition_key: b"user#conditional".to_vec(),
            sort_key: None,
            item: Some(proto_item(attributes)),
            condition_expression: Some("attribute_not_exists(name)".to_string()),
            expression_values,
        });

        let result = service.put(conditional_put_req).await;
        assert!(result.is_err());

        let status = result.unwrap_err();
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    }

    #[tokio::test]
    async fn test_update_basic() {
        let (service, _dir) = create_test_service();

        // First put an item
        let mut attributes = HashMap::new();
        attributes.insert("name".to_string(), proto_string("Alice"));
        attributes.insert("age".to_string(), proto_number("30"));

        let put_req = Request::new(proto::PutRequest {
            partition_key: b"user#update".to_vec(),
            sort_key: None,
            item: Some(proto_item(attributes)),
            condition_expression: None,
            expression_values: HashMap::new(),
        });

        service.put(put_req).await.unwrap();

        // Update the item
        let mut expression_values = HashMap::new();
        expression_values.insert(":new_age".to_string(), proto_number("31"));

        let update_req = Request::new(proto::UpdateRequest {
            partition_key: b"user#update".to_vec(),
            sort_key: None,
            update_expression: "SET age = :new_age".to_string(),
            condition_expression: None,
            expression_values,
        });

        let update_response = service.update(update_req).await.unwrap();
        let response = update_response.into_inner();

        assert!(response.item.is_some());
        let item = response.item.unwrap();

        // Verify age was updated
        if let Some(proto::Value {
            value: Some(ProtoValueEnum::NumberValue(age)),
        }) = item.attributes.get("age")
        {
            assert_eq!(age, "31");
        } else {
            panic!("Age attribute not found or has wrong type");
        }
    }

    #[tokio::test]
    async fn test_scan_basic() {
        let (service, _dir) = create_test_service();

        // Put multiple items
        for i in 1..=5 {
            let mut attributes = HashMap::new();
            attributes.insert("index".to_string(), proto_number(&i.to_string()));

            let put_req = Request::new(proto::PutRequest {
                partition_key: format!("scan#{}", i).into_bytes(),
                sort_key: None,
                item: Some(proto_item(attributes)),
                condition_expression: None,
                expression_values: HashMap::new(),
            });

            service.put(put_req).await.unwrap();
        }

        // Scan all items
        let scan_req = Request::new(proto::ScanRequest {
            limit: None,
            exclusive_start_key: None,
            segment: None,
            total_segments: None,
            index_name: None,
            filter_expression: None,
            expression_values: HashMap::new(),
        });

        let scan_response = service.scan(scan_req).await.unwrap();
        let mut stream = scan_response.into_inner();

        // Get the first (and only) response from the stream
        use futures::StreamExt;
        let response = stream.next().await.unwrap().unwrap();

        // Should return all items
        assert!(response.count >= 5);
        assert!(response.items.len() >= 5);
    }

    #[tokio::test]
    async fn test_scan_with_limit() {
        let (service, _dir) = create_test_service();

        // Put 10 items
        for i in 1..=10 {
            let mut attributes = HashMap::new();
            attributes.insert("value".to_string(), proto_number(&i.to_string()));

            let put_req = Request::new(proto::PutRequest {
                partition_key: format!("scan_limit#{}", i).into_bytes(),
                sort_key: None,
                item: Some(proto_item(attributes)),
                condition_expression: None,
                expression_values: HashMap::new(),
            });

            service.put(put_req).await.unwrap();
        }

        // Scan with limit
        let scan_req = Request::new(proto::ScanRequest {
            limit: Some(5),
            exclusive_start_key: None,
            segment: None,
            total_segments: None,
            index_name: None,
            filter_expression: None,
            expression_values: HashMap::new(),
        });

        let scan_response = service.scan(scan_req).await.unwrap();
        let mut stream = scan_response.into_inner();

        use futures::StreamExt;
        let response = stream.next().await.unwrap().unwrap();

        // Should return at most 5 items
        assert!(response.count <= 5);
        assert!(response.items.len() <= 5);
    }

    // ============================================================================
    // Error Mapping Tests
    // ============================================================================

    #[test]
    fn test_map_error_not_found() {
        let err = KsError::NotFound("Item not found".to_string());
        let status = map_error(err);
        assert_eq!(status.code(), tonic::Code::NotFound);
    }

    #[test]
    fn test_map_error_invalid_query() {
        let err = KsError::InvalidQuery("Bad query".to_string());
        let status = map_error(err);
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn test_map_error_conditional_check_failed() {
        let err = KsError::ConditionalCheckFailed("Condition not met".to_string());
        let status = map_error(err);
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    }

    #[test]
    fn test_map_error_transaction_canceled() {
        let err = KsError::TransactionCanceled("Transaction failed".to_string());
        let status = map_error(err);
        assert_eq!(status.code(), tonic::Code::Aborted);
    }

    #[test]
    fn test_map_error_corruption() {
        let err = KsError::Corruption("Data corrupted".to_string());
        let status = map_error(err);
        assert_eq!(status.code(), tonic::Code::DataLoss);
    }

    #[test]
    fn test_map_error_already_exists() {
        let err = KsError::AlreadyExists("Item exists".to_string());
        let status = map_error(err);
        assert_eq!(status.code(), tonic::Code::AlreadyExists);
    }

    #[test]
    fn test_map_error_resource_exhausted() {
        let err = KsError::ResourceExhausted("Out of resources".to_string());
        let status = map_error(err);
        assert_eq!(status.code(), tonic::Code::ResourceExhausted);
    }
}

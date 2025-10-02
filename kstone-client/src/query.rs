/// Remote query builder and response types
use crate::convert::*;
use crate::error::Result;
use bytes::Bytes;
use kstone_core::Item;
use kstone_proto::{self as proto, keystone_db_client::KeystoneDbClient};
use tonic::transport::Channel;

/// Remote query builder
pub struct RemoteQuery {
    partition_key: Vec<u8>,
    sort_key_condition: Option<proto::SortKeyCondition>,
    limit: Option<u32>,
    exclusive_start_key: Option<proto::LastKey>,
    scan_forward: Option<bool>,
    index_name: Option<String>,
}

impl RemoteQuery {
    /// Create a new query for a partition key
    pub fn new(pk: &[u8]) -> Self {
        Self {
            partition_key: pk.to_vec(),
            sort_key_condition: None,
            limit: None,
            exclusive_start_key: None,
            scan_forward: None,
            index_name: None,
        }
    }

    /// Add a sort key equals condition
    pub fn sk_eq(mut self, sk: &[u8]) -> Self {
        self.sort_key_condition = Some(proto::SortKeyCondition {
            condition: Some(proto::sort_key_condition::Condition::EqualTo(
                value_to_proto_bytes(sk),
            )),
        });
        self
    }

    /// Add a sort key less than condition
    pub fn sk_lt(mut self, sk: &[u8]) -> Self {
        self.sort_key_condition = Some(proto::SortKeyCondition {
            condition: Some(proto::sort_key_condition::Condition::LessThan(
                value_to_proto_bytes(sk),
            )),
        });
        self
    }

    /// Add a sort key less than or equal condition
    pub fn sk_lte(mut self, sk: &[u8]) -> Self {
        self.sort_key_condition = Some(proto::SortKeyCondition {
            condition: Some(proto::sort_key_condition::Condition::LessThanOrEqual(
                value_to_proto_bytes(sk),
            )),
        });
        self
    }

    /// Add a sort key greater than condition
    pub fn sk_gt(mut self, sk: &[u8]) -> Self {
        self.sort_key_condition = Some(proto::SortKeyCondition {
            condition: Some(proto::sort_key_condition::Condition::GreaterThan(
                value_to_proto_bytes(sk),
            )),
        });
        self
    }

    /// Add a sort key greater than or equal condition
    pub fn sk_gte(mut self, sk: &[u8]) -> Self {
        self.sort_key_condition = Some(proto::SortKeyCondition {
            condition: Some(proto::sort_key_condition::Condition::GreaterThanOrEqual(
                value_to_proto_bytes(sk),
            )),
        });
        self
    }

    /// Add a sort key between condition
    pub fn sk_between(mut self, sk1: &[u8], sk2: &[u8]) -> Self {
        self.sort_key_condition = Some(proto::SortKeyCondition {
            condition: Some(proto::sort_key_condition::Condition::Between(
                proto::BetweenCondition {
                    lower: Some(value_to_proto_bytes(sk1)),
                    upper: Some(value_to_proto_bytes(sk2)),
                },
            )),
        });
        self
    }

    /// Add a sort key begins_with condition
    pub fn sk_begins_with(mut self, prefix: &[u8]) -> Self {
        self.sort_key_condition = Some(proto::SortKeyCondition {
            condition: Some(proto::sort_key_condition::Condition::BeginsWith(
                value_to_proto_bytes(prefix),
            )),
        });
        self
    }

    /// Set the scan direction (default: forward)
    pub fn forward(mut self, forward: bool) -> Self {
        self.scan_forward = Some(forward);
        self
    }

    /// Set the maximum number of items to return
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit as u32);
        self
    }

    /// Set the exclusive start key for pagination
    pub fn start_after(mut self, pk: &[u8], sk: Option<&[u8]>) -> Self {
        self.exclusive_start_key = Some(proto::LastKey {
            partition_key: pk.to_vec(),
            sort_key: sk.map(|s| s.to_vec()),
        });
        self
    }

    /// Query a Local Secondary Index instead of the base table
    pub fn index(mut self, index_name: impl Into<String>) -> Self {
        self.index_name = Some(index_name.into());
        self
    }

    /// Execute the query
    pub async fn execute(
        self,
        client: &mut KeystoneDbClient<Channel>,
    ) -> Result<RemoteQueryResponse> {
        let request = proto::QueryRequest {
            partition_key: self.partition_key,
            sort_key_condition: self.sort_key_condition,
            filter_expression: None,
            expression_values: std::collections::HashMap::new(),
            index_name: self.index_name,
            limit: self.limit,
            exclusive_start_key: self.exclusive_start_key,
            scan_forward: self.scan_forward,
        };

        let response = client
            .query(request)
            .await?
            .into_inner();

        // Convert protobuf response to Rust types
        let items: Vec<Item> = response
            .items
            .into_iter()
            .map(|proto_item| proto_item_to_ks(proto_item).expect("Invalid item from server"))
            .collect();

        let last_key = response.last_evaluated_key.map(|key| {
            let (pk, sk) = proto_last_key_to_ks(key);
            (pk, sk)
        });

        Ok(RemoteQueryResponse {
            items,
            count: response.count as usize,
            scanned_count: response.scanned_count as usize,
            last_key,
        })
    }
}

/// Query response
pub struct RemoteQueryResponse {
    /// Items found
    pub items: Vec<Item>,
    /// Number of items returned
    pub count: usize,
    /// Last evaluated key (for pagination)
    pub last_key: Option<(Bytes, Option<Bytes>)>,
    /// Number of items examined
    pub scanned_count: usize,
}

/// Helper function to convert bytes to protobuf Value (for sort key conditions)
fn value_to_proto_bytes(bytes: &[u8]) -> proto::Value {
    proto::Value {
        value: Some(proto::value::Value::BinaryValue(bytes.to_vec())),
    }
}

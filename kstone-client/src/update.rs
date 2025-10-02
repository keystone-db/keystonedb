/// Remote update operations
use crate::convert::*;
use crate::error::Result;
use kstone_core::Item;
use kstone_proto::{self as proto, keystone_db_client::KeystoneDbClient};
use tonic::transport::Channel;
use std::collections::HashMap;

/// Remote update request builder
pub struct RemoteUpdate {
    partition_key: Vec<u8>,
    sort_key: Option<Vec<u8>>,
    update_expression: String,
    condition_expression: Option<String>,
    expression_values: HashMap<String, kstone_core::Value>,
}

impl RemoteUpdate {
    /// Create a new update operation
    pub fn new(pk: &[u8]) -> Self {
        Self {
            partition_key: pk.to_vec(),
            sort_key: None,
            update_expression: String::new(),
            condition_expression: None,
            expression_values: HashMap::new(),
        }
    }

    /// Create update with sort key
    pub fn with_sk(pk: &[u8], sk: &[u8]) -> Self {
        Self {
            partition_key: pk.to_vec(),
            sort_key: Some(sk.to_vec()),
            update_expression: String::new(),
            condition_expression: None,
            expression_values: HashMap::new(),
        }
    }

    /// Set the update expression
    pub fn expression(mut self, expr: impl Into<String>) -> Self {
        self.update_expression = expr.into();
        self
    }

    /// Set a condition expression
    pub fn condition(mut self, condition: impl Into<String>) -> Self {
        self.condition_expression = Some(condition.into());
        self
    }

    /// Add an expression attribute value
    pub fn value(mut self, placeholder: impl Into<String>, value: kstone_core::Value) -> Self {
        self.expression_values.insert(placeholder.into(), value);
        self
    }

    /// Execute the update operation
    pub async fn execute(self, client: &mut KeystoneDbClient<Channel>) -> Result<RemoteUpdateResponse> {
        // Convert expression values to protobuf
        let proto_values: HashMap<String, proto::Value> = self
            .expression_values
            .iter()
            .map(|(k, v)| (k.clone(), ks_value_to_proto(v)))
            .collect();

        let request = proto::UpdateRequest {
            partition_key: self.partition_key,
            sort_key: self.sort_key,
            update_expression: self.update_expression,
            condition_expression: self.condition_expression,
            expression_values: proto_values,
        };

        let response = client
            .update(request)
            .await?
            .into_inner();

        let item = proto_item_to_ks(
            response.item.expect("Server should return updated item")
        )?;

        Ok(RemoteUpdateResponse { item })
    }
}

/// Update response
pub struct RemoteUpdateResponse {
    /// The updated item
    pub item: Item,
}

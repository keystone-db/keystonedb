/// Remote transaction operations
use crate::convert::*;
use crate::error::{ClientError, Result};
use kstone_core::Item;
use kstone_proto::{self as proto, keystone_db_client::KeystoneDbClient};

/// Remote transact get request builder
pub struct RemoteTransactGetRequest {
    keys: Vec<proto::Key>,
}

impl RemoteTransactGetRequest {
    /// Create a new transact get request
    pub fn new() -> Self {
        Self { keys: Vec::new() }
    }

    /// Add a key with partition key only
    pub fn get(mut self, pk: &[u8]) -> Self {
        self.keys.push(proto::Key {
            partition_key: pk.to_vec(),
            sort_key: None,
        });
        self
    }

    /// Add a key with partition key and sort key
    pub fn get_with_sk(mut self, pk: &[u8], sk: &[u8]) -> Self {
        self.keys.push(proto::Key {
            partition_key: pk.to_vec(),
            sort_key: Some(sk.to_vec()),
        });
        self
    }

    /// Execute the transact get operation
    pub async fn execute<T>(self, client: &mut KeystoneDbClient<T>) -> Result<RemoteTransactGetResponse>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody> + Send + 'static,
        T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        T::ResponseBody: tonic::codegen::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as tonic::codegen::Body>::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
    {
        let request = proto::TransactGetRequest {
            keys: self.keys,
        };

        let response = client
            .transact_get(request)
            .await?
            .into_inner();

        // Convert protobuf response to Rust types
        let items: Result<Vec<Option<Item>>> = response
            .items
            .into_iter()
            .map(|tx_item| {
                match tx_item.item {
                    Some(proto_item) => {
                        let item = proto_item_to_ks(proto_item).map_err(ClientError::from)?;
                        Ok(Some(item))
                    }
                    None => Ok(None),
                }
            })
            .collect();
        let items = items?;

        Ok(RemoteTransactGetResponse { items })
    }
}

impl Default for RemoteTransactGetRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Transact get response
pub struct RemoteTransactGetResponse {
    /// Items retrieved (in same order as request, None if not found)
    pub items: Vec<Option<Item>>,
}

/// Remote transact write request builder
pub struct RemoteTransactWriteRequest {
    writes: Vec<proto::TransactWriteItem>,
}

impl RemoteTransactWriteRequest {
    /// Create a new transact write request
    pub fn new() -> Self {
        Self { writes: Vec::new() }
    }

    /// Add a put request
    pub fn put(mut self, pk: &[u8], item: Item) -> Self {
        self.writes.push(proto::TransactWriteItem {
            item: Some(proto::transact_write_item::Item::Put(proto::TransactPut {
                partition_key: pk.to_vec(),
                sort_key: None,
                item: Some(ks_item_to_proto(&item)),
                condition_expression: None,
            })),
        });
        self
    }

    /// Add a put request with sort key
    pub fn put_with_sk(mut self, pk: &[u8], sk: &[u8], item: Item) -> Self {
        self.writes.push(proto::TransactWriteItem {
            item: Some(proto::transact_write_item::Item::Put(proto::TransactPut {
                partition_key: pk.to_vec(),
                sort_key: Some(sk.to_vec()),
                item: Some(ks_item_to_proto(&item)),
                condition_expression: None,
            })),
        });
        self
    }

    /// Add an update request
    pub fn update(mut self, pk: &[u8], update_expression: impl Into<String>) -> Self {
        self.writes.push(proto::TransactWriteItem {
            item: Some(proto::transact_write_item::Item::Update(proto::TransactUpdate {
                partition_key: pk.to_vec(),
                sort_key: None,
                update_expression: update_expression.into(),
                condition_expression: None,
            })),
        });
        self
    }

    /// Add a delete request
    pub fn delete(mut self, pk: &[u8]) -> Self {
        self.writes.push(proto::TransactWriteItem {
            item: Some(proto::transact_write_item::Item::Delete(proto::TransactDelete {
                partition_key: pk.to_vec(),
                sort_key: None,
                condition_expression: None,
            })),
        });
        self
    }

    /// Add a condition check
    pub fn condition_check(mut self, pk: &[u8], condition: impl Into<String>) -> Self {
        self.writes.push(proto::TransactWriteItem {
            item: Some(proto::transact_write_item::Item::ConditionCheck(proto::ConditionCheck {
                partition_key: pk.to_vec(),
                sort_key: None,
                condition_expression: condition.into(),
            })),
        });
        self
    }

    /// Execute the transact write operation
    pub async fn execute<T>(self, client: &mut KeystoneDbClient<T>) -> Result<()>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody> + Send + 'static,
        T::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        T::ResponseBody: tonic::codegen::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as tonic::codegen::Body>::Error: Into<Box<dyn std::error::Error + Send + Sync>> + Send,
    {
        let request = proto::TransactWriteRequest {
            items: self.writes,
        };

        client
            .transact_write(request)
            .await?;

        Ok(())
    }
}

impl Default for RemoteTransactWriteRequest {
    fn default() -> Self {
        Self::new()
    }
}

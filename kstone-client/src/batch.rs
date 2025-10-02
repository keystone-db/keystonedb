/// Remote batch operations
use crate::convert::*;
use crate::error::Result;
use kstone_core::Item;
use kstone_proto::{self as proto, keystone_db_client::KeystoneDbClient};
use tonic::transport::Channel;

/// Remote batch get request builder
pub struct RemoteBatchGetRequest {
    keys: Vec<proto::Key>,
}

impl RemoteBatchGetRequest {
    /// Create a new batch get request
    pub fn new() -> Self {
        Self { keys: Vec::new() }
    }

    /// Add a key with partition key only
    pub fn add_key(mut self, pk: &[u8]) -> Self {
        self.keys.push(proto::Key {
            partition_key: pk.to_vec(),
            sort_key: None,
        });
        self
    }

    /// Add a key with partition key and sort key
    pub fn add_key_with_sk(mut self, pk: &[u8], sk: &[u8]) -> Self {
        self.keys.push(proto::Key {
            partition_key: pk.to_vec(),
            sort_key: Some(sk.to_vec()),
        });
        self
    }

    /// Execute the batch get operation
    pub async fn execute(self, client: &mut KeystoneDbClient<Channel>) -> Result<RemoteBatchGetResponse> {
        let request = proto::BatchGetRequest {
            keys: self.keys,
        };

        let response = client
            .batch_get(request)
            .await?
            .into_inner();

        // Convert protobuf response to Rust types
        let items: Vec<Item> = response
            .items
            .into_iter()
            .map(|proto_item| proto_item_to_ks(proto_item).expect("Invalid item from server"))
            .collect();

        Ok(RemoteBatchGetResponse {
            items,
            count: response.count as usize,
        })
    }
}

impl Default for RemoteBatchGetRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch get response
pub struct RemoteBatchGetResponse {
    /// Items retrieved
    pub items: Vec<Item>,
    /// Number of items returned
    pub count: usize,
}

/// Remote batch write request builder
pub struct RemoteBatchWriteRequest {
    writes: Vec<proto::WriteRequest>,
}

impl RemoteBatchWriteRequest {
    /// Create a new batch write request
    pub fn new() -> Self {
        Self { writes: Vec::new() }
    }

    /// Add a put request with partition key
    pub fn put(mut self, pk: &[u8], item: Item) -> Self {
        self.writes.push(proto::WriteRequest {
            request: Some(proto::write_request::Request::Put(proto::PutItem {
                partition_key: pk.to_vec(),
                sort_key: None,
                item: Some(ks_item_to_proto(&item)),
            })),
        });
        self
    }

    /// Add a put request with partition key and sort key
    pub fn put_with_sk(mut self, pk: &[u8], sk: &[u8], item: Item) -> Self {
        self.writes.push(proto::WriteRequest {
            request: Some(proto::write_request::Request::Put(proto::PutItem {
                partition_key: pk.to_vec(),
                sort_key: Some(sk.to_vec()),
                item: Some(ks_item_to_proto(&item)),
            })),
        });
        self
    }

    /// Add a delete request with partition key
    pub fn delete(mut self, pk: &[u8]) -> Self {
        self.writes.push(proto::WriteRequest {
            request: Some(proto::write_request::Request::Delete(proto::DeleteKey {
                partition_key: pk.to_vec(),
                sort_key: None,
            })),
        });
        self
    }

    /// Add a delete request with partition key and sort key
    pub fn delete_with_sk(mut self, pk: &[u8], sk: &[u8]) -> Self {
        self.writes.push(proto::WriteRequest {
            request: Some(proto::write_request::Request::Delete(proto::DeleteKey {
                partition_key: pk.to_vec(),
                sort_key: Some(sk.to_vec()),
            })),
        });
        self
    }

    /// Execute the batch write operation
    pub async fn execute(self, client: &mut KeystoneDbClient<Channel>) -> Result<RemoteBatchWriteResponse> {
        let request = proto::BatchWriteRequest {
            writes: self.writes,
        };

        let response = client
            .batch_write(request)
            .await?
            .into_inner();

        Ok(RemoteBatchWriteResponse {
            success: response.success,
        })
    }
}

impl Default for RemoteBatchWriteRequest {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch write response
pub struct RemoteBatchWriteResponse {
    /// Whether the batch write succeeded
    pub success: bool,
}

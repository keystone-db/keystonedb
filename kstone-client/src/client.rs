/// KeystoneDB gRPC client implementation
use crate::error::{ClientError, Result};
use kstone_core::Item;
use kstone_proto::{self as proto, keystone_db_client::KeystoneDbClient};
use tonic::transport::Channel;

/// KeystoneDB remote client
pub struct Client {
    inner: KeystoneDbClient<Channel>,
}

impl Client {
    /// Connect to a KeystoneDB server
    ///
    /// # Arguments
    /// * `addr` - Server address (e.g., "http://127.0.0.1:50051")
    ///
    /// # Example
    /// ```no_run
    /// # use kstone_client::Client;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = Client::connect("http://localhost:50051").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(addr: impl Into<String>) -> Result<Self> {
        let addr = addr.into();
        let channel = Channel::from_shared(addr)
            .map_err(|e| ClientError::ConnectionError(format!("Invalid address: {}", e)))?
            .connect()
            .await
            .map_err(|e| ClientError::ConnectionError(format!("Failed to connect: {}", e)))?;

        let inner = KeystoneDbClient::new(channel);
        Ok(Self { inner })
    }

    /// Put an item with a simple partition key
    ///
    /// # Arguments
    /// * `pk` - Partition key
    /// * `item` - Item to store
    ///
    /// # Example
    /// ```no_run
    /// # use kstone_client::Client;
    /// # use std::collections::HashMap;
    /// # use kstone_core::Value;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = Client::connect("http://localhost:50051").await?;
    ///
    /// let mut item = HashMap::new();
    /// item.insert("name".to_string(), Value::S("Alice".to_string()));
    /// item.insert("age".to_string(), Value::N("30".to_string()));
    ///
    /// client.put(b"user#123", item).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn put(&mut self, pk: &[u8], item: Item) -> Result<()> {
        let request = proto::PutRequest {
            partition_key: pk.to_vec(),
            sort_key: None,
            item: Some(crate::convert::ks_item_to_proto(&item)),
            condition_expression: None,
        };

        self.inner
            .put(request)
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }

    /// Put an item with partition key and sort key
    ///
    /// # Arguments
    /// * `pk` - Partition key
    /// * `sk` - Sort key
    /// * `item` - Item to store
    pub async fn put_with_sk(&mut self, pk: &[u8], sk: &[u8], item: Item) -> Result<()> {
        let request = proto::PutRequest {
            partition_key: pk.to_vec(),
            sort_key: Some(sk.to_vec()),
            item: Some(crate::convert::ks_item_to_proto(&item)),
            condition_expression: None,
        };

        self.inner
            .put(request)
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }

    /// Get an item with a simple partition key
    ///
    /// # Arguments
    /// * `pk` - Partition key
    ///
    /// # Returns
    /// The item if found, None otherwise
    pub async fn get(&mut self, pk: &[u8]) -> Result<Option<Item>> {
        let request = proto::GetRequest {
            partition_key: pk.to_vec(),
            sort_key: None,
        };

        let response = self
            .inner
            .get(request)
            .await
            .map_err(|e| ClientError::from(e))?
            .into_inner();

        Ok(response.item.map(|proto_item| {
            crate::convert::proto_item_to_ks(proto_item)
                .expect("Server returned invalid item")
        }))
    }

    /// Get an item with partition key and sort key
    ///
    /// # Arguments
    /// * `pk` - Partition key
    /// * `sk` - Sort key
    ///
    /// # Returns
    /// The item if found, None otherwise
    pub async fn get_with_sk(&mut self, pk: &[u8], sk: &[u8]) -> Result<Option<Item>> {
        let request = proto::GetRequest {
            partition_key: pk.to_vec(),
            sort_key: Some(sk.to_vec()),
        };

        let response = self
            .inner
            .get(request)
            .await
            .map_err(|e| ClientError::from(e))?
            .into_inner();

        Ok(response.item.map(|proto_item| {
            crate::convert::proto_item_to_ks(proto_item)
                .expect("Server returned invalid item")
        }))
    }

    /// Delete an item with a simple partition key
    ///
    /// # Arguments
    /// * `pk` - Partition key
    pub async fn delete(&mut self, pk: &[u8]) -> Result<()> {
        let request = proto::DeleteRequest {
            partition_key: pk.to_vec(),
            sort_key: None,
            condition_expression: None,
        };

        self.inner
            .delete(request)
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }

    /// Delete an item with partition key and sort key
    ///
    /// # Arguments
    /// * `pk` - Partition key
    /// * `sk` - Sort key
    pub async fn delete_with_sk(&mut self, pk: &[u8], sk: &[u8]) -> Result<()> {
        let request = proto::DeleteRequest {
            partition_key: pk.to_vec(),
            sort_key: Some(sk.to_vec()),
            condition_expression: None,
        };

        self.inner
            .delete(request)
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }

    /// Execute a query operation
    ///
    /// # Arguments
    /// * `query` - Query builder with conditions
    ///
    /// # Example
    /// ```no_run
    /// # use kstone_client::{Client, RemoteQuery};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = Client::connect("http://localhost:50051").await?;
    ///
    /// let query = RemoteQuery::new(b"user#org1")
    ///     .sk_begins_with(b"USER#")
    ///     .limit(10);
    ///
    /// let response = client.query(query).await?;
    /// println!("Found {} items", response.count);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn query(&mut self, query: crate::query::RemoteQuery) -> Result<crate::query::RemoteQueryResponse> {
        query.execute(&mut self.inner).await
    }

    /// Execute a scan operation
    ///
    /// # Arguments
    /// * `scan` - Scan builder with options
    ///
    /// # Example
    /// ```no_run
    /// # use kstone_client::{Client, RemoteScan};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = Client::connect("http://localhost:50051").await?;
    ///
    /// let scan = RemoteScan::new()
    ///     .limit(100);
    ///
    /// let response = client.scan(scan).await?;
    /// println!("Scanned {} items", response.count);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn scan(&mut self, scan: crate::scan::RemoteScan) -> Result<crate::scan::RemoteScanResponse> {
        scan.execute(&mut self.inner).await
    }

    /// Execute a batch get operation
    ///
    /// # Arguments
    /// * `request` - Batch get request with keys
    ///
    /// # Example
    /// ```no_run
    /// # use kstone_client::{Client, RemoteBatchGetRequest};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = Client::connect("http://localhost:50051").await?;
    ///
    /// let batch = RemoteBatchGetRequest::new()
    ///     .add_key(b"user#1")
    ///     .add_key(b"user#2");
    ///
    /// let response = client.batch_get(batch).await?;
    /// println!("Retrieved {} items", response.count);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn batch_get(&mut self, request: crate::batch::RemoteBatchGetRequest) -> Result<crate::batch::RemoteBatchGetResponse> {
        request.execute(&mut self.inner).await
    }

    /// Execute a batch write operation
    ///
    /// # Arguments
    /// * `request` - Batch write request with puts/deletes
    ///
    /// # Example
    /// ```no_run
    /// # use kstone_client::{Client, RemoteBatchWriteRequest};
    /// # use std::collections::HashMap;
    /// # use kstone_core::Value;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut client = Client::connect("http://localhost:50051").await?;
    ///
    /// let mut item = HashMap::new();
    /// item.insert("name".to_string(), Value::S("Alice".to_string()));
    ///
    /// let batch = RemoteBatchWriteRequest::new()
    ///     .put(b"user#1", item.clone())
    ///     .delete(b"user#old");
    ///
    /// let response = client.batch_write(batch).await?;
    /// println!("Batch write success: {}", response.success);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn batch_write(&mut self, request: crate::batch::RemoteBatchWriteRequest) -> Result<crate::batch::RemoteBatchWriteResponse> {
        request.execute(&mut self.inner).await
    }

    /// Get a reference to the underlying gRPC client
    pub(crate) fn inner_mut(&mut self) -> &mut KeystoneDbClient<Channel> {
        &mut self.inner
    }
}

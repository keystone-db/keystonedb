/// Remote scan builder and response types
use crate::convert::*;
use crate::error::Result;
use bytes::Bytes;
use kstone_core::Item;
use kstone_proto::{self as proto, keystone_db_client::KeystoneDbClient};
use tonic::transport::Channel;
use tonic::Streaming;

/// Remote scan builder
pub struct RemoteScan {
    limit: Option<u32>,
    exclusive_start_key: Option<proto::LastKey>,
    index_name: Option<String>,
    segment: Option<u32>,
    total_segments: Option<u32>,
}

impl RemoteScan {
    /// Create a new scan
    pub fn new() -> Self {
        Self {
            limit: None,
            exclusive_start_key: None,
            index_name: None,
            segment: None,
            total_segments: None,
        }
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

    /// Scan a secondary index instead of the base table
    pub fn index(mut self, index_name: impl Into<String>) -> Self {
        self.index_name = Some(index_name.into());
        self
    }

    /// Configure parallel scan (segment must be < total_segments)
    pub fn segment(mut self, segment: usize, total_segments: usize) -> Self {
        self.segment = Some(segment as u32);
        self.total_segments = Some(total_segments as u32);
        self
    }

    /// Execute the scan and get a stream of responses
    ///
    /// Note: The server currently returns a single response, but this
    /// interface is prepared for future streaming support.
    pub async fn execute(
        self,
        client: &mut KeystoneDbClient<Channel>,
    ) -> Result<RemoteScanResponse> {
        let request = proto::ScanRequest {
            filter_expression: None,
            expression_values: std::collections::HashMap::new(),
            limit: self.limit,
            exclusive_start_key: self.exclusive_start_key,
            index_name: self.index_name,
            segment: self.segment,
            total_segments: self.total_segments,
        };

        let mut stream: Streaming<proto::ScanResponse> = client
            .scan(request)
            .await?
            .into_inner();

        // Collect all items from the stream
        let mut all_items = Vec::new();
        let mut total_count = 0;
        let mut total_scanned_count = 0;
        let mut last_key = None;

        while let Some(response) = stream.message().await? {
            let items: Vec<Item> = response
                .items
                .into_iter()
                .map(|proto_item| proto_item_to_ks(proto_item).expect("Invalid item from server"))
                .collect();

            total_count += response.count as usize;
            total_scanned_count += response.scanned_count as usize;

            if let Some(key) = response.last_evaluated_key {
                let (pk, sk) = proto_last_key_to_ks(key);
                last_key = Some((pk, sk));
            }

            all_items.extend(items);
        }

        Ok(RemoteScanResponse {
            items: all_items,
            count: total_count,
            scanned_count: total_scanned_count,
            last_key,
        })
    }
}

impl Default for RemoteScan {
    fn default() -> Self {
        Self::new()
    }
}

/// Scan response
pub struct RemoteScanResponse {
    /// Items found
    pub items: Vec<Item>,
    /// Number of items returned
    pub count: usize,
    /// Last evaluated key (for pagination)
    pub last_key: Option<(Bytes, Option<Bytes>)>,
    /// Number of items examined
    pub scanned_count: usize,
}

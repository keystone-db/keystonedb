/// Iterator support for Query and Scan operations
///
/// Provides efficient iteration over memtable and SST files within a stripe,
/// merging results with proper ordering (newest version wins).

use crate::{Key, Item};
use bytes::Bytes;

/// Sort key comparison operator
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKeyCondition {
    /// sk = value
    Equal,
    /// sk < value
    LessThan,
    /// sk <= value
    LessThanOrEqual,
    /// sk > value
    GreaterThan,
    /// sk >= value
    GreaterThanOrEqual,
    /// sk BETWEEN value1 AND value2
    Between,
    /// sk begins_with value
    BeginsWith,
}

/// Query parameters for stripe iteration
#[derive(Debug, Clone)]
pub struct QueryParams {
    /// Partition key (required)
    pub pk: Bytes,
    /// Sort key condition (optional - if None, return all items with PK)
    pub sk_condition: Option<(SortKeyCondition, Bytes, Option<Bytes>)>,
    /// Scan direction
    pub forward: bool,
    /// Maximum items to return
    pub limit: Option<usize>,
    /// Start key for pagination (exclusive)
    pub start_key: Option<Key>,
    /// Index name for LSI queries (Phase 3.1+)
    pub index_name: Option<String>,
}

impl QueryParams {
    /// Create a new query for a partition key
    pub fn new(pk: Bytes) -> Self {
        Self {
            pk,
            sk_condition: None,
            forward: true,
            limit: None,
            start_key: None,
            index_name: None,
        }
    }

    /// Add sort key condition
    pub fn with_sk_condition(
        mut self,
        condition: SortKeyCondition,
        value: Bytes,
        value2: Option<Bytes>,
    ) -> Self {
        self.sk_condition = Some((condition, value, value2));
        self
    }

    /// Set scan direction
    pub fn with_direction(mut self, forward: bool) -> Self {
        self.forward = forward;
        self
    }

    /// Set limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set start key for pagination
    pub fn with_start_key(mut self, key: Key) -> Self {
        self.start_key = Some(key);
        self
    }

    /// Set index name for LSI query (Phase 3.1+)
    pub fn with_index_name(mut self, index_name: impl Into<String>) -> Self {
        self.index_name = Some(index_name.into());
        self
    }

    /// Check if a sort key matches the condition
    pub fn matches_sk(&self, sk: &Option<Bytes>) -> bool {
        match &self.sk_condition {
            None => true, // No condition - accept all
            Some((condition, value, value2)) => {
                let sk_bytes = match sk {
                    Some(b) => b,
                    None => return false, // Has condition but item has no SK
                };

                match condition {
                    SortKeyCondition::Equal => sk_bytes == value,
                    SortKeyCondition::LessThan => sk_bytes < value,
                    SortKeyCondition::LessThanOrEqual => sk_bytes <= value,
                    SortKeyCondition::GreaterThan => sk_bytes > value,
                    SortKeyCondition::GreaterThanOrEqual => sk_bytes >= value,
                    SortKeyCondition::Between => {
                        if let Some(v2) = value2 {
                            sk_bytes >= value && sk_bytes <= v2
                        } else {
                            false
                        }
                    }
                    SortKeyCondition::BeginsWith => sk_bytes.starts_with(value.as_ref()),
                }
            }
        }
    }

    /// Check if we should skip a key based on pagination start_key
    pub fn should_skip(&self, key: &Key) -> bool {
        if let Some(start) = &self.start_key {
            if self.forward {
                // Forward: skip if key <= start_key
                key <= start
            } else {
                // Backward: skip if key >= start_key
                key >= start
            }
        } else {
            false
        }
    }
}

/// Query result with pagination support
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Items found
    pub items: Vec<Item>,
    /// Last evaluated key (for pagination)
    pub last_key: Option<Key>,
    /// Count of items examined (before filter)
    pub scanned_count: usize,
}

impl QueryResult {
    pub fn new(items: Vec<Item>, last_key: Option<Key>, scanned_count: usize) -> Self {
        Self {
            items,
            last_key,
            scanned_count,
        }
    }
}

/// Scan parameters for table/stripe scanning
#[derive(Debug, Clone)]
pub struct ScanParams {
    /// Maximum items to return
    pub limit: Option<usize>,
    /// Start key for pagination (exclusive)
    pub start_key: Option<Key>,
    /// Segment number (for parallel scans, 0-based)
    pub segment: Option<usize>,
    /// Total number of segments (for parallel scans)
    pub total_segments: Option<usize>,
}

impl ScanParams {
    /// Create new scan parameters
    pub fn new() -> Self {
        Self {
            limit: None,
            start_key: None,
            segment: None,
            total_segments: None,
        }
    }

    /// Set limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set start key for pagination
    pub fn with_start_key(mut self, key: Key) -> Self {
        self.start_key = Some(key);
        self
    }

    /// Set parallel scan parameters
    pub fn with_segment(mut self, segment: usize, total_segments: usize) -> Self {
        self.segment = Some(segment);
        self.total_segments = Some(total_segments);
        self
    }

    /// Check if a stripe should be scanned by this segment
    pub fn should_scan_stripe(&self, stripe_id: usize) -> bool {
        match (self.segment, self.total_segments) {
            (Some(seg), Some(total)) => {
                // Distribute stripes across segments
                stripe_id % total == seg
            }
            _ => true, // No parallel scan - scan all stripes
        }
    }

    /// Check if we should skip a key based on pagination start_key
    pub fn should_skip(&self, key: &Key) -> bool {
        if let Some(start) = &self.start_key {
            // Skip if key <= start_key
            key <= start
        } else {
            false
        }
    }
}

impl Default for ScanParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Scan result (same structure as QueryResult)
pub type ScanResult = QueryResult;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_params_sk_equal() {
        let params = QueryParams::new(Bytes::from("pk1"))
            .with_sk_condition(SortKeyCondition::Equal, Bytes::from("sk1"), None);

        assert!(params.matches_sk(&Some(Bytes::from("sk1"))));
        assert!(!params.matches_sk(&Some(Bytes::from("sk2"))));
        assert!(!params.matches_sk(&None));
    }

    #[test]
    fn test_query_params_sk_less_than() {
        let params = QueryParams::new(Bytes::from("pk1"))
            .with_sk_condition(SortKeyCondition::LessThan, Bytes::from("sk5"), None);

        assert!(params.matches_sk(&Some(Bytes::from("sk1"))));
        assert!(params.matches_sk(&Some(Bytes::from("sk4"))));
        assert!(!params.matches_sk(&Some(Bytes::from("sk5"))));
        assert!(!params.matches_sk(&Some(Bytes::from("sk6"))));
    }

    #[test]
    fn test_query_params_sk_between() {
        let params = QueryParams::new(Bytes::from("pk1")).with_sk_condition(
            SortKeyCondition::Between,
            Bytes::from("sk2"),
            Some(Bytes::from("sk5")),
        );

        assert!(!params.matches_sk(&Some(Bytes::from("sk1"))));
        assert!(params.matches_sk(&Some(Bytes::from("sk2"))));
        assert!(params.matches_sk(&Some(Bytes::from("sk3"))));
        assert!(params.matches_sk(&Some(Bytes::from("sk5"))));
        assert!(!params.matches_sk(&Some(Bytes::from("sk6"))));
    }

    #[test]
    fn test_query_params_sk_begins_with() {
        let params = QueryParams::new(Bytes::from("pk1"))
            .with_sk_condition(SortKeyCondition::BeginsWith, Bytes::from("user#"), None);

        assert!(params.matches_sk(&Some(Bytes::from("user#123"))));
        assert!(params.matches_sk(&Some(Bytes::from("user#456"))));
        assert!(!params.matches_sk(&Some(Bytes::from("post#123"))));
        assert!(!params.matches_sk(&Some(Bytes::from("user"))));
    }

    #[test]
    fn test_query_params_no_condition() {
        let params = QueryParams::new(Bytes::from("pk1"));

        assert!(params.matches_sk(&Some(Bytes::from("sk1"))));
        assert!(params.matches_sk(&Some(Bytes::from("anything"))));
        assert!(params.matches_sk(&None));
    }

    #[test]
    fn test_query_params_pagination_skip() {
        let params = QueryParams::new(Bytes::from("pk1"))
            .with_start_key(Key::with_sk(b"pk1".to_vec(), b"sk3".to_vec()))
            .with_direction(true); // forward

        let key1 = Key::with_sk(b"pk1".to_vec(), b"sk1".to_vec());
        let key2 = Key::with_sk(b"pk1".to_vec(), b"sk3".to_vec());
        let key3 = Key::with_sk(b"pk1".to_vec(), b"sk5".to_vec());

        assert!(params.should_skip(&key1)); // sk1 < sk3
        assert!(params.should_skip(&key2)); // sk3 == sk3
        assert!(!params.should_skip(&key3)); // sk5 > sk3
    }
}

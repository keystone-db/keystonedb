/// Scan builder for DynamoDB-style table scans
///
/// Provides a high-level API for scanning all items in a table.

use kstone_core::{Item, Key, iterator::{ScanParams, ScanResult}};
use bytes::Bytes;

/// Scan builder
pub struct Scan {
    params: ScanParams,
}

impl Scan {
    /// Create a new scan
    pub fn new() -> Self {
        Self {
            params: ScanParams::new(),
        }
    }

    /// Set the maximum number of items to return
    pub fn limit(mut self, limit: usize) -> Self {
        self.params = self.params.with_limit(limit);
        self
    }

    /// Set the exclusive start key for pagination
    pub fn start_after(mut self, pk: &[u8], sk: Option<&[u8]>) -> Self {
        let key = if let Some(sk_bytes) = sk {
            Key::with_sk(Bytes::copy_from_slice(pk), Bytes::copy_from_slice(sk_bytes))
        } else {
            Key::new(Bytes::copy_from_slice(pk))
        };
        self.params = self.params.with_start_key(key);
        self
    }

    /// Configure parallel scan (segment must be < total_segments)
    pub fn segment(mut self, segment: usize, total_segments: usize) -> Self {
        self.params = self.params.with_segment(segment, total_segments);
        self
    }

    /// Get the underlying ScanParams
    pub(crate) fn into_params(self) -> ScanParams {
        self.params
    }
}

impl Default for Scan {
    fn default() -> Self {
        Self::new()
    }
}

/// Scan response
pub struct ScanResponse {
    /// Items found
    pub items: Vec<Item>,
    /// Number of items returned
    pub count: usize,
    /// Last evaluated key (for pagination)
    pub last_key: Option<(Bytes, Option<Bytes>)>,
    /// Number of items examined
    pub scanned_count: usize,
}

impl ScanResponse {
    pub(crate) fn from_result(result: ScanResult) -> Self {
        let last_key = result.last_key.map(|k| (k.pk, k.sk));
        let count = result.items.len();
        Self {
            items: result.items,
            count,
            last_key,
            scanned_count: result.scanned_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_builder() {
        let scan = Scan::new().limit(100);
        let params = scan.into_params();
        assert_eq!(params.limit, Some(100));
    }

    #[test]
    fn test_scan_builder_parallel() {
        let scan = Scan::new().segment(2, 4).limit(50);
        let params = scan.into_params();
        assert_eq!(params.segment, Some(2));
        assert_eq!(params.total_segments, Some(4));
        assert_eq!(params.limit, Some(50));
    }

    #[test]
    fn test_scan_segment_distribution() {
        // Segment 0 of 4 should scan stripes 0, 4, 8, 12, etc.
        let params = ScanParams::new().with_segment(0, 4);
        assert!(params.should_scan_stripe(0));
        assert!(!params.should_scan_stripe(1));
        assert!(!params.should_scan_stripe(2));
        assert!(!params.should_scan_stripe(3));
        assert!(params.should_scan_stripe(4));
    }
}

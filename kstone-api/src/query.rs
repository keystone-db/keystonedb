/// Query builder for DynamoDB-style queries
///
/// Provides a high-level API for querying items within a partition.

use kstone_core::{Item, Key, iterator::{QueryParams, QueryResult, SortKeyCondition}};
use bytes::Bytes;

/// Query builder
pub struct Query {
    params: QueryParams,
}

impl Query {
    /// Create a new query for a partition key
    pub fn new(pk: &[u8]) -> Self {
        Self {
            params: QueryParams::new(Bytes::copy_from_slice(pk)),
        }
    }

    /// Add a sort key equals condition
    pub fn sk_eq(mut self, sk: &[u8]) -> Self {
        self.params = self.params.with_sk_condition(
            SortKeyCondition::Equal,
            Bytes::copy_from_slice(sk),
            None,
        );
        self
    }

    /// Add a sort key less than condition
    pub fn sk_lt(mut self, sk: &[u8]) -> Self {
        self.params = self.params.with_sk_condition(
            SortKeyCondition::LessThan,
            Bytes::copy_from_slice(sk),
            None,
        );
        self
    }

    /// Add a sort key less than or equal condition
    pub fn sk_lte(mut self, sk: &[u8]) -> Self {
        self.params = self.params.with_sk_condition(
            SortKeyCondition::LessThanOrEqual,
            Bytes::copy_from_slice(sk),
            None,
        );
        self
    }

    /// Add a sort key greater than condition
    pub fn sk_gt(mut self, sk: &[u8]) -> Self {
        self.params = self.params.with_sk_condition(
            SortKeyCondition::GreaterThan,
            Bytes::copy_from_slice(sk),
            None,
        );
        self
    }

    /// Add a sort key greater than or equal condition
    pub fn sk_gte(mut self, sk: &[u8]) -> Self {
        self.params = self.params.with_sk_condition(
            SortKeyCondition::GreaterThanOrEqual,
            Bytes::copy_from_slice(sk),
            None,
        );
        self
    }

    /// Add a sort key between condition
    pub fn sk_between(mut self, sk1: &[u8], sk2: &[u8]) -> Self {
        self.params = self.params.with_sk_condition(
            SortKeyCondition::Between,
            Bytes::copy_from_slice(sk1),
            Some(Bytes::copy_from_slice(sk2)),
        );
        self
    }

    /// Add a sort key begins_with condition
    pub fn sk_begins_with(mut self, prefix: &[u8]) -> Self {
        self.params = self.params.with_sk_condition(
            SortKeyCondition::BeginsWith,
            Bytes::copy_from_slice(prefix),
            None,
        );
        self
    }

    /// Set the scan direction (default: forward)
    pub fn forward(mut self, forward: bool) -> Self {
        self.params = self.params.with_direction(forward);
        self
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

    /// Get the underlying QueryParams
    pub(crate) fn into_params(self) -> QueryParams {
        self.params
    }
}

/// Query response
pub struct QueryResponse {
    /// Items found
    pub items: Vec<Item>,
    /// Last evaluated key (for pagination)
    pub last_key: Option<(Bytes, Option<Bytes>)>,
    /// Number of items examined
    pub scanned_count: usize,
}

impl QueryResponse {
    pub(crate) fn from_result(result: QueryResult) -> Self {
        let last_key = result.last_key.map(|k| (k.pk, k.sk));
        Self {
            items: result.items,
            last_key,
            scanned_count: result.scanned_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_builder() {
        let query = Query::new(b"user#123")
            .sk_begins_with(b"post#")
            .forward(true)
            .limit(10);

        let params = query.into_params();
        assert_eq!(params.pk, Bytes::from("user#123"));
        assert_eq!(params.limit, Some(10));
        assert_eq!(params.forward, true);
        assert!(params.sk_condition.is_some());
    }

    #[test]
    fn test_query_builder_between() {
        let query = Query::new(b"user#456")
            .sk_between(b"2024-01-01", b"2024-12-31")
            .limit(50);

        let params = query.into_params();
        assert_eq!(params.limit, Some(50));

        if let Some((condition, val1, val2)) = params.sk_condition {
            assert_eq!(condition, SortKeyCondition::Between);
            assert_eq!(val1, Bytes::from("2024-01-01"));
            assert_eq!(val2, Some(Bytes::from("2024-12-31")));
        } else {
            panic!("Expected Between condition");
        }
    }
}

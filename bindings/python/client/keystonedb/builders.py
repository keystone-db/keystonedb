"""Builder classes for KeystoneDB requests."""

from typing import Optional, Dict

from . import keystone_pb2 as pb


class PutRequestBuilder:
    """Builder for PutRequest.

    Example:
        >>> request = PutRequestBuilder(b"user#123") \\
        ...     .with_string("name", "Alice") \\
        ...     .with_number("age", "30") \\
        ...     .with_bool("active", True) \\
        ...     .build()
    """

    def __init__(self, partition_key: bytes):
        """Initialize builder with partition key.

        Args:
            partition_key: Partition key bytes
        """
        self.partition_key = partition_key
        self.sort_key: Optional[bytes] = None
        self.attributes: Dict[str, any] = {}
        self.condition: Optional[str] = None
        self.expression_values: Dict[str, any] = {}

    def with_sort_key(self, sort_key: bytes) -> "PutRequestBuilder":
        """Set sort key.

        Args:
            sort_key: Sort key bytes

        Returns:
            Self for chaining
        """
        self.sort_key = sort_key
        return self

    def with_string(self, name: str, value: str) -> "PutRequestBuilder":
        """Add string attribute.

        Args:
            name: Attribute name
            value: String value

        Returns:
            Self for chaining
        """
        self.attributes[name] = {"string_value": value}
        return self

    def with_number(self, name: str, value: str) -> "PutRequestBuilder":
        """Add number attribute.

        Args:
            name: Attribute name
            value: Number as string (for precision)

        Returns:
            Self for chaining
        """
        self.attributes[name] = {"number_value": value}
        return self

    def with_bool(self, name: str, value: bool) -> "PutRequestBuilder":
        """Add boolean attribute.

        Args:
            name: Attribute name
            value: Boolean value

        Returns:
            Self for chaining
        """
        self.attributes[name] = {"bool_value": value}
        return self

    def with_binary(self, name: str, value: bytes) -> "PutRequestBuilder":
        """Add binary attribute.

        Args:
            name: Attribute name
            value: Binary value

        Returns:
            Self for chaining
        """
        self.attributes[name] = {"binary_value": value}
        return self

    def with_condition(self, condition: str) -> "PutRequestBuilder":
        """Set condition expression.

        Args:
            condition: Condition expression string

        Returns:
            Self for chaining
        """
        self.condition = condition
        return self

    def build(self) -> pb.PutRequest:
        """Build the PutRequest.

        Returns:
            PutRequest message
        """
        item = pb.Item(attributes={
            name: pb.Value(**val) for name, val in self.attributes.items()
        })

        request = pb.PutRequest(
            partition_key=self.partition_key,
            item=item,
        )

        if self.sort_key is not None:
            request.sort_key = self.sort_key
        if self.condition is not None:
            request.condition_expression = self.condition
        if self.expression_values:
            for k, v in self.expression_values.items():
                request.expression_values[k].CopyFrom(pb.Value(**v))

        return request


class GetRequestBuilder:
    """Builder for GetRequest.

    Example:
        >>> request = GetRequestBuilder(b"user#123").build()
        >>> # With sort key:
        >>> request = GetRequestBuilder(b"user#123") \\
        ...     .with_sort_key(b"profile") \\
        ...     .build()
    """

    def __init__(self, partition_key: bytes):
        """Initialize builder with partition key.

        Args:
            partition_key: Partition key bytes
        """
        self.partition_key = partition_key
        self.sort_key: Optional[bytes] = None

    def with_sort_key(self, sort_key: bytes) -> "GetRequestBuilder":
        """Set sort key.

        Args:
            sort_key: Sort key bytes

        Returns:
            Self for chaining
        """
        self.sort_key = sort_key
        return self

    def build(self) -> pb.GetRequest:
        """Build the GetRequest.

        Returns:
            GetRequest message
        """
        request = pb.GetRequest(partition_key=self.partition_key)
        if self.sort_key is not None:
            request.sort_key = self.sort_key
        return request


class QueryRequestBuilder:
    """Builder for QueryRequest.

    Example:
        >>> request = QueryRequestBuilder(b"org#acme") \\
        ...     .with_limit(10) \\
        ...     .with_index("status-index") \\
        ...     .build()
    """

    def __init__(self, partition_key: bytes):
        """Initialize builder with partition key.

        Args:
            partition_key: Partition key bytes
        """
        self.partition_key = partition_key
        self.sort_key_condition: Optional[Dict] = None
        self.filter_expression: Optional[str] = None
        self.expression_values: Dict[str, any] = {}
        self.index_name: Optional[str] = None
        self.limit: Optional[int] = None
        self.exclusive_start_key: Optional[Dict] = None
        self.scan_forward: Optional[bool] = None

    def with_sort_key_equal(self, value) -> "QueryRequestBuilder":
        """Set sort key equal condition.

        Args:
            value: Value to match

        Returns:
            Self for chaining
        """
        self.sort_key_condition = {"equal_to": value}
        return self

    def with_sort_key_begins_with(self, value) -> "QueryRequestBuilder":
        """Set sort key begins_with condition.

        Args:
            value: Prefix to match

        Returns:
            Self for chaining
        """
        self.sort_key_condition = {"begins_with": value}
        return self

    def with_sort_key_between(self, lower, upper) -> "QueryRequestBuilder":
        """Set sort key between condition.

        Args:
            lower: Lower bound
            upper: Upper bound

        Returns:
            Self for chaining
        """
        self.sort_key_condition = {"between": {"lower": lower, "upper": upper}}
        return self

    def with_limit(self, limit: int) -> "QueryRequestBuilder":
        """Set query limit.

        Args:
            limit: Maximum items to return

        Returns:
            Self for chaining
        """
        self.limit = limit
        return self

    def with_index(self, index_name: str) -> "QueryRequestBuilder":
        """Set index name.

        Args:
            index_name: Name of index to query

        Returns:
            Self for chaining
        """
        self.index_name = index_name
        return self

    def with_scan_forward(self, forward: bool) -> "QueryRequestBuilder":
        """Set scan direction.

        Args:
            forward: True for ascending, False for descending

        Returns:
            Self for chaining
        """
        self.scan_forward = forward
        return self

    def build(self):
        """Build the QueryRequest.

        Returns:
            QueryRequest message (once protobuf is generated)
        """
        return {
            "partition_key": self.partition_key,
            "sort_key_condition": self.sort_key_condition,
            "filter_expression": self.filter_expression,
            "expression_values": self.expression_values,
            "index_name": self.index_name,
            "limit": self.limit,
            "exclusive_start_key": self.exclusive_start_key,
            "scan_forward": self.scan_forward,
        }


class ScanRequestBuilder:
    """Builder for ScanRequest.

    Example:
        >>> request = ScanRequestBuilder() \\
        ...     .with_limit(100) \\
        ...     .with_segment(0, 4) \\
        ...     .build()
    """

    def __init__(self):
        """Initialize empty builder."""
        self.filter_expression: Optional[str] = None
        self.expression_values: Dict[str, any] = {}
        self.limit: Optional[int] = None
        self.exclusive_start_key: Optional[Dict] = None
        self.index_name: Optional[str] = None
        self.segment: Optional[int] = None
        self.total_segments: Optional[int] = None

    def with_limit(self, limit: int) -> "ScanRequestBuilder":
        """Set scan limit.

        Args:
            limit: Maximum items to return

        Returns:
            Self for chaining
        """
        self.limit = limit
        return self

    def with_segment(self, segment: int, total_segments: int) -> "ScanRequestBuilder":
        """Set parallel scan segment.

        Args:
            segment: Segment number (0-based)
            total_segments: Total number of parallel segments

        Returns:
            Self for chaining
        """
        self.segment = segment
        self.total_segments = total_segments
        return self

    def build(self):
        """Build the ScanRequest.

        Returns:
            ScanRequest message (once protobuf is generated)
        """
        return {
            "filter_expression": self.filter_expression,
            "expression_values": self.expression_values,
            "limit": self.limit,
            "exclusive_start_key": self.exclusive_start_key,
            "index_name": self.index_name,
            "segment": self.segment,
            "total_segments": self.total_segments,
        }

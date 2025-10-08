"""KeystoneDB gRPC client."""

import grpc
from typing import Optional, List, Iterator

from . import keystone_pb2 as pb
from . import keystone_pb2_grpc as pb_grpc


class Client:
    """Client for KeystoneDB gRPC server.

    Example:
        >>> client = Client.connect("localhost:50051")
        >>> # Use client methods...
        >>> client.close()
    """

    def __init__(self, channel: grpc.Channel, stub):
        """Initialize client with channel and stub.

        Use Client.connect() instead of calling this directly.

        Args:
            channel: gRPC channel
            stub: KeystoneDB service stub
        """
        self._channel = channel
        self._stub = stub

    @classmethod
    def connect(cls, address: str) -> "Client":
        """Connect to KeystoneDB server.

        Args:
            address: Server address (e.g., "localhost:50051")

        Returns:
            Client instance

        Example:
            >>> client = Client.connect("localhost:50051")
        """
        channel = grpc.insecure_channel(address)
        stub = pb_grpc.KeystoneDBStub(channel)
        return cls(channel, stub)

    def close(self):
        """Close the client connection."""
        if self._channel:
            self._channel.close()

    def put(self, request):
        """Store an item in the database.

        Args:
            request: PutRequest message

        Returns:
            PutResponse message

        Example:
            >>> from keystonedb import PutRequestBuilder
            >>> request = PutRequestBuilder(b"user#123") \\
            ...     .with_string("name", "Alice") \\
            ...     .with_number("age", "30") \\
            ...     .build()
            >>> response = client.put(request)
        """
        return self._stub.Put(request)

    def get(self, request):
        """Retrieve an item from the database.

        Args:
            request: GetRequest message

        Returns:
            GetResponse message with optional item

        Example:
            >>> from keystonedb import GetRequestBuilder
            >>> request = GetRequestBuilder(b"user#123").build()
            >>> response = client.get(request)
            >>> if response.item:
            ...     print("Found item")
        """
        return self._stub.Get(request)

    def delete(self, request):
        """Remove an item from the database.

        Args:
            request: DeleteRequest message

        Returns:
            DeleteResponse message
        """
        return self._stub.Delete(request)

    def query(self, request):
        """Perform a query operation.

        Args:
            request: QueryRequest message

        Returns:
            QueryResponse message with items

        Example:
            >>> from keystonedb import QueryRequestBuilder
            >>> request = QueryRequestBuilder(b"org#acme") \\
            ...     .with_limit(10) \\
            ...     .build()
            >>> response = client.query(request)
            >>> print(f"Found {response.count} items")
        """
        return self._stub.Query(request)

    def scan(self, request) -> List:
        """Perform a scan operation with streaming.

        Args:
            request: ScanRequest message

        Returns:
            List of items from all scan responses

        Example:
            >>> from keystonedb import ScanRequestBuilder
            >>> request = ScanRequestBuilder() \\
            ...     .with_limit(100) \\
            ...     .build()
            >>> items = client.scan(request)
        """
        items = []
        for response in self._stub.Scan(request):
            if response.error:
                raise Exception(f"Scan error: {response.error}")
            items.extend(response.items)
        return items

    def batch_get(self, request):
        """Retrieve multiple items.

        Args:
            request: BatchGetRequest message

        Returns:
            BatchGetResponse message
        """
        return self._stub.BatchGet(request)

    def batch_write(self, request):
        """Write multiple items.

        Args:
            request: BatchWriteRequest message

        Returns:
            BatchWriteResponse message
        """
        return self._stub.BatchWrite(request)

    def transact_get(self, request):
        """Perform a transactional get.

        Args:
            request: TransactGetRequest message

        Returns:
            TransactGetResponse message
        """
        return self._stub.TransactGet(request)

    def transact_write(self, request):
        """Perform a transactional write.

        Args:
            request: TransactWriteRequest message

        Returns:
            TransactWriteResponse message
        """
        return self._stub.TransactWrite(request)

    def update(self, request):
        """Update an item.

        Args:
            request: UpdateRequest message

        Returns:
            UpdateResponse message
        """
        return self._stub.Update(request)

    def execute_statement(self, request):
        """Execute a PartiQL statement.

        Args:
            request: ExecuteStatementRequest message

        Returns:
            ExecuteStatementResponse message
        """
        return self._stub.ExecuteStatement(request)

    def __enter__(self):
        """Context manager entry."""
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        """Context manager exit."""
        self.close()

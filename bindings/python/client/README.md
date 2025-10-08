# KeystoneDB Python gRPC Client

Python client library for connecting to a remote KeystoneDB server via gRPC.

## Installation

```bash
pip install keystonedb-client
```

Or from source:
```bash
cd bindings/python/client
pip install -e .
```

## Prerequisites

Before using the client, generate the protobuf files:

```bash
python -m grpc_tools.protoc \
    --python_out=keystonedb \
    --grpc_python_out=keystonedb \
    --proto_path=../../../kstone-proto/proto \
    ../../../kstone-proto/proto/keystone.proto
```

## Usage

### Basic Operations

```python
from keystonedb import Client, PutRequestBuilder, GetRequestBuilder

# Connect to server
client = Client.connect("localhost:50051")

try:
    # Put an item
    request = PutRequestBuilder(b"user#123") \
        .with_string("name", "Alice") \
        .with_number("age", "30") \
        .with_bool("active", True) \
        .build()

    response = client.put(request)
    print(f"Put success: {response.success}")

    # Get an item
    request = GetRequestBuilder(b"user#123").build()
    response = client.get(request)

    if response.item:
        print(f"Found item: {response.item}")

finally:
    client.close()
```

### Using Context Manager

```python
with Client.connect("localhost:50051") as client:
    # Perform operations
    request = GetRequestBuilder(b"user#123").build()
    response = client.get(request)
```

### Query Operations

```python
from keystonedb import QueryRequestBuilder

# Query with sort key condition
request = QueryRequestBuilder(b"org#acme") \
    .with_sort_key_begins_with({"string_value": "USER#"}) \
    .with_limit(10) \
    .with_scan_forward(False) \
    .build()

response = client.query(request)
print(f"Found {response.count} items")

for item in response.items:
    print(item)
```

### Scan Operations

```python
from keystonedb import ScanRequestBuilder

# Scan all items
request = ScanRequestBuilder() \
    .with_limit(100) \
    .build()

items = client.scan(request)  # Returns list of all items
print(f"Scanned {len(items)} items")

# Parallel scan with 4 segments
segment_requests = [
    ScanRequestBuilder().with_segment(i, 4).build()
    for i in range(4)
]

# Process segments in parallel (using threading/asyncio)
all_items = []
for req in segment_requests:
    items = client.scan(req)
    all_items.extend(items)
```

### Batch Operations

```python
from keystonedb import keystone_pb2 as pb

# Batch get
request = pb.BatchGetRequest(
    keys=[
        pb.Key(partition_key=b"user#1"),
        pb.Key(partition_key=b"user#2"),
    ]
)
response = client.batch_get(request)

# Batch write
request = pb.BatchWriteRequest(
    writes=[
        pb.WriteRequest(
            put=pb.PutItem(
                partition_key=b"user#1",
                item=pb.Item(
                    attributes={
                        "name": pb.Value(string_value="Alice"),
                        "age": pb.Value(number_value="30"),
                    }
                ),
            )
        ),
        pb.WriteRequest(
            delete=pb.DeleteKey(partition_key=b"user#2")
        ),
    ]
)
response = client.batch_write(request)
```

## Features

- Connect to remote KeystoneDB server via gRPC
- Full support for CRUD operations (Put, Get, Delete)
- Query with sort key conditions
- Scan with server-side streaming
- Batch operations (BatchGet, BatchWrite)
- Transaction support (TransactGet, TransactWrite)
- Update expressions
- PartiQL query execution
- Builder pattern for common operations
- Context manager support for automatic cleanup

## API Reference

### Client

- `Client.connect(address: str) -> Client` - Connect to server
- `client.close()` - Close connection
- `client.put(request) -> PutResponse` - Store item
- `client.get(request) -> GetResponse` - Retrieve item
- `client.delete(request) -> DeleteResponse` - Remove item
- `client.query(request) -> QueryResponse` - Query items
- `client.scan(request) -> List[Item]` - Scan all items (with streaming)
- `client.batch_get(request) -> BatchGetResponse` - Get multiple items
- `client.batch_write(request) -> BatchWriteResponse` - Write multiple items
- `client.transact_get(request) -> TransactGetResponse` - Transactional get
- `client.transact_write(request) -> TransactWriteResponse` - Transactional write
- `client.update(request) -> UpdateResponse` - Update item
- `client.execute_statement(request) -> ExecuteStatementResponse` - Execute PartiQL

### Builders

- `PutRequestBuilder(pk: bytes)` - Build put request
  - `.with_string(name, value)` - Add string attribute
  - `.with_number(name, value)` - Add number attribute
  - `.with_bool(name, value)` - Add boolean attribute
  - `.with_binary(name, value)` - Add binary attribute
  - `.with_sort_key(sk: bytes)` - Set sort key
  - `.with_condition(expr: str)` - Set condition expression
  - `.build()` - Build request

- `GetRequestBuilder(pk: bytes)` - Build get request
  - `.with_sort_key(sk: bytes)` - Set sort key
  - `.build()` - Build request

- `QueryRequestBuilder(pk: bytes)` - Build query request
  - `.with_sort_key_equal(value)` - Equal condition
  - `.with_sort_key_begins_with(value)` - Begins with condition
  - `.with_sort_key_between(lower, upper)` - Between condition
  - `.with_limit(n: int)` - Set limit
  - `.with_index(name: str)` - Query index
  - `.with_scan_forward(forward: bool)` - Set direction
  - `.build()` - Build request

- `ScanRequestBuilder()` - Build scan request
  - `.with_limit(n: int)` - Set limit
  - `.with_segment(segment: int, total: int)` - Parallel scan
  - `.build()` - Build request

## Development

Generate protobuf files:

```bash
python -m grpc_tools.protoc \
    --python_out=keystonedb \
    --grpc_python_out=keystonedb \
    --proto_path=../../../kstone-proto/proto \
    ../../../kstone-proto/proto/keystone.proto
```

Run tests:

```bash
pytest tests/
```

## License

MIT OR Apache-2.0

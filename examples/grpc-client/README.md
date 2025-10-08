# Multi-Language gRPC Client Examples

This directory contains examples of KeystoneDB gRPC clients in **three languages**: Go, Python, and JavaScript. All examples implement the same task management system to demonstrate:

- **Language Interoperability**: All clients work with the same KeystoneDB server
- **Consistent API**: Similar operations across different language bindings
- **Remote Access**: Database operations over gRPC network protocol

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Go Client  │     │ Python Cli  │     │  JS Client  │
│  (gRPC)     │     │   (gRPC)    │     │   (gRPC)    │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │
       └───────────────────┼───────────────────┘
                           │
                           ▼
                  ┌────────────────┐
                  │  kstone-server │
                  │   (gRPC API)   │
                  └────────┬───────┘
                           │
                           ▼
                  ┌────────────────┐
                  │   KeystoneDB   │
                  │  (Embedded)    │
                  └────────────────┘
```

## Prerequisites

### 1. Start the KeystoneDB gRPC Server

From the repository root:

```bash
# Build the server
cargo build --release --bin kstone-server

# Start the server
./target/release/kstone-server --db-path /tmp/tasks.keystone --port 50051
```

The server will start on `http://127.0.0.1:50051`.

### 2. Language-Specific Setup

#### Go
```bash
cd examples/grpc-client/go
go mod download
```

#### Python
```bash
cd examples/grpc-client/python
pip install -r requirements.txt
```

#### JavaScript
```bash
cd examples/grpc-client/javascript
npm install
```

## Running the Examples

Each example implements the same task management operations:
1. Create tasks
2. Retrieve tasks
3. Update task status
4. Delete tasks
5. Query tasks by project

### Go Example

```bash
cd examples/grpc-client/go
go run main.go
```

### Python Example

```bash
cd examples/grpc-client/python
python tasks.py
```

### JavaScript Example

```bash
cd examples/grpc-client/javascript
node tasks.js
```

## Expected Output

All three examples should produce similar output:

```
Connected to KeystoneDB server at localhost:50051

--- Creating Tasks ---
✅ Created task: task#1
✅ Created task: task#2
✅ Created task: task#3

--- Retrieving Task ---
Task task#1:
  title: Implement user authentication
  status: in-progress
  priority: high

--- Querying Tasks by Project ---
Found 2 tasks for project#backend

--- Updating Task Status ---
✅ Updated task#1 to completed

--- Deleting Task ---
✅ Deleted task#3

--- Batch Operations ---
Retrieved 2 tasks in batch operation

✅ All operations completed successfully!
```

## Feature Comparison

| Feature | Go | Python | JavaScript |
|---------|-------|--------|------------|
| Put (Create) | ✅ | ✅ | ✅ |
| Get (Retrieve) | ✅ | ✅ | ✅ |
| Delete | ✅ | ✅ | ✅ |
| Query | ✅ | ✅ | ✅ |
| BatchGet | ✅ | ✅ | ✅ |
| BatchWrite | ✅ | ✅ | ✅ |
| Async/Promises | No | Yes | Yes |
| Type Safety | Yes | No | Yes (TypeScript) |

## gRPC Service Methods

All examples use the following KeystoneDB gRPC methods:

- `Put(PutRequest)` - Store an item
- `Get(GetRequest)` - Retrieve an item
- `Delete(DeleteRequest)` - Remove an item
- `Query(QueryRequest)` - Query items by partition key with sort key conditions
- `BatchGet(BatchGetRequest)` - Retrieve multiple items in one call
- `BatchWrite(BatchWriteRequest)` - Put/delete multiple items in one call

## Task Data Model

All examples use the same data model:

**Partition Key Pattern**:
- `task#{id}` - Individual tasks

**Partition + Sort Key Pattern**:
- PK: `project#{name}`
- SK: `task#{id}`
- Enables querying all tasks for a project

**Item Attributes**:
```json
{
  "title": "string",
  "description": "string",
  "status": "string",      // pending, in-progress, completed
  "priority": "string",    // low, medium, high
  "created": 1704067200    // Unix timestamp
}
```

## Interoperability Demo

You can demonstrate interoperability by:

1. **Create tasks** using the Python client
2. **Query tasks** using the Go client
3. **Update tasks** using the JavaScript client
4. **Verify changes** using any client

All clients work with the same database!

```bash
# Terminal 1: Start server
./target/release/kstone-server --db-path /tmp/shared-tasks.keystone --port 50051

# Terminal 2: Create tasks with Python
cd examples/grpc-client/python
python tasks.py

# Terminal 3: Query with Go
cd examples/grpc-client/go
go run main.go

# Terminal 4: Update with JavaScript
cd examples/grpc-client/javascript
node tasks.js
```

## Error Handling

All examples demonstrate proper error handling:

- **Connection errors**: Server not running
- **Not found errors**: Item doesn't exist
- **Invalid requests**: Malformed data
- **Network errors**: Connection timeout

## Next Steps

### Production Deployment
- Add TLS/SSL for secure communication
- Implement authentication and authorization
- Add connection pooling and retry logic
- Set up health checks and monitoring

### Advanced Features
- Implement PartiQL queries (when available)
- Use transactions for atomic operations
- Add streaming support for large result sets
- Implement caching strategies

### Testing
- Add integration tests for each client
- Test error scenarios
- Benchmark client performance
- Test concurrent access patterns

## Troubleshooting

### Server Connection Issues

**Error**: `Failed to connect to server`

**Solution**: Ensure kstone-server is running:
```bash
./target/release/kstone-server --db-path /tmp/test.keystone --port 50051
```

**Error**: `Connection refused`

**Solution**: Check the port and host:
- Default: `http://127.0.0.1:50051`
- Verify firewall settings
- Check if port 50051 is available: `lsof -i :50051`

### Protobuf Issues

**Error**: `Module not found` or `Cannot find package`

**Solution**: Regenerate protobuf files:
```bash
# From repository root
make proto  # or your build command for protobuf generation
```

### Language-Specific Issues

See individual README files in each language directory:
- `go/README.md`
- `python/README.md`
- `javascript/README.md`

## Resources

- [KeystoneDB gRPC API Documentation](../../bindings/README.md)
- [Protocol Buffers](https://protobuf.dev/)
- [gRPC Documentation](https://grpc.io/docs/)
- Language-specific bindings:
  - [Go gRPC Client](../../bindings/go/client/README.md)
  - [Python gRPC Client](../../bindings/python/client/README.md)
  - [JavaScript gRPC Client](../../bindings/javascript/client/README.md)

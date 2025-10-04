# Todo List REST API

A comprehensive todo list REST API built with KeystoneDB, demonstrating advanced database features and REST API design patterns.

## Features

- **Full CRUD Operations**: Create, Read, Update, Delete todos
- **Status Management**: Track todos through Pending → InProgress → Completed states
- **Priority System**: 5-level priority system (1-5)
- **Conditional Operations**: Prevent double completion with conditional checks
- **Batch Operations**: Execute multiple operations atomically (create, update, delete)
- **Timestamps**: Automatic tracking of creation, update, and completion times
- **Input Validation**: Robust validation for all inputs
- **Error Handling**: Proper HTTP status codes and error messages
- **Health Monitoring**: Health check and statistics endpoints

## Data Model

### Todo Item

```
PK: "todo#{uuid}"

Attributes:
{
  id: String           // UUID v4
  title: String        // Todo title (required)
  description: String  // Optional description
  status: String       // "pending" | "inprogress" | "completed"
  priority: Number     // 1-5 (higher = more important)
  created_at: Number   // Unix timestamp
  updated_at: Number   // Unix timestamp
  completed_at: Number // Unix timestamp (optional, only when completed)
}
```

### Status Flow

```
Pending → InProgress → Completed
```

## API Endpoints

### Todo Operations

#### Create Todo
```bash
POST /todos
Content-Type: application/json

{
  "title": "Build the REST API",
  "description": "Complete the todo-api example",
  "priority": 4
}

Response: 201 Created
{
  "todo": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "title": "Build the REST API",
    "description": "Complete the todo-api example",
    "status": "pending",
    "priority": 4,
    "created_at": 1704067200,
    "updated_at": 1704067200
  }
}
```

#### Get Todo
```bash
GET /todos/{id}

Response: 200 OK
{
  "todo": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "title": "Build the REST API",
    "status": "pending",
    ...
  }
}
```

#### List Todos
```bash
GET /todos

Response: 200 OK
{
  "todos": [...],
  "count": 5
}
```

#### Update Todo
```bash
PATCH /todos/{id}
Content-Type: application/json

{
  "status": "inprogress",
  "priority": 5
}

Response: 200 OK
{
  "todo": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "status": "inprogress",
    "priority": 5,
    ...
  }
}
```

#### Delete Todo
```bash
DELETE /todos/{id}

Response: 204 No Content
```

### Special Operations

#### Complete Todo (Conditional)
Mark a todo as completed with conditional check to prevent double completion.

```bash
POST /todos/{id}/complete

Response: 200 OK (if not already completed)
Response: 409 Conflict (if already completed)
```

#### Batch Operations
Execute multiple operations in a batch (note: currently sequential, not atomic).

```bash
POST /todos/batch
Content-Type: application/json

{
  "operations": [
    {
      "operation": "create",
      "title": "Todo 1",
      "priority": 3
    },
    {
      "operation": "update",
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "status": "completed"
    },
    {
      "operation": "delete",
      "id": "another-uuid-here"
    }
  ]
}

Response: 200 OK
{
  "success": true,
  "operations_completed": 3,
  "results": [
    {
      "operation": "create",
      "success": true,
      "id": "new-uuid"
    },
    ...
  ]
}
```

### System Endpoints

#### Health Check
```bash
GET /api/health

Response: 200 OK
{
  "status": "Healthy",
  "warnings": [],
  "errors": []
}
```

#### Statistics
```bash
GET /api/stats

Response: 200 OK
{
  "total_todos": 10,
  "by_status": {
    "pending": 3,
    "in_progress": 4,
    "completed": 3
  },
  "database_stats": {
    "total_keys": 10,
    "total_sst_files": 2,
    "wal_size_bytes": 4096,
    "memtable_size_bytes": 8192,
    "total_disk_size_bytes": 12288
  }
}
```

## How to Run

### Prerequisites
- Rust 1.70 or later

### Build and Run

```bash
# From the repository root
cargo build -p todo-api

# Run the API server
cargo run -p todo-api
```

The server will start on `http://127.0.0.1:3002`.

### Example Usage

```bash
# Create a todo
curl -X POST http://127.0.0.1:3002/todos \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Learn KeystoneDB",
    "description": "Study the examples and documentation",
    "priority": 5
  }'

# Get a todo
curl http://127.0.0.1:3002/todos/550e8400-e29b-41d4-a716-446655440000

# Update a todo
curl -X PATCH http://127.0.0.1:3002/todos/550e8400-e29b-41d4-a716-446655440000 \
  -H "Content-Type: application/json" \
  -d '{"status": "inprogress"}'

# Complete a todo
curl -X POST http://127.0.0.1:3002/todos/550e8400-e29b-41d4-a716-446655440000/complete

# Delete a todo
curl -X DELETE http://127.0.0.1:3002/todos/550e8400-e29b-41d4-a716-446655440000

# Check health
curl http://127.0.0.1:3002/api/health

# Get statistics
curl http://127.0.0.1:3002/api/stats
```

## KeystoneDB Features Used

### Core Operations
- **Put**: Store and update todo items
- **Get**: Retrieve individual todos by ID
- **Delete**: Remove todos

### Data Modeling
- **Composite Keys**: Using `todo#{uuid}` pattern for primary keys
- **Attributes**: Rich attribute types (String, Number)
- **Optional Fields**: Handling optional descriptions and completed_at

### Advanced Features
- **Conditional Operations**: Prevent double completion (simulated with status check)
- **Batch Operations**: Multiple operations in sequence (future: atomic transactions)
- **Timestamps**: Unix epoch timestamps for audit trail
- **Input Validation**: Client-side validation before database operations

### Operational Features
- **Health Monitoring**: Database health checks
- **Statistics**: Key counts, storage metrics, compaction stats
- **Error Handling**: Proper error propagation and HTTP status codes

## Architecture Patterns

### REST API Design
- Resource-based URLs (`/todos`, `/todos/{id}`)
- HTTP verbs matching operations (GET, POST, PATCH, DELETE)
- Proper status codes (200, 201, 204, 400, 404, 409, 500)
- JSON request/response bodies
- Idempotent operations where appropriate

### Error Handling
- Custom `AppError` enum for domain errors
- `IntoResponse` implementation for HTTP error responses
- Structured error responses with descriptive messages

### State Management
- `Arc<Database>` for thread-safe shared state
- Axum extractors for clean dependency injection
- Cloneable app state for router sharing

## Limitations & Future Enhancements

### Current Limitations
1. **No List/Scan**: The `list_todos` endpoint returns empty because KeystoneDB doesn't yet have a built-in scan operation. In production, you would:
   - Implement a secondary index for listing
   - Maintain a sorted set of todo IDs
   - Use a separate table for indexing

2. **Sequential Batch Operations**: Batch operations are executed sequentially, not atomically. Future enhancement will use KeystoneDB transactions.

3. **No Query/Filter**: Cannot filter todos by status or priority. Future enhancement will use query operations or secondary indexes.

### Future Enhancements
- **Transactions**: Atomic batch operations using KeystoneDB transactions
- **Secondary Indexes**: Index by status, priority, or due date
- **Query Support**: Filter and sort todos
- **Pagination**: Limit/offset for large todo lists
- **Search**: Full-text search on titles and descriptions
- **Due Dates**: Add TTL-based reminders
- **Tags**: Multi-valued attributes for categorization

## Performance Notes

### Write Performance
- Todo items are small (~200-500 bytes each)
- Writes are fast due to WAL-based storage
- Memtable flush occurs at ~1000 items

### Read Performance
- Point reads (GET /todos/{id}) are very fast
- List operations would benefit from secondary indexes
- Consider caching for frequently accessed todos

### Scaling
- Single file database suitable for thousands of todos
- For millions of todos, consider:
  - Partitioning by user ID
  - Using stripe-based storage (KeystoneDB feature)
  - Implementing compaction strategies

## License

This example is part of the KeystoneDB project and follows the same license (MIT OR Apache-2.0).

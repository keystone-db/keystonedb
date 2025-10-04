# Blog Engine Example

A multi-user blog platform demonstrating advanced KeystoneDB features including composite keys, the Query API, analytics, and hierarchical data modeling.

## Features Demonstrated

This example showcases the most advanced KeystoneDB capabilities:

- **Composite Keys (PK + SK)**: Hierarchical data organization with partition and sort keys
- **Query API**: Efficient retrieval of items within a partition
- **Scan API**: Full table scans for analytics
- **View Tracking**: Counter increments for post popularity
- **Tag-based Filtering**: Simulated Global Secondary Index (GSI) pattern
- **Multi-user Platform**: Multiple authors with their own post collections

## Architecture

### Data Model

The blog uses a composite key structure optimized for access patterns:

```
Primary Key (PK): "author#{author_id}"
Sort Key (SK):    "post#{timestamp}#{post_id}"

Item Attributes:
{
  "author_id": String,      // Author's unique ID
  "post_id": String,         // Post's UUID
  "title": String,           // Post title
  "content": String,         // Post content (markdown/text)
  "tags": String,            // Comma-separated tags
  "views": Number,           // View count
  "created_at": Number,      // Unix timestamp
  "updated_at": Number       // Unix timestamp
}
```

### Access Patterns

This design efficiently supports:

1. **Get all posts by author** (Query on PK)
   - Uses `Query::new(pk)` to retrieve all items with matching partition key
   - Results are automatically sorted by SK (timestamp + post_id)

2. **Get specific post** (Query on PK, filter by post_id)
   - Query the author's partition
   - Filter results by post_id attribute

3. **List posts by tag** (Scan with filter)
   - Currently uses full table scan
   - In production: would use GSI on tags attribute

4. **Popular posts** (Scan and sort by views)
   - Scans all posts
   - Sorts by view count
   - In production: could use PartiQL or maintain separate index

### Composite Key Benefits

Using PK + SK provides several advantages:

- **Automatic Sorting**: Posts are naturally ordered by creation time within each author
- **Efficient Queries**: Retrieve all posts for an author without scanning the entire database
- **Scalability**: Data is partitioned by author, distributing load
- **Range Queries**: Can query posts within a time range using SK conditions (future feature)

## API Endpoints

### Post Operations

#### Create Post
```bash
POST /posts
Content-Type: application/json

{
  "author_id": "alice",
  "title": "Getting Started with KeystoneDB",
  "content": "KeystoneDB is a powerful embedded database...",
  "tags": ["database", "rust", "tutorial"]
}

# Response
{
  "post_id": "550e8400-e29b-41d4-a716-446655440000",
  "author_id": "alice",
  "title": "Getting Started with KeystoneDB",
  "content": "KeystoneDB is a powerful embedded database...",
  "tags": ["database", "rust", "tutorial"],
  "views": 0,
  "created_at": 1709856000,
  "updated_at": 1709856000
}
```

#### List Author's Posts
```bash
GET /posts/:author

# Example
curl http://localhost:3003/posts/alice

# Response
{
  "posts": [
    {
      "post_id": "550e8400-e29b-41d4-a716-446655440000",
      "author_id": "alice",
      "title": "Getting Started with KeystoneDB",
      "content": "...",
      "tags": ["database", "rust", "tutorial"],
      "views": 42,
      "created_at": 1709856000,
      "updated_at": 1709856000
    }
  ],
  "count": 1
}
```

This endpoint uses the **Query API** with composite keys:
```rust
// Query all posts for this author
let pk = format!("author#{}", author_id);
let query = Query::new(pk.as_bytes());
let response = state.db.query(query)?;
```

#### Get Specific Post
```bash
GET /posts/:author/:post_id

# Example
curl http://localhost:3003/posts/alice/550e8400-e29b-41d4-a716-446655440000

# Automatically increments view counter
```

#### Update Post
```bash
PATCH /posts/:author/:post_id
Content-Type: application/json

{
  "title": "Updated Title",
  "tags": ["database", "rust", "tutorial", "beginner"]
}

# Partial updates supported - only include fields you want to change
```

#### Delete Post
```bash
DELETE /posts/:author/:post_id

# Returns 204 No Content on success
```

### Tag Operations

#### List All Tags
```bash
GET /tags

# Response
[
  {
    "tag": "rust",
    "post_count": 15
  },
  {
    "tag": "database",
    "post_count": 12
  },
  {
    "tag": "tutorial",
    "post_count": 8
  }
]
```

#### Get Posts by Tag
```bash
GET /tags/:tag

# Example
curl http://localhost:3003/tags/rust

# Response
{
  "posts": [...],
  "count": 15
}
```

Note: Tag filtering currently uses a full table scan. In a production system with Global Secondary Indexes (GSI), you would:
1. Create a GSI with PK = "tag#{tag_name}"
2. Query the index directly for O(1) tag lookups

### Analytics

#### Popular Posts
```bash
GET /stats/popular

# Returns top 10 most viewed posts
# In production, could use PartiQL:
# SELECT * FROM posts ORDER BY views DESC LIMIT 10
```

### System Endpoints

#### Health Check
```bash
GET /api/health

# Response
{
  "status": "Healthy",
  "warnings": [],
  "errors": []
}
```

#### Database Statistics
```bash
GET /api/stats

# Response
{
  "total_posts": 25,
  "total_keys": 25,
  "total_sst_files": 3,
  "wal_size_bytes": 4096,
  "memtable_size_bytes": 8192,
  "total_disk_size_bytes": 102400,
  "compaction": {
    "total_compactions": 2,
    "total_ssts_merged": 5,
    "total_bytes_read": 51200,
    "total_bytes_written": 50000
  }
}
```

## Running the Example

### Start the Server
```bash
# From the repository root
cargo run -p blog-engine

# Or build and run
cargo build --release -p blog-engine
./target/release/blog-engine
```

The server will start on `http://127.0.0.1:3003`

### Example Workflow

```bash
# 1. Create a post
curl -X POST http://localhost:3003/posts \
  -H "Content-Type: application/json" \
  -d '{
    "author_id": "alice",
    "title": "My First Post",
    "content": "Hello, KeystoneDB!",
    "tags": ["hello", "first-post"]
  }'

# 2. Create another post
curl -X POST http://localhost:3003/posts \
  -H "Content-Type: application/json" \
  -d '{
    "author_id": "alice",
    "title": "Advanced KeystoneDB",
    "content": "Deep dive into composite keys...",
    "tags": ["advanced", "tutorial"]
  }'

# 3. List all posts by Alice (uses Query API)
curl http://localhost:3003/posts/alice

# 4. Create post by another author
curl -X POST http://localhost:3003/posts \
  -H "Content-Type: application/json" \
  -d '{
    "author_id": "bob",
    "title": "Bob's Guide",
    "content": "My perspective on databases",
    "tags": ["tutorial"]
  }'

# 5. View a specific post (increments view counter)
curl http://localhost:3003/posts/alice/{post_id}

# 6. Get posts by tag
curl http://localhost:3003/tags/tutorial

# 7. List all tags with counts
curl http://localhost:3003/tags

# 8. Get most popular posts
curl http://localhost:3003/stats/popular

# 9. Update a post
curl -X PATCH http://localhost:3003/posts/alice/{post_id} \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Updated Title",
    "tags": ["updated", "tutorial"]
  }'

# 10. Delete a post
curl -X DELETE http://localhost:3003/posts/alice/{post_id}

# 11. Check database health
curl http://localhost:3003/api/health

# 12. Get database statistics
curl http://localhost:3003/api/stats
```

## KeystoneDB Features Used

### 1. Composite Keys
```rust
// Store with partition key and sort key
let pk = format!("author#{}", author_id);
let sk = format!("post#{}#{}", timestamp, post_id);
state.db.put_with_sk(pk.as_bytes(), sk.as_bytes(), item)?;

// Retrieve with both keys
state.db.get_with_sk(pk.as_bytes(), sk.as_bytes())?;

// Delete with both keys
state.db.delete_with_sk(pk.as_bytes(), sk.as_bytes())?;
```

### 2. Query API
```rust
// Query all items within a partition
let query = Query::new(pk.as_bytes());
let response = state.db.query(query)?;

// Results are automatically sorted by sort key
for item in response.items {
    // Process posts in chronological order
}
```

### 3. Scan API
```rust
// Scan all items for analytics
let scan = Scan::new();
let response = state.db.scan(scan)?;

// Process all posts for tag extraction or popularity ranking
```

### 4. ItemBuilder for Complex Data
```rust
let item = ItemBuilder::new()
    .string("author_id", &author_id)
    .string("post_id", &post_id)
    .string("title", &title)
    .string("content", &content)
    .string("tags", &tags_str)
    .number("views", views)
    .number("created_at", created_at)
    .number("updated_at", updated_at)
    .build();
```

## Future Enhancements

When additional KeystoneDB features are implemented:

### Global Secondary Indexes (GSI)
```rust
// Create GSI on tags
db.create_gsi("tags-index", |item| {
    // Index each tag separately
    item.get("tags")
        .map(|tags| tags.split(','))
        .map(|tags| tags.map(|tag| format!("tag#{}", tag.trim())))
});

// Query by tag efficiently
let query = Query::new(b"tag#rust").use_index("tags-index");
```

### PartiQL Queries
```sql
-- Most popular posts
SELECT * FROM posts ORDER BY views DESC LIMIT 10;

-- Recent posts across all authors
SELECT * FROM posts
WHERE created_at > 1709856000
ORDER BY created_at DESC;

-- Posts by tag
SELECT * FROM posts
WHERE tags CONTAINS 'rust';
```

### Streams for Activity Feed
```rust
// Stream all post changes
let stream = db.stream(StreamType::NewAndOldImages);

// Build real-time activity feed
for event in stream {
    match event {
        StreamEvent::Insert(item) => {
            // New post published
        }
        StreamEvent::Modify(old, new) => {
            // Post updated
        }
        StreamEvent::Remove(item) => {
            // Post deleted
        }
    }
}
```

### TTL for Draft Posts
```rust
// Auto-delete drafts after 7 days
ItemBuilder::new()
    .string("status", "draft")
    .ttl(now + 7 * 24 * 3600)
    .build();
```

## Technical Notes

### Composite Key Encoding
Keys are encoded as:
```
PK bytes: [pk_len (4 bytes) | pk_bytes | sk_len (4 bytes) | sk_bytes]
```

This allows:
- Efficient prefix matching for queries
- Automatic sorting by SK within each partition
- Proper key comparison for range queries

### Timestamp Format
All timestamps are Unix seconds (u64):
- `created_at`: Post creation time
- `updated_at`: Last modification time
- Used in SK for chronological ordering

### Tag Storage
Tags are stored as comma-separated strings for simplicity:
- Easy to parse and filter
- In production: use List type or separate GSI

## Comparison with DynamoDB

This example mirrors DynamoDB patterns:

| DynamoDB Concept | KeystoneDB Equivalent |
|-----------------|----------------------|
| Partition Key (PK) | First part of composite key |
| Sort Key (SK) | Second part of composite key |
| Query | `Query::new(pk)` |
| Scan | `Scan::new()` |
| GSI | Simulated with scans (native GSI coming) |
| UpdateItem | Manual read-modify-write |
| Streams | Coming in future phase |

## Performance Considerations

- **Query vs Scan**: Always prefer Query when you know the partition key
- **View Counter**: Uses read-modify-write pattern (consider atomic counters in production)
- **Tag Filtering**: Currently O(n) with scan; use GSI for O(log n)
- **Pagination**: Supported via `last_key` in QueryResponse (not shown in example)

## License

This example is part of the KeystoneDB project.

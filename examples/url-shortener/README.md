# URL Shortener Example

A simple URL shortener service built with KeystoneDB and Axum.

## Features Demonstrated

- **Basic CRUD operations**: put, get, delete
- **TTL support**: Automatic link expiration
- **Visit tracking**: Conditional updates to increment counters
- **REST API**: Clean HTTP interface with proper status codes
- **Health checks**: Database health monitoring
- **Statistics**: Database metrics and per-URL stats

## Running the Example

```bash
# From the url-shortener directory
cargo run

# The server will start on http://127.0.0.1:3000
```

## API Endpoints

### Create Short URL

```bash
# Without TTL (permanent)
curl -X POST http://localhost:3000/shorten \
  -H "Content-Type: application/json" \
  -d '{"long_url": "https://example.com/very/long/url"}'

# With TTL (expires in 1 hour)
curl -X POST http://localhost:3000/shorten \
  -H "Content-Type: application/json" \
  -d '{
    "long_url": "https://example.com/very/long/url",
    "ttl_seconds": 3600
  }'

# Response:
{
  "short_code": "abc123",
  "short_url": "http://127.0.0.1:3000/abc123",
  "long_url": "https://example.com/very/long/url",
  "expires_at": 1696521600  // Unix timestamp, null if no TTL
}
```

### Access Short URL

```bash
# Browser or curl will be redirected
curl -L http://localhost:3000/abc123

# Returns HTTP 301 Permanent Redirect to the long URL
# Also increments the visit counter
```

### Get URL Statistics

```bash
curl http://localhost:3000/api/stats/abc123

# Response:
{
  "short_code": "abc123",
  "long_url": "https://example.com/very/long/url",
  "visits": 42,
  "created_at": 1696518000,
  "ttl": 1696521600  // null if no expiration
}
```

### Delete Short URL

```bash
curl -X DELETE http://localhost:3000/api/delete/abc123

# Returns HTTP 204 No Content on success
# Returns HTTP 404 Not Found if URL doesn't exist
```

### Health Check

```bash
curl http://localhost:3000/api/health

# Response:
{
  "status": "Healthy",
  "warnings": [],
  "errors": []
}
```

### Database Statistics

```bash
curl http://localhost:3000/api/stats

# Response:
{
  "total_keys": null,
  "total_sst_files": 0,
  "wal_size_bytes": null,
  "memtable_size_bytes": null,
  "total_disk_size_bytes": null,
  "compaction": {
    "total_compactions": 0,
    "total_ssts_merged": 0,
    "total_bytes_read": 0,
    "total_bytes_written": 0
  }
}
```

## Data Model

```rust
Key: "url#{short_code}"

Attributes: {
  long_url: String,      // The original URL
  short_code: String,    // The generated short code
  visits: Number,        // Number of times accessed
  created_at: Number,    // Unix timestamp
  ttl: Number (optional) // Expiration timestamp
}
```

## Error Handling

The API returns appropriate HTTP status codes:

- `200 OK` - Success
- `204 No Content` - Successful deletion
- `301 Moved Permanently` - Redirect to long URL
- `404 Not Found` - Short URL doesn't exist
- `410 Gone` - Short URL has expired
- `500 Internal Server Error` - Database or server error

## KeystoneDB Features Used

1. **Basic Operations**
   - `db.put()` - Store short URLs
   - `db.get()` - Retrieve URL data
   - `db.delete()` - Remove URLs

2. **TTL Handling**
   - Store expiration timestamp in item
   - Check TTL before redirecting
   - Return 410 Gone for expired URLs

3. **Conditional Updates**
   - Read-modify-write for visit counter
   - (In production, use update expressions for atomic increments)

4. **Observability**
   - `db.health()` - Check database health
   - `db.stats()` - Get database metrics

## Limitations & Production Considerations

This is a simple example for demonstration. For production use, consider:

1. **Atomic Counter Updates**: Use update expressions instead of read-modify-write
   ```rust
   db.update(UpdateExpression::add("visits", 1))?;
   ```

2. **Custom Short Codes**: Allow users to specify custom short codes
3. **Rate Limiting**: Prevent abuse
4. **Analytics**: Track referers, geographic data, user agents
5. **URL Validation**: Validate and sanitize input URLs
6. **Caching**: Cache frequently accessed URLs
7. **Batch Operations**: Use batch writes for high throughput
8. **TTL Cleanup**: Use KeystoneDB's built-in TTL feature when available

## Performance

KeystoneDB's LSM tree architecture provides:
- Fast writes (URLs created quickly)
- Fast reads (redirects happen instantly)
- Efficient storage (compression and compaction)

Expected throughput on modern hardware:
- Shorten: ~10,000 ops/sec
- Redirect: ~50,000 ops/sec (mostly reads)

Run `cargo bench` in kstone-tests to see actual performance on your system.

# In-Memory Cache Server Example

A high-performance in-memory cache server built with KeystoneDB.

## Features Demonstrated

- **In-memory mode**: No disk persistence for maximum speed
- **TTL support**: Automatic expiration of cached values
- **Retry logic**: Automatic retry for transient failures
- **Health checks**: Monitor cache health
- **Statistics**: Cache metrics and monitoring

## Running the Example

```bash
cd examples/cache-server
cargo run
```

Server starts on http://127.0.0.1:3001

## API Endpoints

### Set Value
```bash
# Without TTL
curl -X PUT http://localhost:3001/cache/mykey \
  -H "Content-Type: application/json" \
  -d '{"value": {"name": "Alice", "age": 30}}'

# With 60-second TTL
curl -X PUT http://localhost:3001/cache/session123 \
  -H "Content-Type: application/json" \
  -d '{"value": "user_data", "ttl_seconds": 60}'
```

### Get Value
```bash
curl http://localhost:3001/cache/mykey
# Returns: {"key": "mykey", "value": {"name": "Alice", "age": 30}}
```

### Delete Value
```bash
curl -X DELETE http://localhost:3001/cache/mykey
```

### Flush Cache
```bash
curl -X POST http://localhost:3001/api/flush
```

### Health Check
```bash
curl http://localhost:3001/api/health
```

### Statistics
```bash
curl http://localhost:3001/api/stats
```

## Use Cases

- Session storage
- API response caching
- Rate limiting counters
- Real-time analytics
- Temporary data storage

## Performance

In-memory mode provides exceptional performance:
- Set: ~100,000 ops/sec
- Get: ~500,000 ops/sec

## KeystoneDB Features Used

1. **In-memory mode**: `Database::create_in_memory()`
2. **Retry logic**: `retry()` function for transient errors
3. **Health/Stats APIs**: Monitoring support

# KeystoneDB Node.js Bindings

Native Node.js bindings for KeystoneDB, an embedded DynamoDB-compatible database.

## Installation

### From Source (Development)

```bash
cd bindings/nodejs

# Install dependencies
npm install

# Build native module
npm run build

# Or for production
npm run build-release
```

### From npm (Coming Soon)

```bash
npm install keystonedb
```

## Quick Start

```javascript
const { Database } = require('keystonedb');

// Create a new database
const db = Database.create('mydb.keystone');

// Put an item
db.put('user#123', { name: 'Alice', age: 30, active: true });

// Get an item
const item = db.get('user#123');
console.log(item.name);  // Alice

// Delete an item
db.delete('user#123');
```

## API Reference

### Database Creation

```javascript
const { Database } = require('keystonedb');

// Create a new database on disk
const db = Database.create('path/to/db.keystone');

// Open an existing database
const db = Database.open('path/to/db.keystone');

// Create an in-memory database (no persistence)
const db = Database.createInMemory();
```

### Basic Operations

```javascript
// Put an item (partition key only)
db.put('user#123', { name: 'Alice', age: 30 });

// Put an item with sort key
db.put('user#123', { bio: 'Developer' }, 'profile');

// Put with Buffer keys
db.put(Buffer.from('user#123'), { name: 'Alice' });

// Get an item
const item = db.get('user#123');  // Returns object or null

// Get an item with sort key
const profile = db.get('user#123', 'profile');

// Delete an item
db.delete('user#123');
db.delete('user#123', 'profile');
```

### Query Operations

Query items by partition key with optional sort key conditions:

```javascript
// Query all items for a partition key
const items = db.query('user#123');

// Query with sort key prefix
const posts = db.query('user#123', { skBeginsWith: 'post#' });

// Query with sort key range
const readings = db.query('sensor#456', {
  skGte: '2024-01-01',
  skLt: '2024-02-01',
  limit: 100
});

// Query in reverse order
const recent = db.query('user#123', { forward: false, limit: 10 });

// Query using a secondary index
const byEmail = db.query('org#acme', {
  index: 'email-index',
  skBeginsWith: 'alice'
});
```

### Query Options

```typescript
interface QueryOptions {
  skBeginsWith?: string | Buffer;  // Sort key prefix filter
  skEq?: string | Buffer;          // Sort key equals
  skGt?: string | Buffer;          // Sort key greater than
  skGte?: string | Buffer;         // Sort key greater than or equal
  skLt?: string | Buffer;          // Sort key less than
  skLte?: string | Buffer;         // Sort key less than or equal
  limit?: number;                  // Maximum items to return
  forward?: boolean;               // true for ascending (default)
  index?: string;                  // Secondary index name
}
```

### Scan Operations

Scan all items in the database:

```javascript
// Scan all items
const allItems = db.scan();

// Scan with limit
const items = db.scan({ limit: 100 });

// Parallel scan (for large datasets)
async function parallelScan(db, segments) {
  const promises = [];
  for (let i = 0; i < segments; i++) {
    promises.push(Promise.resolve(db.scan({
      segment: i,
      totalSegments: segments
    })));
  }
  const results = await Promise.all(promises);
  return results.flat();
}

// Usage
const allItems = await parallelScan(db, 4);
```

### Update Operations

Update items using DynamoDB-style expressions:

```javascript
// Set an attribute
const item = db.update('user#123', 'SET age = :val', {
  values: { ':val': 31 }
});

// Increment a counter
const counter = db.update('counter#1', 'SET count = count + :inc', {
  values: { ':inc': 1 }
});

// Remove an attribute
db.update('user#123', 'REMOVE tempField');

// Conditional update
const updated = db.update('user#123', 'SET status = :new', {
  values: { ':new': 'active', ':old': 'pending' },
  condition: 'status = :old'
});

// Update with sort key
db.update('user#123', 'SET verified = :val', {
  sk: 'profile',
  values: { ':val': true }
});
```

### Batch Operations

```javascript
// Batch get multiple items
const results = db.batchGet(['user#1', 'user#2', 'user#3']);
// Returns: { 'user#1': {...}, 'user#2': {...}, ... }

// Batch write (puts and deletes)
const count = db.batchWrite({
  puts: {
    'user#4': { name: 'Dave' },
    'user#5': { name: 'Eve' }
  },
  deletes: ['user#old1', 'user#old2']
});
console.log(`Processed ${count} operations`);
```

### Execute PartiQL

Execute SQL-like queries:

```javascript
// SELECT query
const result = db.execute("SELECT * FROM items WHERE pk = 'user#123'");
result.items.forEach(item => console.log(item));

// INSERT
const insertResult = db.execute(
  "INSERT INTO items VALUE {'pk': 'user#999', 'name': 'New User'}"
);

// UPDATE
const updateResult = db.execute(
  "UPDATE items SET age = 25 WHERE pk = 'user#123'"
);

// DELETE
const deleteResult = db.execute(
  "DELETE FROM items WHERE pk = 'user#123'"
);
```

### Flush

Force pending writes to disk:

```javascript
db.flush();
```

## TypeScript Support

Full TypeScript definitions are included:

```typescript
import { Database, QueryOptions, ScanOptions } from 'keystonedb';

interface User {
  name: string;
  age: number;
  active: boolean;
}

const db = Database.create('mydb.keystone');

// Type-safe operations
db.put<User>('user#123', { name: 'Alice', age: 30, active: true });
const user = db.get<User>('user#123');

if (user) {
  console.log(user.name);  // TypeScript knows this is a string
}
```

## Data Types

KeystoneDB supports DynamoDB-compatible data types:

| JavaScript Type | KeystoneDB Type | Example |
|-----------------|-----------------|---------|
| `string` | String (S) | `"hello"` |
| `number` | Number (N) | `42`, `3.14` |
| `Buffer` | Binary (B) | `Buffer.from([0, 1])` |
| `boolean` | Boolean (Bool) | `true` |
| `null` | Null | `null` |
| `Array` | List (L) | `[1, 2, 3]` |
| `Object` | Map (M) | `{ key: "value" }` |

## Keys

Keys can be either strings or Buffers:

```javascript
// String keys (most common)
db.put('user#123', { name: 'Alice' });

// Buffer keys (for binary data)
db.put(Buffer.from('user#123'), { name: 'Alice' });

// Sort keys work the same way
db.put('user#123', { bio: '...' }, 'profile');
db.put(Buffer.from('user#123'), { bio: '...' }, Buffer.from('profile'));
```

## Error Handling

```javascript
try {
  const item = db.get('user#123');
  if (item === null) {
    console.log('Item not found');
  }
} catch (error) {
  console.error('Database error:', error.message);
}
```

## Async Usage

The bindings are synchronous, but you can use workers for non-blocking operations:

```javascript
const { Worker, isMainThread, parentPort, workerData } = require('worker_threads');

if (isMainThread) {
  // Main thread
  const worker = new Worker(__filename, {
    workerData: { dbPath: 'mydb.keystone', key: 'user#123' }
  });

  worker.on('message', (item) => {
    console.log('Got item:', item);
  });
} else {
  // Worker thread
  const { Database } = require('keystonedb');
  const db = Database.open(workerData.dbPath);
  const item = db.get(workerData.key);
  parentPort.postMessage(item);
}
```

## Performance Tips

1. **Use batch operations** for bulk inserts/reads
2. **Use parallel scans** for large datasets
3. **Use in-memory mode** for testing or temporary data
4. **Reuse Database instances** - don't create new ones per request
5. **Call `flush()`** periodically for durability if needed

## Version

```javascript
const { version } = require('keystonedb');
console.log(version());  // "0.1.0"
```

## Comparison with DynamoDB DocumentClient

| DynamoDB DocumentClient | KeystoneDB |
|-------------------------|------------|
| `new AWS.DynamoDB.DocumentClient()` | `Database.create('path')` |
| `client.put({ TableName, Item })` | `db.put(pk, item, sk?)` |
| `client.get({ TableName, Key })` | `db.get(pk, sk?)` |
| `client.delete({ TableName, Key })` | `db.delete(pk, sk?)` |
| `client.query({ TableName, ... })` | `db.query(pk, options)` |
| `client.scan({ TableName, ... })` | `db.scan(options)` |
| `client.update({ TableName, ... })` | `db.update(pk, expr, options)` |
| `client.batchGet({ RequestItems })` | `db.batchGet(keys)` |
| `client.batchWrite({ RequestItems })` | `db.batchWrite(options)` |

## License

MIT OR Apache-2.0

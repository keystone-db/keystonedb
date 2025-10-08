# @keystonedb/embedded

Native Node.js bindings for KeystoneDB embedded database. Built with napi-rs for maximum performance.

## Installation

```bash
npm install @keystonedb/embedded
```

## Supported Platforms

- Linux (x64, ARM64, ARMv7, musl)
- macOS (x64, ARM64/Apple Silicon)
- Windows (x64)

## Usage

### Basic Operations

```javascript
const { Database } = require('@keystonedb/embedded');

// Create a new database
const db = Database.create('./mydb.keystone');

// Put an item
db.put(Buffer.from('user#123'), {
  name: 'Alice',
  age: 30,
  active: true,
  tags: ['javascript', 'rust'],
});

// Get an item
const item = db.get(Buffer.from('user#123'));
if (item) {
  console.log(`Name: ${item.name}, Age: ${item.age}`);
}

// Delete an item
db.delete(Buffer.from('user#123'));

// Flush to disk
db.flush();
```

### TypeScript

```typescript
import { Database } from '@keystonedb/embedded';

interface User {
  name: string;
  age: number;
  active: boolean;
  tags: string[];
}

const db = Database.create('./mydb.keystone');

db.put(Buffer.from('user#123'), {
  name: 'Alice',
  age: 30,
  active: true,
  tags: ['typescript', 'rust'],
} as User);

const item = db.get(Buffer.from('user#123')) as User | null;
if (item) {
  console.log(item.name); // TypeScript knows this is a string
}
```

### With Sort Keys

```javascript
// Put with sort key
db.putWithSk(Buffer.from('org#acme'), Buffer.from('user#123'), {
  name: 'Alice',
  role: 'admin',
});

// Get with sort key
const item = db.getWithSk(Buffer.from('org#acme'), Buffer.from('user#123'));

// Delete with sort key
db.deleteWithSk(Buffer.from('org#acme'), Buffer.from('user#123'));
```

### In-Memory Database

```javascript
// Create in-memory database (no persistence)
const db = Database.createInMemory();

// Use same API as persistent database
db.put(Buffer.from('key1'), { value: 'test' });
const item = db.get(Buffer.from('key1'));
```

### Opening Existing Database

```javascript
// Open an existing database
const db = Database.open('./mydb.keystone');
```

### Supported Value Types

KeystoneDB supports DynamoDB-style value types, automatically converted to/from JavaScript:

```javascript
db.put(Buffer.from('test'), {
  string: 'hello',                // String (S)
  numberInt: 42,                  // Number (N) - integer
  numberFloat: 3.14,              // Number (N) - float
  binary: Buffer.from('data'),    // Binary (B)
  boolean: true,                  // Boolean (Bool)
  null: null,                     // Null
  array: [1, 2, 'three'],        // List (L)
  nested: {                       // Map (M)
    inner: 'value',
    count: 10,
  },
  vector: [0.1, 0.2, 0.3],       // Vector (VecF32) - for embeddings
});

const item = db.get(Buffer.from('test'));
console.log(item.string);        // "hello"
console.log(item.numberInt);     // 42
console.log(item.boolean);       // true
console.log(item.array);         // [1, 2, "three"]
console.log(item.nested.inner);  // "value"
```

### Error Handling

```javascript
try {
  const item = db.get(Buffer.from('nonexistent'));
  if (!item) {
    console.log('Item not found');
  }
} catch (error) {
  console.error('Database error:', error.message);
}
```

### Resource Management

```javascript
class ManagedDatabase {
  constructor(path) {
    this.db = Database.open(path);
  }

  async close() {
    // Ensure all writes are persisted
    this.db.flush();
  }
}

// Usage
const managed = new ManagedDatabase('./mydb.keystone');
try {
  managed.db.put(Buffer.from('key'), { value: 'data' });
} finally {
  await managed.close();
}
```

## API Reference

### Database Class

#### Static Methods

- `Database.create(path: string): Database`
  - Create a new database at the specified path
  - Throws Error if database already exists or path is invalid

- `Database.open(path: string): Database`
  - Open an existing database
  - Throws Error if database doesn't exist or is corrupted

- `Database.createInMemory(): Database`
  - Create an in-memory database (no persistence)
  - Data is lost when the database object is garbage collected

#### Instance Methods

- `put(pk: Buffer, item: object): void`
  - Store an item with partition key only
  - item: Plain JavaScript object with values
  - Throws Error for invalid item structure

- `putWithSk(pk: Buffer, sk: Buffer, item: object): void`
  - Store an item with partition key and sort key
  - Allows multiple items with same PK but different SKs

- `get(pk: Buffer): object | null`
  - Retrieve an item by partition key
  - Returns null if item not found
  - Returns plain JavaScript object with all attributes

- `getWithSk(pk: Buffer, sk: Buffer): object | null`
  - Retrieve an item by partition key and sort key
  - Returns null if item not found

- `delete(pk: Buffer): void`
  - Remove an item by partition key
  - No error if item doesn't exist (idempotent)

- `deleteWithSk(pk: Buffer, sk: Buffer): void`
  - Remove an item by partition key and sort key

- `flush(): void`
  - Force flush all pending writes to disk
  - Automatically called on normal shutdown
  - Useful for ensuring durability before critical operations

## Building from Source

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Node.js 14+
- npm or yarn

### Build Steps

```bash
cd bindings/javascript/embedded

# Install dependencies
npm install

# Build for current platform (debug)
npm run build:debug

# Build for current platform (release)
npm run build

# Build for all platforms (requires cross-compilation setup)
npm run build --features napi/all

# Run tests
npm test
```

### Cross-Compilation

To build for multiple platforms, use the napi-rs CLI:

```bash
# Install cross-compilation tools
npm install -g @napi-rs/cli

# Build for specific target
napi build --target x86_64-unknown-linux-gnu

# Build universal binary for all targets
napi universal
```

## Performance

The native bindings provide excellent performance:

- **Zero-copy** for Buffers when passing keys
- **Native speed** Rust implementation
- **No serialization overhead** for JavaScript values
- **Optimized** LSM-tree with bloom filters and compaction

## Current Limitations

This is a basic embedded database wrapper. Current limitations:

1. No async/await support (all operations are synchronous)
2. No query/scan operations (only get/put/delete)
3. No batch operations
4. No transactions
5. No conditional writes
6. No TTL support
7. No secondary indexes
8. No PartiQL queries

Advanced features will be added in future releases.

## Platform Support

Pre-built binaries are provided for:

- **Linux**: x86_64, aarch64 (ARM64), armv7, x86_64-musl
- **macOS**: x86_64 (Intel), aarch64 (Apple Silicon)
- **Windows**: x86_64

If your platform is not listed, the package will attempt to compile from source during installation.

## License

MIT OR Apache-2.0

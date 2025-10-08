# @keystonedb/client

TypeScript/JavaScript gRPC client for KeystoneDB.

## Installation

```bash
npm install @keystonedb/client
```

## Usage

### Basic Operations

```typescript
import { Client, stringValue, numberValue, boolValue } from '@keystonedb/client';

// Connect to server
const client = new Client('localhost:50051');

try {
  // Put an item
  await client.put({
    partitionKey: Buffer.from('user#123'),
    item: {
      attributes: {
        name: stringValue('Alice'),
        age: numberValue(30),
        active: boolValue(true),
      },
    },
  });

  // Get an item
  const response = await client.get({
    partitionKey: Buffer.from('user#123'),
  });

  if (response.item) {
    console.log('Name:', response.item.attributes.name.stringValue);
    console.log('Age:', response.item.attributes.age.numberValue);
  }

  // Delete an item
  await client.delete({
    partitionKey: Buffer.from('user#123'),
  });
} finally {
  client.close();
}
```

### With Sort Keys

```typescript
// Put with sort key
await client.put({
  partitionKey: Buffer.from('org#acme'),
  sortKey: Buffer.from('user#123'),
  item: {
    attributes: {
      name: stringValue('Alice'),
      role: stringValue('admin'),
    },
  },
});

// Get with sort key
const response = await client.get({
  partitionKey: Buffer.from('org#acme'),
  sortKey: Buffer.from('user#123'),
});

// Delete with sort key
await client.delete({
  partitionKey: Buffer.from('org#acme'),
  sortKey: Buffer.from('user#123'),
});
```

### Query Operations

```typescript
// Query with sort key condition
const response = await client.query({
  partitionKey: Buffer.from('org#acme'),
  sortKeyCondition: {
    beginsWith: stringValue('USER#'),
  },
  limit: 10,
  scanForward: false, // Descending order
});

console.log(`Found ${response.count} items`);
for (const item of response.items) {
  console.log(item.attributes);
}
```

### Scan Operations

```typescript
// Scan all items (streaming)
const items = await client.scan({
  limit: 100,
});

console.log(`Scanned ${items.length} items`);

// Parallel scan with segments
async function parallelScan(totalSegments: number) {
  const promises = [];
  for (let i = 0; i < totalSegments; i++) {
    promises.push(
      client.scan({
        segment: i,
        totalSegments,
        limit: 1000,
      })
    );
  }

  const results = await Promise.all(promises);
  return results.flat();
}

const allItems = await parallelScan(4);
```

### Batch Operations

```typescript
// Batch get
const response = await client.batchGet({
  keys: [
    { partitionKey: Buffer.from('user#1') },
    { partitionKey: Buffer.from('user#2') },
    { partitionKey: Buffer.from('user#3') },
  ],
});

// Batch write
await client.batchWrite({
  writes: [
    {
      put: {
        partitionKey: Buffer.from('user#1'),
        item: {
          attributes: {
            name: stringValue('Alice'),
          },
        },
      },
    },
    {
      delete: {
        partitionKey: Buffer.from('user#2'),
      },
    },
  ],
});
```

### Value Helpers

```typescript
import {
  stringValue,
  numberValue,
  boolValue,
  binaryValue,
  nullValue,
  listValue,
  mapValue,
} from '@keystonedb/client';

await client.put({
  partitionKey: Buffer.from('test'),
  item: {
    attributes: {
      string: stringValue('hello'),
      number: numberValue(42),
      bool: boolValue(true),
      binary: binaryValue(Buffer.from('data')),
      null: nullValue(),
      list: listValue([numberValue(1), numberValue(2), stringValue('three')]),
      map: mapValue({
        inner: stringValue('value'),
        count: numberValue(10),
      }),
    },
  },
});
```

### Error Handling

```typescript
try {
  const response = await client.get({
    partitionKey: Buffer.from('nonexistent'),
  });

  if (!response.item) {
    console.log('Item not found');
  }
} catch (error) {
  if (error instanceof Error) {
    console.error('gRPC error:', error.message);
  }
}
```

### Secure Connections

```typescript
import * as grpc from '@grpc/grpc-js';
import * as fs from 'fs';

// TLS credentials
const credentials = grpc.credentials.createSsl(
  fs.readFileSync('ca.pem'),
  fs.readFileSync('key.pem'),
  fs.readFileSync('cert.pem')
);

const client = new Client('secure.example.com:50051', credentials);
```

## API Reference

### Client

- `constructor(address: string, credentials?: grpc.ChannelCredentials)`
- `put(request: PutRequest): Promise<PutResponse>`
- `get(request: GetRequest): Promise<GetResponse>`
- `delete(request: DeleteRequest): Promise<DeleteResponse>`
- `query(request: QueryRequest): Promise<QueryResponse>`
- `scan(request: ScanRequest): Promise<Item[]>`
- `batchGet(request): Promise<any>`
- `batchWrite(request): Promise<any>`
- `close(): void`

### Value Helpers

- `stringValue(value: string): Value`
- `numberValue(value: number | string): Value`
- `boolValue(value: boolean): Value`
- `binaryValue(value: Buffer): Value`
- `nullValue(): Value`
- `listValue(items: Value[]): Value`
- `mapValue(fields: { [key: string]: Value }): Value`

## Building from Source

```bash
cd bindings/javascript/client

# Install dependencies
npm install

# Generate TypeScript from protobuf
npm run proto

# Build
npm run build
```

## TypeScript Support

This package includes TypeScript definitions and is written in TypeScript.

## License

MIT OR Apache-2.0

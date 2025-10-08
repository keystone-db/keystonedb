/**
 * KeystoneDB gRPC Client for TypeScript/JavaScript
 *
 * @example
 * ```typescript
 * import { Client } from '@keystonedb/client';
 *
 * const client = new Client('localhost:50051');
 * await client.put({
 *   partitionKey: Buffer.from('user#123'),
 *   item: {
 *     attributes: {
 *       name: { stringValue: 'Alice' },
 *       age: { numberValue: '30' },
 *     },
 *   },
 * });
 *
 * const response = await client.get({
 *   partitionKey: Buffer.from('user#123'),
 * });
 * console.log(response.item);
 *
 * client.close();
 * ```
 */

import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';
import * as path from 'path';

// Note: These types would be generated from proto files
// For now, using simplified interfaces

export interface Value {
  stringValue?: string;
  numberValue?: string;
  binaryValue?: Buffer;
  boolValue?: boolean;
  nullValue?: number;
  listValue?: { items: Value[] };
  mapValue?: { fields: { [key: string]: Value } };
  vectorValue?: { values: number[] };
  timestampValue?: number;
}

export interface Item {
  attributes: { [key: string]: Value };
}

export interface PutRequest {
  partitionKey: Buffer;
  sortKey?: Buffer;
  item: Item;
  conditionExpression?: string;
  expressionValues?: { [key: string]: Value };
}

export interface PutResponse {
  success: boolean;
  error?: string;
}

export interface GetRequest {
  partitionKey: Buffer;
  sortKey?: Buffer;
}

export interface GetResponse {
  item?: Item;
  error?: string;
}

export interface DeleteRequest {
  partitionKey: Buffer;
  sortKey?: Buffer;
  conditionExpression?: string;
  expressionValues?: { [key: string]: Value };
}

export interface DeleteResponse {
  success: boolean;
  error?: string;
}

export interface QueryRequest {
  partitionKey: Buffer;
  sortKeyCondition?: any;
  filterExpression?: string;
  expressionValues?: { [key: string]: Value };
  indexName?: string;
  limit?: number;
  exclusiveStartKey?: { partitionKey: Buffer; sortKey?: Buffer };
  scanForward?: boolean;
}

export interface QueryResponse {
  items: Item[];
  count: number;
  scannedCount: number;
  lastEvaluatedKey?: { partitionKey: Buffer; sortKey?: Buffer };
  error?: string;
}

export interface ScanRequest {
  filterExpression?: string;
  expressionValues?: { [key: string]: Value };
  limit?: number;
  exclusiveStartKey?: { partitionKey: Buffer; sortKey?: Buffer };
  indexName?: string;
  segment?: number;
  totalSegments?: number;
}

export interface ScanResponse {
  items: Item[];
  count: number;
  scannedCount: number;
  lastEvaluatedKey?: { partitionKey: Buffer; sortKey?: Buffer };
  error?: string;
}

/**
 * KeystoneDB gRPC Client
 */
export class Client {
  private client: any;

  /**
   * Create a new KeystoneDB client
   *
   * @param address - Server address (e.g., 'localhost:50051')
   * @param credentials - gRPC credentials (defaults to insecure)
   */
  constructor(
    address: string,
    credentials: grpc.ChannelCredentials = grpc.credentials.createInsecure()
  ) {
    const protoPath = path.join(
      __dirname,
      '../../../kstone-proto/proto/keystone.proto'
    );

    const packageDefinition = protoLoader.loadSync(protoPath, {
      keepCase: true,
      longs: String,
      enums: String,
      defaults: true,
      oneofs: true,
    });

    const proto: any = grpc.loadPackageDefinition(packageDefinition);

    this.client = new proto.keystone.KeystoneDB(address, credentials);
  }

  /**
   * Put an item into the database
   */
  put(request: PutRequest): Promise<PutResponse> {
    return new Promise((resolve, reject) => {
      this.client.Put(request, (error: Error | null, response: PutResponse) => {
        if (error) {
          reject(error);
        } else {
          resolve(response);
        }
      });
    });
  }

  /**
   * Get an item from the database
   */
  get(request: GetRequest): Promise<GetResponse> {
    return new Promise((resolve, reject) => {
      this.client.Get(request, (error: Error | null, response: GetResponse) => {
        if (error) {
          reject(error);
        } else {
          resolve(response);
        }
      });
    });
  }

  /**
   * Delete an item from the database
   */
  delete(request: DeleteRequest): Promise<DeleteResponse> {
    return new Promise((resolve, reject) => {
      this.client.Delete(
        request,
        (error: Error | null, response: DeleteResponse) => {
          if (error) {
            reject(error);
          } else {
            resolve(response);
          }
        }
      );
    });
  }

  /**
   * Query items in the database
   */
  query(request: QueryRequest): Promise<QueryResponse> {
    return new Promise((resolve, reject) => {
      this.client.Query(
        request,
        (error: Error | null, response: QueryResponse) => {
          if (error) {
            reject(error);
          } else {
            resolve(response);
          }
        }
      );
    });
  }

  /**
   * Scan all items in the database (server-side streaming)
   */
  scan(request: ScanRequest): Promise<Item[]> {
    return new Promise((resolve, reject) => {
      const items: Item[] = [];
      const stream = this.client.Scan(request);

      stream.on('data', (response: ScanResponse) => {
        if (response.error) {
          reject(new Error(response.error));
        } else {
          items.push(...response.items);
        }
      });

      stream.on('end', () => {
        resolve(items);
      });

      stream.on('error', (error: Error) => {
        reject(error);
      });
    });
  }

  /**
   * Batch get multiple items
   */
  batchGet(request: { keys: Array<{ partitionKey: Buffer; sortKey?: Buffer }> }): Promise<any> {
    return new Promise((resolve, reject) => {
      this.client.BatchGet(request, (error: Error | null, response: any) => {
        if (error) {
          reject(error);
        } else {
          resolve(response);
        }
      });
    });
  }

  /**
   * Batch write multiple items
   */
  batchWrite(request: any): Promise<any> {
    return new Promise((resolve, reject) => {
      this.client.BatchWrite(request, (error: Error | null, response: any) => {
        if (error) {
          reject(error);
        } else {
          resolve(response);
        }
      });
    });
  }

  /**
   * Close the client connection
   */
  close(): void {
    grpc.closeClient(this.client);
  }
}

/**
 * Helper to create a string value
 */
export function stringValue(value: string): Value {
  return { stringValue: value };
}

/**
 * Helper to create a number value
 */
export function numberValue(value: number | string): Value {
  return { numberValue: typeof value === 'number' ? value.toString() : value };
}

/**
 * Helper to create a boolean value
 */
export function boolValue(value: boolean): Value {
  return { boolValue: value };
}

/**
 * Helper to create a binary value
 */
export function binaryValue(value: Buffer): Value {
  return { binaryValue: value };
}

/**
 * Helper to create a null value
 */
export function nullValue(): Value {
  return { nullValue: 0 };
}

/**
 * Helper to create a list value
 */
export function listValue(items: Value[]): Value {
  return { listValue: { items } };
}

/**
 * Helper to create a map value
 */
export function mapValue(fields: { [key: string]: Value }): Value {
  return { mapValue: { fields } };
}

export default Client;

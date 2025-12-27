/**
 * KeystoneDB Node.js bindings
 *
 * An embedded DynamoDB-compatible database for Node.js.
 *
 * @example
 * ```javascript
 * const { Database } = require('keystonedb');
 * const db = Database.create('mydb.keystone');
 * db.put('user#123', { name: 'Alice', age: 30 });
 * const item = db.get('user#123');
 * console.log(item.name); // Alice
 * ```
 */

export type Key = string | Buffer;
export type Item = Record<string, unknown>;

export interface QueryOptions {
  /** Filter items where sort key starts with this prefix */
  skBeginsWith?: Key;
  /** Filter items where sort key equals this value */
  skEq?: Key;
  /** Filter items where sort key is greater than this value */
  skGt?: Key;
  /** Filter items where sort key is greater than or equal to this value */
  skGte?: Key;
  /** Filter items where sort key is less than this value */
  skLt?: Key;
  /** Filter items where sort key is less than or equal to this value */
  skLte?: Key;
  /** Maximum number of items to return */
  limit?: number;
  /** True for ascending order, false for descending */
  forward?: boolean;
  /** Name of secondary index to query */
  index?: string;
}

export interface ScanOptions {
  /** Maximum number of items to return */
  limit?: number;
  /** Segment number for parallel scans (0-based) */
  segment?: number;
  /** Total number of segments for parallel scans */
  totalSegments?: number;
}

export interface UpdateOptions {
  /** Optional sort key */
  sk?: Key;
  /** Expression values (e.g., { ':age': 30 }) */
  values?: Record<string, unknown>;
  /** Condition expression */
  condition?: string;
}

export interface BatchWriteOptions {
  /** Object mapping keys to items to put */
  puts?: Record<string, Item>;
  /** Array of keys to delete */
  deletes?: Key[];
}

export interface ExecuteResult {
  items?: Item[];
  count?: number;
  success?: boolean;
  item?: Item;
}

/**
 * KeystoneDB Database
 *
 * An embedded DynamoDB-compatible database.
 */
export class Database {
  /**
   * Create a new database at the given path.
   * @param path - Path to the database directory
   * @returns A new Database instance
   */
  static create(path: string): Database;

  /**
   * Open an existing database at the given path.
   * @param path - Path to the database directory
   * @returns A Database instance
   */
  static open(path: string): Database;

  /**
   * Create an in-memory database (no persistence).
   * @returns A new in-memory Database instance
   */
  static createInMemory(): Database;

  /**
   * Put an item into the database.
   * @param pk - Partition key
   * @param item - Item data as an object
   * @param sk - Optional sort key
   */
  put(pk: Key, item: Item, sk?: Key): void;

  /**
   * Get an item from the database.
   * @param pk - Partition key
   * @param sk - Optional sort key
   * @returns Item data as an object, or null if not found
   */
  get(pk: Key, sk?: Key): Item | null;

  /**
   * Delete an item from the database.
   * @param pk - Partition key
   * @param sk - Optional sort key
   */
  delete(pk: Key, sk?: Key): void;

  /**
   * Query items by partition key with optional sort key conditions.
   * @param pk - Partition key
   * @param options - Query options
   * @returns Array of items matching the query
   */
  query(pk: Key, options?: QueryOptions): Item[];

  /**
   * Scan all items in the database.
   * @param options - Scan options
   * @returns Array of all items (or items in the specified segment)
   */
  scan(options?: ScanOptions): Item[];

  /**
   * Update an item in the database using an update expression.
   * @param pk - Partition key
   * @param expression - Update expression (e.g., "SET age = :new_age")
   * @param options - Update options
   * @returns The updated item
   */
  update(pk: Key, expression: string, options?: UpdateOptions): Item;

  /**
   * Execute a PartiQL/SQL statement.
   * @param sql - SQL statement to execute
   * @returns Query results or operation status
   */
  execute(sql: string): ExecuteResult;

  /**
   * Batch get multiple items.
   * @param keys - Array of partition keys to retrieve
   * @returns Object mapping keys to items
   */
  batchGet(keys: Key[]): Record<string, Item>;

  /**
   * Batch write multiple items (put and/or delete).
   * @param options - Batch write options
   * @returns Number of items processed
   */
  batchWrite(options: BatchWriteOptions): number;

  /**
   * Flush pending writes to disk.
   */
  flush(): void;
}

/**
 * Get the version of KeystoneDB.
 */
export function version(): string;

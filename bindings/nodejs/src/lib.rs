//! Node.js bindings for KeystoneDB
//!
//! This module provides Node.js bindings using napi-rs, exposing KeystoneDB
//! as a native Node.js module with JavaScript-friendly API.

#![deny(clippy::all)]

use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;

use kstone_api::{
    BatchGetRequest, BatchWriteRequest, Database as KstoneDatabase,
    Query as KstoneQuery, Scan as KstoneScan, Update as KstoneUpdate,
    KeystoneError, KeystoneValue,
};

/// Convert a KeystoneError to a napi Error
fn to_napi_error(err: KeystoneError) -> Error {
    Error::new(Status::GenericFailure, err.to_string())
}

/// Convert a serde_json::Value to a KeystoneValue
fn json_to_value(v: &serde_json::Value) -> Option<KeystoneValue> {
    match v {
        serde_json::Value::String(s) => Some(KeystoneValue::S(s.clone())),
        serde_json::Value::Number(n) => Some(KeystoneValue::N(n.to_string())),
        serde_json::Value::Bool(b) => Some(KeystoneValue::Bool(*b)),
        serde_json::Value::Null => Some(KeystoneValue::Null),
        serde_json::Value::Array(arr) => {
            let list: Option<Vec<_>> = arr.iter().map(json_to_value).collect();
            list.map(KeystoneValue::L)
        }
        serde_json::Value::Object(obj) => {
            let map: Option<HashMap<_, _>> = obj
                .iter()
                .map(|(k, v)| json_to_value(v).map(|val| (k.clone(), val)))
                .collect();
            map.map(KeystoneValue::M)
        }
    }
}

/// Convert a KeystoneValue to a serde_json::Value
fn value_to_json(v: &KeystoneValue) -> serde_json::Value {
    match v {
        KeystoneValue::S(s) => serde_json::json!(s),
        KeystoneValue::N(n) => {
            if let Ok(i) = n.parse::<i64>() {
                serde_json::json!(i)
            } else if let Ok(f) = n.parse::<f64>() {
                serde_json::json!(f)
            } else {
                serde_json::json!(n)
            }
        }
        KeystoneValue::B(b) => {
            serde_json::json!(base64_encode(b.as_ref()))
        }
        KeystoneValue::Bool(b) => serde_json::json!(b),
        KeystoneValue::Null => serde_json::Value::Null,
        KeystoneValue::L(list) => {
            serde_json::Value::Array(list.iter().map(value_to_json).collect())
        }
        KeystoneValue::M(map) => {
            let obj: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        KeystoneValue::VecF32(vec) => serde_json::json!(vec),
        KeystoneValue::Ts(ts) => serde_json::json!(ts),
    }
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    for chunk in data.chunks(3) {
        let n = match chunk.len() {
            3 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32),
            2 => ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8),
            1 => (chunk[0] as u32) << 16,
            _ => unreachable!(),
        };
        result.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        result.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(ALPHABET[(n & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Convert a KeystoneDB item to a serde_json::Value
fn item_to_json(item: &HashMap<String, KeystoneValue>) -> serde_json::Value {
    let obj: serde_json::Map<String, serde_json::Value> = item
        .iter()
        .map(|(k, v)| (k.clone(), value_to_json(v)))
        .collect();
    serde_json::Value::Object(obj)
}

/// Convert a serde_json::Value to a KeystoneDB item
fn json_to_item(json: &serde_json::Value) -> Result<HashMap<String, KeystoneValue>> {
    match json {
        serde_json::Value::Object(obj) => {
            let mut item = HashMap::new();
            for (k, v) in obj {
                if let Some(val) = json_to_value(v) {
                    item.insert(k.clone(), val);
                }
            }
            Ok(item)
        }
        _ => Err(Error::new(Status::InvalidArg, "Item must be an object")),
    }
}

/// KeystoneDB Database for Node.js
///
/// An embedded DynamoDB-compatible database.
///
/// @example
/// ```javascript
/// const { Database } = require('keystonedb');
/// const db = Database.create('mydb.keystone');
/// db.put('user#123', { name: 'Alice', age: 30 });
/// const item = db.get('user#123');
/// console.log(item.name); // Alice
/// ```
#[napi]
pub struct Database {
    inner: KstoneDatabase,
}

#[napi]
impl Database {
    /// Create a new database at the given path.
    /// @param path - Path to the database directory
    /// @returns A new Database instance
    #[napi(factory)]
    pub fn create(path: String) -> Result<Self> {
        let db = KstoneDatabase::create(&path).map_err(to_napi_error)?;
        Ok(Database { inner: db })
    }

    /// Open an existing database at the given path.
    /// @param path - Path to the database directory
    /// @returns A Database instance
    #[napi(factory)]
    pub fn open(path: String) -> Result<Self> {
        let db = KstoneDatabase::open(&path).map_err(to_napi_error)?;
        Ok(Database { inner: db })
    }

    /// Create an in-memory database (no persistence).
    /// @returns A new in-memory Database instance
    #[napi(factory)]
    pub fn create_in_memory() -> Result<Self> {
        let db = KstoneDatabase::create_in_memory().map_err(to_napi_error)?;
        Ok(Database { inner: db })
    }

    /// Put an item into the database.
    /// @param pk - Partition key (string or Buffer)
    /// @param item - Item data as an object
    /// @param sk - Optional sort key (string or Buffer)
    #[napi]
    pub fn put(&self, pk: Either<String, Buffer>, item: serde_json::Value, sk: Option<Either<String, Buffer>>) -> Result<()> {
        let pk_bytes = key_to_bytes(&pk);
        let item_map = json_to_item(&item)?;

        match sk {
            Some(sk_val) => {
                let sk_bytes = key_to_bytes(&sk_val);
                self.inner.put_with_sk(&pk_bytes, &sk_bytes, item_map).map_err(to_napi_error)
            }
            None => self.inner.put(&pk_bytes, item_map).map_err(to_napi_error)
        }
    }

    /// Get an item from the database.
    /// @param pk - Partition key (string or Buffer)
    /// @param sk - Optional sort key (string or Buffer)
    /// @returns Item data as an object, or null if not found
    #[napi]
    pub fn get(&self, pk: Either<String, Buffer>, sk: Option<Either<String, Buffer>>) -> Result<Option<serde_json::Value>> {
        let pk_bytes = key_to_bytes(&pk);

        let result = match sk {
            Some(sk_val) => {
                let sk_bytes = key_to_bytes(&sk_val);
                self.inner.get_with_sk(&pk_bytes, &sk_bytes)
            }
            None => self.inner.get(&pk_bytes)
        };

        match result {
            Ok(Some(item)) => Ok(Some(item_to_json(&item))),
            Ok(None) => Ok(None),
            Err(e) => Err(to_napi_error(e))
        }
    }

    /// Delete an item from the database.
    /// @param pk - Partition key (string or Buffer)
    /// @param sk - Optional sort key (string or Buffer)
    #[napi]
    pub fn delete(&self, pk: Either<String, Buffer>, sk: Option<Either<String, Buffer>>) -> Result<()> {
        let pk_bytes = key_to_bytes(&pk);

        match sk {
            Some(sk_val) => {
                let sk_bytes = key_to_bytes(&sk_val);
                self.inner.delete_with_sk(&pk_bytes, &sk_bytes).map_err(to_napi_error)
            }
            None => self.inner.delete(&pk_bytes).map_err(to_napi_error)
        }
    }

    /// Query items by partition key with optional sort key conditions.
    /// @param pk - Partition key (string or Buffer)
    /// @param options - Query options
    /// @returns Array of items matching the query
    #[napi]
    pub fn query(&self, pk: Either<String, Buffer>, options: Option<QueryOptions>) -> Result<Vec<serde_json::Value>> {
        let pk_bytes = key_to_bytes(&pk);
        let mut query = KstoneQuery::new(&pk_bytes);

        if let Some(opts) = options {
            if let Some(prefix) = opts.sk_begins_with {
                let prefix_bytes = key_to_bytes(&prefix);
                query = query.sk_begins_with(&prefix_bytes);
            }
            if let Some(eq) = opts.sk_eq {
                let eq_bytes = key_to_bytes(&eq);
                query = query.sk_eq(&eq_bytes);
            }
            if let Some(gt) = opts.sk_gt {
                let gt_bytes = key_to_bytes(&gt);
                query = query.sk_gt(&gt_bytes);
            }
            if let Some(gte) = opts.sk_gte {
                let gte_bytes = key_to_bytes(&gte);
                query = query.sk_gte(&gte_bytes);
            }
            if let Some(lt) = opts.sk_lt {
                let lt_bytes = key_to_bytes(&lt);
                query = query.sk_lt(&lt_bytes);
            }
            if let Some(lte) = opts.sk_lte {
                let lte_bytes = key_to_bytes(&lte);
                query = query.sk_lte(&lte_bytes);
            }
            if let Some(lim) = opts.limit {
                query = query.limit(lim as usize);
            }
            if let Some(fwd) = opts.forward {
                query = query.forward(fwd);
            }
            if let Some(idx) = opts.index {
                query = query.index(idx);
            }
        }

        let response = self.inner.query(query).map_err(to_napi_error)?;
        let items: Vec<serde_json::Value> = response.items
            .iter()
            .map(item_to_json)
            .collect();
        Ok(items)
    }

    /// Scan all items in the database.
    /// @param options - Scan options
    /// @returns Array of all items (or items in the specified segment)
    #[napi]
    pub fn scan(&self, options: Option<ScanOptions>) -> Result<Vec<serde_json::Value>> {
        let mut scan = KstoneScan::new();

        if let Some(opts) = options {
            if let Some(lim) = opts.limit {
                scan = scan.limit(lim as usize);
            }
            if let (Some(seg), Some(total)) = (opts.segment, opts.total_segments) {
                scan = scan.segment(seg as usize, total as usize);
            }
        }

        let response = self.inner.scan(scan).map_err(to_napi_error)?;
        let items: Vec<serde_json::Value> = response.items
            .iter()
            .map(item_to_json)
            .collect();
        Ok(items)
    }

    /// Update an item in the database using an update expression.
    /// @param pk - Partition key (string or Buffer)
    /// @param expression - Update expression (e.g., "SET age = :new_age")
    /// @param options - Update options
    /// @returns The updated item
    #[napi]
    pub fn update(&self, pk: Either<String, Buffer>, expression: String, options: Option<UpdateOptions>) -> Result<serde_json::Value> {
        let pk_bytes = key_to_bytes(&pk);

        let mut update = if let Some(ref opts) = options {
            if let Some(ref sk) = opts.sk {
                let sk_bytes = key_to_bytes(sk);
                KstoneUpdate::with_sk(&pk_bytes, &sk_bytes)
            } else {
                KstoneUpdate::new(&pk_bytes)
            }
        } else {
            KstoneUpdate::new(&pk_bytes)
        };

        update = update.expression(&expression);

        if let Some(opts) = options {
            if let Some(vals) = opts.values {
                if let serde_json::Value::Object(obj) = vals {
                    for (k, v) in obj {
                        if let Some(val) = json_to_value(&v) {
                            update = update.value(k, val);
                        }
                    }
                }
            }
            if let Some(cond) = opts.condition {
                update = update.condition(&cond);
            }
        }

        let response = self.inner.update(update).map_err(to_napi_error)?;
        Ok(item_to_json(&response.item))
    }

    /// Execute a PartiQL/SQL statement.
    /// @param sql - SQL statement to execute
    /// @returns Query results or operation status
    #[napi]
    pub fn execute(&self, sql: String) -> Result<serde_json::Value> {
        let response = self.inner.execute_statement(&sql).map_err(to_napi_error)?;

        match response {
            kstone_api::ExecuteStatementResponse::Select { items, count, .. } => {
                let py_items: Vec<serde_json::Value> = items.iter()
                    .map(item_to_json)
                    .collect();
                Ok(serde_json::json!({
                    "items": py_items,
                    "count": count
                }))
            }
            kstone_api::ExecuteStatementResponse::Insert { success } => {
                Ok(serde_json::json!({ "success": success }))
            }
            kstone_api::ExecuteStatementResponse::Update { item } => {
                Ok(serde_json::json!({ "item": item_to_json(&item) }))
            }
            kstone_api::ExecuteStatementResponse::Delete { success } => {
                Ok(serde_json::json!({ "success": success }))
            }
        }
    }

    /// Batch get multiple items.
    /// @param keys - Array of partition keys to retrieve
    /// @returns Object mapping keys to items
    #[napi]
    pub fn batch_get(&self, keys: Vec<Either<String, Buffer>>) -> Result<serde_json::Value> {
        let mut request = BatchGetRequest::new();

        for key in keys {
            let key_bytes = key_to_bytes(&key);
            request = request.add_key(&key_bytes);
        }

        let response = self.inner.batch_get(request).map_err(to_napi_error)?;

        let mut result = serde_json::Map::new();
        for (key, item) in response.items {
            let key_str = String::from_utf8_lossy(&key.pk).to_string();
            result.insert(key_str, item_to_json(&item));
        }
        Ok(serde_json::Value::Object(result))
    }

    /// Batch write multiple items (put and/or delete).
    /// @param options - Batch write options
    /// @returns Number of items processed
    #[napi]
    pub fn batch_write(&self, options: BatchWriteOptions) -> Result<u32> {
        let mut request = BatchWriteRequest::new();

        if let Some(puts) = options.puts {
            if let serde_json::Value::Object(obj) = puts {
                for (k, v) in obj {
                    let key_bytes = k.into_bytes();
                    let item = json_to_item(&v)?;
                    request = request.put(&key_bytes, item);
                }
            }
        }

        if let Some(deletes) = options.deletes {
            for key in deletes {
                let key_bytes = key_to_bytes(&key);
                request = request.delete(&key_bytes);
            }
        }

        let response = self.inner.batch_write(request).map_err(to_napi_error)?;
        Ok(response.processed_count as u32)
    }

    /// Flush pending writes to disk.
    #[napi]
    pub fn flush(&self) -> Result<()> {
        self.inner.flush().map_err(to_napi_error)
    }
}

/// Convert a key (string or Buffer) to bytes
fn key_to_bytes(key: &Either<String, Buffer>) -> Vec<u8> {
    match key {
        Either::A(s) => s.as_bytes().to_vec(),
        Either::B(b) => b.to_vec(),
    }
}

/// Query options
#[napi(object)]
pub struct QueryOptions {
    /// Filter items where sort key starts with this prefix
    pub sk_begins_with: Option<Either<String, Buffer>>,
    /// Filter items where sort key equals this value
    pub sk_eq: Option<Either<String, Buffer>>,
    /// Filter items where sort key is greater than this value
    pub sk_gt: Option<Either<String, Buffer>>,
    /// Filter items where sort key is greater than or equal to this value
    pub sk_gte: Option<Either<String, Buffer>>,
    /// Filter items where sort key is less than this value
    pub sk_lt: Option<Either<String, Buffer>>,
    /// Filter items where sort key is less than or equal to this value
    pub sk_lte: Option<Either<String, Buffer>>,
    /// Maximum number of items to return
    pub limit: Option<u32>,
    /// True for ascending order, false for descending
    pub forward: Option<bool>,
    /// Name of secondary index to query
    pub index: Option<String>,
}

/// Scan options
#[napi(object)]
pub struct ScanOptions {
    /// Maximum number of items to return
    pub limit: Option<u32>,
    /// Segment number for parallel scans (0-based)
    pub segment: Option<u32>,
    /// Total number of segments for parallel scans
    pub total_segments: Option<u32>,
}

/// Update options
#[napi(object)]
pub struct UpdateOptions {
    /// Optional sort key
    pub sk: Option<Either<String, Buffer>>,
    /// Expression values
    pub values: Option<serde_json::Value>,
    /// Condition expression
    pub condition: Option<String>,
}

/// Batch write options
#[napi(object)]
pub struct BatchWriteOptions {
    /// Object mapping keys to items to put
    pub puts: Option<serde_json::Value>,
    /// Array of keys to delete
    pub deletes: Option<Vec<Either<String, Buffer>>>,
}

/// Get the version of KeystoneDB
#[napi]
pub fn version() -> &'static str {
    "0.1.0"
}

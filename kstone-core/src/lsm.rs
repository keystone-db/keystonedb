use crate::{Error, Result, Record, Key, Item, SeqNo, Value, wal::Wal, sst::{SstWriter, SstReader}};
use crate::iterator::{QueryParams, QueryResult, ScanParams, ScanResult};
use crate::expression::{UpdateAction, UpdateExecutor, ExpressionContext, Expr, ExpressionEvaluator};
use crate::index::{TableSchema, encode_index_key, decode_index_key};
use bytes::Bytes;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::fs;

const MEMTABLE_THRESHOLD: usize = 1000; // Flush after 1000 records per stripe
const NUM_STRIPES: usize = 256;

/// LSM engine with 256-way striping (Phase 1.6+)
pub struct LsmEngine {
    inner: Arc<RwLock<LsmInner>>,
}

/// A single stripe in the LSM tree
struct Stripe {
    memtable: BTreeMap<Vec<u8>, Record>, // Sorted by encoded key
    ssts: Vec<SstReader>,                 // Newest first
}

impl Stripe {
    fn new() -> Self {
        Self {
            memtable: BTreeMap::new(),
            ssts: Vec::new(),
        }
    }
}

struct LsmInner {
    dir: PathBuf,
    wal: Wal,
    stripes: Vec<Stripe>,  // 256 stripes
    next_seq: SeqNo,       // Global sequence number
    next_sst_id: u64,      // Global SST ID counter
    schema: TableSchema,   // Index definitions (Phase 3.1+)
}

/// Transaction write operation (Phase 2.7+)
#[derive(Debug, Clone)]
pub enum TransactWriteOperation {
    /// Put an item with optional condition
    Put {
        item: Item,
        condition: Option<Expr>,
    },
    /// Update an item with optional condition
    Update {
        actions: Vec<UpdateAction>,
        condition: Option<Expr>,
    },
    /// Delete an item with optional condition
    Delete {
        condition: Option<Expr>,
    },
    /// Condition check only (no write)
    ConditionCheck {
        condition: Expr,
    },
}

impl TransactWriteOperation {
    /// Get the condition expression if present
    pub fn condition(&self) -> Option<&Expr> {
        match self {
            Self::Put { condition, .. } => condition.as_ref(),
            Self::Update { condition, .. } => condition.as_ref(),
            Self::Delete { condition } => condition.as_ref(),
            Self::ConditionCheck { condition } => Some(condition),
        }
    }
}

impl LsmEngine {
    /// Create a new database
    pub fn create(dir: impl AsRef<Path>) -> Result<Self> {
        Self::create_with_schema(dir, TableSchema::new())
    }

    /// Create a new database with a table schema (Phase 3.1+)
    pub fn create_with_schema(dir: impl AsRef<Path>, schema: TableSchema) -> Result<Self> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)?;

        let wal_path = dir.join("wal.log");
        if wal_path.exists() {
            return Err(Error::AlreadyExists(dir.display().to_string()));
        }

        let wal = Wal::create(&wal_path)?;

        // Initialize 256 stripes
        let stripes = (0..NUM_STRIPES).map(|_| Stripe::new()).collect();

        Ok(Self {
            inner: Arc::new(RwLock::new(LsmInner {
                dir: dir.to_path_buf(),
                wal,
                stripes,
                next_seq: 1,
                next_sst_id: 1,
                schema,
            })),
        })
    }

    /// Open existing database
    pub fn open(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref();
        let wal_path = dir.join("wal.log");

        let wal = Wal::open(&wal_path)?;

        // Initialize 256 stripes
        let mut stripes: Vec<Stripe> = (0..NUM_STRIPES).map(|_| Stripe::new()).collect();
        let mut max_sst_id = 0u64;

        // Load existing SSTs into appropriate stripes
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "sst" {
                    if let Some(stem) = path.file_stem() {
                        if let Some(name) = stem.to_str() {
                            // Parse filename: {stripe:03}-{sst_id}.sst or legacy {sst_id}.sst
                            if let Some((stripe_str, id_str)) = name.split_once('-') {
                                // New format: stripe-id
                                if let (Ok(stripe), Ok(id)) = (stripe_str.parse::<usize>(), id_str.parse::<u64>()) {
                                    if stripe < NUM_STRIPES {
                                        max_sst_id = max_sst_id.max(id);
                                        let reader = SstReader::open(&path)?;
                                        stripes[stripe].ssts.push(reader);
                                    }
                                }
                            } else {
                                // Legacy format: just id (assign to stripe 0)
                                if let Ok(id) = name.parse::<u64>() {
                                    max_sst_id = max_sst_id.max(id);
                                    let reader = SstReader::open(&path)?;
                                    stripes[0].ssts.push(reader);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Sort SSTs within each stripe (newest first)
        for stripe in &mut stripes {
            stripe.ssts.reverse();
        }

        // Recover from WAL
        let records = wal.read_all()?;
        let mut max_seq = 0;

        for (_lsn, record) in records {
            max_seq = max_seq.max(record.seq);
            let key_enc = record.key.encode().to_vec();
            let stripe_id = record.key.stripe() as usize;
            stripes[stripe_id].memtable.insert(key_enc, record);
        }

        Ok(Self {
            inner: Arc::new(RwLock::new(LsmInner {
                dir: dir.to_path_buf(),
                wal,
                stripes,
                next_seq: max_seq + 1,
                next_sst_id: max_sst_id + 1,
                schema: TableSchema::new(), // TODO: Load from manifest in future
            })),
        })
    }

    /// Put an item
    pub fn put(&self, key: Key, item: Item) -> Result<()> {
        let mut inner = self.inner.write();

        let seq = inner.next_seq;
        inner.next_seq += 1;

        let record = Record::put(key.clone(), item.clone(), seq);

        // Write to WAL
        inner.wal.append(record.clone())?;
        inner.wal.flush()?;

        // Route to correct stripe
        let stripe_id = record.key.stripe() as usize;
        let key_enc = record.key.encode().to_vec();
        inner.stripes[stripe_id].memtable.insert(key_enc, record);

        // Materialize LSI entries (Phase 3.1+)
        if !inner.schema.local_indexes.is_empty() {
            self.materialize_lsi_entries(&mut inner, &key, &item)?;
        }

        // Check if this stripe needs to flush
        if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
            self.flush_stripe(&mut inner, stripe_id)?;
        }

        Ok(())
    }

    /// Put an item with a condition expression (Phase 2.5+)
    pub fn put_conditional(&self, key: Key, item: Item, condition: &Expr, context: &ExpressionContext) -> Result<()> {
        // Get current item (if exists)
        let current_item = self.get(&key)?.unwrap_or_else(|| std::collections::HashMap::new());

        // Evaluate condition
        let evaluator = ExpressionEvaluator::new(&current_item, context);
        let condition_passed = evaluator.evaluate(condition)?;

        if !condition_passed {
            return Err(Error::ConditionalCheckFailed("Put condition failed".into()));
        }

        // Condition passed, proceed with put
        self.put(key, item)
    }

    /// Get an item
    pub fn get(&self, key: &Key) -> Result<Option<Item>> {
        let inner = self.inner.read();

        // Route to correct stripe
        let stripe_id = key.stripe() as usize;
        let stripe = &inner.stripes[stripe_id];
        let key_enc = key.encode().to_vec();

        // Check stripe's memtable first
        if let Some(record) = stripe.memtable.get(&key_enc) {
            return Ok(record.value.clone());
        }

        // Check stripe's SSTs (newest to oldest)
        for sst in &stripe.ssts {
            if let Some(record) = sst.get(key) {
                return Ok(record.value.clone());
            }
        }

        Ok(None)
    }

    /// Delete an item
    pub fn delete(&self, key: Key) -> Result<()> {
        let mut inner = self.inner.write();

        let seq = inner.next_seq;
        inner.next_seq += 1;

        let record = Record::delete(key, seq);

        // Write to WAL
        inner.wal.append(record.clone())?;
        inner.wal.flush()?;

        // Route to correct stripe
        let stripe_id = record.key.stripe() as usize;
        let key_enc = record.key.encode().to_vec();
        inner.stripes[stripe_id].memtable.insert(key_enc, record);

        // Check if this stripe needs to flush
        if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
            self.flush_stripe(&mut inner, stripe_id)?;
        }

        Ok(())
    }

    /// Delete an item with a condition expression (Phase 2.5+)
    pub fn delete_conditional(&self, key: Key, condition: &Expr, context: &ExpressionContext) -> Result<()> {
        // Get current item
        let current_item = self.get(&key)?.unwrap_or_else(|| std::collections::HashMap::new());

        // Evaluate condition
        let evaluator = ExpressionEvaluator::new(&current_item, context);
        let condition_passed = evaluator.evaluate(condition)?;

        if !condition_passed {
            return Err(Error::ConditionalCheckFailed("Delete condition failed".into()));
        }

        // Condition passed, proceed with delete
        self.delete(key)
    }

    /// Update an item using update expression (Phase 2.4+)
    pub fn update(&self, key: &Key, actions: &[UpdateAction], context: &ExpressionContext) -> Result<Item> {
        // First, get the current item (or create empty if doesn't exist)
        let current_item = self.get(key)?.unwrap_or_else(|| std::collections::HashMap::new());

        // Execute update actions
        let executor = UpdateExecutor::new(context);
        let updated_item = executor.execute(&current_item, actions)?;

        // Put the updated item
        self.put(key.clone(), updated_item.clone())?;

        Ok(updated_item)
    }

    /// Update an item with a condition expression (Phase 2.5+)
    pub fn update_conditional(
        &self,
        key: &Key,
        actions: &[UpdateAction],
        condition: &Expr,
        context: &ExpressionContext,
    ) -> Result<Item> {
        // Get current item (or create empty if doesn't exist)
        let current_item = self.get(key)?.unwrap_or_else(|| std::collections::HashMap::new());

        // Evaluate condition
        let evaluator = ExpressionEvaluator::new(&current_item, context);
        let condition_passed = evaluator.evaluate(condition)?;

        if !condition_passed {
            return Err(Error::ConditionalCheckFailed("Update condition failed".into()));
        }

        // Condition passed, execute update
        let executor = UpdateExecutor::new(context);
        let updated_item = executor.execute(&current_item, actions)?;

        // Put the updated item
        self.put(key.clone(), updated_item.clone())?;

        Ok(updated_item)
    }

    /// Query items within a partition (Phase 2.1+)
    pub fn query(&self, params: QueryParams) -> Result<QueryResult> {
        let inner = self.inner.read();

        // Route to correct stripe
        let stripe_id = {
            let temp_key = Key::new(params.pk.clone());
            temp_key.stripe() as usize
        };
        let stripe = &inner.stripes[stripe_id];

        let mut items = Vec::new();
        let mut seen_keys: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();
        let mut scanned_count = 0;
        let mut last_key = None;

        // Collect all matching records from memtable and SSTs
        // We need to merge them by key, taking the newest version (highest SeqNo)
        let mut all_records: BTreeMap<Vec<u8>, Record> = BTreeMap::new();

        // Check if this is an index query (Phase 3.1+)
        let is_index_query = params.index_name.is_some();

        // First, get records from memtable
        for (key_enc, record) in &stripe.memtable {
            if is_index_query {
                // For index queries, check if this is an index key with matching index name and pk
                if let Some(index_name) = &params.index_name {
                    if let Some((idx_name, idx_pk, idx_sk)) = decode_index_key(key_enc) {
                        // Check if index name matches
                        if idx_name != *index_name {
                            continue;
                        }

                        // Check if PK matches
                        if idx_pk != params.pk {
                            continue;
                        }

                        // Check index sort key condition
                        if !params.matches_sk(&Some(idx_sk)) {
                            continue;
                        }

                        all_records.insert(key_enc.clone(), record.clone());
                    }
                }
            } else {
                // Base table query
                // Check if PK matches
                if record.key.pk != params.pk {
                    continue;
                }

                // Check sort key condition
                if !params.matches_sk(&record.key.sk) {
                    continue;
                }

                all_records.insert(key_enc.clone(), record.clone());
            }
        }

        // Then, get records from SSTs (newer SSTs first)
        for _sst in &stripe.ssts {
            // TODO: Scan SST files for matching records
            // For now, we only query from memtable
            // SST scanning will be added when we implement SST iterators
        }

        // Convert to sorted vec based on direction
        let mut sorted_records: Vec<(Vec<u8>, Record)> = all_records.into_iter().collect();

        if !params.forward {
            sorted_records.reverse();
        }

        // Apply pagination and limit
        for (key_enc, record) in sorted_records {
            // Skip based on pagination
            if params.should_skip(&record.key) {
                continue;
            }

            scanned_count += 1;

            // Skip if we've already seen this key (newer version)
            if seen_keys.contains(&key_enc) {
                continue;
            }
            seen_keys.insert(key_enc);

            // Skip tombstones
            if record.value.is_none() {
                continue;
            }

            last_key = Some(record.key.clone());

            if let Some(item) = record.value {
                items.push(item);

                // Check limit
                if let Some(limit) = params.limit {
                    if items.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(QueryResult::new(items, last_key, scanned_count))
    }

    /// Batch get multiple items (Phase 2.6+)
    pub fn batch_get(&self, keys: &[Key]) -> Result<std::collections::HashMap<Key, Option<Item>>> {
        let mut results = std::collections::HashMap::new();

        for key in keys {
            let item = self.get(key)?;
            results.insert(key.clone(), item);
        }

        Ok(results)
    }

    /// Batch write multiple items (Phase 2.6+)
    pub fn batch_write(&self, operations: &[(Key, Option<Item>)]) -> Result<usize> {
        let mut processed = 0;

        for (key, item_opt) in operations {
            match item_opt {
                Some(item) => {
                    self.put(key.clone(), item.clone())?;
                    processed += 1;
                }
                None => {
                    self.delete(key.clone())?;
                    processed += 1;
                }
            }
        }

        Ok(processed)
    }

    /// Transaction get - read multiple items atomically (Phase 2.7+)
    pub fn transact_get(&self, keys: &[Key]) -> Result<Vec<Option<Item>>> {
        // Hold read lock for consistent snapshot
        let _inner = self.inner.read();

        let mut items = Vec::new();
        for key in keys {
            let item = self.get(key)?;
            items.push(item);
        }

        Ok(items)
    }

    /// Transaction write - write multiple items atomically with conditions (Phase 2.7+)
    pub fn transact_write(
        &self,
        operations: &[(Key, TransactWriteOperation)],
        context: &ExpressionContext,
    ) -> Result<usize> {
        // Acquire write lock for atomicity
        let mut inner = self.inner.write();

        // Phase 1: Read all items and check all conditions
        let mut current_items: Vec<Option<Item>> = Vec::new();
        for (key, op) in operations {
            let item = {
                let stripe_id = key.stripe() as usize;
                let stripe = &inner.stripes[stripe_id];
                let key_enc = key.encode().to_vec();

                // Check memtable
                if let Some(record) = stripe.memtable.get(&key_enc) {
                    record.value.clone()
                } else {
                    // Check SSTs
                    let mut found = None;
                    for sst in &stripe.ssts {
                        if let Some(record) = sst.get(key) {
                            found = record.value.clone();
                            break;
                        }
                    }
                    found
                }
            };

            current_items.push(item.clone());

            // Check condition if present
            if let Some(condition_expr) = op.condition() {
                let current_item = item.unwrap_or_else(|| std::collections::HashMap::new());
                let evaluator = ExpressionEvaluator::new(&current_item, context);
                let condition_passed = evaluator.evaluate(condition_expr)?;

                if !condition_passed {
                    return Err(Error::TransactionCanceled(format!(
                        "Condition failed for key {:?}",
                        key
                    )));
                }
            }
        }

        // Phase 2: All conditions passed, perform all writes
        let mut committed = 0;
        for (i, (key, op)) in operations.iter().enumerate() {
            match op {
                TransactWriteOperation::Put { item, .. } => {
                    // Perform put (without going through public API to avoid nested locks)
                    let seq = inner.next_seq;
                    inner.next_seq += 1;
                    let record = Record::put(key.clone(), item.clone(), seq);
                    inner.wal.append(record.clone())?;
                    inner.wal.flush()?;

                    let stripe_id = record.key.stripe() as usize;
                    let key_enc = record.key.encode().to_vec();
                    inner.stripes[stripe_id].memtable.insert(key_enc, record);

                    if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
                        self.flush_stripe(&mut inner, stripe_id)?;
                    }

                    committed += 1;
                }
                TransactWriteOperation::Delete { .. } => {
                    // Perform delete
                    let seq = inner.next_seq;
                    inner.next_seq += 1;
                    let record = Record::delete(key.clone(), seq);
                    inner.wal.append(record.clone())?;
                    inner.wal.flush()?;

                    let stripe_id = record.key.stripe() as usize;
                    let key_enc = record.key.encode().to_vec();
                    inner.stripes[stripe_id].memtable.insert(key_enc, record);

                    if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
                        self.flush_stripe(&mut inner, stripe_id)?;
                    }

                    committed += 1;
                }
                TransactWriteOperation::Update { actions, .. } => {
                    // Perform update
                    let current_item = current_items[i].clone().unwrap_or_else(|| std::collections::HashMap::new());
                    let executor = UpdateExecutor::new(context);
                    let updated_item = executor.execute(&current_item, actions)?;

                    let seq = inner.next_seq;
                    inner.next_seq += 1;
                    let record = Record::put(key.clone(), updated_item, seq);
                    inner.wal.append(record.clone())?;
                    inner.wal.flush()?;

                    let stripe_id = record.key.stripe() as usize;
                    let key_enc = record.key.encode().to_vec();
                    inner.stripes[stripe_id].memtable.insert(key_enc, record);

                    if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
                        self.flush_stripe(&mut inner, stripe_id)?;
                    }

                    committed += 1;
                }
                TransactWriteOperation::ConditionCheck { .. } => {
                    // Condition already checked in phase 1, no write needed
                    committed += 1;
                }
            }
        }

        Ok(committed)
    }

    /// Scan all items across all stripes (Phase 2.2+)
    pub fn scan(&self, params: ScanParams) -> Result<ScanResult> {
        let inner = self.inner.read();

        // Collect all records from all stripes first, then sort globally
        let mut all_records: BTreeMap<Vec<u8>, Record> = BTreeMap::new();

        // Scan all stripes (or subset for parallel scans)
        for stripe_id in 0..NUM_STRIPES {
            // Skip stripes not assigned to this segment
            if !params.should_scan_stripe(stripe_id) {
                continue;
            }

            let stripe = &inner.stripes[stripe_id];

            // Collect from stripe's memtable
            for (key_enc, record) in &stripe.memtable {
                // Skip tombstones
                if record.value.is_none() {
                    continue;
                }

                all_records.insert(key_enc.clone(), record.clone());
            }

            // TODO: Scan stripe's SSTs
            // Will be added when we implement SST iterators
        }

        // Now apply pagination and limit on sorted records
        let mut items = Vec::new();
        let mut scanned_count = 0;
        let mut last_key = None;

        for (_, record) in all_records {
            // Skip based on pagination
            if params.should_skip(&record.key) {
                continue;
            }

            scanned_count += 1;
            last_key = Some(record.key.clone());

            if let Some(item) = record.value {
                items.push(item);

                // Check limit
                if let Some(limit) = params.limit {
                    if items.len() >= limit {
                        return Ok(ScanResult::new(items, last_key, scanned_count));
                    }
                }
            }
        }

        Ok(ScanResult::new(items, last_key, scanned_count))
    }

    /// Materialize LSI entries for an item (Phase 3.1+)
    fn materialize_lsi_entries(&self, inner: &mut LsmInner, key: &Key, item: &Item) -> Result<()> {
        // For each LSI defined in the schema
        for lsi in &inner.schema.local_indexes {
            // Extract the index sort key value from the item
            if let Some(index_sk_value) = item.get(&lsi.sort_key_attribute) {
                // Convert Value to Bytes for the index key
                let index_sk_bytes = match index_sk_value {
                    Value::S(s) => Bytes::copy_from_slice(s.as_bytes()),
                    Value::N(n) => Bytes::copy_from_slice(n.as_bytes()),
                    Value::B(b) => b.clone(),
                    Value::Bool(b) => Bytes::copy_from_slice(if *b { b"true" } else { b"false" }),
                    Value::Ts(ts) => Bytes::copy_from_slice(&ts.to_le_bytes()),
                    _ => continue, // Skip unsupported types for now
                };

                // Create index key
                let index_key_encoded = encode_index_key(&lsi.name, &key.pk, &index_sk_bytes);

                // Create index record with the full item (or projected attributes based on projection type)
                let index_item = item.clone(); // For now, always store full item

                // Create a synthetic Key from the encoded bytes
                // Index records use the base table's PK + encoded index info
                let index_key = Key::new(Bytes::copy_from_slice(&index_key_encoded));

                let seq = inner.next_seq;
                inner.next_seq += 1;

                let index_record = Record::put(index_key, index_item, seq);

                // Write index record to WAL
                inner.wal.append(index_record.clone())?;

                // Add to memtable (route to same stripe as base record for locality)
                let stripe_id = key.stripe() as usize;
                inner.stripes[stripe_id].memtable.insert(index_key_encoded, index_record);
            }
        }

        Ok(())
    }

    /// Flush a specific stripe's memtable to SST
    fn flush_stripe(&self, inner: &mut LsmInner, stripe_id: usize) -> Result<()> {
        if inner.stripes[stripe_id].memtable.is_empty() {
            return Ok(());
        }

        let sst_id = inner.next_sst_id;
        inner.next_sst_id += 1;

        // Filename format: {stripe:03}-{sst_id}.sst
        let sst_path = inner.dir.join(format!("{:03}-{}.sst", stripe_id, sst_id));

        // Write SST from stripe's memtable
        let mut writer = SstWriter::new();
        for record in inner.stripes[stripe_id].memtable.values() {
            writer.add(record.clone());
        }
        writer.finish(&sst_path)?;

        // Load the new SST
        let reader = SstReader::open(&sst_path)?;

        // Add to front (newest SST) of this stripe
        inner.stripes[stripe_id].ssts.insert(0, reader);

        // Clear stripe's memtable
        inner.stripes[stripe_id].memtable.clear();

        Ok(())
    }

    /// Force flush all stripes (for testing/shutdown)
    pub fn flush(&self) -> Result<()> {
        let mut inner = self.inner.write();

        // Flush all non-empty stripes
        for stripe_id in 0..NUM_STRIPES {
            if !inner.stripes[stripe_id].memtable.is_empty() {
                self.flush_stripe(&mut inner, stripe_id)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use tempfile::TempDir;
    use std::collections::HashMap;

    #[test]
    fn test_lsm_create() {
        let dir = TempDir::new().unwrap();
        let _db = LsmEngine::create(dir.path()).unwrap();
    }

    #[test]
    fn test_lsm_put_get() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        let key = Key::new(b"user#123".to_vec());
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("Alice"));
        item.insert("age".to_string(), Value::number(30));

        db.put(key.clone(), item.clone()).unwrap();

        let result = db.get(&key).unwrap();
        assert_eq!(result, Some(item));
    }

    #[test]
    fn test_lsm_delete() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        let key = Key::new(b"user#123".to_vec());
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("Bob"));

        db.put(key.clone(), item).unwrap();
        assert!(db.get(&key).unwrap().is_some());

        db.delete(key.clone()).unwrap();
        assert!(db.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_lsm_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        let key = Key::new(b"persistent".to_vec());
        let mut item = HashMap::new();
        item.insert("data".to_string(), Value::string("test"));

        // Write and close
        {
            let db = LsmEngine::create(&path).unwrap();
            db.put(key.clone(), item.clone()).unwrap();
        }

        // Reopen and verify
        let db = LsmEngine::open(&path).unwrap();
        let result = db.get(&key).unwrap();
        assert_eq!(result, Some(item));
    }

    #[test]
    fn test_lsm_flush() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Write many items to trigger flush
        for i in 0..MEMTABLE_THRESHOLD + 10 {
            let key = Key::new(format!("key{}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("value".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Verify all items are readable
        for i in 0..MEMTABLE_THRESHOLD + 10 {
            let key = Key::new(format!("key{}", i).into_bytes());
            let result = db.get(&key).unwrap();
            assert!(result.is_some());
        }
    }

    #[test]
    fn test_lsm_overwrite() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        let key = Key::new(b"counter".to_vec());

        for i in 0..5 {
            let mut item = HashMap::new();
            item.insert("value".to_string(), Value::number(i));
            db.put(key.clone(), item).unwrap();
        }

        let result = db.get(&key).unwrap().unwrap();
        match result.get("value").unwrap() {
            Value::N(n) => assert_eq!(n, "4"),
            _ => panic!("Expected number value"),
        }
    }

    #[test]
    fn test_lsm_striping() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Insert keys that should go to different stripes
        let mut keys_by_stripe: HashMap<u8, Vec<Key>> = HashMap::new();

        for i in 0..1000 {
            let key = Key::new(format!("key{}", i).into_bytes());
            let stripe = key.stripe();

            let mut item = HashMap::new();
            item.insert("id".to_string(), Value::number(i));
            db.put(key.clone(), item).unwrap();

            keys_by_stripe.entry(stripe).or_insert_with(Vec::new).push(key);
        }

        // Verify multiple stripes were used
        assert!(keys_by_stripe.len() > 1, "Expected keys to be distributed across multiple stripes");

        // Verify all keys are readable
        for (stripe, keys) in keys_by_stripe {
            for key in keys {
                let result = db.get(&key).unwrap();
                assert!(result.is_some(), "Key should exist in stripe {}", stripe);
            }
        }
    }

    #[test]
    fn test_lsm_stripe_independent_flush() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Use composite keys with same PK but different SK - all go to same stripe
        let pk = b"user#123";
        let base_key = Key::new(pk.to_vec());
        let stripe = base_key.stripe();

        // Fill this stripe's memtable using composite keys with sort keys
        for i in 0..MEMTABLE_THRESHOLD + 10 {
            let key = Key::with_sk(pk.to_vec(), format!("item#{}", i).into_bytes());
            // Verify same stripe (stripe is based on PK only)
            assert_eq!(key.stripe(), stripe);

            let mut item = HashMap::new();
            item.insert("value".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Force flush
        db.flush().unwrap();

        // Check that SST file exists with stripe prefix
        let mut found_striped_sst = false;
        for entry in fs::read_dir(dir.path()).unwrap() {
            let entry = entry.unwrap();
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".sst") && name.starts_with(&format!("{:03}-", stripe)) {
                    found_striped_sst = true;
                    break;
                }
            }
        }

        assert!(found_striped_sst, "Expected SST file with stripe prefix");
    }

    #[test]
    fn test_lsm_query_basic() {
        use crate::iterator::QueryParams;
        use bytes::Bytes;

        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        let pk = b"user#123";

        // Insert items with different sort keys
        for i in 0..10 {
            let key = Key::with_sk(pk.to_vec(), format!("item#{:03}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("id".to_string(), Value::number(i));
            item.insert("name".to_string(), Value::string(format!("Item {}", i)));
            db.put(key, item).unwrap();
        }

        // Query all items in partition
        let params = QueryParams::new(Bytes::from(pk.to_vec()));
        let result = db.query(params).unwrap();

        assert_eq!(result.items.len(), 10);
        assert_eq!(result.scanned_count, 10);
    }

    #[test]
    fn test_lsm_query_with_limit() {
        use crate::iterator::QueryParams;
        use bytes::Bytes;

        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        let pk = b"user#456";

        // Insert 20 items
        for i in 0..20 {
            let key = Key::with_sk(pk.to_vec(), format!("item#{:03}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("id".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Query with limit of 5
        let params = QueryParams::new(Bytes::from(pk.to_vec())).with_limit(5);
        let result = db.query(params).unwrap();

        assert_eq!(result.items.len(), 5);
        assert!(result.last_key.is_some());
    }

    #[test]
    fn test_lsm_query_with_sk_condition() {
        use crate::iterator::{QueryParams, SortKeyCondition};
        use bytes::Bytes;

        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        let pk = b"user#789";

        // Insert items
        for i in 0..10 {
            let key = Key::with_sk(pk.to_vec(), format!("item#{:03}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("id".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Query with SK begins_with condition
        let params = QueryParams::new(Bytes::from(pk.to_vec()))
            .with_sk_condition(SortKeyCondition::BeginsWith, Bytes::from("item#00"), None);
        let result = db.query(params).unwrap();

        // Should match item#000 through item#009
        assert_eq!(result.items.len(), 10);
    }

    #[test]
    fn test_lsm_query_reverse() {
        use crate::iterator::QueryParams;
        use bytes::Bytes;

        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        let pk = b"user#999";

        // Insert items
        for i in 0..5 {
            let key = Key::with_sk(pk.to_vec(), format!("item#{}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("id".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Query in reverse order
        let params = QueryParams::new(Bytes::from(pk.to_vec())).with_direction(false);
        let result = db.query(params).unwrap();

        assert_eq!(result.items.len(), 5);
        // First item should have highest ID when reversed
        if let Some(Value::N(n)) = result.items[0].get("id") {
            assert_eq!(n, "4");
        } else {
            panic!("Expected number value");
        }
    }

    #[test]
    fn test_lsm_scan_basic() {
        use crate::iterator::ScanParams;

        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Insert items across multiple partitions
        for i in 0..20 {
            let pk = format!("user#{}", i);
            let key = Key::new(pk.into_bytes());
            let mut item = HashMap::new();
            item.insert("id".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Scan all items
        let params = ScanParams::new();
        let result = db.scan(params).unwrap();

        assert_eq!(result.items.len(), 20);
        assert_eq!(result.scanned_count, 20);
    }

    #[test]
    fn test_lsm_scan_with_limit() {
        use crate::iterator::ScanParams;

        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Insert 50 items
        for i in 0..50 {
            let pk = format!("item#{:03}", i);
            let key = Key::new(pk.into_bytes());
            let mut item = HashMap::new();
            item.insert("value".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Scan with limit
        let params = ScanParams::new().with_limit(10);
        let result = db.scan(params).unwrap();

        assert_eq!(result.items.len(), 10);
        assert!(result.last_key.is_some());
    }

    #[test]
    fn test_lsm_scan_parallel() {
        use crate::iterator::ScanParams;

        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Insert items that will distribute across stripes
        for i in 0..100 {
            let pk = format!("key{}", i);
            let key = Key::new(pk.into_bytes());
            let mut item = HashMap::new();
            item.insert("value".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Parallel scan with 4 segments
        let mut total_items = 0;
        for segment in 0..4 {
            let params = ScanParams::new().with_segment(segment, 4);
            let result = db.scan(params).unwrap();
            total_items += result.items.len();
        }

        // All segments together should return all items
        assert_eq!(total_items, 100);
    }

    #[test]
    fn test_lsm_scan_pagination() {
        use crate::iterator::ScanParams;

        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Insert 30 items
        for i in 0..30 {
            let pk = format!("user#{:03}", i);
            let key = Key::new(pk.into_bytes());
            let mut item = HashMap::new();
            item.insert("id".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // First page
        let params1 = ScanParams::new().with_limit(10);
        let result1 = db.scan(params1).unwrap();
        assert_eq!(result1.items.len(), 10);
        assert!(result1.last_key.is_some());

        // Second page
        let params2 = ScanParams::new()
            .with_limit(10)
            .with_start_key(result1.last_key.unwrap());
        let result2 = db.scan(params2).unwrap();
        assert_eq!(result2.items.len(), 10);

        // Third page
        let params3 = ScanParams::new()
            .with_limit(10)
            .with_start_key(result2.last_key.unwrap());
        let result3 = db.scan(params3).unwrap();
        assert_eq!(result3.items.len(), 10);
    }
}

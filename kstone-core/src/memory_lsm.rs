/// In-memory LSM Engine for testing and temporary databases
///
/// Provides the same API as the disk-based LSM engine but stores all data in memory.
/// All data is lost when the MemoryLsmEngine is dropped.

use crate::{
    Result, Key, Item, Record, Error,
    memory_wal::MemoryWal,
    memory_sst::{MemorySstWriter, MemorySstReader},
    index::TableSchema,
    iterator::{QueryParams, QueryResult, ScanParams, ScanResult},
    expression::{UpdateAction, UpdateExecutor, ExpressionContext, ExpressionEvaluator, Expr},
    lsm::TransactWriteOperation,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, RwLock};

const NUM_STRIPES: usize = 256;
const MEMTABLE_THRESHOLD: usize = 1000;

/// Calculate stripe ID from partition key
fn stripe_id(pk: &[u8]) -> usize {
    crc32fast::hash(pk) as usize % NUM_STRIPES
}

/// In-memory stripe
struct MemoryStripe {
    /// In-memory memtable
    memtable: BTreeMap<Vec<u8>, Record>,
    /// In-memory SSTs
    ssts: Vec<MemorySstReader>,
}

impl MemoryStripe {
    fn new() -> Self {
        Self {
            memtable: BTreeMap::new(),
            ssts: Vec::new(),
        }
    }
}

/// Inner mutable state
struct MemoryLsmInner {
    /// In-memory WAL
    wal: MemoryWal,
    /// Stripes
    stripes: Vec<MemoryStripe>,
    /// Next sequence number
    next_seq: u64,
    /// Next SST ID
    next_sst_id: u64,
    /// Table schema (for indexes, TTL, streams)
    schema: TableSchema,
}

/// In-memory LSM Engine
#[derive(Clone)]
pub struct MemoryLsmEngine {
    inner: Arc<RwLock<MemoryLsmInner>>,
}

impl MemoryLsmEngine {
    /// Create a new in-memory database
    pub fn create() -> Result<Self> {
        Self::create_with_schema(TableSchema::new())
    }

    /// Create a new in-memory database with a table schema
    pub fn create_with_schema(schema: TableSchema) -> Result<Self> {
        let wal = MemoryWal::create()?;
        let stripes = (0..NUM_STRIPES).map(|_| MemoryStripe::new()).collect();

        Ok(Self {
            inner: Arc::new(RwLock::new(MemoryLsmInner {
                wal,
                stripes,
                next_seq: 1,
                next_sst_id: 1,
                schema,
            })),
        })
    }

    /// Put an item
    pub fn put(&self, key: Key, item: Item) -> Result<()> {
        let mut inner = self.inner.write().unwrap();

        let seq = inner.next_seq;
        inner.next_seq += 1;

        let record = Record::put(key.clone(), item, seq);

        // Append to WAL
        inner.wal.append(record.clone())?;

        // Add to memtable
        let stripe_idx = stripe_id(&key.pk);
        inner.stripes[stripe_idx].memtable.insert(key.encode().to_vec(), record);

        // Check if memtable needs flushing
        if inner.stripes[stripe_idx].memtable.len() >= MEMTABLE_THRESHOLD {
            Self::flush_stripe(&mut inner, stripe_idx)?;
        }

        Ok(())
    }

    /// Get an item
    pub fn get(&self, key: &Key) -> Result<Option<Item>> {
        let inner = self.inner.read().unwrap();
        let stripe_idx = stripe_id(&key.pk);
        let stripe = &inner.stripes[stripe_idx];
        let key_bytes = key.encode();

        // Check memtable first
        if let Some(record) = stripe.memtable.get(key_bytes.as_ref()) {
            return Ok(record.value.clone());
        }

        // Check SSTs (newest to oldest)
        for sst in stripe.ssts.iter().rev() {
            if let Some(record) = sst.get(key) {
                return Ok(record.value.clone());
            }
        }

        Ok(None)
    }

    /// Delete an item
    pub fn delete(&self, key: Key) -> Result<()> {
        let mut inner = self.inner.write().unwrap();

        let seq = inner.next_seq;
        inner.next_seq += 1;

        let record = Record::delete(key.clone(), seq);

        // Append to WAL
        inner.wal.append(record.clone())?;

        // Add tombstone to memtable
        let stripe_idx = stripe_id(&key.pk);
        inner.stripes[stripe_idx].memtable.insert(key.encode().to_vec(), record);

        // Check if memtable needs flushing
        if inner.stripes[stripe_idx].memtable.len() >= MEMTABLE_THRESHOLD {
            Self::flush_stripe(&mut inner, stripe_idx)?;
        }

        Ok(())
    }

    /// Flush memtable to SST
    fn flush_stripe(inner: &mut MemoryLsmInner, stripe_idx: usize) -> Result<()> {
        let stripe = &mut inner.stripes[stripe_idx];

        if stripe.memtable.is_empty() {
            return Ok(());
        }

        // Create SST from memtable
        let mut writer = MemorySstWriter::new();
        for record in stripe.memtable.values() {
            writer.add(record.clone());
        }

        let sst_id = inner.next_sst_id;
        inner.next_sst_id += 1;

        let sst_name = format!("mem-{:03}-{}.sst", stripe_idx, sst_id);
        let reader = writer.finish(&sst_name)?;

        stripe.ssts.push(reader);
        stripe.memtable.clear();

        // Clear WAL (in-memory, so just clear it)
        inner.wal.clear();

        Ok(())
    }

    /// Flush all stripes
    pub fn flush(&self) -> Result<()> {
        let mut inner = self.inner.write().unwrap();

        for stripe_idx in 0..NUM_STRIPES {
            if !inner.stripes[stripe_idx].memtable.is_empty() {
                Self::flush_stripe(&mut inner, stripe_idx)?;
            }
        }

        inner.wal.flush()?;
        Ok(())
    }

    /// Clear all data (for testing)
    pub fn clear(&self) -> Result<()> {
        let mut inner = self.inner.write().unwrap();

        for stripe in &mut inner.stripes {
            stripe.memtable.clear();
            stripe.ssts.clear();
        }

        inner.wal.clear();
        inner.next_seq = 1;
        inner.next_sst_id = 1;

        Ok(())
    }

    /// Get the number of items in memory (approximate)
    pub fn len(&self) -> usize {
        let inner = self.inner.read().unwrap();
        let mut count = 0;

        for stripe in &inner.stripes {
            count += stripe.memtable.len();
            for sst in &stripe.ssts {
                count += sst.len();
            }
        }

        count
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Query items within a partition
    pub fn query(&self, params: QueryParams) -> Result<QueryResult> {
        let inner = self.inner.read().unwrap();

        // Route to correct stripe
        let stripe_id = stripe_id(&params.pk);
        let stripe = &inner.stripes[stripe_id];

        let mut all_records: BTreeMap<Vec<u8>, Record> = BTreeMap::new();
        let mut scanned_count = 0;

        // Collect from memtable
        for (key_enc, record) in &stripe.memtable {
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

        // Collect from SSTs
        for sst in &stripe.ssts {
            for record in sst.iter() {
                // Check if PK matches
                if record.key.pk != params.pk {
                    continue;
                }

                // Check sort key condition
                if !params.matches_sk(&record.key.sk) {
                    continue;
                }

                let key_enc = record.key.encode().to_vec();
                // Only add if we don't already have this key (memtable is newer)
                all_records.entry(key_enc).or_insert(record.clone());
            }
        }

        // Convert to sorted vec
        let mut sorted_records: Vec<(Vec<u8>, Record)> = all_records.into_iter().collect();

        if !params.forward {
            sorted_records.reverse();
        }

        // Apply pagination and limit
        let mut items = Vec::new();
        let mut last_key = None;
        let mut seen_keys: HashSet<Vec<u8>> = HashSet::new();

        for (key_enc, record) in sorted_records {
            // Skip based on pagination
            if params.should_skip(&record.key) {
                continue;
            }

            scanned_count += 1;

            // Skip if already seen
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

    /// Scan all items across all stripes
    pub fn scan(&self, params: ScanParams) -> Result<ScanResult> {
        let inner = self.inner.read().unwrap();

        // Collect all records from all stripes
        let mut all_records: BTreeMap<Vec<u8>, Record> = BTreeMap::new();

        for stripe_id in 0..NUM_STRIPES {
            // Skip stripes not assigned to this segment
            if !params.should_scan_stripe(stripe_id) {
                continue;
            }

            let stripe = &inner.stripes[stripe_id];

            // Collect from memtable
            for (key_enc, record) in &stripe.memtable {
                // Skip tombstones
                if record.value.is_none() {
                    continue;
                }

                all_records.insert(key_enc.clone(), record.clone());
            }

            // Collect from SSTs
            for sst in &stripe.ssts {
                for record in sst.iter() {
                    // Skip tombstones
                    if record.value.is_none() {
                        continue;
                    }

                    let key_enc = record.key.encode().to_vec();
                    // Only add if we don't already have this key (memtable is newer)
                    all_records.entry(key_enc).or_insert(record.clone());
                }
            }
        }

        // Apply pagination and limit
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

    /// Update an item using update expression
    pub fn update(&self, key: &Key, actions: &[UpdateAction], context: &ExpressionContext) -> Result<Item> {
        // Get current item (or create empty if doesn't exist)
        let current_item = self.get(key)?.unwrap_or_else(|| HashMap::new());

        // Execute update actions
        let executor = UpdateExecutor::new(context);
        let updated_item = executor.execute(&current_item, actions)?;

        // Put the updated item
        self.put(key.clone(), updated_item.clone())?;

        Ok(updated_item)
    }

    /// Update an item with a condition expression
    pub fn update_conditional(
        &self,
        key: &Key,
        actions: &[UpdateAction],
        condition: &Expr,
        context: &ExpressionContext,
    ) -> Result<Item> {
        // Get current item (or create empty if doesn't exist)
        let current_item = self.get(key)?.unwrap_or_else(|| HashMap::new());

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

    /// Put an item with a condition expression
    pub fn put_conditional(&self, key: Key, item: Item, condition: &Expr, context: &ExpressionContext) -> Result<()> {
        // Get current item (or empty if doesn't exist)
        let current_item = self.get(&key)?.unwrap_or_else(|| HashMap::new());

        // Evaluate condition
        let evaluator = ExpressionEvaluator::new(&current_item, context);
        let condition_passed = evaluator.evaluate(condition)?;

        if !condition_passed {
            return Err(Error::ConditionalCheckFailed("Put condition failed".into()));
        }

        // Condition passed, perform put
        self.put(key, item)
    }

    /// Delete an item with a condition expression
    pub fn delete_conditional(&self, key: Key, condition: &Expr, context: &ExpressionContext) -> Result<()> {
        // Get current item (or empty if doesn't exist)
        let current_item = self.get(&key)?.unwrap_or_else(|| HashMap::new());

        // Evaluate condition
        let evaluator = ExpressionEvaluator::new(&current_item, context);
        let condition_passed = evaluator.evaluate(condition)?;

        if !condition_passed {
            return Err(Error::ConditionalCheckFailed("Delete condition failed".into()));
        }

        // Condition passed, perform delete
        self.delete(key)
    }

    /// Batch get multiple items
    pub fn batch_get(&self, keys: &[Key]) -> Result<HashMap<Key, Option<Item>>> {
        let mut results = HashMap::new();

        for key in keys {
            let item = self.get(key)?;
            results.insert(key.clone(), item);
        }

        Ok(results)
    }

    /// Batch write multiple items
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

    /// Transaction get - read multiple items atomically
    pub fn transact_get(&self, keys: &[Key]) -> Result<Vec<Option<Item>>> {
        // Hold read lock for consistent snapshot
        let _inner = self.inner.read().unwrap();

        let mut items = Vec::new();
        for key in keys {
            let item = self.get(key)?;
            items.push(item);
        }

        Ok(items)
    }

    /// Transaction write - write multiple items atomically with conditions
    pub fn transact_write(
        &self,
        operations: &[(Key, TransactWriteOperation)],
        context: &ExpressionContext,
    ) -> Result<usize> {
        // Acquire write lock for atomicity
        let mut inner = self.inner.write().unwrap();

        // Phase 1: Read all items and check all conditions
        let mut current_items: Vec<Option<Item>> = Vec::new();
        for (key, op) in operations {
            let item = {
                let stripe_id = stripe_id(&key.pk);
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
                let current_item = item.unwrap_or_else(|| HashMap::new());
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
                    // Perform put
                    let seq = inner.next_seq;
                    inner.next_seq += 1;
                    let record = Record::put(key.clone(), item.clone(), seq);
                    inner.wal.append(record.clone())?;

                    let stripe_id = stripe_id(&key.pk);
                    let key_enc = key.encode().to_vec();
                    inner.stripes[stripe_id].memtable.insert(key_enc, record);

                    if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
                        Self::flush_stripe(&mut inner, stripe_id)?;
                    }

                    committed += 1;
                }
                TransactWriteOperation::Delete { .. } => {
                    // Perform delete
                    let seq = inner.next_seq;
                    inner.next_seq += 1;
                    let record = Record::delete(key.clone(), seq);
                    inner.wal.append(record.clone())?;

                    let stripe_id = stripe_id(&key.pk);
                    let key_enc = key.encode().to_vec();
                    inner.stripes[stripe_id].memtable.insert(key_enc, record);

                    if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
                        Self::flush_stripe(&mut inner, stripe_id)?;
                    }

                    committed += 1;
                }
                TransactWriteOperation::Update { actions, .. } => {
                    // Perform update
                    let current_item = current_items[i].clone().unwrap_or_else(|| HashMap::new());
                    let executor = UpdateExecutor::new(context);
                    let updated_item = executor.execute(&current_item, actions)?;

                    let seq = inner.next_seq;
                    inner.next_seq += 1;
                    let record = Record::put(key.clone(), updated_item, seq);
                    inner.wal.append(record.clone())?;

                    let stripe_id = stripe_id(&key.pk);
                    let key_enc = key.encode().to_vec();
                    inner.stripes[stripe_id].memtable.insert(key_enc, record);

                    if inner.stripes[stripe_id].memtable.len() >= MEMTABLE_THRESHOLD {
                        Self::flush_stripe(&mut inner, stripe_id)?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use std::collections::HashMap;

    fn create_test_item(value: &str) -> Item {
        let mut item = HashMap::new();
        item.insert("test".to_string(), Value::string(value));
        item
    }

    #[test]
    fn test_memory_lsm_create() {
        let engine = MemoryLsmEngine::create().unwrap();
        assert!(engine.is_empty());
    }

    #[test]
    fn test_memory_lsm_put_get() {
        let engine = MemoryLsmEngine::create().unwrap();

        let key = Key::new(b"key1".to_vec());
        let item = create_test_item("value1");

        engine.put(key.clone(), item.clone()).unwrap();

        let result = engine.get(&key).unwrap();
        assert_eq!(result, Some(item));
    }

    #[test]
    fn test_memory_lsm_delete() {
        let engine = MemoryLsmEngine::create().unwrap();

        let key = Key::new(b"key1".to_vec());
        let item = create_test_item("value1");

        engine.put(key.clone(), item).unwrap();
        assert!(engine.get(&key).unwrap().is_some());

        engine.delete(key.clone()).unwrap();
        assert!(engine.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_memory_lsm_overwrite() {
        let engine = MemoryLsmEngine::create().unwrap();

        let key = Key::new(b"key1".to_vec());

        engine.put(key.clone(), create_test_item("value1")).unwrap();
        engine.put(key.clone(), create_test_item("value2")).unwrap();

        let result = engine.get(&key).unwrap().unwrap();
        assert_eq!(result.get("test").unwrap().as_string(), Some("value2"));
    }

    #[test]
    fn test_memory_lsm_flush() {
        let engine = MemoryLsmEngine::create().unwrap();

        // Add items to trigger flush
        for i in 0..1500 {
            let key = Key::new(format!("key{}", i).into_bytes());
            engine.put(key, create_test_item(&format!("value{}", i))).unwrap();
        }

        // Verify we can still read items after flush
        for i in 0..1500 {
            let key = Key::new(format!("key{}", i).into_bytes());
            let result = engine.get(&key).unwrap();
            assert!(result.is_some());
        }
    }

    #[test]
    fn test_memory_lsm_clear() {
        let engine = MemoryLsmEngine::create().unwrap();

        let key = Key::new(b"key1".to_vec());
        engine.put(key.clone(), create_test_item("value1")).unwrap();
        assert!(!engine.is_empty());

        engine.clear().unwrap();
        assert!(engine.is_empty());
        assert!(engine.get(&key).unwrap().is_none());
    }

    #[test]
    fn test_memory_lsm_multiple_stripes() {
        let engine = MemoryLsmEngine::create().unwrap();

        // Put items that will go to different stripes
        for i in 0..100 {
            let key = Key::new(format!("user{}", i).into_bytes());
            engine.put(key, create_test_item(&format!("value{}", i))).unwrap();
        }

        // Verify all items are retrievable
        for i in 0..100 {
            let key = Key::new(format!("user{}", i).into_bytes());
            let result = engine.get(&key).unwrap();
            assert!(result.is_some());
        }
    }

    #[test]
    fn test_memory_lsm_with_sort_key() {
        let engine = MemoryLsmEngine::create().unwrap();

        let key = Key::with_sk(b"pk1".to_vec(), b"sk1".to_vec());
        let item = create_test_item("value1");

        engine.put(key.clone(), item.clone()).unwrap();

        let result = engine.get(&key).unwrap();
        assert_eq!(result, Some(item));

        // Different sort key should not match
        let key2 = Key::with_sk(b"pk1".to_vec(), b"sk2".to_vec());
        assert!(engine.get(&key2).unwrap().is_none());
    }
}

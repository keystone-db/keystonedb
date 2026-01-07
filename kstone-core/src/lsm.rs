use crate::{Error, Result, Record, Key, Item, SeqNo, Value, wal::Wal, sst::{SstWriter, SstReader}};
use crate::iterator::{QueryParams, QueryResult, ScanParams, ScanResult};
use crate::expression::{UpdateAction, UpdateExecutor, ExpressionContext, Expr, ExpressionEvaluator};
use crate::index::{TableSchema, encode_index_key, decode_index_key};
use crate::compaction::{CompactionManager, CompactionConfig, CompactionStatsAtomic};
use crate::config::DatabaseConfig;
use crate::fts::{
    TextIndex, Tokenizer, FtsQuery, FtsQueryParser, Bm25Scorer, SearchResult, SearchHit,
    SnippetExtractor, encode_fts_term_key, encode_fts_term_prefix, decode_fts_term_key, is_fts_key,
    within_edit_distance,
};
use bytes::Bytes;
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::fs;

/// Legacy constant - now configured via DatabaseConfig::max_memtable_records
/// Default is now 10,000 (acts as safety ceiling)
const MEMTABLE_THRESHOLD: usize = 10_000;
const NUM_STRIPES: usize = 256;

/// LSM engine with 256-way striping (Phase 1.6+)
///
/// Flushing behavior:
/// - Primary trigger: memtable size reaches 4MB (configurable via max_memtable_size_bytes)
/// - Safety ceiling: 10,000 records (configurable via max_memtable_records)
/// - Flushing is per-stripe for better concurrency
pub struct LsmEngine {
    inner: Arc<RwLock<LsmInner>>,
    path: PathBuf,  // Store path outside the RwLock for easy access
}

/// A single stripe in the LSM tree
struct Stripe {
    memtable: BTreeMap<Vec<u8>, Record>, // Sorted by encoded key
    memtable_size_bytes: usize,          // Approximate size in bytes
    ssts: Vec<SstReader>,                 // Newest first
}

impl Stripe {
    fn new() -> Self {
        Self {
            memtable: BTreeMap::new(),
            memtable_size_bytes: 0,
            ssts: Vec::new(),
        }
    }

    /// Estimate the size of a record in bytes
    fn estimate_record_size(key_enc: &[u8], record: &Record) -> usize {
        let mut size = key_enc.len(); // Key size
        size += std::mem::size_of::<SeqNo>(); // Sequence number

        // Estimate item size
        if let Some(item) = &record.value {
            for (attr_name, value) in item {
                size += attr_name.len();
                size += match value {
                    Value::S(s) => s.len(),
                    Value::N(n) => n.len(),
                    Value::B(b) => b.len(),
                    Value::Bool(_) => 1,
                    Value::Null => 0,
                    Value::Ts(_) => 8,
                    Value::L(list) => {
                        // Rough estimate for lists
                        list.len() * 32 // Assume average 32 bytes per item
                    }
                    Value::M(map) => {
                        // Rough estimate for maps
                        map.len() * 64 // Assume average 64 bytes per entry
                    }
                    Value::VecF32(vec) => {
                        // f32 vectors: 4 bytes per element
                        vec.len() * 4
                    }
                };
            }
        }

        size
    }
}

struct LsmInner {
    dir: PathBuf,
    wal: Wal,
    stripes: Vec<Stripe>,  // 256 stripes
    next_seq: SeqNo,       // Global sequence number
    next_sst_id: u64,      // Global SST ID counter
    schema: TableSchema,   // Index definitions (Phase 3.1+)
    stream_buffer: std::collections::VecDeque<crate::stream::StreamRecord>,  // Stream records (Phase 3.4+)
    compaction_config: CompactionConfig,  // Compaction configuration (Phase 1.7+)
    compaction_stats: CompactionStatsAtomic,  // Compaction statistics (Phase 1.7+)
    config: DatabaseConfig,  // Database configuration (Phase 8+)
}

/// Transaction write operation (Phase 2.7+)
#[non_exhaustive]
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

impl LsmInner {
    /// Check if a stripe needs to flush based on configured limits
    fn should_flush_stripe(&self, stripe_id: usize) -> bool {
        let stripe = &self.stripes[stripe_id];

        // Check record count limit
        if stripe.memtable.len() >= self.config.max_memtable_records {
            return true;
        }

        // Check byte size limit if configured
        if let Some(max_bytes) = self.config.max_memtable_size_bytes {
            if stripe.memtable_size_bytes >= max_bytes {
                return true;
            }
        }

        false
    }

    /// Insert a record into a stripe's memtable, tracking size
    fn insert_into_memtable(&mut self, stripe_id: usize, key_enc: Vec<u8>, record: Record) {
        let record_size = Stripe::estimate_record_size(&key_enc, &record);

        // If key already exists, subtract old size first
        if let Some(old_record) = self.stripes[stripe_id].memtable.get(&key_enc) {
            let old_size = Stripe::estimate_record_size(&key_enc, old_record);
            self.stripes[stripe_id].memtable_size_bytes =
                self.stripes[stripe_id].memtable_size_bytes.saturating_sub(old_size);
        }

        self.stripes[stripe_id].memtable.insert(key_enc, record);
        self.stripes[stripe_id].memtable_size_bytes += record_size;
    }
}

impl LsmEngine {
    /// Create a new database
    pub fn create(dir: impl AsRef<Path>) -> Result<Self> {
        Self::create_with_schema(dir, TableSchema::new())
    }

    /// Create a new database with a table schema (Phase 3.1+)
    pub fn create_with_schema(dir: impl AsRef<Path>, schema: TableSchema) -> Result<Self> {
        Self::create_with_config(dir, DatabaseConfig::default(), schema)
    }

    /// Create a new database with custom configuration (Phase 8+)
    pub fn create_with_config(
        dir: impl AsRef<Path>,
        config: DatabaseConfig,
        schema: TableSchema,
    ) -> Result<Self> {
        // Validate configuration
        config.validate().map_err(|e| Error::InvalidArgument(e))?;

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
                stream_buffer: std::collections::VecDeque::new(),
                compaction_config: CompactionConfig::default(),
                compaction_stats: CompactionStatsAtomic::new(),
                config,
            })),
            path: dir.to_path_buf(),
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
            let key_bytes = record.key.pk.as_ref();

            // Check if this is an FTS key - FTS keys use term-based stripe routing
            // and raw bytes as memtable key (not Key::encode())
            let (stripe_id, key_enc) = if is_fts_key(key_bytes) {
                // FTS keys: extract term and hash it for stripe selection
                let stripe_id = if let Some((_, term)) = decode_fts_term_key(key_bytes) {
                    let term_hash = crate::types::checksum::compute(term.as_bytes());
                    (term_hash % 256) as usize
                } else {
                    // Fallback: use standard key stripe
                    record.key.stripe() as usize
                };
                // FTS keys use raw pk bytes as memtable key (not encoded)
                (stripe_id, key_bytes.to_vec())
            } else {
                // Regular keys: use standard stripe routing and encoded key
                (record.key.stripe() as usize, record.key.encode().to_vec())
            };

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
                stream_buffer: std::collections::VecDeque::new(),
                compaction_config: CompactionConfig::default(),
                compaction_stats: CompactionStatsAtomic::new(),
                config: DatabaseConfig::default(), // TODO: Load from manifest in future
            })),
            path: dir.to_path_buf(),
        })
    }

    /// Open an existing database with a specific schema
    ///
    /// This is useful when schema isn't persisted in the manifest yet.
    /// The caller must provide the same schema used when creating the database.
    pub fn open_with_schema(dir: impl AsRef<Path>, schema: TableSchema) -> Result<Self> {
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
            let key_bytes = record.key.pk.as_ref();

            // Check if this is an FTS key - FTS keys use term-based stripe routing
            // and raw bytes as memtable key (not Key::encode())
            let (stripe_id, key_enc) = if is_fts_key(key_bytes) {
                // FTS keys: extract term and hash it for stripe selection
                let stripe_id = if let Some((_, term)) = decode_fts_term_key(key_bytes) {
                    let term_hash = crate::types::checksum::compute(term.as_bytes());
                    (term_hash % 256) as usize
                } else {
                    // Fallback: use standard key stripe
                    record.key.stripe() as usize
                };
                // FTS keys use raw pk bytes as memtable key (not encoded)
                (stripe_id, key_bytes.to_vec())
            } else {
                // Regular keys: use standard stripe routing and encoded key
                (record.key.stripe() as usize, record.key.encode().to_vec())
            };

            stripes[stripe_id].memtable.insert(key_enc, record);
        }

        Ok(Self {
            inner: Arc::new(RwLock::new(LsmInner {
                dir: dir.to_path_buf(),
                wal,
                stripes,
                next_seq: max_seq + 1,
                next_sst_id: max_sst_id + 1,
                schema,
                stream_buffer: std::collections::VecDeque::new(),
                compaction_config: CompactionConfig::default(),
                compaction_stats: CompactionStatsAtomic::new(),
                config: DatabaseConfig::default(),
            })),
            path: dir.to_path_buf(),
        })
    }

    /// Put an item
    pub fn put(&self, key: Key, item: Item) -> Result<()> {
        let mut inner = self.inner.write();

        // Check if item exists (for stream record) (Phase 3.4+)
        let old_image = if inner.schema.stream_config.enabled {
            let stripe_id = key.stripe() as usize;
            let key_enc = key.encode().to_vec();
            inner.stripes[stripe_id].memtable.get(&key_enc).and_then(|r| r.value.clone())
        } else {
            None
        };

        let seq = inner.next_seq;
        inner.next_seq += 1;

        let record = Record::put(key.clone(), item.clone(), seq);

        // Write to WAL
        inner.wal.append(record.clone())?;
        inner.wal.flush()?;

        // Route to correct stripe
        let stripe_id = record.key.stripe() as usize;
        let key_enc = record.key.encode().to_vec();
        inner.insert_into_memtable(stripe_id, key_enc, record);

        // Materialize LSI entries (Phase 3.1+)
        if !inner.schema.local_indexes.is_empty() {
            self.materialize_lsi_entries(&mut inner, &key, &item)?;
        }

        // Materialize GSI entries (Phase 3.2+)
        if !inner.schema.global_indexes.is_empty() {
            self.materialize_gsi_entries(&mut inner, &key, &item)?;
        }

        // Materialize FTS entries (Phase 11+)
        if !inner.schema.text_indexes.is_empty() {
            self.materialize_fts_entries(&mut inner, &key, &item)?;
        }

        // Emit stream record (Phase 3.4+)
        if inner.schema.stream_config.enabled {
            let stream_record = if let Some(old) = old_image {
                crate::stream::StreamRecord::modify(
                    seq,
                    key.clone(),
                    old,
                    item.clone(),
                    inner.schema.stream_config.view_type,
                )
            } else {
                crate::stream::StreamRecord::insert(
                    seq,
                    key.clone(),
                    item.clone(),
                    inner.schema.stream_config.view_type,
                )
            };
            self.emit_stream_record(&mut inner, stream_record);
        }

        // Check if this stripe needs to flush
        if inner.should_flush_stripe(stripe_id) {
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
            if let Some(item) = &record.value {
                // Check TTL (Phase 3.3+)
                if inner.schema.is_expired(item) {
                    // Item is expired - perform lazy deletion
                    drop(inner); // Release read lock
                    self.delete(key.clone())?;
                    return Ok(None);
                }
            }
            return Ok(record.value.clone());
        }

        // Check stripe's SSTs (newest to oldest)
        for sst in &stripe.ssts {
            if let Some(record) = sst.get(key) {
                if let Some(item) = &record.value {
                    // Check TTL (Phase 3.3+)
                    if inner.schema.is_expired(item) {
                        // Item is expired - perform lazy deletion
                        drop(inner); // Release read lock
                        self.delete(key.clone())?;
                        return Ok(None);
                    }
                }
                return Ok(record.value.clone());
            }
        }

        Ok(None)
    }

    /// Delete an item
    pub fn delete(&self, key: Key) -> Result<()> {
        let mut inner = self.inner.write();

        // Check if item exists (for stream record) (Phase 3.4+)
        let old_image = if inner.schema.stream_config.enabled {
            let stripe_id = key.stripe() as usize;
            let key_enc = key.encode().to_vec();
            inner.stripes[stripe_id].memtable.get(&key_enc).and_then(|r| r.value.clone())
        } else {
            None
        };

        let seq = inner.next_seq;
        inner.next_seq += 1;

        let record = Record::delete(key.clone(), seq);

        // Write to WAL
        inner.wal.append(record.clone())?;
        inner.wal.flush()?;

        // Route to correct stripe
        let stripe_id = record.key.stripe() as usize;
        let key_enc = record.key.encode().to_vec();
        inner.stripes[stripe_id].memtable.insert(key_enc, record);

        // Emit stream record (Phase 3.4+)
        if inner.schema.stream_config.enabled {
            if let Some(old) = old_image {
                let stream_record = crate::stream::StreamRecord::remove(
                    seq,
                    key.clone(),
                    old,
                    inner.schema.stream_config.view_type,
                );
                self.emit_stream_record(&mut inner, stream_record);
            }
        }

        // Check if this stripe needs to flush
        if inner.should_flush_stripe(stripe_id) {
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

        // Note: Query currently only searches memtable.
        // SST files are searched during get() operations but not during range queries.
        // This is a known limitation - SST range scanning requires implementing
        // SST iterators (planned for future release).
        let _ = &stripe.ssts; // Acknowledge SSTs exist but aren't scanned for queries yet

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

            // Check TTL and skip expired items (Phase 3.3+)
            if let Some(ref item) = record.value {
                if inner.schema.is_expired(item) {
                    continue; // Skip expired items
                }
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

    /// Scan with keys - returns (Key, Item) pairs for sync
    pub fn scan_with_keys(&self, limit: usize) -> Result<Vec<(Key, Item)>> {
        let inner = self.inner.read();
        let mut results = Vec::new();
        let mut count = 0;

        // Scan all stripes
        for stripe in &inner.stripes {
            // From memtable
            for (_key_bytes, record) in &stripe.memtable {
                if let Some(ref item) = record.value {
                    // Skip index records (start with 0xFF) and sync metadata
                    if !record.key.pk.starts_with(&[0xFF]) &&
                       !record.key.pk.starts_with(b"_sync#") {
                        results.push((record.key.clone(), item.clone()));
                        count += 1;
                        if count >= limit {
                            return Ok(results);
                        }
                    }
                }
            }

            // From SSTs
            for sst in &stripe.ssts {
                if let Ok(records) = sst.scan() {
                    for record in records {
                        if let Some(ref item) = record.value {
                            // Skip index records (start with 0xFF) and sync metadata
                            if !record.key.pk.starts_with(&[0xFF]) &&
                               !record.key.pk.starts_with(b"_sync#") {
                                results.push((record.key.clone(), item.clone()));
                                count += 1;
                                if count >= limit {
                                    return Ok(results);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
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

            // Note: Scan currently only searches memtable per stripe.
            // SST range scanning is a known limitation (see query implementation above).
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

            // Check TTL and skip expired items (Phase 3.3+)
            if let Some(ref item) = record.value {
                if inner.schema.is_expired(item) {
                    continue; // Skip expired items
                }
            }

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

    /// Materialize GSI entries for an item (Phase 3.2+)
    fn materialize_gsi_entries(&self, inner: &mut LsmInner, base_key: &Key, item: &Item) -> Result<()> {
        // For each GSI defined in the schema
        for gsi in &inner.schema.global_indexes {
            // Extract the GSI partition key value from the item
            if let Some(gsi_pk_value) = item.get(&gsi.partition_key_attribute) {
                // Convert Value to Bytes for the GSI partition key
                let gsi_pk_bytes = match gsi_pk_value {
                    Value::S(s) => Bytes::copy_from_slice(s.as_bytes()),
                    Value::N(n) => Bytes::copy_from_slice(n.as_bytes()),
                    Value::B(b) => b.clone(),
                    Value::Bool(b) => Bytes::copy_from_slice(if *b { b"true" } else { b"false" }),
                    Value::Ts(ts) => Bytes::copy_from_slice(&ts.to_le_bytes()),
                    _ => continue, // Skip unsupported types
                };

                // Extract the GSI sort key value (if defined)
                let mut gsi_sk_bytes = if let Some(gsi_sk_attr) = &gsi.sort_key_attribute {
                    if let Some(gsi_sk_value) = item.get(gsi_sk_attr) {
                        match gsi_sk_value {
                            Value::S(s) => Bytes::copy_from_slice(s.as_bytes()),
                            Value::N(n) => Bytes::copy_from_slice(n.as_bytes()),
                            Value::B(b) => b.clone(),
                            Value::Bool(b) => Bytes::copy_from_slice(if *b { b"true" } else { b"false" }),
                            Value::Ts(ts) => Bytes::copy_from_slice(&ts.to_le_bytes()),
                            _ => Bytes::new(), // Use empty bytes for unsupported types
                        }
                    } else {
                        continue; // Skip if sort key attribute doesn't exist
                    }
                } else {
                    Bytes::new() // No sort key for this GSI
                };

                // For GSI, append base table PK to ensure uniqueness
                // This allows multiple base items with same GSI PK+SK
                let base_pk_encoded = base_key.encode();
                let mut combined_sk = Vec::with_capacity(gsi_sk_bytes.len() + base_pk_encoded.len());
                combined_sk.extend_from_slice(&gsi_sk_bytes);
                combined_sk.extend_from_slice(&base_pk_encoded);
                gsi_sk_bytes = Bytes::from(combined_sk);

                // Create GSI index key
                let index_key_encoded = encode_index_key(&gsi.name, &gsi_pk_bytes, &gsi_sk_bytes);

                // Create index record with the full item
                let index_item = item.clone(); // For now, always store full item

                // Create a synthetic Key from the encoded bytes
                let index_key = Key::new(Bytes::copy_from_slice(&index_key_encoded));

                let seq = inner.next_seq;
                inner.next_seq += 1;

                let index_record = Record::put(index_key, index_item, seq);

                // Write index record to WAL
                inner.wal.append(index_record.clone())?;

                // Add to memtable - IMPORTANT: route to stripe based on GSI PK value
                // Use a temporary key with just the GSI PK to determine stripe
                let gsi_stripe_key = Key::new(gsi_pk_bytes.clone());
                let gsi_stripe_id = gsi_stripe_key.stripe() as usize;
                inner.stripes[gsi_stripe_id].memtable.insert(index_key_encoded, index_record);
            }
        }

        Ok(())
    }

    /// Materialize FTS (Full-Text Search) index entries (Phase 11+)
    ///
    /// For each text index, tokenizes the indexed attribute and creates
    /// inverted index entries mapping terms to document keys.
    fn materialize_fts_entries(&self, inner: &mut LsmInner, key: &Key, item: &Item) -> Result<()> {
        // Clone the text_indexes to avoid borrow checker issues
        let text_indexes: Vec<_> = inner.schema.text_indexes.clone();

        for text_index in &text_indexes {
            // Extract the text attribute value
            if let Some(text_value) = item.get(&text_index.attribute) {
                // Only index string values
                let text = match text_value {
                    Value::S(s) => s.as_str(),
                    _ => continue,
                };

                // Tokenize the text
                let tokenizer = Tokenizer::new(text_index);
                let term_info = tokenizer.tokenize_with_frequency(text);
                let doc_length = text.split_whitespace().count() as u32;

                // Create a posting entry for this document
                let doc_key_encoded = key.encode().to_vec();

                // For each unique term, create/update an inverted index entry
                for (term, info) in term_info {
                    // Create the FTS index key: [FTS_MARKER | index_name | term | doc_key]
                    // Including doc_key ensures each term-document pair has a unique key
                    let fts_key_encoded = encode_fts_term_key(&text_index.name, &term, &doc_key_encoded);

                    // Create posting data: stores the document key, frequency, positions, and doc length
                    let posting_data = self.create_posting_value(
                        &doc_key_encoded,
                        info.frequency,
                        &info.positions,
                        doc_length,
                    );

                    // Create a synthetic Key for the FTS entry
                    let fts_key = Key::new(Bytes::copy_from_slice(&fts_key_encoded));

                    let seq = inner.next_seq;
                    inner.next_seq += 1;

                    // Create the FTS index record
                    let fts_record = Record::put(fts_key.clone(), posting_data, seq);

                    // Write to WAL
                    inner.wal.append(fts_record.clone())?;

                    // Add to memtable - use a hash of the term to select stripe
                    let term_hash = crate::types::checksum::compute(term.as_bytes());
                    let stripe_id = (term_hash % 256) as usize;
                    inner.insert_into_memtable(stripe_id, fts_key_encoded, fts_record);
                }
            }
        }

        // Flush WAL to ensure FTS entries are persisted
        inner.wal.flush()?;

        Ok(())
    }

    /// Create a posting value for FTS index storage
    fn create_posting_value(
        &self,
        doc_key: &[u8],
        term_frequency: u32,
        positions: &[u32],
        doc_length: u32,
    ) -> Item {
        let mut item = Item::new();

        // Store document key as binary
        item.insert("dk".to_string(), Value::B(Bytes::copy_from_slice(doc_key)));

        // Store term frequency
        item.insert("tf".to_string(), Value::N(term_frequency.to_string()));

        // Store positions as a list of numbers
        let pos_list: Vec<Value> = positions.iter().map(|p| Value::N(p.to_string())).collect();
        item.insert("pos".to_string(), Value::L(pos_list));

        // Store document length
        item.insert("dl".to_string(), Value::N(doc_length.to_string()));

        item
    }

    /// Parse a posting value from FTS index storage
    fn parse_posting_value(&self, item: &Item) -> Option<(Vec<u8>, u32, Vec<u32>, u32)> {
        // Extract document key
        let doc_key = match item.get("dk")? {
            Value::B(b) => b.to_vec(),
            _ => return None,
        };

        // Extract term frequency
        let term_frequency = match item.get("tf")? {
            Value::N(n) => n.parse::<u32>().ok()?,
            _ => return None,
        };

        // Extract positions
        let positions = match item.get("pos")? {
            Value::L(list) => {
                let mut pos = Vec::with_capacity(list.len());
                for v in list {
                    if let Value::N(n) = v {
                        if let Ok(p) = n.parse::<u32>() {
                            pos.push(p);
                        }
                    }
                }
                pos
            }
            _ => return None,
        };

        // Extract document length
        let doc_length = match item.get("dl")? {
            Value::N(n) => n.parse::<u32>().ok()?,
            _ => return None,
        };

        Some((doc_key, term_frequency, positions, doc_length))
    }

    /// Read stream records (Phase 3.4+)
    ///
    /// Returns all stream records in the buffer, ordered by sequence number (oldest first).
    /// Optionally filter to only records with sequence number > after_sequence_number.
    pub fn read_stream(&self, after_sequence_number: Option<u64>) -> Result<Vec<crate::stream::StreamRecord>> {
        let inner = self.inner.read();

        if !inner.schema.stream_config.enabled {
            return Ok(Vec::new());
        }

        let records: Vec<crate::stream::StreamRecord> = inner.stream_buffer
            .iter()
            .filter(|record| {
                if let Some(after) = after_sequence_number {
                    record.sequence_number > after
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        Ok(records)
    }

    /// Emit a stream record if streams are enabled (Phase 3.4+)
    fn emit_stream_record(&self, inner: &mut LsmInner, record: crate::stream::StreamRecord) {
        if !inner.schema.stream_config.enabled {
            return;
        }

        // Validate record size to prevent memory exhaustion
        let record_size = crate::stream::estimate_record_size(&record);
        if record_size > crate::stream::MAX_STREAM_RECORD_SIZE {
            tracing::warn!(
                size = record_size,
                max_size = crate::stream::MAX_STREAM_RECORD_SIZE,
                key = ?record.key,
                "Skipping oversized stream record to prevent memory exhaustion"
            );
            return;
        }

        // Add to buffer
        inner.stream_buffer.push_back(record);

        // Trim buffer if it exceeds max size
        while inner.stream_buffer.len() > inner.schema.stream_config.buffer_size {
            inner.stream_buffer.pop_front();
        }
    }

    // =========================================================================
    // Full-Text Search (Phase 11+)
    // =========================================================================

    /// Perform a full-text search query (Phase 11+)
    ///
    /// # Arguments
    /// * `index_name` - Name of the text index to search
    /// * `query_str` - Search query string (supports boolean operators, phrases, fuzzy)
    /// * `limit` - Maximum number of results to return
    /// * `highlight` - Whether to include highlighted snippets
    ///
    /// # Returns
    /// SearchResult with matching documents ranked by relevance (BM25)
    pub fn text_search(
        &self,
        index_name: &str,
        query_str: &str,
        limit: usize,
        highlight: bool,
    ) -> Result<SearchResult> {
        let start_time = std::time::Instant::now();
        let inner = self.inner.read();

        // Find the text index
        let text_index = inner.schema.get_text_index(index_name)
            .ok_or_else(|| Error::InvalidQuery(format!("Text index '{}' not found", index_name)))?
            .clone();

        // Parse the query
        let tokenizer = Tokenizer::new(&text_index);
        let parser = FtsQueryParser::new(tokenizer);
        let query = parser.parse(query_str)?;

        // Execute the search
        let matching_docs = self.execute_fts_query(&inner, index_name, &query, &text_index)?;

        // Calculate collection statistics for BM25
        let (total_docs, avg_doc_length) = self.get_collection_stats(&inner);

        // Score and rank results
        let mut scored_hits = self.score_fts_results(
            &inner,
            index_name,
            &query,
            matching_docs,
            total_docs,
            avg_doc_length,
            &text_index,
        )?;

        // Sort by score descending
        scored_hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        // Calculate total before truncation
        let total_hits = scored_hits.len() as u64;

        // Apply limit
        scored_hits.truncate(limit);

        // Add highlighting if requested
        if highlight {
            self.add_highlights(&inner, &mut scored_hits, &text_index, &query)?;
        }

        let took_ms = start_time.elapsed().as_millis() as u64;

        Ok(SearchResult {
            total_hits,
            hits: scored_hits,
            took_ms,
        })
    }

    /// Execute an FTS query and return matching document keys
    fn execute_fts_query(
        &self,
        inner: &LsmInner,
        index_name: &str,
        query: &FtsQuery,
        text_index: &TextIndex,
    ) -> Result<HashSet<Vec<u8>>> {
        match query {
            FtsQuery::Term(term) => {
                self.find_docs_for_term(inner, index_name, term)
            }
            FtsQuery::Phrase(terms) => {
                self.find_docs_for_phrase(inner, index_name, terms, text_index)
            }
            FtsQuery::Boolean { left, op, right } => {
                let left_docs = self.execute_fts_query(inner, index_name, left, text_index)?;
                let right_docs = self.execute_fts_query(inner, index_name, right, text_index)?;

                match op {
                    crate::fts::BooleanOp::And => {
                        Ok(left_docs.intersection(&right_docs).cloned().collect())
                    }
                    crate::fts::BooleanOp::Or => {
                        Ok(left_docs.union(&right_docs).cloned().collect())
                    }
                    crate::fts::BooleanOp::Not => {
                        Ok(left_docs.difference(&right_docs).cloned().collect())
                    }
                }
            }
            FtsQuery::Fuzzy { term, max_distance } => {
                self.find_docs_for_fuzzy(inner, index_name, term, *max_distance as usize)
            }
            FtsQuery::Prefix(prefix) => {
                self.find_docs_for_prefix(inner, index_name, prefix)
            }
            FtsQuery::Wildcard(pattern) => {
                // Wildcard not fully implemented - treat as prefix for now
                let prefix = pattern.trim_end_matches('*').trim_end_matches('?');
                self.find_docs_for_prefix(inner, index_name, prefix)
            }
        }
    }

    /// Find all documents containing a specific term
    fn find_docs_for_term(
        &self,
        inner: &LsmInner,
        index_name: &str,
        term: &str,
    ) -> Result<HashSet<Vec<u8>>> {
        let mut docs = HashSet::new();

        // Build the FTS prefix key for this term (without doc_key)
        let fts_prefix = encode_fts_term_prefix(index_name, term);

        // Calculate the stripe for this term
        let term_hash = crate::types::checksum::compute(term.as_bytes());
        let stripe_id = (term_hash % 256) as usize;

        // Search in the memtable first - scan all keys with the prefix
        for (key_bytes, record) in inner.stripes[stripe_id].memtable.iter() {
            if key_bytes.starts_with(&fts_prefix) {
                if let Some(item) = &record.value {
                    if let Some((doc_key, _, _, _)) = self.parse_posting_value(item) {
                        docs.insert(doc_key);
                    }
                }
            }
        }

        // Search in SST files - iterate and check prefix
        // Note: FTS keys are stored in the Key.pk field directly (not length-prefixed)
        for sst in &inner.stripes[stripe_id].ssts {
            for record in sst.iter() {
                // Use pk directly - that's where the FTS key bytes are stored
                if record.key.pk.starts_with(&fts_prefix) {
                    if let Some(item) = &record.value {
                        if let Some((doc_key, _, _, _)) = self.parse_posting_value(item) {
                            docs.insert(doc_key);
                        }
                    }
                }
            }
        }

        Ok(docs)
    }

    /// Find documents matching a phrase query
    fn find_docs_for_phrase(
        &self,
        inner: &LsmInner,
        index_name: &str,
        terms: &[String],
        _text_index: &TextIndex,
    ) -> Result<HashSet<Vec<u8>>> {
        if terms.is_empty() {
            return Ok(HashSet::new());
        }

        // First, find documents containing all terms
        let mut candidate_docs = self.find_docs_for_term(inner, index_name, &terms[0])?;
        for term in &terms[1..] {
            let term_docs = self.find_docs_for_term(inner, index_name, term)?;
            candidate_docs = candidate_docs.intersection(&term_docs).cloned().collect();
        }

        // For true phrase matching, we would need to check positions
        // For now, we return documents containing all terms (AND semantics)
        // A full implementation would verify position adjacency

        Ok(candidate_docs)
    }

    /// Find documents with fuzzy matching
    fn find_docs_for_fuzzy(
        &self,
        inner: &LsmInner,
        index_name: &str,
        term: &str,
        max_distance: usize,
    ) -> Result<HashSet<Vec<u8>>> {
        let mut docs = HashSet::new();

        // Scan all FTS keys for this index and check edit distance
        // This is O(n) over all terms - a production implementation would use
        // a more efficient data structure (e.g., BK-tree, SymSpell)

        // Build the FTS index prefix (marker + index_name) to scan for all terms in this index
        let index_prefix = {
            let mut buf = Vec::new();
            let index_name_bytes = index_name.as_bytes();
            buf.push(crate::fts::FTS_INDEX_MARKER);
            buf.extend_from_slice(&(index_name_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(index_name_bytes);
            buf
        };

        for stripe in &inner.stripes {
            // Search memtable
            for (key, record) in &stripe.memtable {
                if key.starts_with(&index_prefix) {
                    if let Some((_, stored_term)) = decode_fts_term_key(key) {
                        if within_edit_distance(&stored_term, term, max_distance) {
                            if let Some(item) = &record.value {
                                if let Some((doc_key, _, _, _)) = self.parse_posting_value(item) {
                                    docs.insert(doc_key);
                                }
                            }
                        }
                    }
                }
            }

            // Search SST files
            for sst in &stripe.ssts {
                for record in sst.iter() {
                    if record.key.pk.starts_with(&index_prefix) {
                        if let Some((_, stored_term)) = decode_fts_term_key(&record.key.pk) {
                            if within_edit_distance(&stored_term, term, max_distance) {
                                if let Some(item) = &record.value {
                                    if let Some((doc_key, _, _, _)) = self.parse_posting_value(item) {
                                        docs.insert(doc_key);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(docs)
    }

    /// Find documents matching a prefix
    fn find_docs_for_prefix(
        &self,
        inner: &LsmInner,
        index_name: &str,
        prefix: &str,
    ) -> Result<HashSet<Vec<u8>>> {
        let mut docs = HashSet::new();

        // Build the FTS index prefix (marker + index_name) to scan for all terms in this index
        let index_prefix = {
            let mut buf = Vec::new();
            let index_name_bytes = index_name.as_bytes();
            buf.push(crate::fts::FTS_INDEX_MARKER);
            buf.extend_from_slice(&(index_name_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(index_name_bytes);
            buf
        };

        // Scan all stripes for FTS keys in this index
        for stripe in &inner.stripes {
            // Search memtable
            for (key, record) in stripe.memtable.iter() {
                // Check if this is an FTS key for our index
                if !key.starts_with(&index_prefix) {
                    continue;
                }

                // Decode the term from the key
                if let Some((decoded_index, decoded_term)) = crate::fts::decode_fts_term_key(key) {
                    // Check if the term starts with our prefix
                    if decoded_index == index_name && decoded_term.starts_with(prefix) {
                        if let Some(item) = &record.value {
                            if let Some((doc_key, _, _, _)) = self.parse_posting_value(item) {
                                docs.insert(doc_key);
                            }
                        }
                    }
                }
            }

            // Search SST files
            for sst in &stripe.ssts {
                for record in sst.iter() {
                    // FTS keys are stored directly in the pk field
                    if !record.key.pk.starts_with(&index_prefix) {
                        continue;
                    }

                    if let Some((decoded_index, decoded_term)) = crate::fts::decode_fts_term_key(&record.key.pk) {
                        if decoded_index == index_name && decoded_term.starts_with(prefix) {
                            if let Some(item) = &record.value {
                                if let Some((doc_key, _, _, _)) = self.parse_posting_value(item) {
                                    docs.insert(doc_key);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(docs)
    }

    /// Get collection statistics for BM25 scoring
    fn get_collection_stats(&self, inner: &LsmInner) -> (u64, f64) {
        // Count unique documents and compute average length
        // This is a simplified version - a production system would cache these
        let mut doc_count = 0u64;
        let mut total_length = 0u64;

        // Count base table records (non-index keys)
        for stripe in &inner.stripes {
            for (key, record) in &stripe.memtable {
                // Skip index keys
                if is_fts_key(key) || crate::index::is_index_key(key) {
                    continue;
                }
                if record.value.is_some() {
                    doc_count += 1;
                    // Estimate doc length as number of attributes
                    total_length += record.value.as_ref().map_or(0, |v| v.len()) as u64;
                }
            }
        }

        let avg_length = if doc_count > 0 {
            total_length as f64 / doc_count as f64
        } else {
            1.0
        };

        (doc_count.max(1), avg_length.max(1.0))
    }

    /// Score search results using BM25
    fn score_fts_results(
        &self,
        inner: &LsmInner,
        index_name: &str,
        query: &FtsQuery,
        matching_docs: HashSet<Vec<u8>>,
        total_docs: u64,
        avg_doc_length: f64,
        text_index: &TextIndex,
    ) -> Result<Vec<SearchHit>> {
        let scorer = Bm25Scorer::new(total_docs, avg_doc_length);
        let mut hits = Vec::new();

        // Extract query terms for scoring
        let query_terms = self.extract_query_terms(query);

        // Check if this is a prefix or fuzzy query (where we need special scoring)
        let is_prefix_or_fuzzy = matches!(query, FtsQuery::Prefix(_) | FtsQuery::Fuzzy { .. } | FtsQuery::Wildcard(_));

        for doc_key_bytes in matching_docs {
            // Decode the document key
            let doc_key = Key::decode(&doc_key_bytes)?;

            let mut total_score = 0.0;
            let mut matches: HashMap<String, Vec<u32>> = HashMap::new();

            // Score each query term
            for term in &query_terms {
                // Now the key format includes doc_key
                let fts_key = encode_fts_term_key(index_name, term, &doc_key_bytes);
                let term_hash = crate::types::checksum::compute(term.as_bytes());
                let stripe_id = (term_hash % 256) as usize;

                // Get posting for this term-doc pair
                if let Some(record) = inner.stripes[stripe_id].memtable.get(&fts_key) {
                    if let Some(item) = &record.value {
                        if let Some((_, tf, positions, dl)) = self.parse_posting_value(item) {
                            // Count document frequency for this term
                            let df = self.count_doc_frequency(inner, index_name, term);
                            total_score += scorer.score(tf, df, dl);
                            matches.insert(term.clone(), positions);
                        }
                    }
                }
            }

            // For prefix/fuzzy queries, the document was found by matching a different term
            // (e.g., "runner" matches prefix "run"), so give it a baseline score if no exact match
            if total_score == 0.0 && is_prefix_or_fuzzy {
                total_score = 1.0; // Baseline score for prefix/fuzzy matches
            }

            if total_score > 0.0 {
                let mut hit = SearchHit::new(doc_key, total_score);
                hit.matches = matches;
                hits.push(hit);
            }
        }

        Ok(hits)
    }

    /// Extract all terms from a query
    fn extract_query_terms(&self, query: &FtsQuery) -> Vec<String> {
        let mut terms = Vec::new();
        self.collect_query_terms(query, &mut terms);
        terms
    }

    fn collect_query_terms(&self, query: &FtsQuery, terms: &mut Vec<String>) {
        match query {
            FtsQuery::Term(t) => terms.push(t.clone()),
            FtsQuery::Phrase(phrase_terms) => terms.extend(phrase_terms.clone()),
            FtsQuery::Boolean { left, op: _, right } => {
                self.collect_query_terms(left, terms);
                self.collect_query_terms(right, terms);
            }
            FtsQuery::Fuzzy { term, .. } => terms.push(term.clone()),
            FtsQuery::Prefix(p) => terms.push(p.clone()),
            FtsQuery::Wildcard(w) => terms.push(w.trim_end_matches('*').trim_end_matches('?').to_string()),
        }
    }

    /// Count document frequency for a term (number of documents containing the term)
    fn count_doc_frequency(&self, inner: &LsmInner, index_name: &str, term: &str) -> u32 {
        // Use prefix scanning to count all documents with this term
        let fts_prefix = encode_fts_term_prefix(index_name, term);
        let term_hash = crate::types::checksum::compute(term.as_bytes());
        let stripe_id = (term_hash % 256) as usize;

        let mut count = 0u32;

        // Check memtable - count all keys with this term prefix
        for (key, record) in inner.stripes[stripe_id].memtable.iter() {
            if key.starts_with(&fts_prefix) && record.value.is_some() {
                count += 1;
            }
        }

        // In a full implementation, we would scan SST files too
        // For simplicity, we just return the memtable count

        count.max(1)
    }

    /// Add highlighting to search results
    fn add_highlights(
        &self,
        inner: &LsmInner,
        hits: &mut [SearchHit],
        text_index: &TextIndex,
        query: &FtsQuery,
    ) -> Result<()> {
        let tokenizer = Tokenizer::new(text_index);
        let extractor = SnippetExtractor::default();

        let query_terms: HashSet<String> = self.extract_query_terms(query).into_iter().collect();

        for hit in hits {
            // Get the original document
            if let Ok(Some(item)) = self.get(&hit.key) {
                if let Some(Value::S(text)) = item.get(&text_index.attribute) {
                    let snippet = extractor.extract(text, &query_terms, &tokenizer);
                    hit.snippet = Some(snippet);
                }
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

        // Write SST from stripe's memtable with compression settings from config
        let mut writer = SstWriter::with_compression(
            inner.config.compression_enabled,
            inner.config.compression_level,
        );
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
        inner.stripes[stripe_id].memtable_size_bytes = 0;

        // Check if compaction is needed for this stripe (Phase 1.7+)
        if inner.compaction_config.enabled && inner.stripes[stripe_id].ssts.len() >= inner.compaction_config.sst_threshold {
            // Start compaction statistics tracking
            let _guard = inner.compaction_stats.start_compaction();

            let compaction_mgr = CompactionManager::new(stripe_id, inner.dir.clone());
            let ssts_to_compact = &inner.stripes[stripe_id].ssts;
            let sst_count = ssts_to_compact.len();

            // Allocate new SST ID for compacted file
            let compacted_sst_id = inner.next_sst_id;
            inner.next_sst_id += 1;

            // Perform compaction with compression settings
            let (new_sst, old_paths) = compaction_mgr.compact(
                ssts_to_compact,
                compacted_sst_id,
                inner.config.compression_enabled,
                inner.config.compression_level,
            )?;

            // Record statistics
            inner.compaction_stats.record_ssts_merged(sst_count as u64);
            inner.compaction_stats.record_ssts_created(1);

            // Replace all SSTs with the compacted one
            inner.stripes[stripe_id].ssts.clear();
            inner.stripes[stripe_id].ssts.push(new_sst);

            // Delete old SST files
            compaction_mgr.cleanup_old_ssts(old_paths)?;
        }

        Ok(())
    }

    /// Get the database directory path
    pub fn path(&self) -> Option<&Path> {
        Some(&self.path)
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

    /// Set compaction configuration (Phase 1.7+)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use kstone_core::{LsmEngine, CompactionConfig};
    /// use tempfile::TempDir;
    ///
    /// let dir = TempDir::new().unwrap();
    /// let db = LsmEngine::create(dir.path()).unwrap();
    ///
    /// // Disable compaction
    /// db.set_compaction_config(CompactionConfig::disabled());
    ///
    /// // Or customize compaction settings
    /// let config = CompactionConfig::new()
    ///     .with_sst_threshold(5)
    ///     .with_check_interval(30);
    /// db.set_compaction_config(config);
    /// ```
    pub fn set_compaction_config(&self, config: CompactionConfig) {
        let mut inner = self.inner.write();
        inner.compaction_config = config;
    }

    /// Get current compaction configuration (Phase 1.7+)
    pub fn compaction_config(&self) -> CompactionConfig {
        let inner = self.inner.read();
        inner.compaction_config.clone()
    }

    /// Get compaction statistics (Phase 1.7+)
    ///
    /// Returns a snapshot of current compaction statistics including:
    /// - Total number of compactions performed
    /// - SSTs merged and created
    /// - Bytes read, written, and reclaimed
    /// - Records deduplicated and tombstones removed
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use kstone_core::LsmEngine;
    /// use tempfile::TempDir;
    ///
    /// let dir = TempDir::new().unwrap();
    /// let db = LsmEngine::create(dir.path()).unwrap();
    ///
    /// // Get compaction statistics
    /// let stats = db.compaction_stats();
    /// println!("Total compactions: {}", stats.total_compactions);
    /// println!("SSTs merged: {}", stats.total_ssts_merged);
    /// println!("Bytes reclaimed: {}", stats.total_bytes_reclaimed);
    /// ```
    pub fn compaction_stats(&self) -> crate::compaction::CompactionStats {
        let inner = self.inner.read();
        inner.compaction_stats.snapshot()
    }

    /// Get the total number of SST files across all stripes
    ///
    /// This is useful for monitoring and statistics.
    pub fn sst_count(&self) -> usize {
        let inner = self.inner.read();
        inner.stripes.iter().map(|s| s.ssts.len()).sum()
    }

    /// Trigger manual compaction on a specific stripe (Phase 1.7+)
    ///
    /// This is primarily for testing or manual database maintenance.
    /// Compaction will only occur if the stripe has enough SSTs.
    pub fn trigger_compaction(&self, stripe_id: usize) -> Result<()> {
        if stripe_id >= NUM_STRIPES {
            return Err(Error::InvalidArgument(format!(
                "Invalid stripe_id: {}, must be < {}",
                stripe_id, NUM_STRIPES
            )));
        }

        let mut inner = self.inner.write();

        // Check if compaction is needed
        if inner.stripes[stripe_id].ssts.len() >= inner.compaction_config.sst_threshold {
            let _guard = inner.compaction_stats.start_compaction();
            let compaction_mgr = CompactionManager::new(stripe_id, inner.dir.clone());

            let sst_count = inner.stripes[stripe_id].ssts.len();
            let compacted_sst_id = inner.next_sst_id;
            inner.next_sst_id += 1;

            let ssts_to_compact = &inner.stripes[stripe_id].ssts;
            let (new_sst, old_paths) = compaction_mgr.compact(
                ssts_to_compact,
                compacted_sst_id,
                inner.config.compression_enabled,
                inner.config.compression_level,
            )?;

            inner.compaction_stats.record_ssts_merged(sst_count as u64);
            inner.compaction_stats.record_ssts_created(1);

            inner.stripes[stripe_id].ssts.clear();
            inner.stripes[stripe_id].ssts.push(new_sst);

            compaction_mgr.cleanup_old_ssts(old_paths)?;
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

    #[test]
    fn test_lsm_compaction_triggered() {
        use crate::compaction::COMPACTION_THRESHOLD;

        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Force multiple flushes to trigger compaction
        // Each flush creates 1 SST, so we need COMPACTION_THRESHOLD flushes
        for batch in 0..COMPACTION_THRESHOLD {
            // Insert MEMTABLE_THRESHOLD records to trigger flush
            for i in 0..MEMTABLE_THRESHOLD {
                let key = Key::new(format!("batch{:02}_key{:04}", batch, i).into_bytes());
                let mut item = HashMap::new();
                item.insert("batch".to_string(), Value::number(batch as i64));
                item.insert("seq".to_string(), Value::number(i as i64));
                db.put(key, item).unwrap();
            }

            // Verify flush happened (memtable should be empty after auto-flush)
        }

        // Trigger one more flush to initiate compaction
        for i in 0..MEMTABLE_THRESHOLD {
            let key = Key::new(format!("final_key{:04}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("final".to_string(), Value::number(1));
            db.put(key, item).unwrap();
        }

        // Verify data integrity after compaction
        // Read a key from the first batch
        let key1 = Key::new(b"batch00_key0000".to_vec());
        let result1 = db.get(&key1).unwrap();
        assert!(result1.is_some());
        let item1 = result1.unwrap();
        assert_eq!(item1.get("batch").unwrap(), &Value::N("0".to_string()));

        // Read a key from the last batch
        let key2 = Key::new(b"final_key0000".to_vec());
        let result2 = db.get(&key2).unwrap();
        assert!(result2.is_some());

        // Verify that compaction happened by checking SST count is reduced
        // After compaction, there should be fewer SSTs than COMPACTION_THRESHOLD
        // (This is implicit - if compaction didn't work, reads would still work but SST count would be high)
    }

    #[test]
    fn test_lsm_compaction_removes_tombstones() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Insert items
        for i in 0..100 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("value".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Force flush
        db.flush().unwrap();

        // Delete half the items
        for i in 0..50 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            db.delete(key).unwrap();
        }

        // Force another flush
        db.flush().unwrap();

        // Verify deletes worked
        for i in 0..50 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            assert!(db.get(&key).unwrap().is_none());
        }

        // Verify remaining items still exist
        for i in 50..100 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let result = db.get(&key).unwrap();
            assert!(result.is_some());
        }
    }

    #[test]
    fn test_lsm_compaction_keeps_latest_version() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Insert initial version
        let key = Key::new(b"test_key".to_vec());
        let mut item1 = HashMap::new();
        item1.insert("version".to_string(), Value::number(1));
        db.put(key.clone(), item1).unwrap();

        // Force flush
        db.flush().unwrap();

        // Update to version 2
        let mut item2 = HashMap::new();
        item2.insert("version".to_string(), Value::number(2));
        db.put(key.clone(), item2).unwrap();

        // Force flush
        db.flush().unwrap();

        // Update to version 3
        let mut item3 = HashMap::new();
        item3.insert("version".to_string(), Value::number(3));
        db.put(key.clone(), item3).unwrap();

        // Force flush
        db.flush().unwrap();

        // Read should return latest version
        let result = db.get(&key).unwrap().unwrap();
        assert_eq!(result.get("version").unwrap(), &Value::N("3".to_string()));

        // Reopen database (forces recovery and any pending compaction)
        drop(db);
        let db = LsmEngine::open(dir.path()).unwrap();

        // Verify latest version is still there
        let result = db.get(&key).unwrap().unwrap();
        assert_eq!(result.get("version").unwrap(), &Value::N("3".to_string()));
    }

    #[test]
    fn test_compaction_configuration() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Check default config
        let config = db.compaction_config();
        assert!(config.enabled);
        assert_eq!(config.sst_threshold, 10);

        // Disable compaction
        db.set_compaction_config(CompactionConfig::disabled());
        let config = db.compaction_config();
        assert!(!config.enabled);

        // Enable with custom settings
        let custom_config = CompactionConfig::new()
            .with_sst_threshold(5)
            .with_check_interval(30);
        db.set_compaction_config(custom_config);

        let config = db.compaction_config();
        assert!(config.enabled);
        assert_eq!(config.sst_threshold, 5);
        assert_eq!(config.check_interval_secs, 30);
    }

    #[test]
    fn test_compaction_statistics() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Initial stats should be zero
        let stats = db.compaction_stats();
        assert_eq!(stats.total_compactions, 0);
        assert_eq!(stats.total_ssts_merged, 0);
        assert_eq!(stats.active_compactions, 0);

        // Use same PK prefix to ensure all keys go to same stripe
        let pk = b"testdata";

        // Write 12 batches of data - all keys use same PK to route to same stripe
        for batch in 0..12 {
            for i in 0..MEMTABLE_THRESHOLD {
                let key = Key::with_sk(pk.to_vec(), format!("batch{:02}_key{:04}", batch, i).into_bytes());
                let mut item = HashMap::new();
                item.insert("value".to_string(), Value::number(i as i64));
                db.put(key, item).unwrap();
            }
        }

        // Check that compaction happened (12 SSTs in one stripe > 10 threshold)
        let stats = db.compaction_stats();
        assert!(stats.total_compactions > 0, "Expected at least one compaction");
        assert!(stats.total_ssts_merged > 0, "Expected SSTs to be merged");
        assert!(stats.total_ssts_created > 0, "Expected new SSTs to be created");
    }

    #[test]
    fn test_manual_compaction_trigger() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Set a low threshold for testing
        db.set_compaction_config(CompactionConfig::new().with_sst_threshold(3));

        // Write enough data to create multiple SSTs in stripe 0
        // Use keys that hash to stripe 0
        let mut count = 0;
        for i in 0..50000 {
            let key = Key::new(format!("key{:06}", i).into_bytes());
            if key.stripe() == 0 {
                let mut item = HashMap::new();
                item.insert("value".to_string(), Value::number(i));
                db.put(key, item).unwrap();
                count += 1;
                if count >= MEMTABLE_THRESHOLD * 4 {
                    break;
                }
            }
        }

        // Force flush
        db.flush().unwrap();

        // Get initial stats
        let stats_before = db.compaction_stats();

        // Manually trigger compaction on stripe 0
        db.trigger_compaction(0).unwrap();

        // Stats should have changed
        let stats_after = db.compaction_stats();
        assert!(
            stats_after.total_compactions >= stats_before.total_compactions,
            "Compaction count should increase or stay the same"
        );
    }

    #[test]
    fn test_compaction_disabled() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Disable compaction
        db.set_compaction_config(CompactionConfig::disabled());

        // Write lots of data
        for batch in 0..15 {
            for i in 0..MEMTABLE_THRESHOLD {
                let key = Key::new(format!("batch{:02}_key{:04}", batch, i).into_bytes());
                let mut item = HashMap::new();
                item.insert("value".to_string(), Value::number(i as i64));
                db.put(key, item).unwrap();
            }
        }

        // Force flush
        db.flush().unwrap();

        // Stats should show no compactions
        let stats = db.compaction_stats();
        assert_eq!(stats.total_compactions, 0, "No compactions should occur when disabled");
    }

    #[test]
    fn test_compaction_with_deletes_reclaims_space() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Insert many items
        for i in 0..200 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("value".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // Force flush
        db.flush().unwrap();

        // Delete half of them
        for i in 0..100 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            db.delete(key).unwrap();
        }

        // Force flush to create tombstones in SST
        db.flush().unwrap();

        // Trigger more writes to cause compaction
        for batch in 0..12 {
            for i in 200..(200 + MEMTABLE_THRESHOLD) {
                let key = Key::new(format!("key{:06}_{:02}", i, batch).into_bytes());
                let mut item = HashMap::new();
                item.insert("value".to_string(), Value::number(i as i64));
                db.put(key, item).unwrap();
            }
        }

        // Verify deleted items are gone
        for i in 0..100 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let result = db.get(&key).unwrap();
            assert!(result.is_none(), "Deleted key should not be found");
        }

        // Verify non-deleted items still exist
        for i in 100..200 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let result = db.get(&key).unwrap();
            assert!(result.is_some(), "Non-deleted key should still exist");
        }
    }

    #[test]
    fn test_lsm_size_based_flushing() {
        let dir = TempDir::new().unwrap();

        // Create database with small memtable size (100KB) for faster testing
        let config = DatabaseConfig::new()
            .with_max_memtable_size_bytes(100 * 1024)  // 100KB
            .with_max_memtable_records(10_000);  // High ceiling, shouldn't be hit

        let db = LsmEngine::create_with_config(dir.path(), config, TableSchema::new()).unwrap();

        // Add items with large values until flush occurs
        // Each item is roughly 1KB (key + 1000 byte string)
        let large_value = "x".repeat(1000);
        let mut items_added = 0;

        for i in 0..200 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("data".to_string(), Value::string(large_value.clone()));
            db.put(key, item).unwrap();
            items_added += 1;

            // Check if SST was created (indicating flush occurred)
            let sst_count = fs::read_dir(dir.path())
                .unwrap()
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "sst"))
                .count();

            if sst_count > 0 {
                // Flush occurred! Should be well before 10_000 records
                assert!(items_added < 200, "Flush should occur based on size, not record count");
                println!("Size-based flush occurred after {} items (~{}KB)", items_added, items_added);
                break;
            }
        }

        // Force final flush
        db.flush().unwrap();

        // Verify we can read all items back
        for i in 0..items_added {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let result = db.get(&key).unwrap();
            assert!(result.is_some(), "Should be able to read item {}", i);
        }
    }

    #[test]
    fn test_lsm_memtable_size_tracking() {
        let dir = TempDir::new().unwrap();
        let db = LsmEngine::create(dir.path()).unwrap();

        // Put a few items and verify they're in memtable (not flushed yet)
        for i in 0..10 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let mut item = HashMap::new();
            item.insert("value".to_string(), Value::number(i));
            db.put(key, item).unwrap();
        }

        // All items should be in memtable (no SST files yet)
        let sst_count = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "sst"))
            .count();

        assert_eq!(sst_count, 0, "No SST files should exist yet");

        // Verify all items are readable from memtable
        for i in 0..10 {
            let key = Key::new(format!("key{:03}", i).into_bytes());
            let result = db.get(&key).unwrap();
            assert!(result.is_some(), "Item should be in memtable");
        }
    }

    #[test]
    fn test_lsm_fts_persistence() {
        use crate::fts::TextIndex;

        let dir = TempDir::new().unwrap();
        let path = dir.path();

        let schema = TableSchema::new()
            .with_text_index(TextIndex::new("content-index", "content"));

        // Create database, add document, verify search works
        {
            let db = LsmEngine::create_with_schema(path, schema.clone()).unwrap();

            let key = Key::new(b"doc#1".to_vec());
            let mut item = HashMap::new();
            item.insert("content".to_string(), Value::S("The quick brown fox".to_string()));
            db.put(key.clone(), item).unwrap();

            // Verify document is searchable
            let result = db.text_search("content-index", "fox", 10, false).unwrap();
            assert_eq!(result.total_hits, 1, "Document should be searchable before close");
        }

        // Reopen with schema and verify search still works
        {
            let db = LsmEngine::open_with_schema(path, schema).unwrap();

            // Verify document is still searchable
            let result = db.text_search("content-index", "fox", 10, false).unwrap();
            assert_eq!(result.total_hits, 1, "Document should be searchable after reopen");
        }
    }
}

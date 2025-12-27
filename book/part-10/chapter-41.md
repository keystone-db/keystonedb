# Chapter 41: Future Roadmap

KeystoneDB has achieved significant milestones through Phase 8, delivering a production-ready embedded database with DynamoDB compatibility, PartiQL support, cloud synchronization, and comprehensive language bindings. This chapter outlines the planned features, enhancements, and strategic direction for the project.

## Vision Statement

**Goal:** KeystoneDB aims to be the definitive embedded database for applications that need DynamoDB-compatible features with local-first capabilities, cloud synchronization, and advanced search features.

**Core Principles:**
1. **Local-First** - Work offline, sync when online
2. **DynamoDB Compatible** - Drop-in replacement for local development and edge deployments
3. **Developer Friendly** - Great documentation, tooling, and debugging experience
4. **Performance** - Competitive with best-in-class embedded databases
5. **Open Source** - Community-driven development

## Completed Phases

### Phase 0-7: Core Database (COMPLETE ✅)

All foundational phases are complete:
- **Phase 0**: Walking skeleton with basic CRUD
- **Phase 1**: Core storage engine (256-stripe LSM, WAL, SST, compaction)
- **Phase 2**: Complete DynamoDB API (Query, Scan, Batch, Transactions)
- **Phase 3**: Secondary indexes (LSI, GSI), TTL, and Streams
- **Phase 4**: PartiQL SQL-compatible query language
- **Phase 5**: In-memory database mode
- **Phase 6**: gRPC server and client library
- **Phase 7**: Interactive CLI shell with autocomplete

### Phase 8: Cloud Synchronization (COMPLETE ✅)

Full bidirectional synchronization with cloud storage and remote databases:

**Features Implemented:**
- **Vector Clocks**: Causality tracking for distributed operations
- **Merkle Trees**: Efficient diff detection with 16-way fanout
- **Sync Engine**: State machine (Idle → Connecting → Handshaking → Discovering → Transferring → Committing → Completed)
- **Conflict Resolution**: LastWriterWins, FirstWriterWins, and Custom strategies
- **S3 Protocol**: Upload/download snapshots to S3-compatible storage
- **Filesystem Protocol**: Sync between local databases
- **Offline Queue**: Store pending operations with retry policies

**API Example:**
```rust
use kstone_sync::{CloudSyncBuilder, SyncEndpoint, ConflictStrategy};
use kstone_api::Database;
use std::sync::Arc;

let db = Arc::new(Database::create("local.keystone")?);
let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(SyncEndpoint::S3 {
        bucket: "my-bucket".to_string(),
        prefix: "backups/".to_string(),
        region: "us-east-1".to_string(),
        endpoint_url: None,
        credentials: None,
    })
    .with_conflict_strategy(ConflictStrategy::LastWriterWins)
    .with_sync_interval(std::time::Duration::from_secs(30))
    .with_batch_size(100)
    .build()?;

// Perform sync
let stats = engine.sync(endpoint).await?;
println!("Sent: {}, Received: {}", stats.items_sent, stats.items_received);
```

See **Chapter 42: Cloud Synchronization** for complete documentation.

### Language Bindings (COMPLETE ✅)

Production-ready bindings for multiple languages:

**Python Bindings (PyO3):**
```python
from keystonedb import Database

db = Database.create("mydb.keystone")
db.put(b"user#123", {"name": "Alice", "age": 30})
item = db.get(b"user#123")
print(item["name"])  # Alice

# Full query support
results = db.query(b"org#acme", sk_begins_with=b"USER#", limit=10)

# PartiQL execution
db.execute("SELECT * FROM items WHERE pk = 'user#123'")
```

**Node.js Bindings (napi-rs):**
```javascript
const { Database } = require('keystonedb');

const db = Database.create('mydb.keystone');
db.put('user#123', { name: 'Alice', age: 30 });
const item = db.get('user#123');
console.log(item.name);  // Alice

// Full query support
const results = db.query('org#acme', { skBeginsWith: 'USER#', limit: 10 });

// PartiQL execution
db.execute("SELECT * FROM items WHERE pk = 'user#123'");
```

**C-FFI Bindings:**
```c
#include "keystonedb.h"

kstone_db_t* db = kstone_db_create("mydb.keystone");
kstone_item_t* item = kstone_item_create();
kstone_item_set_string(item, "name", "Alice");
kstone_item_set_number(item, "age", 30);
kstone_db_put(db, "user#123", 8, item);
kstone_item_free(item);
kstone_db_free(db);
```

See **Chapter 43: Language Bindings** for complete documentation.

## Phase 9: Advanced Compaction (Planned)

**Status:** Planned for Q2 2025

### Leveled Compaction Strategy

**Objective:** Reduce write amplification for write-heavy workloads.

**Current (Size-Tiered):**
- All SSTs in stripe merged at once
- High write amplification (records rewritten multiple times)
- Good for read-heavy workloads

**Planned (Leveled):**
- SSTs organized into levels (L0, L1, L2, ...)
- Each level 10x larger than previous
- Selective compaction (only overlapping SSTs)
- Lower write amplification

**Architecture:**
```
L0: [SST SST SST SST]  (recent flushes, unsorted)
L1: [SST SST SST SST SST SST SST SST]  (10x L0, sorted)
L2: [SST SST ... (80 SSTs) ...]  (10x L1, sorted)
L3: [SST SST ... (800 SSTs) ...]  (10x L2, sorted)
```

**Benefits:**
- 5-10x reduction in write amplification
- Predictable read performance (max log(N) levels)
- Space amplification bounded

### Parallel Compaction

**Objective:** Compact multiple stripes concurrently.

**Implementation:**
```rust
// Compact up to 4 stripes in parallel
let config = CompactionConfig::new()
    .with_max_concurrent_compactions(4);

// Background worker spawns 4 compaction threads
for stripe_id in stripes_needing_compaction {
    if active_compactions.len() < config.max_concurrent_compactions {
        thread::spawn(move || {
            compact_stripe(stripe_id);
        });
    }
}
```

### Compaction Prioritization

**Objective:** Compact stripes with highest benefit first.

**Priority Score:**
```rust
fn compaction_priority(stripe: &Stripe) -> f64 {
    let space_amp = stripe.total_sst_size / stripe.live_data_size;
    let read_amp = stripe.sst_count as f64;
    let write_load = stripe.recent_write_rate;

    // Higher score = higher priority
    space_amp * 0.5 + read_amp * 0.3 + write_load * 0.2
}
```

## Phase 10: Vector Search (Planned)

**Status:** Planned for Q3 2025

### Native Vector Support

**Objective:** First-class support for embeddings and similarity search.

**Features:**
- VecF32 value type (already implemented)
- HNSW (Hierarchical Navigable Small World) index
- Approximate nearest neighbor search
- Cosine and Euclidean distance metrics

**API Example:**
```rust
// Store embeddings
let embedding = vec![0.1, 0.2, 0.3, /* ... 768 dimensions */];
db.put(b"doc#123", ItemBuilder::new()
    .string("content", "The quick brown fox...")
    .vector("embedding", embedding)
    .build())?;

// Create vector index
let schema = TableSchema::new()
    .with_vector_index(VectorIndex::new("embedding_idx")
        .attribute("embedding")
        .dimensions(768)
        .metric(DistanceMetric::Cosine)
        .index_type(IndexType::HNSW)
    );

// Similarity search
let query_vector = vec![0.15, 0.22, 0.31, /* ... */];
let results = db.vector_search("embedding_idx", &query_vector)
    .limit(10)
    .min_score(0.8)
    .execute()?;

for result in results {
    println!("Document: {}, Score: {:.3}", result.key, result.score);
}
```

### HNSW Index Structure

**Architecture:**
- Multi-layer graph structure
- Greedy search with backtracking
- Incremental index updates
- Disk-based storage (not in-memory)

**Performance:**
- Query: ~1-10ms for top-10 on 1M vectors
- Insert: ~10-50ms per vector
- Index size: ~50 bytes per vector + graph overhead

### Use Cases

- **Semantic search** - Find similar documents by meaning
- **Recommendation systems** - Similar items/users
- **Image similarity** - Find visually similar images
- **Anomaly detection** - Find outliers in embeddings

## Phase 11: Full-Text Search (Planned)

**Status:** Planned for Q4 2025

### Inverted Index

**Objective:** Fast keyword search across text attributes.

**Features:**
- Tokenization (Unicode, stemming, stop words)
- Inverted index per text attribute
- Boolean queries (AND, OR, NOT)
- Phrase queries
- Fuzzy matching (Levenshtein distance)

**API Example:**
```rust
// Create full-text index
let schema = TableSchema::new()
    .with_text_index(TextIndex::new("content_idx")
        .attribute("content")
        .language(Language::English)
        .stemming(true)
        .stop_words(true)
    );

// Text search
let results = db.text_search("content_idx", "quick brown fox")
    .limit(20)
    .highlight(true)
    .execute()?;

for result in results {
    println!("Match: {}", result.key);
    println!("Snippet: {}", result.snippet);
}
```

### Search Features

**Ranking:**
- TF-IDF (Term Frequency-Inverse Document Frequency)
- BM25 (Best Match 25) algorithm
- Custom scoring functions

**Highlighting:**
- Return snippets with matched terms highlighted
- Context window around matches

**Filtering:**
- Combine full-text search with DynamoDB queries
- Filter by attributes while searching text

## Phase 12: DynamoDB Attachment (Planned)

**Status:** Planned for 2026

### Bidirectional DynamoDB Sync

**Objective:** Seamless synchronization with AWS DynamoDB.

**Architecture:**
```
┌─────────────────┐
│  Local KeystoneDB│
│  (Embedded)      │
└────────┬─────────┘
         │
         ▼
    ┌────────┐
    │ Sync   │ ← Conflict Resolution
    │ Engine │ ← Incremental Sync
    └────┬───┘ ← Change Tracking
         │
         ▼
┌─────────────────┐
│  AWS DynamoDB    │
│  (Cloud)         │
└──────────────────┘
```

**Features:**
- Initial sync (full table download)
- Incremental sync (changes only via DynamoDB Streams)
- Conflict resolution (last-write-wins or custom)
- Offline operation (sync when reconnected)
- Bi-directional sync (local ↔ cloud)

**API Example:**
```rust
let sync = DynamoSync::new()
    .table_name("prod-users")
    .region("us-east-1")
    .credentials(aws_creds)
    .sync_interval(Duration::from_secs(60))
    .conflict_resolution(ConflictResolution::LastWriteWins);

// Start background sync
let handle = sync.start(db.clone())?;

// Local writes sync to DynamoDB
db.put(b"user#123", item)?;  // → synced to DynamoDB

// Stop sync
handle.stop()?;
```

## Phase 13: Advanced Features (Long-term)

**Status:** Long-term (2026+)

### Encryption at Rest

**Features:**
- AES-256-GCM encryption
- Per-block encryption (SST and WAL)
- Key rotation support
- Hardware acceleration (AES-NI)

**API:**
```rust
let encryption = EncryptionConfig::new()
    .algorithm(Algorithm::Aes256Gcm)
    .key_provider(KeyProvider::File("/secure/keys/db.key"));

let db = Database::create_with_encryption(path, encryption)?;
```

### Point-in-Time Recovery (PITR)

**Features:**
- Continuous WAL archiving
- Restore to any point in time
- Incremental backups

**API:**
```rust
// Enable PITR
db.enable_pitr(PITRConfig {
    archive_dir: "/backups/wal-archive",
    retention: Duration::from_days(30),
})?;

// Restore to specific time
let restored = Database::restore_to_time(
    backup_path,
    DateTime::parse_from_rfc3339("2025-01-15T10:30:00Z")?
)?;
```

### Multi-Tenancy

**Features:**
- Namespace isolation within single database
- Per-tenant resource limits
- Tenant-level statistics

**API:**
```rust
let db = Database::create_multi_tenant(path)?;

// Tenant A
let tenant_a = db.tenant("tenant-a")?;
tenant_a.put(b"user#123", item)?;

// Tenant B (isolated from A)
let tenant_b = db.tenant("tenant-b")?;
tenant_b.put(b"user#123", different_item)?;  // No conflict
```

## Performance Targets

Target performance for v2.0 (end of Phase 13):

### Write Performance
- **Single-threaded:** 10k ops/sec
- **Multi-threaded (group commit):** 50k ops/sec
- **Batch writes:** 100k ops/sec
- **P99 latency:** <10ms

### Read Performance
- **Hot data (memtable):** 200k ops/sec
- **Cold data (SST):** 50k ops/sec
- **Query with SK range:** 20k ops/sec
- **P99 latency:** <5ms

### Scalability
- **Database size:** 1TB per instance
- **Table size:** 100M items
- **Concurrent connections (server):** 10,000
- **Replication lag:** <100ms (99th percentile)

## Contributing

KeystoneDB is open source and welcomes contributions:

**Priority Areas:**
1. Performance benchmarking and optimization
2. Documentation and tutorials
3. Additional language bindings (Go, Java, Swift)
4. Integration tests and fuzzing
5. Example applications and use cases

**How to Contribute:**
1. Check GitHub issues for "good first issue" labels
2. Read CONTRIBUTING.md for guidelines
3. Join discussions in GitHub Discussions
4. Submit PRs with tests and documentation

**Governance:**
- Benevolent Dictator For Life (BDFL) model currently
- Transition to governance committee as project matures
- Major decisions via RFC (Request for Comments) process

## Release Cadence

**Current Plan:**
- **Minor releases (0.x):** Every 1-2 months
- **Major releases (1.0, 2.0):** Annually
- **Patch releases (x.y.z):** As needed for critical bugs

**Version 1.0 Criteria:**
- All Phase 9-11 features complete
- Production deployments in 3+ organizations
- 90%+ test coverage
- Complete documentation
- Stable API (semantic versioning)

## Long-Term Vision (3-5 Years)

**Goal:** Become the default choice for:
1. **Local-first applications** - Offline-capable web and mobile apps
2. **Edge computing** - Databases at the edge (CDN, IoT gateways)
3. **Embedded systems** - Resource-constrained devices
4. **DynamoDB development** - Local development and testing
5. **Hybrid cloud** - Applications spanning edge and cloud

**Success Metrics:**
- 10,000+ GitHub stars
- 100+ production deployments
- 1M+ downloads per month
- Active community of 50+ contributors
- Comprehensive ecosystem (tools, libraries, integrations)

## Getting Involved

**Stay Updated:**
- GitHub: https://github.com/keystonedb/keystonedb
- Documentation: https://docs.keystonedb.io
- Blog: https://keystonedb.io/blog
- Twitter: @keystonedb

**Community:**
- Discord: https://discord.gg/keystonedb
- GitHub Discussions: https://github.com/keystonedb/keystonedb/discussions
- Monthly community calls (see calendar)

**Support:**
- GitHub Issues for bugs
- GitHub Discussions for questions
- Email: support@keystonedb.io

The future of KeystoneDB is bright, and we're excited to build it together with the community!

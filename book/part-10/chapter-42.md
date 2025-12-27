# Chapter 42: Cloud Synchronization

KeystoneDB's cloud synchronization feature (kstone-sync) enables bidirectional synchronization between local databases and remote storage backends like S3 or other KeystoneDB instances. This chapter covers the sync architecture, configuration, conflict resolution strategies, and practical usage patterns.

## Overview

The sync system provides:

- **Vector Clocks**: Track causality across distributed operations
- **Merkle Trees**: Efficiently detect differences between databases
- **Conflict Resolution**: Multiple strategies including LastWriterWins, VectorClock, and AttributeMerge
- **Offline Support**: Queue operations when disconnected, sync when reconnected
- **Multiple Backends**: S3-compatible storage and filesystem sync

## Architecture

### Component Structure

```
┌────────────────────────────────────────────────────────────┐
│                    SyncEngine                               │
│  ┌──────────────────────────────────────────────────────┐  │
│  │  State Machine                                        │  │
│  │  Idle → Connecting → Handshaking → Discovering →      │  │
│  │  Transferring → ResolvingConflicts → Committing       │  │
│  └──────────────────────────────────────────────────────┘  │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐   │
│  │ ChangeTracker│  │MerkleTree   │  │ ConflictManager │   │
│  │ - Track local│  │- 16-way fan │  │ - Resolution    │   │
│  │   changes   │  │- Diff detect │  │   strategies    │   │
│  └─────────────┘  └─────────────┘  └──────────────────┘   │
│                                                             │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────────┐   │
│  │VectorClock  │  │OfflineQueue │  │ SyncMetadata     │   │
│  │- Causality  │  │- Pending ops│  │ - Checkpoints    │   │
│  │  tracking   │  │- Retry logic│  │ - Endpoint info  │   │
│  └─────────────┘  └─────────────┘  └──────────────────┘   │
└────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────┐
│                   SyncProtocol                              │
│  ┌─────────────────┐        ┌─────────────────────────┐    │
│  │ S3Protocol      │        │ FilesystemProtocol      │    │
│  │ - S3 storage    │        │ - Local file sync       │    │
│  │ - Upload/download│       │ - Database replication  │    │
│  └─────────────────┘        └─────────────────────────┘    │
└────────────────────────────────────────────────────────────┘
```

### Sync State Machine

The sync engine operates as a state machine:

```rust
pub enum SyncState {
    /// Not connected
    Idle,
    /// Connecting to endpoint
    Connecting,
    /// Performing handshake
    Handshaking,
    /// Discovering changes via Merkle tree
    Discovering,
    /// Negotiating what to sync
    Negotiating,
    /// Transferring data
    Transferring { sent: usize, received: usize, total: usize },
    /// Resolving conflicts
    ResolvingConflicts,
    /// Committing changes
    Committing,
    /// Sync completed
    Completed,
    /// Error occurred
    Error(String),
}
```

## Getting Started

### Basic Setup

Create a sync engine with the `CloudSyncBuilder`:

```rust
use kstone_sync::{CloudSyncBuilder, SyncEndpoint, ConflictStrategy};
use kstone_api::Database;
use std::sync::Arc;

// Open or create your database
let db = Arc::new(Database::create("mydb.keystone")?);

// Create sync engine
let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(SyncEndpoint::FileSystem {
        path: "/path/to/remote.keystone".to_string(),
    })
    .with_conflict_strategy(ConflictStrategy::LastWriterWins)
    .with_sync_interval(std::time::Duration::from_secs(30))
    .with_batch_size(100)
    .build()?;
```

### Sync Configuration

The `SyncConfig` structure controls sync behavior:

```rust
pub struct SyncConfig {
    /// Endpoint to sync with
    pub endpoint: SyncEndpoint,
    /// Conflict resolution strategy
    pub conflict_strategy: ConflictStrategy,
    /// Automatic sync interval (None for manual sync only)
    pub sync_interval: Option<Duration>,
    /// Batch size for operations
    pub batch_size: usize,
    /// Maximum retries for failed operations
    pub max_retries: u32,
    /// Enable compression for transfers
    pub enable_compression: bool,
}
```

## Sync Endpoints

### S3-Compatible Storage

Sync to any S3-compatible storage (AWS S3, MinIO, DigitalOcean Spaces, etc.):

```rust
use kstone_sync::SyncEndpoint;

let endpoint = SyncEndpoint::S3 {
    bucket: "my-keystone-backup".to_string(),
    prefix: "databases/production/".to_string(),
    region: "us-east-1".to_string(),
    endpoint_url: None,  // Use default AWS endpoint
    credentials: None,   // Use default credential chain
};

let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(endpoint)
    .build()?;

// Perform sync
let stats = engine.sync().await?;
println!("Uploaded {} items, downloaded {} items",
    stats.items_sent, stats.items_received);
```

### Custom S3 Endpoint (MinIO)

```rust
let endpoint = SyncEndpoint::S3 {
    bucket: "keystone".to_string(),
    prefix: "backups/".to_string(),
    region: "us-east-1".to_string(),
    endpoint_url: Some("http://localhost:9000".to_string()),
    credentials: Some(S3Credentials {
        access_key: "minioadmin".to_string(),
        secret_key: "minioadmin".to_string(),
    }),
};
```

### Filesystem Sync

Sync between local databases (useful for backup or development):

```rust
let endpoint = SyncEndpoint::FileSystem {
    path: "/backups/mydb-replica.keystone".to_string(),
};

let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(endpoint)
    .build()?;

// Sync creates the remote database if it doesn't exist
let stats = engine.sync().await?;
```

## Vector Clocks

Vector clocks track causality across distributed systems, enabling detection of concurrent modifications.

### How Vector Clocks Work

```rust
use kstone_sync::VectorClock;

// Each endpoint has its own counter
let mut clock = VectorClock::new();

// Increment on local write
clock.increment(&local_endpoint_id);

// Merge with remote clock on sync
clock.merge(&remote_clock);

// Compare causality
match clock.compare(&other_clock) {
    ClockOrdering::Before => println!("This happened before other"),
    ClockOrdering::After => println!("This happened after other"),
    ClockOrdering::Equal => println!("Same version"),
    ClockOrdering::Concurrent => println!("Conflict! Concurrent modifications"),
}
```

### Vector Clock Operations

```rust
// Create with initial local entry
let clock = VectorClock::with_local(endpoint_id, 1);

// Check if concurrent (conflict detection)
if clock.concurrent_with(&remote_clock) {
    println!("Concurrent modifications detected!");
}

// Update specific endpoint's counter
clock.update(endpoint_id, new_value);

// Get logical timestamp for an endpoint
let ts = clock.get(&endpoint_id);
```

## Merkle Trees

Merkle trees enable efficient difference detection between databases without transferring all data.

### Tree Structure

```rust
use kstone_sync::MerkleTree;

// Create a 16-way fanout tree
let mut tree = MerkleTree::new();

// Add items (typically done during change tracking)
tree.insert(&key, &value_hash);

// Get root hash for comparison
let root_hash = tree.root_hash();

// Find differences with remote tree
let diffs = tree.diff(&remote_tree);
for diff in diffs {
    match diff {
        DiffType::LocalOnly(key) => println!("Only in local: {:?}", key),
        DiffType::RemoteOnly(key) => println!("Only in remote: {:?}", key),
        DiffType::Different(key) => println!("Modified: {:?}", key),
    }
}
```

### Efficient Sync with Merkle Trees

The sync process:

1. Exchange root hashes
2. If hashes match, no changes needed
3. If different, recursively compare subtrees
4. Only transfer actual differences

This reduces bandwidth for large databases with small changes.

## Conflict Resolution

### Conflict Strategies

```rust
use kstone_sync::ConflictStrategy;

// Last writer wins (based on timestamp)
let strategy = ConflictStrategy::LastWriterWins;

// First writer wins (keep existing)
let strategy = ConflictStrategy::FirstWriterWins;

// Use vector clock causality
let strategy = ConflictStrategy::VectorClock;

// Merge at attribute level
let strategy = ConflictStrategy::AttributeMerge;

// Manual resolution (queue for later)
let strategy = ConflictStrategy::Manual;

// Custom resolver
let strategy = ConflictStrategy::Custom("my_resolver".to_string());
```

### Conflict Structure

When conflicts are detected:

```rust
pub struct Conflict {
    pub id: String,
    pub key: Key,
    pub local_item: Option<Item>,
    pub remote_item: Option<Item>,
    pub local_clock: VectorClock,
    pub remote_clock: VectorClock,
    pub local_timestamp: i64,
    pub remote_timestamp: i64,
    pub detected_at: i64,
    pub strategy: ConflictStrategy,
    pub resolved: bool,
    pub resolution: Option<ConflictResolution>,
}
```

### Resolution Outcomes

```rust
pub enum ConflictResolution {
    /// Use the local version
    UseLocal(Option<Item>),
    /// Use the remote version
    UseRemote(Option<Item>),
    /// Use a merged version
    Merged(Option<Item>),
    /// Resolution deferred (manual intervention needed)
    Deferred,
}
```

### Attribute-Level Merge

The `AttributeMerge` strategy combines attributes from both versions:

```rust
// Local item: { "name": "Alice", "age": 30 }
// Remote item: { "age": 31, "email": "alice@example.com" }
// Merged (remote is newer): { "name": "Alice", "age": 31, "email": "alice@example.com" }

let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(endpoint)
    .with_conflict_strategy(ConflictStrategy::AttributeMerge)
    .build()?;
```

### Custom Conflict Resolver

Implement the `ConflictResolver` trait:

```rust
use kstone_sync::{ConflictResolver, Conflict, ConflictResolution};

struct MyResolver;

impl ConflictResolver for MyResolver {
    fn resolve(&self, conflict: &Conflict) -> Result<ConflictResolution> {
        // Custom logic
        if let (Some(local), Some(remote)) = (&conflict.local_item, &conflict.remote_item) {
            // Example: prefer item with higher priority
            let local_priority = local.get("priority")
                .and_then(|v| v.as_number())
                .unwrap_or(0);
            let remote_priority = remote.get("priority")
                .and_then(|v| v.as_number())
                .unwrap_or(0);

            if local_priority >= remote_priority {
                Ok(ConflictResolution::UseLocal(Some(local.clone())))
            } else {
                Ok(ConflictResolution::UseRemote(Some(remote.clone())))
            }
        } else {
            // Fall back to last writer wins
            Ok(conflict.resolve_last_writer_wins())
        }
    }

    fn name(&self) -> &str {
        "priority_resolver"
    }
}
```

## Offline Support

The offline queue stores operations when the endpoint is unreachable.

### Offline Queue

```rust
use kstone_sync::{OfflineQueue, PendingOperation, RetryPolicy};

// Configure retry policy
let policy = RetryPolicy {
    max_retries: 5,
    initial_delay: Duration::from_secs(1),
    max_delay: Duration::from_secs(60),
    multiplier: 2.0,
};

let queue = OfflineQueue::new(policy, 1000); // Max 1000 pending ops

// Operations are automatically queued when sync fails
```

### Retry Policy

```rust
pub struct RetryPolicy {
    /// Maximum number of retries
    pub max_retries: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay (caps exponential backoff)
    pub max_delay: Duration,
    /// Backoff multiplier
    pub multiplier: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(300), // 5 minutes
            multiplier: 2.0,
        }
    }
}
```

### Pending Operations

```rust
pub struct PendingOperation {
    pub id: String,
    pub operation: SyncOperation,
    pub created_at: i64,
    pub retry_count: u32,
    pub last_error: Option<String>,
    pub next_retry: i64,
}

pub enum SyncOperation {
    Put { key: Key, item: Item },
    Delete { key: Key },
}
```

## Sync Events

Subscribe to sync events for monitoring and UI updates:

```rust
let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(endpoint)
    .build()?;

// Subscribe to events
let mut rx = engine.subscribe().unwrap();

// Handle events
tokio::spawn(async move {
    while let Some(event) = rx.recv().await {
        match event {
            SyncEvent::Started { endpoint_id } => {
                println!("Sync started with {:?}", endpoint_id);
            }
            SyncEvent::Progress { sent, received, total } => {
                println!("Progress: {}/{} items", sent + received, total);
            }
            SyncEvent::ConflictDetected { key, conflict_id } => {
                println!("Conflict on {:?}: {}", key, conflict_id);
            }
            SyncEvent::ConflictResolved { conflict_id } => {
                println!("Resolved conflict: {}", conflict_id);
            }
            SyncEvent::Completed { stats } => {
                println!("Sync complete! Sent: {}, Received: {}",
                    stats.items_sent, stats.items_received);
            }
            SyncEvent::Failed { error } => {
                eprintln!("Sync failed: {}", error);
            }
            _ => {}
        }
    }
});

// Perform sync
engine.sync().await?;
```

## Sync Statistics

Track sync performance and history:

```rust
pub struct SyncStats {
    pub total_syncs: u64,
    pub successful_syncs: u64,
    pub failed_syncs: u64,
    pub conflicts_detected: u64,
    pub conflicts_resolved: u64,
    pub conflicts_pending: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub items_sent: u64,
    pub items_received: u64,
    pub last_sync_time: Option<i64>,
    pub avg_sync_duration_ms: u64,
}

// Get current stats
let stats = engine.get_stats();
println!("Total syncs: {}, Success rate: {:.1}%",
    stats.total_syncs,
    (stats.successful_syncs as f64 / stats.total_syncs as f64) * 100.0);
```

## Automatic Sync

Configure automatic periodic sync:

```rust
let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(endpoint)
    .with_sync_interval(Duration::from_secs(60)) // Sync every minute
    .build()?;

// Start background sync
let handle = engine.start_background_sync()?;

// Your application continues running...
// Syncs happen automatically every 60 seconds

// Stop when done
handle.stop()?;
```

## Best Practices

### 1. Choose the Right Conflict Strategy

```rust
// For user-facing data where recency matters
ConflictStrategy::LastWriterWins

// For append-only or immutable data
ConflictStrategy::FirstWriterWins

// For complex documents with independent fields
ConflictStrategy::AttributeMerge

// When you need full control
ConflictStrategy::Custom("my_resolver".to_string())
```

### 2. Handle Offline Gracefully

```rust
// Configure generous retry policy
let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(endpoint)
    .with_max_retries(10)
    .build()?;

// Check offline queue status
let queue_stats = engine.offline_queue_stats();
if queue_stats.pending_count > 0 {
    println!("{} operations pending sync", queue_stats.pending_count);
}
```

### 3. Monitor Sync Health

```rust
let mut rx = engine.subscribe().unwrap();

tokio::spawn(async move {
    while let Some(event) = rx.recv().await {
        match event {
            SyncEvent::Failed { error } => {
                // Alert, log, or trigger recovery
                metrics::counter!("sync_failures").increment(1);
                log::error!("Sync failed: {}", error);
            }
            SyncEvent::Completed { stats } => {
                metrics::gauge!("sync_items_sent").set(stats.items_sent as f64);
                metrics::gauge!("sync_items_received").set(stats.items_received as f64);
            }
            _ => {}
        }
    }
});
```

### 4. Optimize Batch Size

```rust
// Smaller batches for real-time sync
let engine = CloudSyncBuilder::new()
    .with_batch_size(10)
    .build()?;

// Larger batches for bulk sync
let engine = CloudSyncBuilder::new()
    .with_batch_size(1000)
    .build()?;
```

### 5. Enable Compression for Slow Networks

```rust
let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(endpoint)
    .with_compression(true) // Enable Zstd compression
    .build()?;
```

## Use Cases

### Backup to S3

```rust
let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(SyncEndpoint::S3 {
        bucket: "backups".to_string(),
        prefix: "daily/".to_string(),
        region: "us-east-1".to_string(),
        endpoint_url: None,
        credentials: None,
    })
    .with_sync_interval(Duration::from_hours(24))
    .build()?;

engine.start_background_sync()?;
```

### Multi-Device Sync

```rust
// Same configuration on all devices
let engine = CloudSyncBuilder::new()
    .with_database(db)
    .with_endpoint(SyncEndpoint::S3 {
        bucket: "user-data".to_string(),
        prefix: format!("users/{}/", user_id),
        region: "us-east-1".to_string(),
        endpoint_url: None,
        credentials: Some(user_credentials),
    })
    .with_conflict_strategy(ConflictStrategy::LastWriterWins)
    .with_sync_interval(Duration::from_secs(30))
    .build()?;
```

### Development Replication

```rust
// Replicate production to development
let engine = CloudSyncBuilder::new()
    .with_database(dev_db)
    .with_endpoint(SyncEndpoint::FileSystem {
        path: "/path/to/prod.keystone".to_string(),
    })
    .with_conflict_strategy(ConflictStrategy::FirstWriterWins) // Keep prod data
    .build()?;

// One-time sync
engine.sync().await?;
```

## Summary

KeystoneDB's cloud synchronization provides:

- **Flexible Backends**: S3-compatible storage and filesystem sync
- **Robust Conflict Resolution**: Multiple strategies with custom resolver support
- **Efficient Diff Detection**: Merkle trees minimize data transfer
- **Offline Resilience**: Queue operations and sync when reconnected
- **Event-Driven**: Subscribe to sync events for monitoring and UI

The sync system enables local-first applications that work offline and sync seamlessly when connected, making KeystoneDB ideal for mobile apps, edge computing, and hybrid cloud scenarios.

For language-specific sync usage, see Chapter 43: Language Bindings.

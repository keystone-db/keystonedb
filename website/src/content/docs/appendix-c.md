# Appendix C: Glossary

This appendix provides definitions for technical terms used throughout the KeystoneDB documentation.

## A

### ACID
**Atomicity, Consistency, Isolation, Durability** - The four key properties that guarantee database transactions are processed reliably. KeystoneDB provides full ACID guarantees for all write operations.

### Append-Only
A write pattern where data is only added to the end of a file, never modified in place. The WAL (Write-Ahead Log) uses an append-only design for durability and performance.

### Atomicity
The property that database operations either complete entirely or have no effect. If a transaction fails partway through, all changes are rolled back.

## B

### BTree / BTreeMap
A self-balancing tree data structure that maintains sorted data and allows searches, insertions, and deletions in O(log n) time. KeystoneDB uses Rust's `BTreeMap` for in-memory memtables.

### Binary Search
An efficient search algorithm that works on sorted data by repeatedly dividing the search space in half. KeystoneDB uses binary search to find keys in sorted SST files.

### Bincode
A binary serialization format used by KeystoneDB for encoding records. Bincode provides compact, deterministic encoding with good performance.

### Bloom Filter
A space-efficient probabilistic data structure used to test whether an element is a member of a set. False positives are possible, but false negatives are not. KeystoneDB uses bloom filters to avoid unnecessary SST file reads.

## C

### Checksum
A value computed from data to detect errors. KeystoneDB uses CRC32C checksums to detect corruption in WAL and SST files.

### Compaction
The process of merging multiple SST files into a single file, removing deleted records (tombstones) and duplicate versions. Compaction reclaims disk space and improves read performance.

### Composite Key
A key consisting of both a partition key and a sort key. Example: `(b"user#123", b"profile")`.

### Conditional Write
A write operation that only succeeds if a specified condition is met. Used for optimistic locking and ensuring data consistency.

### Consistency
The property that database operations maintain all defined rules and constraints. In ACID, consistency ensures the database moves from one valid state to another.

### CRC32C
Cyclic Redundancy Check with Castagnoli polynomial - A checksum algorithm with hardware acceleration on modern CPUs. KeystoneDB uses CRC32C for corruption detection.

### Crash Recovery
The process of restoring a database to a consistent state after an unexpected shutdown or crash. KeystoneDB recovers by replaying the Write-Ahead Log.

## D

### Durability
The property that committed changes survive system failures. KeystoneDB ensures durability through write-ahead logging with fsync.

## E

### Embedded Database
A database that runs within the application process rather than as a separate server. KeystoneDB can run in embedded mode (in-process) or server mode (remote access).

### Encoding
The process of converting data structures into a binary format for storage. KeystoneDB uses bincode encoding for records.

### Endianness
The byte order used to represent multi-byte numbers. KeystoneDB uses little-endian for all integers (except magic numbers), matching most modern CPUs.

### Expression
A syntax for specifying update operations or conditions in DynamoDB-style. Examples: `"SET age = age + 1"`, `"balance >= :amount"`.

## F

### False Positive
An error where a test indicates something is present when it is not. Bloom filters can produce false positives (saying a key might be in an SST when it's not), but this is acceptable because it only causes an unnecessary read.

### Flush
The process of writing in-memory memtable data to disk as an SST file. Triggered when memtable reaches a size threshold (default: 1000 records).

### Fsync
A system call that forces buffered data to be written to physical disk storage. KeystoneDB uses fsync to ensure durability of WAL writes.

## G

### Global Secondary Index (GSI)
An index with a partition key different from the base table's partition key. Enables queries across different partitions. Example: Query all users by email address.

### Group Commit
An optimization where multiple concurrent write operations share a single fsync call, improving throughput. KeystoneDB implements automatic group commit for WAL writes.

## H

### Hash
A function that maps data to a fixed-size value. KeystoneDB uses CRC32 hashing to determine which stripe a key belongs to.

### HNSW (Hierarchical Navigable Small World)
A graph-based algorithm for approximate nearest neighbor search. Planned for KeystoneDB's vector search feature (Phase 10).

## I

### Immutable
Data that cannot be changed after creation. SST files in KeystoneDB are immutable once written, enabling concurrent reads without locking.

### Index
A data structure that improves query performance by providing alternative access paths. KeystoneDB supports Local Secondary Indexes (LSI) and Global Secondary Indexes (GSI).

### Isolation
The property that concurrent transactions don't interfere with each other. KeystoneDB provides Read Committed isolation level.

### Item
A collection of attributes (key-value pairs) stored in the database. Equivalent to a row in a relational database or a document in a document database.

## K

### Key
A unique identifier for an item in the database. Keys can be simple (partition key only) or composite (partition key + sort key).

## L

### Lexicographic Order
Ordering based on alphabetical/dictionary rules. KeystoneDB encodes keys to ensure correct lexicographic ordering for range queries.

### Little-Endian
A byte ordering where the least significant byte is stored first. KeystoneDB uses little-endian encoding for all integers.

### Local Secondary Index (LSI)
An index with the same partition key as the base table but a different sort key. Enables different sort orders for querying within a partition.

### Log Sequence Number (LSN)
A monotonically increasing number assigned to each WAL record. Used for ordering and recovery.

### Log-Structured Merge Tree (LSM)
A data structure optimized for write-heavy workloads. Writes go to in-memory structures and are periodically flushed to disk. KeystoneDB uses an LSM architecture.

## M

### Magic Number
A constant value at the start of a file that identifies its type. KeystoneDB uses `0x57414C00` ("WAL\0") for WAL files and `0x53535400` ("SST\0") for SST files.

### Memtable
An in-memory buffer for recent writes. Implemented as a sorted BTreeMap. When memtable reaches threshold size, it's flushed to an SST file.

### Merge
The process of combining multiple sorted lists into one. Used during compaction to merge SST files and during queries to merge results from memtable and SST files.

## O

### Optimistic Locking
A concurrency control strategy that assumes conflicts are rare. Operations check conditions before committing. KeystoneDB supports optimistic locking via conditional writes.

## P

### Partition Key (PK)
The primary component of a key, used to determine which stripe stores the data. All items with the same partition key are stored together.

### PartiQL
A SQL-compatible query language for DynamoDB. KeystoneDB implements a subset of PartiQL for querying.

## Q

### Query
An operation that retrieves items from a single partition, optionally filtering by sort key. More efficient than a scan because it operates on a single stripe.

## R

### Range Query
A query that retrieves items where the sort key falls within a range. Example: All posts between dates A and B.

### Read Committed
An isolation level where reads see all committed writes. KeystoneDB's default isolation level.

### Record
A unit of data stored in the database, consisting of a key, value, operation type (Put/Delete), and sequence number.

### Replica
A copy of a database that receives updates from a primary instance. Planned for KeystoneDB Phase 12.

### RwLock (Reader-Writer Lock)
A synchronization primitive that allows multiple concurrent readers OR a single writer. KeystoneDB uses RwLock for the LSM engine.

## S

### Scan
An operation that reads all items in the table (or a filtered subset). Less efficient than queries because it must read all stripes.

### Sequence Number (SeqNo)
A globally unique, monotonically increasing number assigned to each write operation. Used for versioning and determining which version of a key is newest.

### Snapshot Isolation
An isolation level where reads see a consistent point-in-time view of the database. Planned for future KeystoneDB versions.

### Sort Key (SK)
The optional second component of a composite key. Determines the sort order within a partition. Enables range queries.

### Sorted String Table (SST)
An immutable file containing sorted key-value records. Created when memtable is flushed. Supports efficient binary search and range scans.

### Stripe
One of 256 independent LSM sub-trees in KeystoneDB. Each stripe has its own memtable and SST files. Keys route to stripes based on `crc32(pk) % 256`.

### Striping
Distributing data across multiple independent structures for parallelism and load balancing. KeystoneDB uses 256-way striping.

## T

### Tombstone
A special record indicating a key has been deleted. Stored until compaction removes it. Tombstones are necessary because SST files are immutable.

### Transaction
A group of operations that execute atomically. Either all operations succeed or all fail. KeystoneDB supports multi-item transactions.

### TTL (Time To Live)
An expiration timestamp for database items. Items past their TTL are automatically filtered from results. KeystoneDB supports TTL via a configurable attribute.

## V

### Value
The data associated with a key. KeystoneDB supports DynamoDB value types: String (S), Number (N), Binary (B), Boolean, Null, List (L), Map (M), VecF32, and Timestamp (Ts).

### Vector Search
Similarity search on high-dimensional vectors (embeddings). Planned for KeystoneDB Phase 10 using HNSW indexes.

### Version
Each write operation creates a new version of a key. Versions are ordered by sequence number. Newer versions shadow older ones.

## W

### WAL (Write-Ahead Log)
A sequential log file where all writes are recorded before updating in-memory structures. Ensures durability and enables crash recovery.

### WAL Rotation
The process of creating a new WAL file and deleting the old one after memtable flush. Old WAL can be deleted because its data is now in SST files.

### Write Amplification
The ratio of bytes written to disk versus bytes written by the application. LSM trees have write amplification due to compaction. Typical values: 3-10x.

## Acronyms Quick Reference

| Acronym | Full Name |
|---------|-----------|
| ACID | Atomicity, Consistency, Isolation, Durability |
| CRC | Cyclic Redundancy Check |
| CPU | Central Processing Unit |
| ECC | Error-Correcting Code |
| EOF | End of File |
| FPR | False Positive Rate |
| GSI | Global Secondary Index |
| HNSW | Hierarchical Navigable Small World |
| I/O | Input/Output |
| LSI | Local Secondary Index |
| LSM | Log-Structured Merge |
| LSN | Log Sequence Number |
| MVCC | Multi-Version Concurrency Control |
| OCC | Optimistic Concurrency Control |
| OS | Operating System |
| PK | Partition Key |
| PITR | Point-In-Time Recovery |
| REPL | Read-Eval-Print Loop |
| RPC | Remote Procedure Call |
| SeqNo | Sequence Number |
| SK | Sort Key |
| SMART | Self-Monitoring, Analysis and Reporting Technology |
| SST | Sorted String Table |
| TTL | Time To Live |
| UUID | Universally Unique Identifier |
| WAL | Write-Ahead Log |

## Common Patterns

### PK-Only Key
A simple key with only a partition key component. Example: `b"user#123"`.

### Composite Key
A key with both partition key and sort key. Example: `(b"user#123", b"profile")`.

### Key Encoding
The binary format used to store keys. Format: `[pk_len(4) | pk_bytes | sk_len(4) | sk_bytes]`.

### Hot Data
Recently written data that exists in memtable. Reads are very fast (in-memory).

### Cold Data
Older data that exists only in SST files. Reads require disk I/O and binary search.

### Write Path
The sequence of operations for a write: assign SeqNo → WAL append → fsync → memtable insert → check flush threshold.

### Read Path
The sequence of operations for a read: check memtable → check SSTs (newest first) → binary search within SST.

## Units and Notation

### Size Units
- **B** - Byte
- **KB** - Kilobyte (1,024 bytes)
- **MB** - Megabyte (1,024 KB)
- **GB** - Gigabyte (1,024 MB)
- **TB** - Terabyte (1,024 GB)

### Time Units
- **μs** - Microsecond (1/1,000,000 second)
- **ms** - Millisecond (1/1,000 second)
- **s** - Second

### Performance Metrics
- **ops/sec** - Operations per second (throughput)
- **P50** - 50th percentile (median) latency
- **P99** - 99th percentile latency (1% of operations are slower)
- **P99.9** - 99.9th percentile latency

## See Also

- [Architecture Documentation](../ARCHITECTURE.md)
- [Performance Guide](../PERFORMANCE.md)
- [API Reference](https://docs.rs/kstone-api)
- [Error Reference](./appendix-b.md)

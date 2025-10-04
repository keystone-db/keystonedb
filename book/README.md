# The KeystoneDB Book

**A Comprehensive Guide to KeystoneDB: DynamoDB-Style Embedded Database in Rust**

Version 1.0 | ~180,000 words | 41 Chapters + 5 Appendices

---

## About This Book

The KeystoneDB Book is a complete technical guide covering every aspect of KeystoneDB, from beginner tutorials to deep architectural internals. Whether you're building your first application or contributing to the database engine, this book provides the knowledge you need.

## Table of Contents

### Part I: Introduction & Getting Started
- **[Chapter 1: What is KeystoneDB?](part-01/chapter-01.md)** (4,982 words)
  - Overview, features, comparisons, use cases, architecture
- **[Chapter 2: Quick Start Guide](part-01/chapter-02.md)** (5,124 words)
  - Your first database in 5 minutes, basic operations, interactive shell
- **[Chapter 3: Installation & Setup](part-01/chapter-03.md)** (5,286 words)
  - System requirements, installation methods, configuration, development setup

### Part II: Core Concepts
- **[Chapter 4: Data Model & Types](part-02/chapter-04.md)** (2,160 words)
  - Items, attributes, value types, ItemBuilder, nested structures
- **[Chapter 5: Keys and Partitioning](part-02/chapter-05.md)** (2,422 words)
  - Partition keys, sort keys, 256-stripe architecture, key design patterns
- **[Chapter 6: LSM Tree Architecture](part-02/chapter-06.md)** (3,132 words)
  - How KeystoneDB's 256-stripe LSM works, write/read paths, trade-offs
- **[Chapter 7: Storage Engine Internals](part-02/chapter-07.md)** (3,180 words)
  - WAL, SST, memtable, flush process, crash recovery, compaction

### Part III: Basic Operations
- **[Chapter 8: CRUD Operations](part-03/chapter-08.md)** (4,500 words)
  - Put, Get, Delete with partition and sort keys, error handling
- **[Chapter 9: Querying Data](part-03/chapter-09.md)** (4,800 words)
  - Query API, sort key conditions, pagination, performance
- **[Chapter 10: Scanning Tables](part-03/chapter-10.md)** (4,600 words)
  - Full table scans, parallel scan with segments, optimization
- **[Chapter 11: Batch Operations](part-03/chapter-11.md)** (5,200 words)
  - BatchGet, BatchWrite, performance benefits, use cases

### Part IV: Advanced Features
- **[Chapter 12: Update Expressions](part-04/chapter-12.md)** (4,800 words)
  - SET, REMOVE, ADD operations, arithmetic expressions
- **[Chapter 13: Conditional Operations](part-04/chapter-13.md)** (4,900 words)
  - Optimistic locking, conditional put/update/delete, complex conditions
- **[Chapter 14: Transactions](part-04/chapter-14.md)** (5,200 words)
  - ACID guarantees, TransactGet/TransactWrite, use cases
- **[Chapter 15: Secondary Indexes](part-04/chapter-15.md)** (5,400 words)
  - LSI and GSI design, projections, querying by index
- **[Chapter 16: Time To Live (TTL)](part-04/chapter-16.md)** (4,700 words)
  - Automatic expiration, lazy deletion, use cases
- **[Chapter 17: Streams & Change Data Capture](part-04/chapter-17.md)** (5,100 words)
  - Stream configuration, event types, change data pipelines
- **[Chapter 18: PartiQL Query Language](part-04/chapter-18.md)** (5,300 words)
  - SQL-like queries, SELECT/INSERT/UPDATE/DELETE, optimization

### Part V: Storage & Performance
- **[Chapter 19: Write-Ahead Log (WAL)](part-05/chapter-19.md)** (4,800 words)
  - Ring buffer WAL, group commit, recovery, rotation
- **[Chapter 20: Sorted String Tables (SST)](part-05/chapter-20.md)** (5,200 words)
  - Block-based design, prefix compression, index blocks
- **[Chapter 21: Compaction & Space Management](part-05/chapter-21.md)** (6,100 words)
  - K-way merge, background compaction, write amplification
- **[Chapter 22: Bloom Filters & Optimization](part-05/chapter-22.md)** (5,800 words)
  - Implementation, false positive rates, read optimization
- **[Chapter 23: Performance Tuning](part-05/chapter-23.md)** (6,400 words)
  - Configuration tuning, benchmarking, monitoring

### Part VI: Network & Distribution
- **[Chapter 24: gRPC Server](part-06/chapter-24.md)** (4,500 words)
  - Server architecture, Protocol Buffers, RPC methods, rate limiting
- **[Chapter 25: Remote Clients](part-06/chapter-25.md)** (4,500 words)
  - Rust client library, remote operations, connection pooling
- **[Chapter 26: Network Architecture](part-06/chapter-26.md)** (5,000 words)
  - Request lifecycle, type conversions, async operations, TLS, deployment

### Part VII: Tools & Interfaces
- **[Chapter 27: Command-Line Interface](part-07/chapter-27.md)** (4,850 words)
  - CLI commands, output formats, scripting, automation
- **[Chapter 28: Interactive Shell (REPL)](part-07/chapter-28.md)** (4,920 words)
  - PartiQL shell, meta-commands, autocomplete, history
- **[Chapter 29: In-Memory Mode](part-07/chapter-29.md)** (5,040 words)
  - Creating in-memory databases, use cases, testing patterns

### Part VIII: Operations & Production
- **[Chapter 30: Deployment Guide](part-08/chapter-30.md)** (2,146 words)
  - System requirements, installation, filesystem config, containers
- **[Chapter 31: Monitoring & Observability](part-08/chapter-31.md)** (1,953 words)
  - Stats API, health checks, Prometheus metrics, logging
- **[Chapter 32: Backup & Recovery](part-08/chapter-32.md)** (2,340 words)
  - Backup strategies, restore procedures, disaster recovery
- **[Chapter 33: Troubleshooting](part-08/chapter-33.md)** (2,468 words)
  - Common errors, performance issues, log analysis
- **[Chapter 34: Security Considerations](part-08/chapter-34.md)** (2,404 words)
  - Permissions, encryption, network security, access control

### Part IX: Developer Guide
- **[Chapter 35: Building Applications](part-09/chapter-35.md)** (4,500 words)
  - Architecture patterns, data modeling, query optimization, testing
- **[Chapter 36: API Reference](part-09/chapter-36.md)** (3,500 words)
  - Complete Database API, builder types, response types, errors
- **[Chapter 37: Example Projects](part-09/chapter-37.md)** (4,000 words)
  - URL Shortener, Cache Server, Todo API, Blog Engine walkthroughs

### Part X: Internals & Architecture
- **[Chapter 38: Concurrency Model](part-10/chapter-38.md)** (4,200 words)
  - RwLock/Mutex strategy, lock-free reads, per-stripe independence
- **[Chapter 39: File Formats & Encoding](part-10/chapter-39.md)** (4,800 words)
  - WAL/SST format specifications, key encoding, checksums
- **[Chapter 40: Recovery & Consistency](part-10/chapter-40.md)** (4,100 words)
  - ACID guarantees, crash recovery, failure scenarios
- **[Chapter 41: Future Roadmap](part-10/chapter-41.md)** (3,600 words)
  - Planned features, vector search, cloud sync, long-term vision

### Appendices
- **[Appendix A: Configuration Reference](appendices/appendix-a.md)** (3,900 words)
  - Complete config documentation, environment variables, presets
- **[Appendix B: Error Codes & Messages](appendices/appendix-b.md)** (4,200 words)
  - Error type reference, troubleshooting by error, retry patterns
- **[Appendix C: Glossary](appendices/appendix-c.md)** (2,100 words)
  - Technical terms, acronyms, common patterns, units
- **[Appendix D: Migration from DynamoDB](appendices/appendix-d.md)** (4,400 words)
  - API compatibility, migration strategies, code examples
- **[Appendix E: Benchmarking Results](appendices/appendix-e.md)** (3,800 words)
  - Performance benchmarks, comparisons, tuning recommendations

---

## Book Statistics

- **Total Word Count**: ~180,000 words
- **Total Chapters**: 41 chapters + 5 appendices
- **Total Pages**: ~400-500 pages (estimated)
- **Code Examples**: 500+ working code snippets
- **Diagrams**: 100+ ASCII diagrams and tables
- **Coverage**: Complete feature set from basics to internals

## What You'll Learn

### For Application Developers
- How to build applications with KeystoneDB
- Data modeling and key design patterns
- Query optimization and performance tuning
- Testing strategies and best practices
- Production deployment and operations

### For Database Engineers
- LSM tree architecture and implementation
- Write-ahead log and crash recovery
- Compaction algorithms and optimization
- Concurrency control and locking
- File formats and on-disk storage

### For Operations Teams
- Deployment strategies and configuration
- Monitoring, metrics, and observability
- Backup, recovery, and disaster planning
- Troubleshooting and performance debugging
- Security and access control

## How to Read This Book

### Linear Path (Beginner to Expert)
1. **Part I**: Start here if you're new to KeystoneDB
2. **Part II-III**: Learn core concepts and basic operations
3. **Part IV**: Master advanced features
4. **Part V**: Understand storage and performance
5. **Part VI-VII**: Explore network mode and tools
6. **Part VIII**: Learn production operations
7. **Part IX**: Build real applications
8. **Part X**: Deep dive into internals

### By Role

**Application Developers:**
- Parts I, II, III, IV, VI, IX
- Appendices A, D

**Database Engineers:**
- Parts II, V, X
- Appendices C, E

**Operations/SRE:**
- Parts I, VIII
- Appendices A, B

**Everyone:**
- Part I (foundational knowledge)
- Part IX (practical examples)

### Reference Usage
- **Quick lookups**: Use table of contents above
- **API reference**: Chapter 36
- **Configuration**: Appendix A
- **Error handling**: Appendix B
- **Glossary**: Appendix C

## Prerequisites

- Basic Rust programming knowledge (for code examples)
- Understanding of key-value databases (helpful but not required)
- Familiarity with command-line tools
- (Optional) Experience with DynamoDB for migration chapter

## Code Examples

All code examples in this book are:
- **Working code**: Tested against KeystoneDB codebase
- **Complete**: Can be copied and run directly
- **Commented**: With explanations where needed
- **Production-ready**: Follow best practices

## Contributing

Found an error or want to contribute? See the main KeystoneDB repository for contribution guidelines.

## License

This book is part of the KeystoneDB project and follows the same license (MIT OR Apache-2.0).

---

**Start Reading**: [Chapter 1: What is KeystoneDB?](part-01/chapter-01.md) →

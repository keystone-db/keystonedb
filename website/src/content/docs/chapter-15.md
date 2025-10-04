# Chapter 15: Secondary Indexes

Secondary indexes are one of the most powerful features in KeystoneDB, enabling efficient queries on attributes other than the primary key. Inspired by DynamoDB's indexing system, KeystoneDB provides both Local Secondary Indexes (LSI) and Global Secondary Indexes (GSI) to support flexible data access patterns.

## Understanding Primary Keys

Before diving into secondary indexes, let's review primary keys in KeystoneDB:

```rust
// Partition key only
db.put(b"user#123", item)?;

// Partition key + sort key
db.put_with_sk(b"user#123", b"profile", item)?;
```

**Primary key access patterns:**
- ✅ Get specific item by exact key match
- ✅ Query items with same partition key
- ✅ Range queries on sort key within partition
- ❌ Query by non-key attributes (requires full scan)

**Problem:** What if you need to query by email address, status, or creation date?

```rust
// ❌ Cannot do this efficiently:
// "Find user by email = alice@example.com"
// Without indexes, this requires scanning ALL users
```

**Solution:** Secondary indexes!

## Local Secondary Indexes (LSI)

A Local Secondary Index allows you to query items using an **alternative sort key** while maintaining the same partition key as the base table.

### LSI Characteristics

**Key properties:**
- **Same partition key** as base table
- **Different sort key** (uses an attribute value)
- **Co-located data**: Index entries stored in the same partition as base item
- **Automatic maintenance**: Index updates happen automatically with base table writes
- **Consistent reads**: Can perform strongly consistent reads

**LSI Structure:**
```
Base Table:   partition_key = "user#123" + sort_key = "profile"
LSI:          partition_key = "user#123" + sort_key = email_value
```

### Creating a Table with LSI

```rust
use kstone_api::{Database, TableSchema, LocalSecondaryIndex};

// Define schema with LSI
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"));

let db = Database::create_with_schema("mydb.keystone", schema)?;
```

**This creates:**
- A base table with standard partition key + optional sort key
- An LSI named `"email-index"` that uses the `email` attribute as its sort key

### Inserting Items with LSI

When you put an item, KeystoneDB automatically creates index entries:

```rust
let item = ItemBuilder::new()
    .string("name", "Alice")
    .string("email", "alice@example.com")
    .number("age", 30)
    .build();

db.put_with_sk(b"org#acme", b"user#alice", item)?;
```

**What happens internally:**
1. Base table entry created: `org#acme + user#alice`
2. LSI entry created: `org#acme + alice@example.com` (using email value)
3. Both stored in the same partition (stripe)

### Querying by LSI

Use the `.index()` method to query by the LSI sort key:

```rust
use kstone_api::Query;

let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice");

let response = db.query(query)?;

for item in response.items {
    println!("Found user: {:?}", item.get("name"));
}
```

**This query:**
- Searches within partition `org#acme`
- Uses `email-index` to filter by email starting with "alice"
- Returns all matching users efficiently (no full scan)

### LSI with Sort Key Conditions

All sort key conditions work with LSI:

```rust
// Equal
let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_eq(b"alice@example.com");

// Greater than
let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_gt(b"m"); // Emails starting with n-z

// Between
let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_between(b"a", b"m"); // Emails starting with a-m

// Begins with (prefix)
let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice@"); // All alice@* emails
```

### Multiple LSI on Same Table

You can create multiple LSIs for different access patterns:

```rust
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"))
    .add_local_index(LocalSecondaryIndex::new("score-index", "score"))
    .add_local_index(LocalSecondaryIndex::new("created-index", "created_at"));

let db = Database::create_with_schema("mydb.keystone", schema)?;
```

**Now you can query by:**
- Email: `Query::new(pk).index("email-index").sk_begins_with(...)`
- Score: `Query::new(pk).index("score-index").sk_gte(...)`
- Creation date: `Query::new(pk).index("created-index").sk_between(...)`

### LSI Use Case: User Directory

```rust
// Schema with email and lastName indexes
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"))
    .add_local_index(LocalSecondaryIndex::new("lastName-index", "lastName"));

let db = Database::create_with_schema("company.keystone", schema)?;

// Add employees
db.put_with_sk(b"company#acme", b"emp#001",
    ItemBuilder::new()
        .string("firstName", "Alice")
        .string("lastName", "Anderson")
        .string("email", "alice@acme.com")
        .build())?;

db.put_with_sk(b"company#acme", b"emp#002",
    ItemBuilder::new()
        .string("firstName", "Bob")
        .string("lastName", "Brown")
        .string("email", "bob@acme.com")
        .build())?;

// Query by email
let query = Query::new(b"company#acme")
    .index("email-index")
    .sk_begins_with(b"alice");
println!("Found by email: {:?}", db.query(query)?);

// Query by last name
let query = Query::new(b"company#acme")
    .index("lastName-index")
    .sk_between(b"A", b"C"); // Anderson, Brown
println!("Found by last name: {:?}", db.query(query)?);
```

## Global Secondary Indexes (GSI)

A Global Secondary Index allows you to query items using a **completely different partition key** (and optionally a different sort key) from the base table.

### GSI Characteristics

**Key properties:**
- **Different partition key** from base table
- **Optional different sort key**
- **Cross-partition queries**: Can query across all base table partitions
- **Automatic maintenance**: Index updates happen automatically
- **Eventually consistent**: GSI reads may lag slightly behind base table

**GSI Structure:**
```
Base Table:   partition_key = "user#alice"
GSI:          partition_key = status_value + optional_sort_key
```

### Creating a Table with GSI

```rust
use kstone_api::{Database, TableSchema, GlobalSecondaryIndex};

// GSI with partition key only
let schema = TableSchema::new()
    .add_global_index(GlobalSecondaryIndex::new("status-index", "status"));

let db = Database::create_with_schema("mydb.keystone", schema)?;

// GSI with partition key AND sort key
let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::with_sort_key("category-price-index", "category", "price")
    );

let db = Database::create_with_schema("products.keystone", schema)?;
```

### GSI Key Difference from LSI

**LSI Example:**
```rust
// Base table partition key: "org#acme"
// LSI uses: "org#acme" + email_value
// Query: All users in org#acme with email starting with "alice"

let query = Query::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice");
```

**GSI Example:**
```rust
// Base table partition key: "user#alice", "user#bob", etc.
// GSI uses: status_value (e.g., "active") as partition key
// Query: All users with status = "active" (across ALL base partitions)

let query = Query::new(b"active")
    .index("status-index");
```

### Cross-Partition Queries with GSI

GSI enables queries across different base table partitions:

```rust
// Insert items with different base partition keys
db.put(b"user#alice",
    ItemBuilder::new()
        .string("name", "Alice")
        .string("status", "active") // GSI partition key
        .build())?;

db.put(b"user#bob",
    ItemBuilder::new()
        .string("name", "Bob")
        .string("status", "active") // Same GSI partition key
        .build())?;

db.put(b"user#charlie",
    ItemBuilder::new()
        .string("name", "Charlie")
        .string("status", "inactive") // Different GSI partition key
        .build())?;

// Query by status (finds Alice and Bob across different base partitions)
let query = Query::new(b"active")
    .index("status-index");

let response = db.query(query)?;
println!("Found {} active users", response.items.len()); // 2 users
```

**Without GSI, you would need to:**
1. Scan all users (expensive)
2. Filter by status in application code
3. Process potentially millions of records

**With GSI:**
1. Direct query to GSI partition "active"
2. Get only matching items
3. Efficient and fast

### GSI with Sort Key

Create a GSI with both partition and sort keys for complex queries:

```rust
let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::with_sort_key("category-price-index", "category", "price")
    );

let db = Database::create_with_schema("products.keystone", schema)?;

// Insert products
db.put(b"product#widget",
    ItemBuilder::new()
        .string("name", "Widget")
        .string("category", "electronics")
        .number("price", 299)
        .build())?;

db.put(b"product#gadget",
    ItemBuilder::new()
        .string("name", "Gadget")
        .string("category", "electronics")
        .number("price", 499)
        .build())?;

// Query electronics with price between $200-$400
let query = Query::new(b"electronics")
    .index("category-price-index")
    .sk_between(b"200", b"400");

let response = db.query(query)?;
// Returns only Widget (price = 299)
```

### GSI Stripe Distribution

GSI entries route to different stripes based on the GSI partition key:

```rust
// Base table uses user ID for striping
db.put(b"user#alice", ...)?;  // Stripe based on crc32("user#alice")

// GSI entry uses status for striping
// Creates index entry at: stripe based on crc32("active")
```

**Benefits:**
- Load distribution: Different access patterns use different stripes
- Parallelism: Queries on different GSI partitions hit different stripes
- Scalability: GSI spreads across all 256 stripes

### GSI Use Case: Product Catalog

```rust
let schema = TableSchema::new()
    .add_global_index(
        GlobalSecondaryIndex::with_sort_key("category-price-index", "category", "price")
    )
    .add_global_index(
        GlobalSecondaryIndex::new("seller-index", "seller_id")
    );

let db = Database::create_with_schema("marketplace.keystone", schema)?;

// Add products from different sellers
db.put(b"product#001",
    ItemBuilder::new()
        .string("name", "Laptop")
        .string("category", "electronics")
        .number("price", 999)
        .string("seller_id", "seller#alice")
        .build())?;

db.put(b"product#002",
    ItemBuilder::new()
        .string("name", "Mouse")
        .string("category", "electronics")
        .number("price", 29)
        .string("seller_id", "seller#bob")
        .build())?;

// Query 1: Find all electronics under $100
let query = Query::new(b"electronics")
    .index("category-price-index")
    .sk_lte(b"100");

// Query 2: Find all products by seller Alice
let query = Query::new(b"seller#alice")
    .index("seller-index");
```

## Index Projections

Index projections control which attributes are stored in the index. This affects query performance and storage costs.

### Projection Types

#### 1. ALL (Default)

All attributes from the base table are projected into the index:

```rust
let lsi = LocalSecondaryIndex::new("email-index", "email");
// Default: IndexProjection::All

// Query returns complete items
let response = db.query(Query::new(b"org#acme").index("email-index"))?;
for item in response.items {
    // All attributes available
    println!("Name: {:?}", item.get("name"));
    println!("Email: {:?}", item.get("email"));
    println!("Age: {:?}", item.get("age"));
    println!("Address: {:?}", item.get("address"));
}
```

**Pros:**
- No additional fetches needed
- Complete item data available

**Cons:**
- Higher storage cost
- All attributes duplicated in index

#### 2. KEYS_ONLY

Only the partition key, sort key, and index keys are projected:

```rust
let lsi = LocalSecondaryIndex::new("email-index", "email")
    .keys_only();

// Query returns only keys
let response = db.query(Query::new(b"org#acme").index("email-index"))?;
for item in response.items {
    // Only partition key, sort key, and email available
    println!("Email: {:?}", item.get("email")); // Available
    println!("Name: {:?}", item.get("name"));   // None (not projected)
}
```

**Use case:** When you only need identifiers, then fetch full items separately:

```rust
let response = db.query(Query::new(b"org#acme")
    .index("email-index")
    .sk_begins_with(b"alice"))?;

for item in response.items {
    let email = item.get("email").unwrap();
    // Fetch full item if needed
    let full_item = db.get(b"org#acme")?;
}
```

**Pros:**
- Minimal storage cost
- Fast index queries

**Cons:**
- May need additional fetches for full item data

#### 3. INCLUDE (Specific Attributes)

Project only specified attributes:

```rust
let lsi = LocalSecondaryIndex::new("email-index", "email")
    .include(vec![
        "name".to_string(),
        "department".to_string(),
    ]);

// Query returns keys + included attributes
let response = db.query(Query::new(b"org#acme").index("email-index"))?;
for item in response.items {
    println!("Name: {:?}", item.get("name"));         // Available
    println!("Department: {:?}", item.get("department")); // Available
    println!("Age: {:?}", item.get("age"));           // None (not projected)
}
```

**Use case:** Balance between storage cost and fetch efficiency:

```rust
// Frequently accessed attributes in common queries
let gsi = GlobalSecondaryIndex::new("status-index", "status")
    .include(vec![
        "name".to_string(),
        "email".to_string(),
        "last_login".to_string(),
    ]);
```

**Pros:**
- Lower storage than ALL
- Avoid fetches for common attributes

**Cons:**
- Need to choose attributes carefully
- May still need fetches for non-projected attributes

### Choosing a Projection Type

```rust
// Use ALL when:
// - You always need complete items
// - Storage cost is not a concern
let lsi_all = LocalSecondaryIndex::new("email-index", "email");

// Use KEYS_ONLY when:
// - You only need identifiers
// - You'll fetch full items selectively
let lsi_keys = LocalSecondaryIndex::new("id-index", "external_id")
    .keys_only();

// Use INCLUDE when:
// - You need specific attributes frequently
// - Storage is a concern
let lsi_include = LocalSecondaryIndex::new("status-index", "status")
    .include(vec!["name".to_string(), "email".to_string()]);
```

## LSI vs GSI Comparison

| Feature | Local Secondary Index (LSI) | Global Secondary Index (GSI) |
|---------|----------------------------|------------------------------|
| **Partition Key** | Same as base table | Different from base table |
| **Sort Key** | Alternative attribute value | Optional, can be different |
| **Scope** | Within single partition | Across all partitions |
| **Storage** | Same stripe as base item | Different stripe (based on GSI PK) |
| **Use Case** | Alternative sort orders within partition | Cross-partition queries |
| **Example** | Users in org sorted by email | All active users (any org) |

### When to Use LSI

✅ **Use LSI when:**
- You need alternative sort orders within the same partition
- You query by partition key + different sort key
- You want strongly consistent reads
- Data is naturally partitioned (e.g., users within organization)

**Example scenarios:**
- Find users in organization sorted by email
- Get products in category sorted by price
- List posts by author sorted by date
- Find employees in department sorted by hire date

```rust
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"))
    .add_local_index(LocalSecondaryIndex::new("hire-date-index", "hire_date"));

// Query employees in dept sorted by hire date
let query = Query::new(b"dept#engineering")
    .index("hire-date-index")
    .sk_gte(b"2024-01-01");
```

### When to Use GSI

✅ **Use GSI when:**
- You need to query across different partitions
- You want to partition data differently from base table
- You need to support multiple access patterns
- You're querying by non-key attributes

**Example scenarios:**
- Find all active users (across all organizations)
- Get products by category (across all sellers)
- List orders by status (across all customers)
- Find items by tag (across all users)

```rust
let schema = TableSchema::new()
    .add_global_index(GlobalSecondaryIndex::new("status-index", "status"))
    .add_global_index(GlobalSecondaryIndex::with_sort_key("tag-date-index", "tag", "created_at"));

// Query all active items across partitions
let query = Query::new(b"active")
    .index("status-index");

// Query items with tag "urgent" created in last week
let query = Query::new(b"urgent")
    .index("tag-date-index")
    .sk_gte(b"2024-03-01");
```

## Combining LSI and GSI

You can use both LSI and GSI on the same table for maximum flexibility:

```rust
let schema = TableSchema::new()
    // LSI: Query users within organization by different attributes
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"))
    .add_local_index(LocalSecondaryIndex::new("lastName-index", "lastName"))
    // GSI: Query users across organizations by role or status
    .add_global_index(GlobalSecondaryIndex::new("role-index", "role"))
    .add_global_index(GlobalSecondaryIndex::new("status-index", "status"));

let db = Database::create_with_schema("company.keystone", schema)?;

// LSI Query: Users in org#acme sorted by last name
let query = Query::new(b"org#acme")
    .index("lastName-index")
    .sk_begins_with(b"S");

// GSI Query: All admins across all organizations
let query = Query::new(b"admin")
    .index("role-index");
```

## Index Maintenance

KeystoneDB automatically maintains indexes - you don't need to update them manually:

```rust
// Put creates base item + all index entries
db.put(b"user#123",
    ItemBuilder::new()
        .string("name", "Alice")
        .string("email", "alice@example.com")
        .string("status", "active")
        .build())?;

// Automatically creates:
// 1. Base table entry: user#123
// 2. LSI entry: org#acme + alice@example.com (if LSI exists)
// 3. GSI entry: active (if GSI exists)

// Update automatically updates all indexes
db.update(Update::new(b"user#123")
    .expression("SET status = :new_status")
    .value(":new_status", Value::string("inactive")))?;

// Automatically updates:
// 1. Base table entry
// 2. GSI entry: moves from "active" partition to "inactive" partition

// Delete removes base item + all index entries
db.delete(b"user#123")?;
// Automatically removes all index entries
```

## Index Best Practices

### 1. Design Indexes for Query Patterns

```rust
// Bad: Creating indexes you don't use
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("idx1", "attr1"))
    .add_local_index(LocalSecondaryIndex::new("idx2", "attr2"))
    .add_local_index(LocalSecondaryIndex::new("idx3", "attr3"));
// Storage waste if you only query by attr1

// Good: Only create indexes for actual query patterns
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("email-index", "email"));
// Just what you need
```

### 2. Use KEYS_ONLY or INCLUDE for Storage Efficiency

```rust
// Good: Include only frequently accessed attributes
let gsi = GlobalSecondaryIndex::new("status-index", "status")
    .include(vec![
        "name".to_string(),
        "email".to_string(),
    ]);
// Saves storage compared to ALL projection
```

### 3. Choose LSI for Same-Partition Queries

```rust
// Good: LSI for within-partition alternative sort
let schema = TableSchema::new()
    .add_local_index(LocalSecondaryIndex::new("date-index", "created_at"));

let query = Query::new(b"org#acme")
    .index("date-index")
    .sk_gte(b"2024-01-01");
```

### 4. Choose GSI for Cross-Partition Queries

```rust
// Good: GSI for cross-partition queries
let schema = TableSchema::new()
    .add_global_index(GlobalSecondaryIndex::new("category-index", "category"));

let query = Query::new(b"electronics")
    .index("category-index");
// Finds all electronics across all sellers
```

### 5. Use Sparse Indexes for Subset Queries

Not all items need to have the indexed attribute:

```rust
// Only premium users have "premium_expires_at"
// Index will only contain premium users
let schema = TableSchema::new()
    .add_global_index(GlobalSecondaryIndex::new(
        "premium-expiry-index",
        "premium_expires_at"
    ));

// Query only indexes items with the attribute
let query = Query::new(b"2024-12-31")
    .index("premium-expiry-index");
// Returns only premium users expiring on 2024-12-31
```

## Summary

Secondary indexes in KeystoneDB provide:

✅ **LSI**: Alternative sort keys within same partition
✅ **GSI**: Cross-partition queries with different partition key
✅ **Automatic maintenance**: Index updates happen with base table writes
✅ **Flexible projections**: ALL, KEYS_ONLY, or INCLUDE specific attributes
✅ **Multiple indexes**: Create multiple LSI/GSI per table

**LSI characteristics:**
- Same partition key as base table
- Alternative sort key
- Stored in same stripe
- Strongly consistent reads

**GSI characteristics:**
- Different partition key from base table
- Optional sort key
- Cross-partition queries
- Eventually consistent reads

**Projection types:**
- **ALL**: Complete item data (highest storage)
- **KEYS_ONLY**: Only keys (lowest storage)
- **INCLUDE**: Specific attributes (balanced)

**Best practices:**
- Design indexes for actual query patterns
- Use LSI for same-partition alternative sorts
- Use GSI for cross-partition queries
- Choose appropriate projections
- Consider sparse indexes for subset queries

Master secondary indexes to unlock powerful query capabilities in KeystoneDB!

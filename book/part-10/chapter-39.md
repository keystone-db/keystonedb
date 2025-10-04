# Chapter 39: File Formats & Encoding

KeystoneDB's on-disk file formats are designed for simplicity, reliability, and performance. This chapter provides a comprehensive reference to the binary formats used for WAL files, SST files, manifest files, and the key encoding scheme.

## Endianness Convention

**Critical Design Decision:** All multi-byte integers in KeystoneDB use **little-endian** byte order, with one exception: magic numbers use big-endian for human readability.

### Why Little-Endian?

1. **x86/ARM compatibility** - Matches native byte order on most modern CPUs
2. **Performance** - No byte-swapping needed on little-endian architectures
3. **Consistency** - Simplifies implementation and reduces bugs

### Magic Numbers Exception

Magic numbers (file type identifiers) use big-endian so they appear readable in hex dumps:

```
WAL magic: 0x57414C00 → "WAL\0" in hex editor
SST magic: 0x53535400 → "SST\0" in hex editor
```

This helps with debugging and manual file inspection.

## Write-Ahead Log (WAL) Format

### File Structure

The WAL file consists of a fixed-size header followed by a sequence of variable-length records:

```
┌─────────────────────────────────────────┐
│ Header (16 bytes)                       │
├─────────────────────────────────────────┤
│ Record 1                                │
├─────────────────────────────────────────┤
│ Record 2                                │
├─────────────────────────────────────────┤
│ ...                                     │
├─────────────────────────────────────────┤
│ Record N                                │
└─────────────────────────────────────────┘
```

### Header Format (16 bytes)

```
Offset | Size | Type    | Endianness    | Description
-------|------|---------|---------------|---------------------------
0      | 4    | u32     | Big-endian    | Magic (0x57414C00 = "WAL\0")
4      | 4    | u32     | Little-endian | Version (currently 1)
8      | 8    | u64     | Little-endian | Reserved (set to 0)
```

**Rust Serialization:**
```rust
use bytes::{BytesMut, BufMut};

const WAL_MAGIC: u32 = 0x57414C00;

fn write_wal_header(buf: &mut BytesMut) {
    buf.put_u32(WAL_MAGIC);      // big-endian
    buf.put_u32_le(1);           // version (little-endian)
    buf.put_u64_le(0);           // reserved (little-endian)
}
```

**Rust Deserialization:**
```rust
fn read_wal_header(header: &[u8; 16]) -> Result<()> {
    let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    if magic != WAL_MAGIC {
        return Err(Error::Corruption("Invalid WAL magic".into()));
    }

    let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    if version != 1 {
        return Err(Error::Corruption(format!("Unsupported WAL version: {}", version)));
    }

    Ok(())
}
```

### Record Format

Each record in the WAL has the following structure:

```
Offset        | Size     | Type    | Endianness    | Description
--------------|----------|---------|---------------|---------------------------
0             | 8        | u64     | Little-endian | LSN (Log Sequence Number)
8             | 4        | u32     | Little-endian | Data length (bytes)
12            | variable | bytes   | N/A           | Bincode-encoded record data
12 + length   | 4        | u32     | Little-endian | CRC32C checksum of data
```

**Total record size:** 16 bytes (header) + data length + 4 bytes (checksum)

**Rust Serialization:**
```rust
fn write_wal_record(buf: &mut BytesMut, lsn: u64, record: &Record) -> Result<()> {
    // Serialize record to bincode
    let data = bincode::serialize(record)
        .map_err(|e| Error::Internal(format!("Serialize error: {}", e)))?;

    // Compute CRC32C checksum
    let crc = crc32fast::hash(&data);

    // Write record header and data
    buf.put_u64_le(lsn);                    // LSN
    buf.put_u32_le(data.len() as u32);      // length
    buf.put_slice(&data);                   // data
    buf.put_u32_le(crc);                    // checksum

    Ok(())
}
```

**Rust Deserialization:**
```rust
fn read_wal_record(file: &mut File) -> Result<Option<(u64, Record)>> {
    // Read record header
    let mut header = [0u8; 12];
    match file.read_exact(&mut header) {
        Ok(_) => {},
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }

    let lsn = u64::from_le_bytes([
        header[0], header[1], header[2], header[3],
        header[4], header[5], header[6], header[7],
    ]);

    let len = u32::from_le_bytes([
        header[8], header[9], header[10], header[11],
    ]) as usize;

    // Read data and checksum
    let mut data = vec![0u8; len];
    file.read_exact(&mut data)?;

    let mut crc_bytes = [0u8; 4];
    file.read_exact(&mut crc_bytes)?;
    let expected_crc = u32::from_le_bytes(crc_bytes);

    // Verify checksum
    let actual_crc = crc32fast::hash(&data);
    if actual_crc != expected_crc {
        return Err(Error::ChecksumMismatch);
    }

    // Deserialize record
    let record: Record = bincode::deserialize(&data)
        .map_err(|e| Error::Corruption(format!("Deserialize error: {}", e)))?;

    Ok(Some((lsn, record)))
}
```

### WAL Record Data Format

The WAL record data is serialized using bincode encoding of the `Record` struct:

```rust
pub struct Record {
    pub key: Key,              // Partition key + optional sort key
    pub record_type: RecordType,  // Put or Delete
    pub seqno: u64,            // Sequence number
    pub value: Option<Item>,   // None for deletes
}

pub enum RecordType {
    Put,
    Delete,
}

pub type Item = HashMap<String, Value>;
```

Bincode provides a compact, deterministic binary encoding. The exact bytes depend on the record contents but typically:
- Key: ~4-100 bytes (depends on key length)
- RecordType: 1 byte (enum discriminant)
- SeqNo: 8 bytes (u64)
- Value: Variable (depends on item attributes)

## Sorted String Table (SST) Format

### File Structure

SST files contain sorted records with a header, record data, and a trailing checksum:

```
┌─────────────────────────────────────────┐
│ Header (16 bytes)                       │
├─────────────────────────────────────────┤
│ Record 1 (length-prefixed)              │
├─────────────────────────────────────────┤
│ Record 2 (length-prefixed)              │
├─────────────────────────────────────────┤
│ ...                                     │
├─────────────────────────────────────────┤
│ Record N (length-prefixed)              │
├─────────────────────────────────────────┤
│ CRC32C checksum (4 bytes)               │
└─────────────────────────────────────────┘
```

### Header Format (16 bytes)

```
Offset | Size | Type    | Endianness    | Description
-------|------|---------|---------------|---------------------------
0      | 4    | u32     | Big-endian    | Magic (0x53535400 = "SST\0")
4      | 4    | u32     | Little-endian | Version (currently 1)
8      | 4    | u32     | Little-endian | Record count
12     | 4    | u32     | Little-endian | Reserved (set to 0)
```

**Rust Serialization:**
```rust
const SST_MAGIC: u32 = 0x53535400;

fn write_sst_header(buf: &mut BytesMut, record_count: u32) {
    buf.put_u32(SST_MAGIC);         // big-endian
    buf.put_u32_le(1);              // version
    buf.put_u32_le(record_count);   // count
    buf.put_u32_le(0);              // reserved
}
```

**Rust Deserialization:**
```rust
fn read_sst_header(header: &[u8; 16]) -> Result<u32> {
    let magic = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    if magic != SST_MAGIC {
        return Err(Error::Corruption("Invalid SST magic".into()));
    }

    let version = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    if version != 1 {
        return Err(Error::Corruption(format!("Unsupported SST version: {}", version)));
    }

    let count = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    Ok(count)
}
```

### Record Format

Each record in an SST file is length-prefixed:

```
Offset        | Size     | Type    | Endianness    | Description
--------------|----------|---------|---------------|---------------------------
0             | 4        | u32     | Little-endian | Record data length
4             | variable | bytes   | N/A           | Bincode-encoded record
```

Records are sorted by their encoded key (see Key Encoding section below).

**Rust Serialization:**
```rust
fn write_sst_records(buf: &mut BytesMut, records: &[Record]) -> Result<Vec<u8>> {
    let mut data = Vec::new();

    for record in records {
        let rec_data = bincode::serialize(record)
            .map_err(|e| Error::Internal(format!("Serialize error: {}", e)))?;

        // Write length prefix
        data.extend_from_slice(&(rec_data.len() as u32).to_le_bytes());
        // Write data
        data.extend_from_slice(&rec_data);
    }

    buf.put_slice(&data);
    Ok(data)
}
```

**Rust Deserialization:**
```rust
fn read_sst_records(data: &[u8], expected_count: usize) -> Result<Vec<Record>> {
    let mut records = Vec::with_capacity(expected_count);
    let mut offset = 0;

    while offset < data.len() {
        // Read length prefix
        let len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        // Read and deserialize record
        let record: Record = bincode::deserialize(&data[offset..offset + len])
            .map_err(|e| Error::Corruption(format!("Deserialize error: {}", e)))?;
        offset += len;

        records.push(record);
    }

    if records.len() != expected_count {
        return Err(Error::Corruption(format!(
            "Record count mismatch: expected {}, got {}",
            expected_count,
            records.len()
        )));
    }

    Ok(records)
}
```

### SST Checksum

The final 4 bytes of the SST file contain a CRC32C checksum of all record data (excluding the header):

```rust
fn write_sst_checksum(buf: &mut BytesMut, data: &[u8]) {
    let crc = crc32fast::hash(data);
    buf.put_u32_le(crc);
}

fn verify_sst_checksum(data: &[u8], expected_crc: u32) -> Result<()> {
    let actual_crc = crc32fast::hash(data);
    if actual_crc != expected_crc {
        return Err(Error::ChecksumMismatch);
    }
    Ok(())
}
```

### SST File Properties

**Immutability:** Once written, SST files are never modified. This enables:
- Concurrent reads without locking
- Simple caching strategies
- Safe memory-mapping

**Sorting:** Records are sorted by encoded key, enabling:
- Binary search for point lookups
- Efficient range scans
- Merge algorithms for compaction

## Key Encoding Format

Keys in KeystoneDB consist of a partition key and an optional sort key. The encoding must support:
1. Lexicographic ordering (for BTreeMap and binary search)
2. Efficient parsing (to extract partition key for stripe routing)
3. Composite key comparison

### Partition Key Only

For keys without a sort key:

```
┌─────────────────────────────────────────┐
│ PK Length (4 bytes, little-endian)      │
├─────────────────────────────────────────┤
│ Partition Key Bytes (variable length)   │
└─────────────────────────────────────────┘
```

**Example:** Key `b"user#123"`

```
Bytes: [08 00 00 00  75 73 65 72 23 31 32 33]
       └─ length=8  └─ "user#123" (UTF-8)
```

**Rust Encoding:**
```rust
pub fn encode_pk_only(pk: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(4 + pk.len());
    encoded.extend_from_slice(&(pk.len() as u32).to_le_bytes());
    encoded.extend_from_slice(pk);
    encoded
}
```

### Partition Key + Sort Key

For composite keys with both partition and sort keys:

```
┌─────────────────────────────────────────┐
│ PK Length (4 bytes, little-endian)      │
├─────────────────────────────────────────┤
│ Partition Key Bytes (variable length)   │
├─────────────────────────────────────────┤
│ SK Length (4 bytes, little-endian)      │
├─────────────────────────────────────────┤
│ Sort Key Bytes (variable length)        │
└─────────────────────────────────────────┘
```

**Example:** Key `(b"user#123", b"profile")`

```
Bytes: [08 00 00 00  75 73 65 72 23 31 32 33  07 00 00 00  70 72 6F 66 69 6C 65]
       └─ pk_len=8  └─ "user#123"            └─ sk_len=7  └─ "profile"
```

**Rust Encoding:**
```rust
pub fn encode_composite(pk: &[u8], sk: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(8 + pk.len() + sk.len());
    encoded.extend_from_slice(&(pk.len() as u32).to_le_bytes());
    encoded.extend_from_slice(pk);
    encoded.extend_from_slice(&(sk.len() as u32).to_le_bytes());
    encoded.extend_from_slice(sk);
    encoded
}
```

### Key Decoding

To extract the partition key (for stripe routing):

```rust
pub fn decode_partition_key(encoded: &[u8]) -> Result<&[u8]> {
    if encoded.len() < 4 {
        return Err(Error::Corruption("Encoded key too short".into()));
    }

    let pk_len = u32::from_le_bytes([
        encoded[0], encoded[1], encoded[2], encoded[3]
    ]) as usize;

    if encoded.len() < 4 + pk_len {
        return Err(Error::Corruption("Invalid partition key length".into()));
    }

    Ok(&encoded[4..4 + pk_len])
}
```

To extract both keys:

```rust
pub fn decode_key(encoded: &[u8]) -> Result<(&[u8], Option<&[u8]>)> {
    if encoded.len() < 4 {
        return Err(Error::Corruption("Encoded key too short".into()));
    }

    // Extract partition key
    let pk_len = u32::from_le_bytes([
        encoded[0], encoded[1], encoded[2], encoded[3]
    ]) as usize;

    if encoded.len() < 4 + pk_len {
        return Err(Error::Corruption("Invalid partition key length".into()));
    }

    let pk = &encoded[4..4 + pk_len];

    // Check for sort key
    let sk_offset = 4 + pk_len;
    if encoded.len() == sk_offset {
        // No sort key
        return Ok((pk, None));
    }

    if encoded.len() < sk_offset + 4 {
        return Err(Error::Corruption("Incomplete sort key length".into()));
    }

    // Extract sort key
    let sk_len = u32::from_le_bytes([
        encoded[sk_offset],
        encoded[sk_offset + 1],
        encoded[sk_offset + 2],
        encoded[sk_offset + 3],
    ]) as usize;

    if encoded.len() < sk_offset + 4 + sk_len {
        return Err(Error::Corruption("Invalid sort key length".into()));
    }

    let sk = &encoded[sk_offset + 4..sk_offset + 4 + sk_len];
    Ok((pk, Some(sk)))
}
```

### Lexicographic Ordering Properties

The key encoding ensures correct lexicographic ordering:

1. **Keys with same PK, different SK** sort by SK:
   ```
   (b"user#123", b"addr")     < (b"user#123", b"profile")
   ^─ same pk              ^─ sorts by sk
   ```

2. **Keys with different PK** sort by PK:
   ```
   (b"user#100", b"zzz")      < (b"user#200", b"aaa")
   ^─ pk determines order
   ```

3. **PK-only keys come before composite keys with same PK**:
   ```
   b"user#123"                < (b"user#123", b"any_sk")
   ^─ shorter encoding
   ```

This is crucial for range queries and efficient BTreeMap/binary search.

## Value Encoding Format

Values in KeystoneDB items use the DynamoDB-style type system, serialized with bincode:

### Supported Value Types

```rust
pub enum Value {
    S(String),              // String
    N(String),              // Number (stored as string for precision)
    B(Bytes),               // Binary data
    Bool(bool),             // Boolean
    Null,                   // Null
    L(Vec<Value>),          // List (nested values)
    M(HashMap<String, Value>),  // Map (nested object)
    VecF32(Vec<f32>),       // Vector of f32 (embeddings)
    Ts(i64),                // Timestamp (milliseconds since epoch)
}
```

### Bincode Encoding Examples

**String:**
```
Value::S("hello".to_string())

Bytes: [00  05 00 00 00 00 00 00 00  68 65 6C 6C 6F]
       └─ discriminant (0=String)
           └─ length (5, as u64 LE)
                                    └─ "hello" UTF-8
```

**Number:**
```
Value::N("42.5".to_string())

Bytes: [01  04 00 00 00 00 00 00 00  34 32 2E 35]
       └─ discriminant (1=Number)
           └─ length (4)
                                    └─ "42.5" UTF-8
```

**Boolean:**
```
Value::Bool(true)

Bytes: [03  01]
       └─ discriminant (3=Bool)
           └─ value (1=true, 0=false)
```

**List:**
```
Value::L(vec![Value::N("1".into()), Value::N("2".into())])

Bytes: [04  02 00 00 00 00 00 00 00  ...]
       └─ discriminant (4=List)
           └─ element count (2, as u64 LE)
                                    └─ serialized elements
```

## CRC32C Checksum Computation

KeystoneDB uses CRC32C (Castagnoli) for all checksums. This variant:
- Has better error detection properties than CRC32
- Has hardware acceleration on modern CPUs (SSE 4.2)
- Is used by Google (LevelDB, RocksDB) and ScyllaDB

### Computing CRC32C

```rust
use crc32fast;

fn compute_checksum(data: &[u8]) -> u32 {
    crc32fast::hash(data)
}
```

The `crc32fast` crate automatically uses hardware acceleration when available, falling back to software implementation otherwise.

### Verifying CRC32C

```rust
fn verify_checksum(data: &[u8], expected: u32) -> Result<()> {
    let actual = crc32fast::hash(data);
    if actual != expected {
        return Err(Error::ChecksumMismatch);
    }
    Ok(())
}
```

### Checksum Scope

**WAL:** Checksum covers only the record data (not LSN or length fields)

**SST:** Checksum covers all record data (not the header)

This allows detecting:
- Bit flips in data
- Incomplete writes
- File corruption

But does not protect against:
- Header corruption (detected by magic number check)
- Whole-file replacement (would need external verification)

## File Naming Conventions

### WAL Files

Format: `wal.log`

Single WAL file per database directory. Rotated when memtable flushes to SST.

### SST Files

Format: `{stripe:03}-{sst_id}.sst`

Examples:
- `000-1.sst` - Stripe 0, SST ID 1
- `042-15.sst` - Stripe 42, SST ID 15
- `255-999.sst` - Stripe 255, SST ID 999

**Stripe ID:** 3-digit zero-padded (000-255)
**SST ID:** Auto-incrementing global counter (no padding)

### Database Directory Structure

```
mydb.keystone/
├── wal.log              # Write-ahead log
├── 000-1.sst            # SST for stripe 0
├── 000-2.sst            # Another SST for stripe 0
├── 001-1.sst            # SST for stripe 1
├── 042-1.sst            # SST for stripe 42
├── 042-2.sst
├── ...
└── 255-1.sst            # SST for stripe 255
```

## Corruption Detection

KeystoneDB uses multiple layers of corruption detection:

### Layer 1: Magic Numbers

Detect file type mismatches:
```rust
if magic != EXPECTED_MAGIC {
    return Err(Error::Corruption("Wrong file type"));
}
```

### Layer 2: Version Numbers

Detect version incompatibilities:
```rust
if version != SUPPORTED_VERSION {
    return Err(Error::Corruption(format!("Unsupported version: {}", version)));
}
```

### Layer 3: CRC32C Checksums

Detect data corruption:
```rust
let actual = crc32fast::hash(data);
if actual != expected {
    return Err(Error::ChecksumMismatch);
}
```

### Layer 4: Length Validation

Detect truncation:
```rust
if records.len() != expected_count {
    return Err(Error::Corruption("Record count mismatch"));
}
```

### Layer 5: Bincode Deserialization

Detect invalid encodings:
```rust
let record: Record = bincode::deserialize(data)
    .map_err(|e| Error::Corruption(format!("Invalid encoding: {}", e)))?;
```

## Backward Compatibility

### Version Evolution Strategy

Current version: 1 (for both WAL and SST)

Future versions will maintain backward compatibility:

**Reading older versions:**
```rust
match version {
    1 => read_v1_format(data)?,
    2 => read_v2_format(data)?,  // Future
    _ => return Err(Error::Corruption(format!("Unsupported version: {}", version))),
}
```

**Writing new versions:**
```rust
// Always write latest version
const CURRENT_VERSION: u32 = 2;  // Future

// But support reading older versions
const MIN_SUPPORTED_VERSION: u32 = 1;
```

### Format Migration

When introducing breaking changes:

1. Implement reader for old format
2. Implement writer for new format
3. Provide migration tool:
   ```bash
   kstone migrate --from-version 1 --to-version 2 mydb.keystone
   ```

4. Support reading both formats for transition period
5. Eventually deprecate old format support

## Performance Considerations

### Sequential Access

File formats optimized for sequential access:
- WAL: Append-only (sequential writes)
- SST: Written once, read sequentially during compaction

### Binary Search

SST records sorted by key enable O(log n) binary search:
```rust
let pos = sst.records.binary_search_by(|record| {
    let record_key = record.key.encode();
    record_key.cmp(&target_key)
});
```

### Memory Mapping

SST files can be memory-mapped for zero-copy reads:
```rust
let mmap = unsafe { MmapOptions::new().map(&file)? };
let data = &mmap[header_size..mmap.len() - 4];  // Skip header and checksum
```

### Alignment

No special alignment requirements. File formats use naturally aligned types (4-byte and 8-byte boundaries) for efficiency but don't require page alignment.

## Summary

KeystoneDB's file formats prioritize:

1. **Simplicity** - Easy to implement and debug
2. **Reliability** - Multiple layers of corruption detection
3. **Performance** - Optimized for sequential access and binary search
4. **Compatibility** - Stable formats with version numbers for evolution

Key design decisions:
- Little-endian for CPU efficiency
- Big-endian magic numbers for debugging
- CRC32C checksums for corruption detection
- Length-prefixed encoding for variable-length data
- Sorted records for efficient lookups
- Immutable SST files for concurrency

The formats are production-ready and provide a solid foundation for future enhancements like compression, encryption, and columnar encoding.

# Chapter 3: Installation & Setup

## Introduction

This chapter provides comprehensive guidance on installing KeystoneDB in various environments, from development laptops to production servers. We'll cover building from source, binary installation, development setup, configuration options, and verification procedures.

By the end of this chapter, you'll have a production-ready KeystoneDB installation tailored to your needs.

## System Requirements

### Minimum Requirements

- **Operating System**: Linux, macOS, or Windows
- **Architecture**: x86_64 (AMD64) or ARM64 (Apple Silicon supported)
- **RAM**: 512 MB minimum, 2 GB recommended
- **Disk Space**: 50 MB for binaries, additional space for data
- **Rust**: 1.70 or later (for building from source)

### Recommended Specifications

For production workloads:

- **CPU**: 2+ cores (benefits from multi-core for parallel operations)
- **RAM**: 4-8 GB (allows larger in-memory memtables)
- **Disk**: SSD strongly recommended (LSM tree benefits from fast sequential writes)
- **OS**: Linux with ext4 or XFS filesystem

### Platform Support

KeystoneDB is written in Rust and runs on any platform supported by the Rust toolchain:

| Platform | Status | Notes |
|----------|--------|-------|
| **Linux (x86_64)** | ✅ Fully Supported | Primary development platform |
| **Linux (ARM64)** | ✅ Fully Supported | Tested on Raspberry Pi 4 |
| **macOS (Intel)** | ✅ Fully Supported | macOS 10.15+ |
| **macOS (Apple Silicon)** | ✅ Fully Supported | M1/M2/M3 Macs |
| **Windows (x86_64)** | ✅ Fully Supported | Windows 10+ |
| **FreeBSD** | ⚠️ Experimental | Community-tested |

## Installation Methods

### Method 1: Building from Source (Recommended)

Building from source gives you the latest features, optimizations for your CPU, and full control over the build process.

#### Step 1: Install Rust

If you don't have Rust installed:

**Linux and macOS**:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

**Windows**:
Download and run [rustup-init.exe](https://rustup.rs/)

Verify installation:
```bash
rustc --version
cargo --version
```

Expected output (versions may vary):
```
rustc 1.75.0 (82e1608df 2023-12-21)
cargo 1.75.0 (1d8b05cdd 2023-11-20)
```

#### Step 2: Install Build Dependencies

**Linux (Debian/Ubuntu)**:
```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev
```

**Linux (Fedora/RHEL/CentOS)**:
```bash
sudo dnf install -y gcc openssl-devel pkg-config
```

**macOS**:
```bash
# Xcode Command Line Tools
xcode-select --install

# Homebrew (optional, for additional tools)
brew install openssl pkg-config
```

**Windows**:
- Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/) with C++ support
- Or install [MSYS2](https://www.msys2.org/) for a Unix-like environment

#### Step 3: Clone and Build

```bash
# Clone the repository
git clone https://github.com/yourusername/keystonedb.git
cd keystonedb

# Build in release mode (optimized)
cargo build --release

# This creates:
# - target/release/kstone (CLI binary)
# - target/release/kstone-server (gRPC server binary)
# - target/release/libkstone_*.{a,so,dylib} (library files)
```

Build time varies by system:
- **Fast machine** (modern CPU, SSD): 2-3 minutes
- **Slow machine** (older CPU, HDD): 5-10 minutes

#### Step 4: Install Binaries

**Option A: System-wide installation**

```bash
# Copy to /usr/local/bin (requires sudo)
sudo cp target/release/kstone /usr/local/bin/
sudo cp target/release/kstone-server /usr/local/bin/

# Verify
kstone --version
```

**Option B: User-local installation**

```bash
# Copy to ~/.local/bin (no sudo required)
mkdir -p ~/.local/bin
cp target/release/kstone ~/.local/bin/
cp target/release/kstone-server ~/.local/bin/

# Add to PATH if not already there
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc

# Verify
kstone --version
```

**Option C: Create alias**

```bash
# Add to ~/.bashrc or ~/.zshrc
echo 'alias kstone="~/keystonedb/target/release/kstone"' >> ~/.bashrc
source ~/.bashrc
```

#### Step 5: Build with CPU-Specific Optimizations

For maximum performance, enable CPU-specific optimizations:

```bash
# Intel/AMD CPUs with AVX2
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Apple Silicon (M1/M2/M3)
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Specific CPU features
RUSTFLAGS="-C target-feature=+sse4.2,+avx2" cargo build --release
```

This can improve performance by 10-20% by enabling SIMD instructions for CRC32C checksums and other operations.

### Method 2: Binary Installation (Coming Soon)

Pre-built binaries will be available for download:

```bash
# Download latest release
curl -L https://github.com/yourusername/keystonedb/releases/latest/download/kstone-linux-x86_64.tar.gz | tar xz

# Move to PATH
sudo mv kstone /usr/local/bin/
sudo mv kstone-server /usr/local/bin/
```

Check the [releases page](https://github.com/yourusername/keystonedb/releases) for available versions.

### Method 3: Using Cargo Install (Coming Soon)

Once published to crates.io:

```bash
cargo install kstone-cli
cargo install kstone-server
```

This downloads, builds, and installs the latest version from the Rust package registry.

### Method 4: Docker Installation (Coming Soon)

Run KeystoneDB in a container:

```bash
# Pull the latest image
docker pull keystonedb/keystone:latest

# Run CLI
docker run --rm -v $(pwd)/data:/data keystonedb/keystone kstone create /data/mydb.keystone

# Run server
docker run -d \
  -p 50051:50051 \
  -v $(pwd)/data:/data \
  keystonedb/keystone \
  kstone-server --db-path /data/mydb.keystone
```

## Development Setup

For developers working on KeystoneDB or applications using the library:

### Setting Up a Development Environment

#### Step 1: Clone and Configure

```bash
git clone https://github.com/yourusername/keystonedb.git
cd keystonedb

# Use stable Rust toolchain
rustup default stable

# Add clippy and rustfmt for code quality
rustup component add clippy rustfmt
```

#### Step 2: Build in Debug Mode

```bash
# Debug build (faster compilation, includes debug symbols)
cargo build

# Run tests
cargo test

# Run specific test
cargo test -p kstone-core test_lsm_put_get
```

#### Step 3: Set Up Your IDE

**Visual Studio Code**:

Install extensions:
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) - Language server
- [CodeLLDB](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb) - Debugger
- [crates](https://marketplace.visualstudio.com/items?itemName=serayuzgur.crates) - Dependency management

Create `.vscode/settings.json`:
```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.cargo.features": "all",
  "editor.formatOnSave": true
}
```

**IntelliJ IDEA / CLion**:

Install the Rust plugin and open the project. CLion provides excellent debugging support.

**Vim/Neovim**:

Use [rust.vim](https://github.com/rust-lang/rust.vim) or [nvim-lspconfig](https://github.com/neovim/nvim-lspconfig) with rust-analyzer.

#### Step 4: Understanding the Workspace Structure

KeystoneDB is organized as a Cargo workspace:

```
keystonedb/
├── kstone-core/        # Storage engine (LSM, WAL, SST)
│   ├── src/
│   │   ├── lsm.rs      # LSM engine with 256 stripes
│   │   ├── wal.rs      # Write-ahead log
│   │   ├── sst.rs      # Sorted string tables
│   │   ├── bloom.rs    # Bloom filters
│   │   └── ...
│   └── Cargo.toml
├── kstone-api/         # Public API layer
│   ├── src/
│   │   ├── lib.rs      # Database operations
│   │   ├── query.rs    # Query builder
│   │   ├── batch.rs    # Batch operations
│   │   └── ...
│   └── Cargo.toml
├── kstone-proto/       # Protocol Buffers definitions
├── kstone-server/      # gRPC server implementation
├── kstone-client/      # gRPC client library
├── kstone-cli/         # Command-line interface
├── kstone-tests/       # Integration tests
├── examples/           # Example applications
└── Cargo.toml          # Workspace manifest
```

#### Step 5: Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run tests for specific crate
cargo test -p kstone-core
cargo test -p kstone-api

# Run integration tests only
cargo test -p kstone-tests

# Run with release optimizations
cargo test --release
```

#### Step 6: Benchmarking

```bash
# Run benchmarks
cargo bench -p kstone-tests

# Benchmark specific operation
cargo bench -p kstone-tests --bench write_throughput
```

Benchmark results are saved to `target/criterion/` as HTML reports.

### Using KeystoneDB as a Library

To use KeystoneDB in your Rust project:

#### Step 1: Add Dependencies

**Option A: Local path dependency** (for development):

```toml
[dependencies]
kstone-api = { path = "../keystonedb/kstone-api" }
kstone-core = { path = "../keystonedb/kstone-core" }
```

**Option B: Git dependency** (before crates.io publish):

```toml
[dependencies]
kstone-api = { git = "https://github.com/yourusername/keystonedb", branch = "main" }
kstone-core = { git = "https://github.com/yourusername/keystonedb", branch = "main" }
```

**Option C: Crates.io** (once published):

```toml
[dependencies]
kstone-api = "0.1"
kstone-core = "0.1"
```

#### Step 2: Basic Usage

```rust
use kstone_api::{Database, ItemBuilder, Query};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create or open database
    let db = Database::create("myapp.keystone")?;

    // Insert item
    let item = ItemBuilder::new()
        .string("name", "Alice")
        .number("age", 30)
        .build();

    db.put(b"user#alice", item)?;

    // Query
    let query = Query::new(b"user#alice");
    let response = db.query(query)?;

    println!("Found {} items", response.items.len());

    Ok(())
}
```

#### Step 3: Example Projects

Study the example applications in the `examples/` directory:

```bash
# URL shortener (simple key-value)
cd examples/url-shortener
cargo run

# Cache server (TTL, expiration)
cd examples/cache-server
cargo run

# Todo API (CRUD, queries)
cd examples/todo-api
cargo run

# Blog engine (indexes, pagination)
cd examples/blog-engine
cargo run
```

## Configuration Basics

### Database Configuration

KeystoneDB supports configuration at database creation time:

```rust
use kstone_api::{Database, DatabaseConfig};

let config = DatabaseConfig {
    // Memtable flush threshold (records per stripe)
    memtable_threshold: 5000,

    // Compaction trigger (SST files per stripe)
    compaction_threshold: 4,

    // Maximum concurrent compactions
    max_concurrent_compactions: 4,

    // Enable/disable compaction
    compaction_enabled: true,

    ..Default::default()
};

let db = Database::create_with_config("mydb.keystone", config)?;
```

### Environment Variables

Control runtime behavior with environment variables:

```bash
# Logging level (error, warn, info, debug, trace)
export RUST_LOG=kstone_core=info,kstone_api=debug

# Increase stack size for deeply nested data
export RUST_MIN_STACK=8388608  # 8 MB

# Rust backtrace on panic
export RUST_BACKTRACE=1
```

### Filesystem Considerations

**Linux**:
- **ext4**: Excellent general-purpose filesystem, recommended
- **XFS**: Better for large files and high concurrency
- **btrfs**: Snapshot support, but may have performance overhead

```bash
# Check filesystem type
df -T mydb.keystone
```

**macOS**:
- **APFS**: Default and recommended for modern macOS
- **HFS+**: Older filesystem, slower metadata operations

**Windows**:
- **NTFS**: Default and supported
- Avoid network drives (SMB/CIFS) for best performance

### Performance Tuning

#### Adjust Memtable Size

Larger memtables reduce write amplification but use more memory:

```rust
let config = DatabaseConfig {
    memtable_threshold: 10000,  // Default: 1000
    ..Default::default()
};
```

**Trade-offs**:
- **Larger**: Fewer SST files, less compaction, more memory
- **Smaller**: More frequent flushes, more SST files, less memory

#### Tune Compaction

Control when and how compaction runs:

```rust
let config = DatabaseConfig {
    compaction_threshold: 10,   // Default: 10 SST files
    max_concurrent_compactions: 8,  // Default: 4
    ..Default::default()
};
```

#### Disable Compaction (Testing Only)

For testing or write-heavy workloads:

```rust
let config = DatabaseConfig {
    compaction_enabled: false,
    ..Default::default()
};
```

**Warning**: Without compaction, disk usage grows unbounded and read performance degrades.

## Verifying Installation

### Basic Verification

```bash
# Check CLI version
kstone --version

# Check server version
kstone-server --version

# Create test database
kstone create test.keystone

# Insert item
kstone put test.keystone test#1 '{"value": "hello"}'

# Get item
kstone get test.keystone test#1

# Clean up
rm -rf test.keystone
```

Expected output:
```
Database created: test.keystone
Item inserted successfully
{"value":"hello"}
```

### Running Test Suite

Verify everything works correctly:

```bash
cd keystonedb

# Run all tests
cargo test

# Expected output:
# test result: ok. 193 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Performance Baseline

Run a simple benchmark:

```bash
# Create benchmark script
cat > bench.sh << 'EOF'
#!/bin/bash
DB="bench.keystone"
rm -rf $DB
kstone create $DB

echo "Writing 10,000 items..."
time for i in {1..10000}; do
  kstone put $DB "key#$i" "{\"value\": $i}" > /dev/null
done

echo "Reading 1,000 items..."
time for i in {1..1000}; do
  kstone get $DB "key#$i" > /dev/null
done

rm -rf $DB
EOF

chmod +x bench.sh
./bench.sh
```

Expected results (on modern hardware):
- **Writes**: 5,000-15,000 ops/sec
- **Reads**: 50,000+ ops/sec

## Server Setup (gRPC)

### Starting the Server

```bash
# Start server on default port (50051)
kstone-server --db-path mydb.keystone

# Start on custom port
kstone-server --db-path mydb.keystone --port 8080

# Bind to all interfaces (not just localhost)
kstone-server --db-path mydb.keystone --host 0.0.0.0 --port 50051
```

Server output:
```
KeystoneDB Server v0.1.0
Listening on http://127.0.0.1:50051
Database: mydb.keystone
Press Ctrl+C to stop
```

### Client Connection

```rust
use kstone_client::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to server
    let mut client = Client::connect("http://localhost:50051").await?;

    // Use like local database
    let item = HashMap::new();
    client.put(b"key#1", item).await?;

    let result = client.get(b"key#1").await?;
    println!("Retrieved: {:?}", result);

    Ok(())
}
```

### Systemd Service (Linux)

Create `/etc/systemd/system/kstone-server.service`:

```ini
[Unit]
Description=KeystoneDB Server
After=network.target

[Service]
Type=simple
User=kstone
Group=kstone
WorkingDirectory=/var/lib/kstone
ExecStart=/usr/local/bin/kstone-server --db-path /var/lib/kstone/production.keystone --port 50051
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
# Create user
sudo useradd -r -s /bin/false kstone
sudo mkdir -p /var/lib/kstone
sudo chown kstone:kstone /var/lib/kstone

# Enable service
sudo systemctl daemon-reload
sudo systemctl enable kstone-server
sudo systemctl start kstone-server

# Check status
sudo systemctl status kstone-server

# View logs
sudo journalctl -u kstone-server -f
```

### Docker Compose (Coming Soon)

```yaml
version: '3.8'

services:
  keystone:
    image: keystonedb/keystone:latest
    command: kstone-server --db-path /data/production.keystone --host 0.0.0.0
    ports:
      - "50051:50051"
    volumes:
      - ./data:/data
    restart: always
```

Start with:
```bash
docker-compose up -d
```

## Common Installation Issues

### Issue: "command not found: kstone"

**Cause**: Binary not in PATH

**Solution**:
```bash
# Find the binary
which kstone
# If empty, add to PATH
export PATH="$HOME/.local/bin:$PATH"

# Or use full path
~/keystonedb/target/release/kstone --help
```

### Issue: "Permission denied" when creating database

**Cause**: Insufficient permissions in target directory

**Solution**:
```bash
# Create database in home directory
kstone create ~/mydb.keystone

# Or fix permissions
sudo chown -R $USER:$USER /path/to/directory
```

### Issue: "error: linking with cc failed"

**Cause**: Missing C compiler or linker

**Solution**:
```bash
# Linux
sudo apt-get install build-essential

# macOS
xcode-select --install
```

### Issue: Slow compilation

**Cause**: Debug build or slow hardware

**Solution**:
```bash
# Use release mode
cargo build --release

# Enable parallel compilation
export CARGO_BUILD_JOBS=8

# Use mold linker (Linux, much faster)
cargo install mold
export RUSTFLAGS="-C link-arg=-fuse-ld=mold"
cargo build --release
```

### Issue: Out of memory during compilation

**Cause**: Large project, insufficient RAM

**Solution**:
```bash
# Reduce parallel compilation
export CARGO_BUILD_JOBS=1

# Or add swap space (Linux)
sudo fallocate -l 4G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
```

## Security Considerations

### File Permissions

Ensure database files are properly protected:

```bash
# Create database with restricted permissions
kstone create mydb.keystone
chmod 700 mydb.keystone
chmod 600 mydb.keystone/*
```

### Server Security

When running the gRPC server:

```bash
# Bind to localhost only (default)
kstone-server --db-path mydb.keystone --host 127.0.0.1

# For production, use firewall rules
sudo ufw allow from 192.168.1.0/24 to any port 50051

# Consider TLS encryption (future feature)
# kstone-server --db-path mydb.keystone --tls-cert cert.pem --tls-key key.pem
```

### Data Encryption

KeystoneDB supports block-level encryption (optional):

```rust
use kstone_api::{Database, DatabaseConfig};

let config = DatabaseConfig {
    encryption_key: Some([42u8; 32]),  // AES-256 key
    ..Default::default()
};

let db = Database::create_with_config("encrypted.keystone", config)?;
```

**Important**: Store encryption keys securely, not in code.

## Upgrading

### Upgrading the Binary

```bash
# Pull latest code
cd keystonedb
git pull

# Rebuild
cargo build --release

# Replace binary
sudo cp target/release/kstone /usr/local/bin/
```

### Database Migration

KeystoneDB maintains backward compatibility. Databases created with older versions can be opened with newer versions.

To verify compatibility:
```bash
kstone get mydb.keystone --version
```

## Next Steps

You now have a fully functional KeystoneDB installation. Here's what to explore next:

1. **Part II: Core Concepts** - Deep dive into data modeling, queries, and indexes
2. **Part III: Advanced Features** - Transactions, streams, PartiQL, and performance tuning
3. **Part IV: Production Deployment** - Monitoring, backup, disaster recovery
4. **Examples** - Study real-world applications in the `examples/` directory

### Recommended Reading Order

For application developers:
1. Chapter 4: Data Modeling
2. Chapter 5: Querying Data
3. Chapter 6: Secondary Indexes

For database administrators:
1. Chapter 10: Performance Tuning
2. Chapter 11: Backup and Recovery
3. Chapter 12: Monitoring and Observability

For system architects:
1. Chapter 13: Architecture Deep Dive
2. Chapter 14: Scaling Strategies
3. Chapter 15: Integration Patterns

Happy building with KeystoneDB!

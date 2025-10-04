# Chapter 30: Deployment Guide

Deploying KeystoneDB in production requires careful consideration of system requirements, filesystem configuration, and operational best practices. This chapter provides a comprehensive guide to deploying KeystoneDB in both embedded and server modes, from initial installation through production hardening.

## System Requirements

### Minimum Requirements

For development and testing environments, KeystoneDB can run on modest hardware:

- **Operating System**: Linux (x86_64), macOS (Intel/Apple Silicon), or Windows 10+
- **CPU**: 2 cores (x86_64 architecture)
- **Memory**: 2 GB RAM
- **Disk**: 10 GB available space
- **Filesystem**: ext4, XFS, APFS, or NTFS

These minimum requirements are suitable for:
- Development environments
- Testing and CI/CD pipelines
- Small-scale embedded applications (< 1M records)
- Proof-of-concept deployments

### Recommended Production Requirements

For production deployments, KeystoneDB benefits significantly from better hardware:

- **Operating System**: Linux (Ubuntu 20.04+, RHEL 8+, Debian 11+, or similar)
- **CPU**: 4-8 cores (more cores improve parallel scan and compaction performance)
- **Memory**: 8-16 GB RAM minimum, 32 GB+ recommended
- **Disk**: NVMe SSD with 100+ GB available space
- **Filesystem**: ext4 or XFS with `noatime` mount option
- **Network**: 1 Gbps+ for server deployments

### Hardware Performance Impact

**CPU Impact:**

The 256-stripe architecture of KeystoneDB is designed to leverage multiple CPU cores. Each stripe can be flushed and compacted independently, allowing parallel operations across stripes.

- **2 cores**: Adequate for basic operations, limited parallel compaction
- **4 cores**: Good balance for most workloads, enables concurrent compaction
- **8+ cores**: Excellent for write-heavy workloads with frequent compaction

**Memory Impact:**

KeystoneDB uses memory for several critical components:

- **Memtable**: ~1 MB per stripe (256 MB total for all 256 stripes at max capacity)
- **Bloom Filters**: ~1% of SST size (10 bits per key at 1% FPR)
- **OS Page Cache**: Benefits from additional RAM for SST file caching
- **Compaction Buffers**: Temporary memory during compaction operations

Memory recommendations by workload:
- **Read-heavy**: 8 GB+ (benefits from larger page cache)
- **Write-heavy**: 16 GB+ (larger memtables, concurrent compaction)
- **Mixed workload**: 12 GB+ (balanced allocation)

**Disk Type Impact:**

Disk I/O is the primary bottleneck for database operations. KeystoneDB's performance varies significantly based on storage type:

| Storage Type | Sequential Read | Random Read | Sequential Write | Use Case |
|--------------|----------------|-------------|------------------|----------|
| NVMe SSD | 3-7 GB/s | 500k-1M IOPS | 2-3 GB/s | Production (recommended) |
| SATA SSD | 500 MB/s | 50k-100k IOPS | 500 MB/s | Production (acceptable) |
| HDD | 150 MB/s | 100-200 IOPS | 150 MB/s | Development only |

**Performance characteristics:**
- **NVMe SSD**: Sub-millisecond latency, ideal for production workloads
- **SATA SSD**: Low-millisecond latency, acceptable for most workloads
- **HDD**: Not recommended for production; high latency (10-20ms) causes poor performance

## Installation Methods

### Installing from Source

Building from source provides the latest features and allows customization:

```bash
# Install Rust toolchain (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Clone the repository
git clone https://github.com/yourusername/keystonedb.git
cd keystonedb

# Build release binaries (optimized)
cargo build --release

# Binaries are now available in target/release/
ls -lh target/release/kstone*

# Install to system path (optional)
sudo install -m 755 target/release/kstone /usr/local/bin/
sudo install -m 755 target/release/kstone-server /usr/local/bin/

# Verify installation
kstone --version
kstone-server --version
```

**Build options:**

```bash
# Build with all optimizations (recommended for production)
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Build specific components only
cargo build --release --bin kstone        # CLI only
cargo build --release --bin kstone-server # Server only

# Build with debug symbols (for profiling)
cargo build --release --profile=release-with-debug
```

### Installing from Binary Distribution

For production deployments, pre-built binaries provide a faster installation path:

```bash
# Download latest release (Linux x86_64)
VERSION="0.4.0"
curl -LO https://github.com/yourusername/keystonedb/releases/download/v${VERSION}/keystonedb-${VERSION}-linux-x86_64.tar.gz

# Verify checksum
sha256sum -c keystonedb-${VERSION}-linux-x86_64.tar.gz.sha256

# Extract binaries
tar xzf keystonedb-${VERSION}-linux-x86_64.tar.gz

# Install to system path
sudo install -m 755 kstone /usr/local/bin/
sudo install -m 755 kstone-server /usr/local/bin/

# Verify installation
kstone --version
```

### Creating a Dedicated User

For security, run KeystoneDB under a dedicated non-root user:

```bash
# Create keystonedb user (no login shell)
sudo useradd -r -s /bin/false -d /var/lib/keystonedb keystonedb

# Create data directory
sudo mkdir -p /var/lib/keystonedb/data
sudo chown -R keystonedb:keystonedb /var/lib/keystonedb

# Create log directory
sudo mkdir -p /var/log/keystonedb
sudo chown keystonedb:keystonedb /var/log/keystonedb
```

## Filesystem Configuration

Proper filesystem configuration is critical for optimal performance and reliability.

### Filesystem Selection

**ext4** (Recommended for most deployments):
- Mature, well-tested filesystem
- Good balance of performance and reliability
- Wide compatibility across Linux distributions
- Supports large files and volumes

**XFS** (Recommended for large datasets):
- Excellent performance for large files
- Better scaling for multi-TB databases
- Superior parallel I/O performance
- Preferred for high-throughput workloads

**Avoid:**
- **Btrfs**: Copy-on-write overhead can impact performance
- **ZFS**: Additional complexity, potential licensing issues
- **NFS**: Network latency impacts performance; only for specific use cases

### Mount Options

Mount the database filesystem with optimized options:

```bash
# Add to /etc/fstab
/dev/nvme0n1  /var/lib/keystonedb  ext4  noatime,nodiratime,data=ordered,barrier=1  0  2

# For XFS
/dev/nvme0n1  /var/lib/keystonedb  xfs   noatime,nodiratime,logbufs=8,logbsize=256k  0  2
```

**Key mount options explained:**

- **noatime**: Don't update access times (reduces write I/O by 10-30%)
- **nodiratime**: Don't update directory access times
- **data=ordered** (ext4): Ensure metadata consistency
- **barrier=1** (ext4): Enable write barriers for crash safety
- **logbufs/logbsize** (XFS): Larger transaction log for better performance

**Apply mount options without reboot:**

```bash
sudo mount -o remount,noatime,nodiratime /var/lib/keystonedb
```

### File Descriptor Limits

KeystoneDB opens multiple file descriptors (WAL files, SST files, network sockets). Increase system limits:

```bash
# Check current limits
ulimit -n

# Temporary increase (current session)
ulimit -n 65536

# Permanent increase (all users)
sudo tee -a /etc/security/limits.conf <<EOF
*  soft  nofile  65536
*  hard  nofile  65536
EOF

# Permanent increase (specific user)
sudo tee -a /etc/security/limits.conf <<EOF
keystonedb  soft  nofile  65536
keystonedb  hard  nofile  65536
EOF

# For systemd services, set in service file
[Service]
LimitNOFILE=65536
```

**Why 65536?**
- 256 stripes × ~100 SST files per stripe = ~25,600 files
- Additional headroom for WAL, connections, and temporary files

### Disk I/O Scheduler

Configure the I/O scheduler for optimal SSD performance:

```bash
# Check current scheduler
cat /sys/block/nvme0n1/queue/scheduler

# Set to none (for NVMe) or mq-deadline (for SATA SSD)
echo none | sudo tee /sys/block/nvme0n1/queue/scheduler

# For SATA SSD
echo mq-deadline | sudo tee /sys/block/sda/queue/scheduler

# Make persistent (add to /etc/rc.local or udev rules)
sudo tee /etc/udev/rules.d/60-scheduler.rules <<EOF
ACTION=="add|change", KERNEL=="nvme[0-9]n[0-9]", ATTR{queue/scheduler}="none"
ACTION=="add|change", KERNEL=="sd[a-z]", ATTR{queue/scheduler}="mq-deadline"
EOF
```

## Production Configuration

### Database Creation

Create production databases with proper ownership and permissions:

```bash
# As keystonedb user
sudo -u keystonedb kstone create /var/lib/keystonedb/data/production.keystone

# Verify creation
sudo -u keystonedb ls -lh /var/lib/keystonedb/data/production.keystone/

# Set restrictive permissions
sudo chmod 700 /var/lib/keystonedb/data/production.keystone
```

### Systemd Service Setup

Deploy KeystoneDB server as a systemd service for automatic startup and management:

```bash
# Create service file
sudo tee /etc/systemd/system/keystonedb.service <<'EOF'
[Unit]
Description=KeystoneDB Server
Documentation=https://github.com/yourusername/keystonedb
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=keystonedb
Group=keystonedb
WorkingDirectory=/var/lib/keystonedb

# Command line
ExecStart=/usr/local/bin/kstone-server \
    --db-path /var/lib/keystonedb/data/production.keystone \
    --host 0.0.0.0 \
    --port 50051

# Restart policy
Restart=always
RestartSec=5
StartLimitInterval=0

# Resource limits
LimitNOFILE=65536
MemoryMax=16G
MemoryHigh=14G

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=keystonedb

# Environment
Environment="RUST_LOG=info"
Environment="RUST_BACKTRACE=1"

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/keystonedb

[Install]
WantedBy=multi-user.target
EOF

# Reload systemd
sudo systemctl daemon-reload

# Enable service (start on boot)
sudo systemctl enable keystonedb

# Start service
sudo systemctl start keystonedb

# Check status
sudo systemctl status keystonedb

# View logs
sudo journalctl -u keystonedb -f
```

### Environment Configuration

Configure logging and runtime behavior via environment variables:

```bash
# Add to systemd service file or /etc/environment
Environment="RUST_LOG=info,kstone_core=debug"
Environment="RUST_BACKTRACE=1"
Environment="KEYSTONEDB_MAX_CONNECTIONS=1000"
```

**Common environment variables:**

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level (error, warn, info, debug, trace) |
| `RUST_BACKTRACE` | `0` | Enable backtraces (1=enabled, full=verbose) |
| `KEYSTONEDB_MAX_CONNECTIONS` | `100` | Max concurrent connections (server mode) |

## Container Deployment

### Docker Deployment

Deploy KeystoneDB in Docker for isolated, portable deployments:

**Dockerfile:**

```dockerfile
# Build stage
FROM rust:1.75-alpine as builder

WORKDIR /build

# Install build dependencies
RUN apk add --no-cache musl-dev

# Copy source
COPY . .

# Build release binary
RUN cargo build --release --bin kstone-server

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache ca-certificates

# Create user
RUN addgroup -g 1000 keystonedb && \
    adduser -D -u 1000 -G keystonedb keystonedb

# Copy binary
COPY --from=builder /build/target/release/kstone-server /usr/local/bin/

# Create data directory
RUN mkdir -p /data && \
    chown keystonedb:keystonedb /data

# Switch to non-root user
USER keystonedb

# Expose gRPC port
EXPOSE 50051

# Set working directory
WORKDIR /data

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/kstone-server", "--version"]

# Run server
ENTRYPOINT ["/usr/local/bin/kstone-server"]
CMD ["--db-path", "/data/db.keystone", "--host", "0.0.0.0", "--port", "50051"]
```

**Build and run:**

```bash
# Build image
docker build -t keystonedb:latest .

# Run container
docker run -d \
    --name keystonedb \
    -p 50051:50051 \
    -v /var/lib/keystonedb/data:/data \
    --restart unless-stopped \
    --memory 8g \
    --cpus 4 \
    keystonedb:latest

# View logs
docker logs -f keystonedb

# Stop container
docker stop keystonedb

# Remove container
docker rm keystonedb
```

**Docker Compose:**

```yaml
# docker-compose.yml
version: '3.8'

services:
  keystonedb:
    image: keystonedb:latest
    container_name: keystonedb
    restart: unless-stopped

    ports:
      - "50051:50051"

    volumes:
      - keystonedb-data:/data

    environment:
      - RUST_LOG=info
      - RUST_BACKTRACE=1

    command:
      - --db-path=/data/db.keystone
      - --host=0.0.0.0
      - --port=50051

    deploy:
      resources:
        limits:
          cpus: '4'
          memory: 8G
        reservations:
          cpus: '2'
          memory: 4G

    healthcheck:
      test: ["CMD", "kstone-server", "--version"]
      interval: 30s
      timeout: 3s
      retries: 3
      start_period: 5s

volumes:
  keystonedb-data:
    driver: local
```

**Deploy with Docker Compose:**

```bash
# Start services
docker-compose up -d

# View logs
docker-compose logs -f

# Stop services
docker-compose down

# Remove volumes
docker-compose down -v
```

### Kubernetes Deployment

Deploy KeystoneDB on Kubernetes for orchestrated, scalable deployments:

**StatefulSet with PersistentVolume:**

```yaml
# keystonedb-statefulset.yaml
apiVersion: v1
kind: Service
metadata:
  name: keystonedb
  labels:
    app: keystonedb
spec:
  ports:
  - port: 50051
    name: grpc
    targetPort: 50051
  clusterIP: None
  selector:
    app: keystonedb

---
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: keystonedb
spec:
  serviceName: keystonedb
  replicas: 1
  selector:
    matchLabels:
      app: keystonedb

  template:
    metadata:
      labels:
        app: keystonedb
    spec:
      containers:
      - name: keystonedb
        image: keystonedb:latest
        imagePullPolicy: IfNotPresent

        ports:
        - containerPort: 50051
          name: grpc

        args:
        - --db-path=/data/db.keystone
        - --host=0.0.0.0
        - --port=50051

        env:
        - name: RUST_LOG
          value: "info"
        - name: RUST_BACKTRACE
          value: "1"

        resources:
          requests:
            cpu: 2000m
            memory: 4Gi
          limits:
            cpu: 4000m
            memory: 8Gi

        volumeMounts:
        - name: data
          mountPath: /data

        livenessProbe:
          exec:
            command:
            - /usr/local/bin/kstone-server
            - --version
          initialDelaySeconds: 10
          periodSeconds: 30
          timeoutSeconds: 5

        readinessProbe:
          tcpSocket:
            port: 50051
          initialDelaySeconds: 5
          periodSeconds: 10

  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 100Gi
```

**Deploy to Kubernetes:**

```bash
# Create namespace
kubectl create namespace keystonedb

# Apply configuration
kubectl apply -f keystonedb-statefulset.yaml -n keystonedb

# Check status
kubectl get statefulset -n keystonedb
kubectl get pods -n keystonedb

# View logs
kubectl logs -f keystonedb-0 -n keystonedb

# Expose service (LoadBalancer)
kubectl expose statefulset keystonedb --type=LoadBalancer --port=50051 -n keystonedb

# Get external IP
kubectl get svc keystonedb -n keystonedb
```

## Resource Limits and Tuning

### Memory Limits

Configure memory limits to prevent OOM issues:

**Systemd:**

```ini
[Service]
MemoryMax=16G
MemoryHigh=14G
MemorySwapMax=0
```

**Docker:**

```bash
docker run --memory 8g --memory-swap 8g keystonedb:latest
```

**Kubernetes:**

```yaml
resources:
  requests:
    memory: 4Gi
  limits:
    memory: 8Gi
```

### CPU Limits

Allocate CPU resources appropriately:

**Systemd:**

```ini
[Service]
CPUQuota=400%  # 4 cores
```

**Docker:**

```bash
docker run --cpus 4 keystonedb:latest
```

**Kubernetes:**

```yaml
resources:
  requests:
    cpu: 2000m  # 2 cores
  limits:
    cpu: 4000m  # 4 cores
```

### Disk I/O Limits

Control disk I/O to prevent resource starvation:

**Systemd:**

```ini
[Service]
IOWeight=500
IOReadBandwidthMax=/dev/nvme0n1 100M
IOWriteBandwidthMax=/dev/nvme0n1 100M
```

### Network Configuration

For server deployments, configure network settings:

**Connection limits:**

```bash
# Increase connection tracking
sudo sysctl -w net.netfilter.nf_conntrack_max=1000000

# Increase socket buffer sizes
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.wmem_max=134217728
sudo sysctl -w net.ipv4.tcp_rmem="4096 87380 134217728"
sudo sysctl -w net.ipv4.tcp_wmem="4096 65536 134217728"

# Make permanent
sudo tee -a /etc/sysctl.conf <<EOF
net.netfilter.nf_conntrack_max=1000000
net.core.rmem_max=134217728
net.core.wmem_max=134217728
net.ipv4.tcp_rmem=4096 87380 134217728
net.ipv4.tcp_wmem=4096 65536 134217728
EOF

sudo sysctl -p
```

## Performance Tuning

### Linux System Tuning

**Disable Transparent Huge Pages** (can cause latency spikes):

```bash
# Check current status
cat /sys/kernel/mm/transparent_hugepage/enabled

# Disable
echo never | sudo tee /sys/kernel/mm/transparent_hugepage/enabled
echo never | sudo tee /sys/kernel/mm/transparent_hugepage/defrag

# Make permanent (add to /etc/rc.local)
sudo tee -a /etc/rc.local <<'EOF'
echo never > /sys/kernel/mm/transparent_hugepage/enabled
echo never > /sys/kernel/mm/transparent_hugepage/defrag
EOF

sudo chmod +x /etc/rc.local
```

**Increase vm.swappiness** (for database workloads):

```bash
# Reduce swappiness (prefer keeping data in RAM)
sudo sysctl -w vm.swappiness=1

# Make permanent
echo "vm.swappiness=1" | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
```

### Monitoring and Validation

After deployment, validate the configuration:

```bash
# Check service status
sudo systemctl status keystonedb

# Check resource usage
top -p $(pgrep kstone-server)

# Check file descriptors
lsof -p $(pgrep kstone-server) | wc -l

# Check disk I/O
iostat -x 1

# Check network connections
netstat -an | grep :50051
```

This deployment guide provides a solid foundation for running KeystoneDB in production environments. The next chapters will cover monitoring, backup strategies, and troubleshooting.

# Chapter 34: Security Considerations

Security is a critical aspect of database deployment. This chapter covers security best practices for KeystoneDB, including file permissions, encryption, network security, audit logging, and defense in depth strategies.

## File Permissions and Ownership

Proper file permissions are the first line of defense against unauthorized access.

### Creating a Dedicated User

Run KeystoneDB under a dedicated, unprivileged user account:

```bash
# Create keystonedb user (system account, no login)
sudo useradd -r -s /bin/false -d /var/lib/keystonedb -c "KeystoneDB Service Account" keystonedb

# Create home directory
sudo mkdir -p /var/lib/keystonedb
sudo chown keystonedb:keystonedb /var/lib/keystonedb

# Verify user creation
id keystonedb
# Output: uid=998(keystonedb) gid=998(keystonedb) groups=998(keystonedb)
```

**Why a dedicated user?**
- **Principle of Least Privilege**: Process runs with minimal permissions
- **Isolation**: Compromised database can't access other users' files
- **Audit Trail**: All file operations clearly attributed to keystonedb user
- **Defense in Depth**: Additional security layer even if application is compromised

### Setting Restrictive Permissions

Configure strict file permissions to prevent unauthorized access:

```bash
# Database directory: Only keystonedb user can access
sudo chown -R keystonedb:keystonedb /var/lib/keystonedb/data
sudo chmod 700 /var/lib/keystonedb/data

# Database files: Only keystonedb user can read/write
sudo find /var/lib/keystonedb/data -type f -exec chmod 600 {} \;

# Log directory: keystonedb writes, group can read
sudo chown keystonedb:adm /var/log/keystonedb
sudo chmod 750 /var/log/keystonedb
sudo chmod 640 /var/log/keystonedb/*.log

# Configuration files: Root owns, keystonedb can read
sudo chown root:keystonedb /etc/keystonedb/config.toml
sudo chmod 640 /etc/keystonedb/config.toml

# Binary: Root owns, all can execute
sudo chown root:root /usr/local/bin/kstone-server
sudo chmod 755 /usr/local/bin/kstone-server
```

**Permission breakdown:**

| Path | Owner | Permissions | Purpose |
|------|-------|-------------|---------|
| `/var/lib/keystonedb` | keystonedb:keystonedb | 700 (rwx------) | Data directory |
| `*.sst`, `wal.log` | keystonedb:keystonedb | 600 (rw-------) | Database files |
| `/var/log/keystonedb` | keystonedb:adm | 750 (rwxr-x---) | Log directory |
| `*.log` | keystonedb:adm | 640 (rw-r-----) | Log files |
| `/etc/keystonedb/config.toml` | root:keystonedb | 640 (rw-r-----) | Configuration |
| `/usr/local/bin/kstone-server` | root:root | 755 (rwxr-xr-x) | Executable |

### Verifying Permissions

Regular permission audits ensure security configuration remains intact:

```bash
#!/bin/bash
# audit-permissions.sh - Verify file permissions

set -euo pipefail

ERRORS=0

# Check database directory
if [ "$(stat -c %a /var/lib/keystonedb/data)" != "700" ]; then
    echo "ERROR: Database directory has incorrect permissions"
    ((ERRORS++))
fi

# Check database files
find /var/lib/keystonedb/data -type f | while read file; do
    perms=$(stat -c %a "$file")
    if [ "$perms" != "600" ]; then
        echo "ERROR: $file has incorrect permissions: $perms (expected 600)"
        ((ERRORS++))
    fi
done

# Check ownership
owner=$(stat -c %U:%G /var/lib/keystonedb/data)
if [ "$owner" != "keystonedb:keystonedb" ]; then
    echo "ERROR: Incorrect ownership: $owner (expected keystonedb:keystonedb)"
    ((ERRORS++))
fi

if [ $ERRORS -eq 0 ]; then
    echo "✓ All permissions correct"
else
    echo "✗ Found $ERRORS permission issues"
    exit 1
fi
```

## Encryption at Rest

Protect data on disk from unauthorized access.

### Block-Level Encryption (Phase 1.2+)

KeystoneDB supports optional AES-256-GCM encryption for all database blocks:

```rust
use kstone_api::Database;

// Create database with encryption
let encryption_key: [u8; 32] = [/* 256-bit key */];
let db = Database::create_with_encryption(
    "/var/lib/keystonedb/data/encrypted.keystone",
    encryption_key,
)?;

// All blocks (SST, WAL, Manifest) are encrypted transparently
db.put(b"user#123", item)?;
```

**Key management considerations:**
- **Never hardcode keys** in source code
- Store keys in environment variables or key management service
- Rotate keys periodically
- Use different keys for different environments

**Example: Loading key from environment:**

```rust
use std::env;

fn load_encryption_key() -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let key_hex = env::var("KEYSTONEDB_ENCRYPTION_KEY")?;

    if key_hex.len() != 64 {
        return Err("Encryption key must be 64 hex characters (32 bytes)".into());
    }

    let mut key = [0u8; 32];
    hex::decode_to_slice(&key_hex, &mut key)?;

    Ok(key)
}

// Usage
let key = load_encryption_key()?;
let db = Database::create_with_encryption(path, key)?;
```

**Generating secure keys:**

```bash
# Generate 256-bit (32 byte) random key
openssl rand -hex 32

# Store in environment variable (systemd)
sudo systemctl edit keystonedb
# Add:
[Service]
Environment="KEYSTONEDB_ENCRYPTION_KEY=a1b2c3d4e5f6..."

# Or use a secret management service
# AWS Secrets Manager, HashiCorp Vault, etc.
```

### Filesystem-Level Encryption

Alternative to application-level encryption using Linux LUKS:

```bash
# Create encrypted volume with LUKS
sudo cryptsetup luksFormat /dev/nvme0n1p1

# Provide strong passphrase when prompted
# WARNING: Losing passphrase means data is permanently lost

# Open encrypted volume
sudo cryptsetup luksOpen /dev/nvme0n1p1 keystonedb_encrypted

# Create filesystem
sudo mkfs.ext4 /dev/mapper/keystonedb_encrypted

# Mount encrypted volume
sudo mkdir -p /var/lib/keystonedb
sudo mount /dev/mapper/keystonedb_encrypted /var/lib/keystonedb

# Add to /etc/crypttab for automatic unlock
echo "keystonedb_encrypted /dev/nvme0n1p1 none luks" | \
    sudo tee -a /etc/crypttab

# Add to /etc/fstab for automatic mount
echo "/dev/mapper/keystonedb_encrypted /var/lib/keystonedb ext4 defaults 0 2" | \
    sudo tee -a /etc/fstab
```

**LUKS advantages:**
- ✅ Transparent to application
- ✅ Entire partition encrypted (metadata, logs, backups)
- ✅ Well-tested, proven technology
- ✅ Independent of database code

**LUKS disadvantages:**
- ❌ Requires root access for setup
- ❌ Passphrase management complexity
- ❌ Entire partition encrypted/decrypted as unit
- ❌ Key rotation requires re-encryption

### Backup Encryption

Encrypt backups before storing off-site:

```bash
#!/bin/bash
# encrypted-backup.sh - Create encrypted backup

set -euo pipefail

BACKUP_DIR="/var/lib/keystonedb/data/production.keystone"
BACKUP_FILE="/tmp/backup-$(date +%Y%m%d_%H%M%S).tar.gz"
ENCRYPTED_FILE="/var/backups/keystonedb/encrypted-$(date +%Y%m%d_%H%M%S).tar.gz.gpg"
GPG_RECIPIENT="backup-admin@example.com"

echo "Creating encrypted backup..."

# Create backup
tar czf "$BACKUP_FILE" -C "$BACKUP_DIR" .

# Encrypt with GPG
gpg --encrypt --recipient "$GPG_RECIPIENT" --output "$ENCRYPTED_FILE" "$BACKUP_FILE"

# Remove unencrypted backup
rm "$BACKUP_FILE"

# Upload to S3 with server-side encryption
aws s3 cp "$ENCRYPTED_FILE" s3://my-backups/ \
    --server-side-encryption AES256

echo "Encrypted backup completed: $ENCRYPTED_FILE"
```

**Decrypting backup for restore:**

```bash
# Decrypt backup
gpg --decrypt backup-encrypted.tar.gz.gpg > backup.tar.gz

# Verify integrity
tar tzf backup.tar.gz > /dev/null

# Restore
tar xzf backup.tar.gz -C /var/lib/keystonedb/data/
```

## Network Security

Secure network communications for server deployments.

### TLS/SSL Configuration

Enable TLS for gRPC server to encrypt data in transit:

```bash
# Generate self-signed certificate (development only)
openssl req -x509 -newkey rsa:4096 -nodes \
    -keyout server-key.pem \
    -out server-cert.pem \
    -days 365 \
    -subj "/CN=keystonedb.example.com"

# For production, use certificates from trusted CA
# Let's Encrypt, DigiCert, etc.

# Start server with TLS
kstone-server \
    --db-path /var/lib/keystonedb/data/production.keystone \
    --tls-cert /etc/keystonedb/certs/server-cert.pem \
    --tls-key /etc/keystonedb/certs/server-key.pem \
    --host 0.0.0.0 \
    --port 50051
```

**Client-side TLS:**

```rust
use kstone_client::Client;

// Connect with TLS
let client = Client::connect_with_tls(
    "https://keystonedb.example.com:50051",
    "/path/to/ca-cert.pem",
).await?;
```

### Firewall Configuration

Restrict network access using firewall rules:

```bash
# UFW (Ubuntu/Debian)
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow from 10.0.0.0/24 to any port 50051 proto tcp  # Application servers
sudo ufw allow from 192.168.1.100 to any port 50051 proto tcp  # Admin workstation
sudo ufw enable

# iptables (manual configuration)
sudo iptables -A INPUT -p tcp --dport 50051 -s 10.0.0.0/24 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 50051 -j DROP

# Make permanent
sudo iptables-save > /etc/iptables/rules.v4
```

**Network segmentation:**
```
┌─────────────────┐
│  Public Internet│
└────────┬────────┘
         │
    ┌────▼────┐
    │   WAF   │ (Web Application Firewall)
    └────┬────┘
         │
    ┌────▼────────┐
    │ Load Balancer│
    └────┬────────┘
         │
    ┌────▼──────────┐
    │ App Servers   │ (Private subnet: 10.0.1.0/24)
    └────┬──────────┘
         │
    ┌────▼──────────┐
    │  KeystoneDB   │ (Database subnet: 10.0.2.0/24)
    │               │ (No internet access)
    └───────────────┘
```

### Connection Limits

Prevent resource exhaustion attacks by limiting connections:

```rust
// Server configuration
const MAX_CONNECTIONS: usize = 1000;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

// Example connection limiter (pseudo-code)
struct ConnectionLimiter {
    active: Arc<AtomicUsize>,
    max: usize,
}

impl ConnectionLimiter {
    fn try_acquire(&self) -> Result<ConnectionGuard, Error> {
        let current = self.active.fetch_add(1, Ordering::SeqCst);
        if current >= self.max {
            self.active.fetch_sub(1, Ordering::SeqCst);
            return Err(Error::TooManyConnections);
        }
        Ok(ConnectionGuard { limiter: self })
    }
}
```

## Access Control Patterns

Implement application-level access control.

### Role-Based Access Control (RBAC)

```rust
use std::collections::HashSet;

#[derive(Debug, Clone)]
enum Permission {
    Read,
    Write,
    Delete,
    Admin,
}

#[derive(Debug, Clone)]
struct Role {
    name: String,
    permissions: HashSet<Permission>,
}

#[derive(Debug, Clone)]
struct User {
    id: String,
    roles: Vec<Role>,
}

impl User {
    fn can(&self, permission: Permission) -> bool {
        self.roles.iter().any(|role| role.permissions.contains(&permission))
    }

    fn can_access_key(&self, key: &[u8]) -> bool {
        // Implement key-based access control
        // Example: Users can only access keys starting with their user ID
        let user_prefix = format!("user#{}", self.id);
        key.starts_with(user_prefix.as_bytes())
    }
}

// Usage in application
fn authorize_write(user: &User, key: &[u8]) -> Result<(), Error> {
    if !user.can(Permission::Write) {
        return Err(Error::PermissionDenied("User lacks write permission".into()));
    }

    if !user.can_access_key(key) {
        return Err(Error::PermissionDenied("User cannot access this key".into()));
    }

    Ok(())
}

// Before database operations
authorize_write(&user, b"user#123#profile")?;
db.put(b"user#123#profile", item)?;
```

### Attribute-Based Access Control (ABAC)

More fine-grained access control based on attributes:

```rust
#[derive(Debug, Clone)]
struct AccessPolicy {
    user_id: String,
    allowed_partitions: Vec<String>,
    allowed_operations: HashSet<String>,
    ip_whitelist: Vec<String>,
}

impl AccessPolicy {
    fn evaluate(&self, request: &AccessRequest) -> bool {
        // Check operation permission
        if !self.allowed_operations.contains(&request.operation) {
            return false;
        }

        // Check partition access
        let partition = extract_partition(&request.key);
        if !self.allowed_partitions.contains(&partition) {
            return false;
        }

        // Check IP whitelist
        if !self.ip_whitelist.is_empty() {
            if !self.ip_whitelist.contains(&request.client_ip) {
                return false;
            }
        }

        true
    }
}

struct AccessRequest {
    operation: String,
    key: Vec<u8>,
    client_ip: String,
}

fn extract_partition(key: &[u8]) -> String {
    // Extract partition from key (e.g., "user#123" -> "user")
    String::from_utf8_lossy(key)
        .split('#')
        .next()
        .unwrap_or("")
        .to_string()
}
```

### Multi-Tenancy Isolation

Ensure tenant data isolation in multi-tenant applications:

```rust
struct TenantDatabase {
    tenant_id: String,
    db: Database,
}

impl TenantDatabase {
    fn new(tenant_id: String, base_path: &Path) -> Result<Self, Error> {
        // Each tenant gets isolated database directory
        let tenant_path = base_path.join(&tenant_id);
        let db = Database::open(&tenant_path)?;

        Ok(TenantDatabase { tenant_id, db })
    }

    fn put(&self, key: &[u8], item: Item) -> Result<(), Error> {
        // Automatically prefix keys with tenant ID
        let tenant_key = self.tenant_key(key);
        self.db.put(&tenant_key, item)
    }

    fn get(&self, key: &[u8]) -> Result<Option<Item>, Error> {
        let tenant_key = self.tenant_key(key);
        self.db.get(&tenant_key)
    }

    fn tenant_key(&self, key: &[u8]) -> Vec<u8> {
        let mut tenant_key = Vec::new();
        tenant_key.extend_from_slice(self.tenant_id.as_bytes());
        tenant_key.push(b'#');
        tenant_key.extend_from_slice(key);
        tenant_key
    }
}

// Usage
let tenant_db = TenantDatabase::new("tenant-abc".to_string(), Path::new("/var/lib/keystonedb"))?;

// Keys automatically prefixed with "tenant-abc#"
tenant_db.put(b"user#123", item)?;
// Stored as: "tenant-abc#user#123"
```

## Audit Logging

Track and log security-relevant events for compliance and forensics.

### Structured Audit Logs

```rust
use tracing::{info, warn, error};
use serde_json::json;

fn audit_log(event_type: &str, user: &User, key: &[u8], success: bool) {
    let audit_data = json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "event_type": event_type,
        "user_id": user.id,
        "user_ip": user.ip_address,
        "key": String::from_utf8_lossy(key),
        "success": success,
    });

    if success {
        info!(
            audit = true,
            %event_type,
            user_id = %user.id,
            "Audit event"
        );
    } else {
        warn!(
            audit = true,
            %event_type,
            user_id = %user.id,
            "Audit event (FAILED)"
        );
    }
}

// Usage
pub fn secure_put(user: &User, key: &[u8], item: Item) -> Result<(), Error> {
    match authorize_write(user, key) {
        Ok(_) => {
            db.put(key, item)?;
            audit_log("PUT", user, key, true);
            Ok(())
        }
        Err(e) => {
            audit_log("PUT", user, key, false);
            Err(e)
        }
    }
}
```

### Audit Log Analysis

```bash
# Extract audit events
grep 'audit=true' /var/log/keystonedb/server.log

# Failed access attempts
grep 'audit=true.*FAILED' /var/log/keystonedb/server.log

# Access by user
grep 'audit=true.*user_id="alice"' /var/log/keystonedb/server.log

# Export audit trail
grep 'audit=true' /var/log/keystonedb/server.log | \
    jq -r '[.timestamp, .user_id, .event_type, .success] | @csv' > audit_trail.csv
```

## Security Best Practices

### 1. Principle of Least Privilege

Grant minimal permissions necessary:

```bash
# ❌ Wrong: Running as root
sudo kstone-server --db-path /var/lib/keystonedb/data/db.keystone

# ✅ Correct: Running as dedicated user
sudo -u keystonedb kstone-server --db-path /var/lib/keystonedb/data/db.keystone
```

### 2. Defense in Depth

Layer multiple security controls:

1. **Network**: Firewall rules, VPN, private subnets
2. **Transport**: TLS encryption
3. **Authentication**: API keys, OAuth tokens
4. **Authorization**: RBAC/ABAC policies
5. **Application**: Input validation, rate limiting
6. **Data**: Encryption at rest
7. **Audit**: Comprehensive logging
8. **Monitoring**: Intrusion detection, anomaly detection

### 3. Regular Security Audits

```bash
#!/bin/bash
# security-audit.sh - Regular security check

set -euo pipefail

echo "=== KeystoneDB Security Audit ==="

# Check file permissions
echo "Checking file permissions..."
/opt/scripts/audit-permissions.sh

# Check for exposed ports
echo "Checking for exposed ports..."
sudo netstat -tlnp | grep kstone

# Check user accounts
echo "Checking user accounts..."
getent passwd keystonedb

# Check for security updates
echo "Checking for updates..."
apt list --upgradable 2>/dev/null | grep -i security

# Check logs for suspicious activity
echo "Checking for failed access attempts..."
grep -c "FAILED" /var/log/keystonedb/server.log || echo "0 failed attempts"

# Check certificate expiration
echo "Checking TLS certificate..."
openssl x509 -enddate -noout -in /etc/keystonedb/certs/server-cert.pem

echo "Security audit completed"
```

### 4. Secrets Management

Never store secrets in code or configuration files:

```bash
# ❌ Wrong: Hardcoded in config
encryption_key = "a1b2c3d4e5f6..."

# ✅ Correct: Environment variable
export KEYSTONEDB_ENCRYPTION_KEY="a1b2c3d4e5f6..."

# ✅ Better: Secrets management service
# AWS Secrets Manager
aws secretsmanager get-secret-value --secret-id keystonedb/encryption-key

# HashiCorp Vault
vault kv get secret/keystonedb/encryption-key
```

### 5. Input Validation

Always validate and sanitize inputs:

```rust
fn validate_key(key: &[u8]) -> Result<(), Error> {
    // Check length
    if key.is_empty() {
        return Err(Error::InvalidArgument("Key cannot be empty".into()));
    }

    if key.len() > 1024 {
        return Err(Error::InvalidArgument("Key too long (max 1024 bytes)".into()));
    }

    // Check for null bytes (prevent injection)
    if key.contains(&0) {
        return Err(Error::InvalidArgument("Key contains null byte".into()));
    }

    // Validate UTF-8 (if required)
    if std::str::from_utf8(key).is_err() {
        return Err(Error::InvalidArgument("Key must be valid UTF-8".into()));
    }

    Ok(())
}

// Usage
validate_key(user_input)?;
db.put(user_input, item)?;
```

### 6. Rate Limiting

Prevent abuse and DoS attacks:

```rust
use std::time::{Duration, Instant};
use std::collections::HashMap;

struct RateLimiter {
    requests: HashMap<String, Vec<Instant>>,
    max_requests: usize,
    window: Duration,
}

impl RateLimiter {
    fn check(&mut self, user_id: &str) -> Result<(), Error> {
        let now = Instant::now();
        let cutoff = now - self.window;

        // Get user's recent requests
        let requests = self.requests.entry(user_id.to_string()).or_insert_with(Vec::new);

        // Remove old requests
        requests.retain(|&t| t > cutoff);

        // Check limit
        if requests.len() >= self.max_requests {
            return Err(Error::RateLimitExceeded);
        }

        // Add current request
        requests.push(now);
        Ok(())
    }
}

// Usage: 100 requests per minute
let mut limiter = RateLimiter {
    requests: HashMap::new(),
    max_requests: 100,
    window: Duration::from_secs(60),
};

limiter.check(&user.id)?;
db.put(key, item)?;
```

### 7. Secure Defaults

Configure secure settings by default:

```rust
// ✅ Good: Secure by default
const DEFAULT_MAX_CONNECTIONS: usize = 100;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const REQUIRE_TLS: bool = true;

// ❌ Bad: Insecure defaults
const DEFAULT_MAX_CONNECTIONS: usize = usize::MAX;  // Unlimited
const REQUIRE_TLS: bool = false;  // TLS optional
```

## Compliance Considerations

### GDPR (General Data Protection Regulation)

- **Right to Erasure**: Implement hard delete (not just tombstone)
- **Data Portability**: Provide export functionality
- **Encryption**: Encrypt personal data at rest and in transit
- **Audit Logging**: Log all access to personal data
- **Data Minimization**: Store only necessary data

### HIPAA (Health Insurance Portability and Accountability Act)

- **Access Controls**: Implement role-based access
- **Audit Trails**: Log all PHI access
- **Encryption**: Encrypt all PHI data
- **Backup Security**: Encrypt backups
- **Disaster Recovery**: Tested recovery procedures

### SOC 2 (Service Organization Control 2)

- **Security**: Firewall, encryption, access controls
- **Availability**: Monitoring, redundancy, backups
- **Confidentiality**: Encryption, access controls
- **Processing Integrity**: Input validation, checksums
- **Privacy**: Data classification, retention policies

By implementing these security best practices, KeystoneDB deployments can achieve defense-in-depth protection against a wide range of threats while maintaining compliance with regulatory requirements.

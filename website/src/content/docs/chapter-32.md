# Chapter 32: Backup & Recovery

Data durability is paramount for any database system. This chapter provides comprehensive backup and recovery strategies for KeystoneDB, covering full and incremental backups, point-in-time recovery, automation, and disaster recovery planning.

## Understanding KeystoneDB's Backup Model

KeystoneDB's architecture enables straightforward backup procedures:

**Immutable SST Files:**
- SST files are immutable once written
- Can be safely copied while the database is running
- Provide consistent point-in-time snapshots

**Write-Ahead Log (WAL):**
- Contains recent writes not yet flushed to SST files
- Critical for crash recovery
- Must be included in backups for consistency

**Stripe-Based Architecture:**
- 256 independent stripes
- Each stripe has its own WAL and SST files
- Enables parallel backup operations

## Backup Strategies

### Full Backup

A full backup captures all database files at a specific point in time. This is the simplest and most reliable backup method.

**Basic full backup script:**

```bash
#!/bin/bash
# full-backup.sh - Complete database backup

set -euo pipefail

# Configuration
DB_DIR="/var/lib/keystonedb/data/production.keystone"
BACKUP_ROOT="/var/backups/keystonedb"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="$BACKUP_ROOT/$TIMESTAMP"

# Create backup directory
mkdir -p "$BACKUP_DIR"

echo "Starting full backup at $(date)"
echo "Database: $DB_DIR"
echo "Backup destination: $BACKUP_DIR"

# Optional: Signal application to reduce writes (if supported)
# kill -USR1 $(pidof myapp)

# Copy all database files
echo "Copying database files..."
rsync -av --progress "$DB_DIR/" "$BACKUP_DIR/"

# Optional: Resume normal operation
# kill -USR2 $(pidof myapp)

# Create metadata file
cat > "$BACKUP_DIR/backup-metadata.json" <<EOF
{
  "timestamp": "$TIMESTAMP",
  "database_path": "$DB_DIR",
  "backup_type": "full",
  "hostname": "$(hostname)",
  "size_bytes": $(du -sb "$BACKUP_DIR" | cut -f1)
}
EOF

# Compress backup
echo "Compressing backup..."
tar czf "$BACKUP_ROOT/backup-$TIMESTAMP.tar.gz" -C "$BACKUP_DIR" .

# Calculate checksum
echo "Calculating checksum..."
sha256sum "$BACKUP_ROOT/backup-$TIMESTAMP.tar.gz" > "$BACKUP_ROOT/backup-$TIMESTAMP.tar.gz.sha256"

# Remove uncompressed backup
rm -rf "$BACKUP_DIR"

BACKUP_SIZE=$(du -h "$BACKUP_ROOT/backup-$TIMESTAMP.tar.gz" | cut -f1)
echo "Backup completed at $(date)"
echo "Backup file: backup-$TIMESTAMP.tar.gz"
echo "Size: $BACKUP_SIZE"
```

**Usage:**

```bash
# Run backup
sudo -u keystonedb /opt/scripts/full-backup.sh

# Verify backup
ls -lh /var/backups/keystonedb/

# Verify checksum
cd /var/backups/keystonedb
sha256sum -c backup-20250115_020000.tar.gz.sha256
```

### Incremental Backup

Incremental backups only copy files that have changed since the last backup, reducing backup time and storage requirements.

**Incremental backup script:**

```bash
#!/bin/bash
# incremental-backup.sh - Backup only changed files

set -euo pipefail

# Configuration
DB_DIR="/var/lib/keystonedb/data/production.keystone"
BACKUP_ROOT="/var/backups/keystonedb/incremental"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="$BACKUP_ROOT/$TIMESTAMP"
LAST_BACKUP_MARKER="$BACKUP_ROOT/.last_backup_timestamp"

# Create backup directory
mkdir -p "$BACKUP_DIR"

echo "Starting incremental backup at $(date)"

# Determine reference time
if [ -f "$LAST_BACKUP_MARKER" ]; then
    REFERENCE_TIME=$(cat "$LAST_BACKUP_MARKER")
    echo "Last backup: $REFERENCE_TIME"
    REFERENCE_FILE="$BACKUP_ROOT/.last_backup"
    touch -d "$REFERENCE_TIME" "$REFERENCE_FILE"
else
    echo "No previous backup found, performing full backup"
    REFERENCE_FILE="/tmp/never_existed"
fi

# Find files newer than last backup
echo "Finding changed files..."
find "$DB_DIR" -type f -newer "$REFERENCE_FILE" 2>/dev/null | while read file; do
    # Preserve directory structure
    relative_path="${file#$DB_DIR/}"
    target_dir="$BACKUP_DIR/$(dirname "$relative_path")"
    mkdir -p "$target_dir"
    cp -p "$file" "$target_dir/"
    echo "  $relative_path"
done

# Count backed up files
FILE_COUNT=$(find "$BACKUP_DIR" -type f | wc -l)

if [ "$FILE_COUNT" -eq 0 ]; then
    echo "No changes detected since last backup"
    rmdir "$BACKUP_DIR"
    exit 0
fi

# Create metadata
cat > "$BACKUP_DIR/backup-metadata.json" <<EOF
{
  "timestamp": "$TIMESTAMP",
  "backup_type": "incremental",
  "previous_backup": "$(cat $LAST_BACKUP_MARKER 2>/dev/null || echo 'none')",
  "file_count": $FILE_COUNT,
  "size_bytes": $(du -sb "$BACKUP_DIR" | cut -f1)
}
EOF

# Update marker
echo "$TIMESTAMP" > "$LAST_BACKUP_MARKER"

# Compress backup
tar czf "$BACKUP_ROOT/incremental-$TIMESTAMP.tar.gz" -C "$BACKUP_DIR" .
rm -rf "$BACKUP_DIR"

BACKUP_SIZE=$(du -h "$BACKUP_ROOT/incremental-$TIMESTAMP.tar.gz" | cut -f1)
echo "Incremental backup completed at $(date)"
echo "Files backed up: $FILE_COUNT"
echo "Size: $BACKUP_SIZE"
```

### Differential Backup

Differential backups capture all changes since the last *full* backup (not the last incremental).

```bash
#!/bin/bash
# differential-backup.sh - Backup changes since last full backup

set -euo pipefail

# Configuration
DB_DIR="/var/lib/keystonedb/data/production.keystone"
BACKUP_ROOT="/var/backups/keystonedb"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="$BACKUP_ROOT/differential/$TIMESTAMP"
FULL_BACKUP_MARKER="$BACKUP_ROOT/.last_full_backup"

# Verify last full backup exists
if [ ! -f "$FULL_BACKUP_MARKER" ]; then
    echo "ERROR: No full backup found. Run full backup first."
    exit 1
fi

LAST_FULL=$(cat "$FULL_BACKUP_MARKER")
echo "Differential backup since full backup: $LAST_FULL"

# Create reference file
REFERENCE_FILE="$BACKUP_ROOT/.differential_ref"
touch -d "$LAST_FULL" "$REFERENCE_FILE"

# Find all files changed since full backup
mkdir -p "$BACKUP_DIR"
find "$DB_DIR" -type f -newer "$REFERENCE_FILE" | while read file; do
    relative_path="${file#$DB_DIR/}"
    target_dir="$BACKUP_DIR/$(dirname "$relative_path")"
    mkdir -p "$target_dir"
    cp -p "$file" "$target_dir/"
done

# Create metadata
FILE_COUNT=$(find "$BACKUP_DIR" -type f | wc -l)
cat > "$BACKUP_DIR/backup-metadata.json" <<EOF
{
  "timestamp": "$TIMESTAMP",
  "backup_type": "differential",
  "base_backup": "$LAST_FULL",
  "file_count": $FILE_COUNT
}
EOF

# Compress
tar czf "$BACKUP_ROOT/differential-$TIMESTAMP.tar.gz" -C "$BACKUP_DIR" .
rm -rf "$BACKUP_DIR"

echo "Differential backup completed"
echo "Files backed up: $FILE_COUNT"
```

## Automated Backup with Cron

Schedule regular backups using cron:

```bash
# Edit crontab
sudo crontab -e -u keystonedb

# Add backup schedule
# Full backup: Daily at 2 AM
0 2 * * * /opt/scripts/full-backup.sh >> /var/log/keystonedb/backup.log 2>&1

# Incremental backup: Every 4 hours
0 */4 * * * /opt/scripts/incremental-backup.sh >> /var/log/keystonedb/backup.log 2>&1

# Differential backup: Every 6 hours
0 */6 * * * /opt/scripts/differential-backup.sh >> /var/log/keystonedb/backup.log 2>&1

# Cleanup old backups: Daily at 3 AM (keep 7 days)
0 3 * * * find /var/backups/keystonedb -name "*.tar.gz" -mtime +7 -delete

# Verify backups: Daily at 4 AM
0 4 * * * /opt/scripts/verify-backup.sh >> /var/log/keystonedb/verify.log 2>&1
```

### Backup Retention Policy

Implement a retention policy to manage storage costs:

```bash
#!/bin/bash
# cleanup-backups.sh - Retention policy enforcement

set -euo pipefail

BACKUP_ROOT="/var/backups/keystonedb"

echo "Enforcing backup retention policy at $(date)"

# Keep full backups for 30 days
echo "Cleaning full backups older than 30 days..."
find "$BACKUP_ROOT" -name "backup-*.tar.gz" -mtime +30 -delete
find "$BACKUP_ROOT" -name "backup-*.tar.gz.sha256" -mtime +30 -delete

# Keep incremental backups for 7 days
echo "Cleaning incremental backups older than 7 days..."
find "$BACKUP_ROOT/incremental" -name "*.tar.gz" -mtime +7 -delete

# Keep differential backups for 14 days
echo "Cleaning differential backups older than 14 days..."
find "$BACKUP_ROOT/differential" -name "*.tar.gz" -mtime +14 -delete

# Report storage usage
echo "Current backup storage usage:"
du -sh "$BACKUP_ROOT"
echo "Backup count:"
find "$BACKUP_ROOT" -name "*.tar.gz" | wc -l
```

## Backup Verification

Always verify backups to ensure they can be restored when needed.

### Checksum Verification

```bash
#!/bin/bash
# verify-backup.sh - Verify backup integrity

set -euo pipefail

BACKUP_ROOT="/var/backups/keystonedb"

echo "Verifying backups at $(date)"

# Find all backup files
find "$BACKUP_ROOT" -name "*.tar.gz" | while read backup_file; do
    checksum_file="${backup_file}.sha256"

    if [ ! -f "$checksum_file" ]; then
        echo "WARNING: No checksum for $backup_file"
        continue
    fi

    echo "Verifying: $(basename "$backup_file")"

    # Verify checksum
    if sha256sum -c "$checksum_file" > /dev/null 2>&1; then
        echo "  ✓ Checksum valid"
    else
        echo "  ✗ CHECKSUM FAILED"
        # Alert on checksum failure
        echo "CRITICAL: Backup checksum failed: $backup_file" | \
            mail -s "Backup Verification Failed" admin@example.com
    fi
done

echo "Verification completed"
```

### Test Restore Verification

Periodically test restore procedures to ensure backups are usable:

```bash
#!/bin/bash
# test-restore.sh - Test backup restore procedure

set -euo pipefail

BACKUP_FILE="$1"
TEST_DIR="/tmp/keystonedb-restore-test-$$"

if [ -z "$BACKUP_FILE" ]; then
    echo "Usage: $0 <backup-file.tar.gz>"
    exit 1
fi

echo "Testing restore of: $BACKUP_FILE"
echo "Test directory: $TEST_DIR"

# Create test directory
mkdir -p "$TEST_DIR"

# Extract backup
echo "Extracting backup..."
tar xzf "$BACKUP_FILE" -C "$TEST_DIR"

# Verify database can be opened
echo "Attempting to open database..."
kstone-cli --db-path "$TEST_DIR" --command "stats" > /dev/null

if [ $? -eq 0 ]; then
    echo "✓ Restore test PASSED"
    echo "  Database opens successfully"
    echo "  Stats command executed successfully"
    RESULT=0
else
    echo "✗ Restore test FAILED"
    echo "  Database failed to open or execute commands"
    RESULT=1
fi

# Cleanup
rm -rf "$TEST_DIR"

exit $RESULT
```

## Restore Procedures

### Full Restore from Backup

Restore a database from a full backup:

```bash
#!/bin/bash
# restore.sh - Restore database from backup

set -euo pipefail

BACKUP_FILE="$1"
DB_DIR="/var/lib/keystonedb/data/production.keystone"
RESTORE_TIMESTAMP=$(date +%Y%m%d_%H%M%S)

if [ -z "$BACKUP_FILE" ]; then
    echo "Usage: $0 <backup-file.tar.gz>"
    exit 1
fi

echo "=== KeystoneDB Restore ==="
echo "Backup file: $BACKUP_FILE"
echo "Target database: $DB_DIR"
echo "Timestamp: $RESTORE_TIMESTAMP"
echo ""
read -p "This will overwrite the existing database. Continue? (yes/no): " confirm

if [ "$confirm" != "yes" ]; then
    echo "Restore cancelled"
    exit 0
fi

# Stop application
echo "Stopping application..."
sudo systemctl stop keystonedb

# Backup current database (safety measure)
if [ -d "$DB_DIR" ]; then
    SAFETY_BACKUP="$DB_DIR.before-restore-$RESTORE_TIMESTAMP"
    echo "Creating safety backup: $SAFETY_BACKUP"
    mv "$DB_DIR" "$SAFETY_BACKUP"
fi

# Create database directory
mkdir -p "$DB_DIR"

# Extract backup
echo "Extracting backup..."
tar xzf "$BACKUP_FILE" -C "$DB_DIR"

# Set ownership
echo "Setting ownership..."
chown -R keystonedb:keystonedb "$DB_DIR"
chmod 700 "$DB_DIR"

# Verify restore
echo "Verifying restored database..."
sudo -u keystonedb kstone-cli --db-path "$DB_DIR" --command "stats"

if [ $? -eq 0 ]; then
    echo "✓ Database verified successfully"

    # Start application
    echo "Starting application..."
    sudo systemctl start keystonedb

    # Wait for startup
    sleep 5

    # Check status
    sudo systemctl status keystonedb

    echo ""
    echo "Restore completed successfully at $(date)"
    echo "Safety backup available at: $SAFETY_BACKUP"
else
    echo "✗ Database verification failed"
    echo "Restore aborted. Original database preserved at: $SAFETY_BACKUP"
    exit 1
fi
```

### Incremental Restore

Restore from a full backup plus incremental backups:

```bash
#!/bin/bash
# restore-incremental.sh - Restore full + incremental backups

set -euo pipefail

FULL_BACKUP="$1"
DB_DIR="/var/lib/keystonedb/data/production.keystone"

if [ -z "$FULL_BACKUP" ]; then
    echo "Usage: $0 <full-backup.tar.gz> [incremental1.tar.gz] [incremental2.tar.gz] ..."
    exit 1
fi

echo "=== Incremental Restore ==="
echo "Full backup: $FULL_BACKUP"
echo "Incremental backups: ${@:2}"

# Stop application
sudo systemctl stop keystonedb

# Backup current database
SAFETY_BACKUP="$DB_DIR.before-restore-$(date +%s)"
mv "$DB_DIR" "$SAFETY_BACKUP" 2>/dev/null || true

# Restore full backup
echo "Restoring full backup..."
mkdir -p "$DB_DIR"
tar xzf "$FULL_BACKUP" -C "$DB_DIR"

# Apply incremental backups in order
for incremental in "${@:2}"; do
    echo "Applying incremental: $incremental"
    tar xzf "$incremental" -C "$DB_DIR"
done

# Set ownership
chown -R keystonedb:keystonedb "$DB_DIR"

# Verify and start
sudo -u keystonedb kstone-cli --db-path "$DB_DIR" --command "stats"
sudo systemctl start keystonedb

echo "Incremental restore completed"
```

## Point-in-Time Recovery

KeystoneDB's WAL enables recovery to a specific point in time.

### Understanding Point-in-Time Recovery

**How it works:**
1. Restore full backup (base state)
2. Apply incremental backups up to desired point
3. Replay WAL records to exact timestamp
4. Discard records after target point

**WAL-based recovery:**

```bash
#!/bin/bash
# point-in-time-restore.sh - Restore to specific timestamp

set -euo pipefail

FULL_BACKUP="$1"
TARGET_TIMESTAMP="$2"  # Unix timestamp
DB_DIR="/var/lib/keystonedb/data/production.keystone"

echo "Point-in-Time Restore to: $(date -d @$TARGET_TIMESTAMP)"

# Restore full backup
echo "Restoring base backup..."
sudo systemctl stop keystonedb
rm -rf "$DB_DIR"
mkdir -p "$DB_DIR"
tar xzf "$FULL_BACKUP" -C "$DB_DIR"

# Note: Actual WAL truncation requires database support
# This is a conceptual example

# The database would need to:
# 1. Read WAL and identify LSN at target timestamp
# 2. Truncate WAL after that LSN
# 3. Discard any SST files created after timestamp

echo "Point-in-time restore completed"
echo "Database restored to: $(date -d @$TARGET_TIMESTAMP)"
```

## Off-Site Backup

Store backups in remote locations for disaster recovery.

### S3 Backup

Upload backups to AWS S3:

```bash
#!/bin/bash
# s3-backup.sh - Upload backup to S3

set -euo pipefail

BACKUP_FILE="$1"
S3_BUCKET="s3://my-keystonedb-backups"
RETENTION_DAYS=90

if [ -z "$BACKUP_FILE" ]; then
    echo "Usage: $0 <backup-file.tar.gz>"
    exit 1
fi

echo "Uploading to S3: $BACKUP_FILE"

# Upload to S3 with server-side encryption
aws s3 cp "$BACKUP_FILE" "$S3_BUCKET/" \
    --storage-class STANDARD_IA \
    --server-side-encryption AES256 \
    --metadata "backup-date=$(date -u +%Y-%m-%d)"

# Upload checksum
aws s3 cp "${BACKUP_FILE}.sha256" "$S3_BUCKET/"

# Set lifecycle policy (auto-delete after retention period)
aws s3api put-object-tagging \
    --bucket "${S3_BUCKET#s3://}" \
    --key "$(basename $BACKUP_FILE)" \
    --tagging "TagSet=[{Key=retention,Value=${RETENTION_DAYS}days}]"

echo "Backup uploaded successfully"
```

### Encrypted Remote Backup

Encrypt backups before uploading:

```bash
#!/bin/bash
# encrypted-backup.sh - Encrypt and upload backup

set -euo pipefail

BACKUP_FILE="$1"
GPG_RECIPIENT="backup@example.com"
REMOTE_HOST="backup-server.example.com"
REMOTE_PATH="/backups/keystonedb"

echo "Encrypting backup: $BACKUP_FILE"

# Encrypt with GPG
gpg --encrypt --recipient "$GPG_RECIPIENT" "$BACKUP_FILE"

# Upload via SCP
scp "${BACKUP_FILE}.gpg" "${REMOTE_HOST}:${REMOTE_PATH}/"

# Verify upload
ssh "$REMOTE_HOST" "sha256sum ${REMOTE_PATH}/$(basename ${BACKUP_FILE}.gpg)"

echo "Encrypted backup uploaded"
```

### Restore from S3

```bash
#!/bin/bash
# s3-restore.sh - Download and restore from S3

set -euo pipefail

S3_PATH="$1"
LOCAL_FILE="/tmp/backup-restore-$$.tar.gz"

echo "Downloading from S3: $S3_PATH"

# Download backup
aws s3 cp "$S3_PATH" "$LOCAL_FILE"

# Download and verify checksum
aws s3 cp "${S3_PATH}.sha256" "${LOCAL_FILE}.sha256"
sha256sum -c "${LOCAL_FILE}.sha256"

# Restore
./restore.sh "$LOCAL_FILE"

# Cleanup
rm -f "$LOCAL_FILE" "${LOCAL_FILE}.sha256"
```

## Disaster Recovery Planning

### Disaster Recovery Checklist

**Before Disaster:**
- [ ] Regular automated backups (daily full, hourly incremental)
- [ ] Off-site backup storage (S3, remote server)
- [ ] Encrypted backups for sensitive data
- [ ] Documented restore procedures
- [ ] Tested restore process (monthly)
- [ ] Contact list for recovery team
- [ ] Backup of configuration files and scripts

**During Disaster:**
- [ ] Assess extent of data loss
- [ ] Identify most recent valid backup
- [ ] Notify stakeholders of recovery timeline
- [ ] Provision new infrastructure if needed
- [ ] Execute restore procedure
- [ ] Verify data integrity post-restore
- [ ] Resume operations

**After Disaster:**
- [ ] Document incident and timeline
- [ ] Review and improve recovery procedures
- [ ] Update backup frequency if needed
- [ ] Conduct post-mortem analysis

### Recovery Time Objective (RTO)

Plan for acceptable downtime:

| Backup Strategy | RTO | Complexity |
|----------------|-----|------------|
| Full backup only | 1-4 hours | Low |
| Full + Incremental | 30 min - 2 hours | Medium |
| Continuous replication | < 5 minutes | High |

### Recovery Point Objective (RPO)

Determine acceptable data loss:

| Backup Frequency | RPO | Storage Cost |
|-----------------|-----|--------------|
| Daily | 24 hours | Low |
| Every 6 hours | 6 hours | Medium |
| Hourly | 1 hour | Medium-High |
| Every 15 minutes | 15 minutes | High |
| Continuous | < 1 minute | Very High |

## Best Practices

1. **Test Restores Regularly**: Monthly test restores to verify backup integrity
2. **Automate Everything**: Use cron, systemd timers, or orchestration tools
3. **Monitor Backup Success**: Alert on failed backups immediately
4. **Store Backups Off-Site**: Protect against site-wide disasters
5. **Encrypt Sensitive Data**: Use GPG or S3 server-side encryption
6. **Document Procedures**: Maintain up-to-date restore runbooks
7. **Version Control Scripts**: Keep backup scripts in git
8. **Verify Checksums**: Always validate backup integrity
9. **Retain Multiple Versions**: Follow 3-2-1 rule (3 copies, 2 media types, 1 off-site)
10. **Plan for Growth**: Monitor backup size trends and adjust storage

With robust backup and recovery procedures in place, you can confidently operate KeystoneDB in production, knowing your data is protected against any failure scenario.

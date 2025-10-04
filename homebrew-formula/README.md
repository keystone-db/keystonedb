# KeystoneDB Homebrew Tap

Official Homebrew formulas for KeystoneDB.

## Installation

```bash
# Add tap
brew tap keystone-db/keystonedb

# Install CLI
brew install kstone

# Install server
brew install kstone-server
```

## Usage

### CLI

```bash
# Create database
kstone create mydb.keystone

# Put an item
kstone put mydb.keystone user#123 '{"name":"Alice","age":30}'

# Get an item
kstone get mydb.keystone user#123

# PartiQL query
kstone query mydb.keystone "SELECT * FROM items WHERE pk = 'user#123'"
```

### Server

```bash
# Start server manually
kstone-server --db-path /path/to/db.keystone --port 50051

# Start as a service (macOS)
brew services start kstone-server

# Stop service
brew services stop kstone-server

# View logs
tail -f /usr/local/var/log/kstone-server.log
```

## Updating

```bash
# Update tap
brew update

# Upgrade kstone
brew upgrade kstone
brew upgrade kstone-server
```

## Uninstalling

```bash
# Uninstall packages
brew uninstall kstone
brew uninstall kstone-server

# Remove tap
brew untap keystone-db/keystonedb
```

## Maintainer Notes

### Updating Formulas After Release

After creating a new release, update the formulas with new SHA256 checksums:

```bash
# Download release artifacts
VERSION=0.1.0
cd /tmp

# macOS ARM64
curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-aarch64-apple-darwin.tar.gz
shasum -a 256 kstone-aarch64-apple-darwin.tar.gz

# macOS x86_64
curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-x86_64-apple-darwin.tar.gz
shasum -a 256 kstone-x86_64-apple-darwin.tar.gz

# Linux ARM64
curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-aarch64-unknown-linux-gnu.tar.gz
shasum -a 256 kstone-aarch64-unknown-linux-gnu.tar.gz

# Linux x86_64
curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-x86_64-unknown-linux-gnu.tar.gz
shasum -a 256 kstone-x86_64-unknown-linux-gnu.tar.gz
```

Update the `sha256` values in `kstone.rb` and `kstone-server.rb` with the computed checksums.

### Testing Formulas Locally

```bash
# Install from local formula
brew install --build-from-source ./Formula/kstone.rb

# Test formula
brew test kstone

# Audit formula
brew audit --strict kstone
```

## Repository Setup

This directory should be pushed to a separate GitHub repository named `homebrew-keystonedb`:

```bash
# Create new repo
gh repo create keystone-db/homebrew-keystonedb --public

# Initialize and push
cd homebrew-formula
git init
git add .
git commit -m "Initial Homebrew formulas for KeystoneDB"
git branch -M main
git remote add origin https://github.com/keystone-db/homebrew-keystonedb.git
git push -u origin main
```

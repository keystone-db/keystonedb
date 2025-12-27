# KeystoneDB Release Process

This document describes the complete release process for KeystoneDB, including the core Rust crates, Python bindings, Node.js bindings, and Docker images.

## Table of Contents

- [Release Checklist](#release-checklist)
- [Automated Release Process](#automated-release-process)
- [Manual Release Process](#manual-release-process)
- [Language Bindings](#language-bindings)
- [Post-Release Verification](#post-release-verification)
- [Hotfix Process](#hotfix-process)
- [Rollback Process](#rollback-process)
- [Troubleshooting](#troubleshooting)

## Release Checklist

### Pre-Release

- [ ] All tests passing (`cargo test --workspace`)
- [ ] Documentation updated (README.md, CLAUDE.md)
- [ ] CHANGELOG.md updated with new features and fixes
- [ ] Version numbers consistent across:
  - [ ] `Cargo.toml` (workspace version)
  - [ ] `bindings/python/pyproject.toml`
  - [ ] `bindings/nodejs/package.json`
- [ ] All PRs merged and main branch is stable
- [ ] Local builds successful (`cargo build --release`)
- [ ] Binding tests pass:
  - [ ] Python: `cd bindings/python && maturin develop && pytest`
  - [ ] Node.js: `cd bindings/nodejs && npm install && npm run build && npm test`

### Required Secrets

Ensure these GitHub secrets are configured for automated publishing:

| Secret | Purpose | How to Get |
|--------|---------|------------|
| `CARGO_REGISTRY_TOKEN` | Publish to crates.io | https://crates.io/me |
| `PYPI_TOKEN` | Publish to PyPI | https://pypi.org/manage/account/token/ |
| `NPM_TOKEN` | Publish to npm | https://www.npmjs.com/settings/~/tokens |
| `DOCKERHUB_USERNAME` | Docker Hub login | https://hub.docker.com/settings/security |
| `DOCKERHUB_TOKEN` | Docker Hub password | https://hub.docker.com/settings/security |

## Automated Release Process

### 1. Using the Release Script (Recommended)

The release script handles everything automatically:

```bash
./scripts/release.sh 0.2.0
```

This script will:
1. Validate repository state (clean working directory, on main branch)
2. Check CHANGELOG.md has an entry for the version
3. Update version numbers in all files:
   - `Cargo.toml` (workspace version)
   - `bindings/python/pyproject.toml`
   - `bindings/nodejs/package.json`
4. Run all tests
5. Build release binaries
6. Commit changes with appropriate message
7. Create annotated git tag
8. Push to GitHub (triggers CI/CD)

### 2. GitHub Actions Workflow

When you push a tag, GitHub Actions automatically:

1. **Build Stage** - Multi-platform binary compilation:
   - Linux x86_64 (GNU)
   - macOS x86_64 (Intel)
   - macOS ARM64 (Apple Silicon)
   - Windows x86_64

2. **Release Stage** - Create GitHub Release:
   - Upload all platform binaries
   - Generate release notes from CHANGELOG.md
   - Include SHA256 checksums

3. **Publish Crates** - Publish to crates.io:
   - kstone-core
   - kstone-proto
   - kstone-api
   - kstone-client

4. **Publish Python** - Build and publish to PyPI:
   - Build wheels for all platforms
   - Upload to PyPI

5. **Publish Node.js** - Build and publish to npm:
   - Build native modules
   - Publish to npm registry

6. **Docker** - Build and push images:
   - `parkerdgabel/kstone:VERSION`
   - `parkerdgabel/kstone-server:VERSION`

## Manual Release Process

If you need to release manually:

### 1. Update Versions

```bash
# Update workspace version
sed -i 's/version = "0.1.0"/version = "0.2.0"/' Cargo.toml

# Update Python bindings
sed -i 's/version = "0.1.0"/version = "0.2.0"/' bindings/python/pyproject.toml

# Update Node.js bindings
node -e "
  const fs = require('fs');
  const pkg = JSON.parse(fs.readFileSync('bindings/nodejs/package.json'));
  pkg.version = '0.2.0';
  fs.writeFileSync('bindings/nodejs/package.json', JSON.stringify(pkg, null, 2));
"
```

### 2. Run Tests

```bash
cargo test --workspace
cd bindings/python && maturin develop && pytest
cd bindings/nodejs && npm install && npm run build && npm test
```

### 3. Commit and Tag

```bash
git add -A
git commit -m "Release version 0.2.0"
git tag -a v0.2.0 -m "Release 0.2.0"
git push origin main
git push origin v0.2.0
```

## Language Bindings

### Python Bindings

**Location:** `bindings/python/`

**Build System:** Maturin (PyO3)

**Local Development:**
```bash
cd bindings/python
pip install maturin pytest
maturin develop
pytest tests/ -v
```

**Manual Publishing:**
```bash
cd bindings/python
MATURIN_PYPI_TOKEN=<your-token> maturin publish
```

**Testing Installation:**
```bash
pip install keystonedb
python -c "import keystonedb; print(keystonedb.__version__)"
```

### Node.js Bindings

**Location:** `bindings/nodejs/`

**Build System:** NAPI-RS

**Local Development:**
```bash
cd bindings/nodejs
npm install
npm run build
npm test
```

**Manual Publishing:**
```bash
cd bindings/nodejs
npm login
npm publish --access public
```

**Testing Installation:**
```bash
npm install keystonedb
node -e "const db = require('keystonedb'); console.log('OK');"
```

### Binding Version Synchronization

The release script automatically updates versions in:
- `bindings/python/pyproject.toml`
- `bindings/nodejs/package.json`

To manually sync versions:
```bash
./scripts/release-bindings.sh --build-only 0.2.0
```

## Post-Release Verification

### 1. Verify GitHub Release

- [ ] Visit https://github.com/keystone-db/keystonedb/releases
- [ ] Verify all platform binaries are present
- [ ] Verify SHA256 checksums are present
- [ ] Test download and extraction

### 2. Verify crates.io

```bash
# Wait ~5 minutes for indexing
cargo search kstone-core
# Should show the new version

# Test installation
cargo new test-project && cd test-project
cargo add kstone-api@0.2.0
cargo build
```

### 3. Verify PyPI

```bash
# Wait ~5 minutes for indexing
pip install keystonedb==0.2.0
python -c "import keystonedb; print(keystonedb.__version__)"
```

### 4. Verify npm

```bash
# Usually available immediately
npm show keystonedb version
npm install keystonedb@0.2.0
node -e "const db = require('keystonedb'); console.log('OK');"
```

### 5. Verify Docker Images

```bash
docker pull parkerdgabel/kstone:0.2.0
docker run parkerdgabel/kstone:0.2.0 --version

docker pull parkerdgabel/kstone-server:0.2.0
docker run parkerdgabel/kstone-server:0.2.0 --version
```

### 6. Update Homebrew Tap (Manual)

```bash
VERSION=0.2.0

# Download binaries and compute checksums
curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-aarch64-apple-darwin.tar.gz
MACOS_ARM64_SHA=$(shasum -a 256 kstone-aarch64-apple-darwin.tar.gz | cut -d' ' -f1)

curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-x86_64-apple-darwin.tar.gz
MACOS_X64_SHA=$(shasum -a 256 kstone-x86_64-apple-darwin.tar.gz | cut -d' ' -f1)

# Update formulas with new version and checksums
# Then push to homebrew-keystonedb repo
```

### 7. Announce Release

- [ ] Create release announcement (blog post, social media)
- [ ] Update website with new version
- [ ] Notify community channels (Discord, Twitter, Reddit)

## Release Cadence

- **Major releases** (1.0, 2.0): Breaking changes, major new features
- **Minor releases** (0.1, 0.2): New features, non-breaking changes
- **Patch releases** (0.1.1, 0.1.2): Bug fixes, security patches

Target release schedule:
- **Patch releases**: As needed for critical bugs
- **Minor releases**: Monthly (during active development)
- **Major releases**: When API is stable and production-ready

## Versioning

KeystoneDB follows [Semantic Versioning](https://semver.org/):

- **MAJOR**: Incompatible API changes
- **MINOR**: New functionality (backwards-compatible)
- **PATCH**: Bug fixes (backwards-compatible)

Pre-1.0 releases:
- 0.x.y versions may include breaking changes in minor versions
- 1.0.0 will mark the first stable API

## Hotfix Process

For critical bugs in production:

```bash
# Create hotfix branch from tag
git checkout -b hotfix-0.2.1 v0.2.0

# Fix bug
git commit -am "Fix critical bug in X"

# Update version
sed -i 's/version = "0.2.0"/version = "0.2.1"/' Cargo.toml
sed -i 's/version = "0.2.0"/version = "0.2.1"/' bindings/python/pyproject.toml
# ... update other version files

git commit -am "Bump version to 0.2.1"

# Create tag
git tag -a v0.2.1 -m "Hotfix release 0.2.1"

# Push
git push origin hotfix-0.2.1
git push origin v0.2.1

# Merge back to main
git checkout main
git merge hotfix-0.2.1
git push origin main
```

## Rollback Process

If a release has critical issues:

1. **Delete the Git tag:**
   ```bash
   git tag -d v0.2.0
   git push origin :refs/tags/v0.2.0
   ```

2. **Delete the GitHub release** (via web interface)

3. **Yank crates.io versions:**
   ```bash
   cargo yank --version 0.2.0 kstone-core
   cargo yank --version 0.2.0 kstone-api
   # etc.
   ```

4. **Yank PyPI version:**
   ```bash
   pip install twine
   # Cannot delete, but can yank specific files
   ```

5. **Deprecate npm version:**
   ```bash
   npm deprecate keystonedb@0.2.0 "Critical bug, use 0.2.1"
   ```

6. **Fix issues and create new release with incremented version**

## Troubleshooting

### Build Fails on CI

- Check GitHub Actions logs for specific errors
- Ensure all dependencies are available
- Verify cross-compilation setup is correct
- Test locally with: `cargo build --target <target-triple>`

### Checksums Don't Match

- Re-download binary from GitHub release
- Compute checksum again: `shasum -a 256 <file>`
- Ensure you're downloading the correct file for the platform

### Python Wheel Build Fails

- Ensure Rust toolchain is installed
- Check maturin version compatibility
- Try: `pip install --upgrade maturin`
- For cross-compilation: `pip install ziglang`

### Node.js Build Fails

- Ensure node version >= 14
- Check napi-rs version compatibility
- Clear node_modules: `rm -rf node_modules && npm install`
- Check protobuf compiler is installed

### crates.io Publish Fails

- Verify `CARGO_REGISTRY_TOKEN` is set
- Check crate dependencies are published first
- Wait 30 seconds between publishes
- Verify you have ownership of the crates

### Docker Build Fails

- Check Dockerfile syntax
- Ensure all required files are present (not excluded by .dockerignore)
- Test local build: `docker build -t test .`
- Check Docker Hub credentials if push fails

## Resources

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Maturin Documentation](https://www.maturin.rs/)
- [NAPI-RS Documentation](https://napi.rs/)
- [crates.io Publishing Guide](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [PyPI Publishing Guide](https://packaging.python.org/tutorials/packaging-projects/)
- [npm Publishing Guide](https://docs.npmjs.com/packages-and-modules/contributing-packages-to-the-registry)
- [Docker Build Documentation](https://docs.docker.com/engine/reference/commandline/build/)
- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/)

# KeystoneDB Release Process

This document describes the release process for KeystoneDB and its language bindings.

## Table of Contents

- [Release Types](#release-types)
- [Prerequisites](#prerequisites)
- [Language Bindings Release](#language-bindings-release)
- [Core Database Release](#core-database-release)
- [Versioning Strategy](#versioning-strategy)
- [Troubleshooting](#troubleshooting)

## Release Types

KeystoneDB has two independent release tracks:

1. **Core Database** (`v*` tags) - Rust binaries, Docker images, core crates
2. **Language Bindings** (`bindings-v*` tags) - Python, JavaScript, Go, C FFI

These can be released independently since bindings are forward-compatible with the core database.

## Prerequisites

### For All Releases

- Write access to the repository
- Ability to create and push tags
- Clean working directory (`git status` should show no changes)

### For Bindings Releases

- **PyPI**: API token set as `PYPI_API_TOKEN` secret in GitHub
- **npm**: Access token set as `NPM_TOKEN` secret in GitHub
- **GitHub**: Automatic (uses `GITHUB_TOKEN`)

### For Core Releases

- **crates.io**: Token set as `CARGO_REGISTRY_TOKEN` secret
- **Docker Hub**: Credentials set as `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` secrets

---

## Language Bindings Release

Release language bindings (Go, Python, JavaScript, C FFI) to package registries.

### Step 1: Prepare the Release

1. **Ensure all tests pass**:
   ```bash
   # Run binding tests locally
   cd bindings/go/embedded && go test -v
   cd bindings/python/embedded && pytest test_smoke.py -v
   ```

2. **Update CHANGELOG** (if exists):
   ```bash
   # Document changes in bindings/CHANGELOG.md or similar
   ```

3. **Check current versions**:
   ```bash
   grep version bindings/python/embedded/pyproject.toml
   grep version bindings/javascript/client/package.json
   ```

### Step 2: Bump Version Numbers

Use the version bump script:

```bash
# Dry run (shows what will change)
./scripts/bump-bindings-version.sh 0.2.0

# Apply changes
./scripts/bump-bindings-version.sh 0.2.0 --commit

# Or apply changes and create tag in one step
./scripts/bump-bindings-version.sh 0.2.0 --commit --tag
```

The script updates:
- `bindings/python/embedded/pyproject.toml`
- `bindings/javascript/client/package.json`
- `bindings/BUILD_STATUS.md`

### Step 3: Create Pull Request (Optional)

For major releases, create a PR for review:

```bash
git checkout -b release/bindings-v0.2.0
git push origin release/bindings-v0.2.0
# Create PR on GitHub
```

### Step 4: Merge and Tag

If you created a PR:

```bash
# After PR is merged
git checkout main
git pull origin main
./scripts/bump-bindings-version.sh 0.2.0 --tag
git push origin bindings-v0.2.0
```

If you skipped the PR:

```bash
# Push commit and tag
git push origin main
git push origin bindings-v0.2.0
```

### Step 5: Monitor Release Workflow

1. Go to **Actions** tab on GitHub
2. Watch the **"Release Language Bindings"** workflow
3. Verify each job completes successfully:
   - ✅ Build C FFI libraries (4 platforms)
   - ✅ Build Python wheels (5+ platforms)
   - ✅ Publish to PyPI
   - ✅ Build JavaScript package
   - ✅ Publish to npm
   - ✅ Create GitHub Release
   - ✅ Verify published packages

Expected duration: **15-20 minutes**

### Step 6: Verify Published Packages

#### PyPI (Python)

```bash
# Wait a minute for propagation, then:
pip install keystonedb==0.2.0
python -c "import keystonedb; print(keystonedb.__version__)"
```

Check on [https://pypi.org/project/keystonedb/](https://pypi.org/project/keystonedb/)

#### npm (JavaScript)

```bash
npm install @keystonedb/client@0.2.0
node -e "console.log(require('@keystonedb/client'))"
```

Check on [https://www.npmjs.com/package/@keystonedb/client](https://www.npmjs.com/package/@keystonedb/client)

#### Go Modules

```bash
go get github.com/keystone-db/keystonedb/bindings/go/embedded@bindings-v0.2.0
go get github.com/keystone-db/keystonedb/bindings/go/client@bindings-v0.2.0
```

Check on [https://pkg.go.dev/github.com/keystone-db/keystonedb/bindings/go/embedded](https://pkg.go.dev/github.com/keystone-db/keystonedb/bindings/go/embedded)

#### GitHub Release

Verify artifacts at: `https://github.com/keystone-db/keystonedb/releases/tag/bindings-v0.2.0`

Should include:
- C FFI libraries (`.tar.gz` / `.zip` for each platform)
- Python wheels (`.whl` for each platform)
- JavaScript package (`.tgz`)
- SHA256 checksums

### Step 7: Announce Release

- Post on project blog / changelog
- Tweet / social media
- Update documentation links
- Notify users in Discord / Slack

---

## Core Database Release

Release the KeystoneDB core binaries, Docker images, and Rust crates.

### Step 1: Prepare the Release

1. **Ensure all tests pass**:
   ```bash
   cargo test --all
   cargo test -p kstone-tests
   ```

2. **Update version in Cargo.toml**:
   ```toml
   [workspace.package]
   version = "0.2.0"
   ```

3. **Update CHANGELOG.md**:
   - Document all changes since last release
   - Categorize: Features, Bug Fixes, Performance, Breaking Changes

4. **Build and test locally**:
   ```bash
   cargo build --release
   ./target/release/kstone --version
   ./target/release/kstone-server --help
   ```

### Step 2: Create Release Commit

```bash
git add Cargo.toml CHANGELOG.md
git commit -m "chore: release v0.2.0"
git push origin main
```

### Step 3: Create and Push Tag

```bash
git tag -a v0.2.0 -m "KeystoneDB v0.2.0

Features:
- Feature 1
- Feature 2

Bug Fixes:
- Fix 1
- Fix 2
"
git push origin v0.2.0
```

### Step 4: Monitor Release Workflow

1. Go to **Actions** tab on GitHub
2. Watch the **"Release"** workflow
3. Verify each job completes successfully:
   - ✅ Build binaries (6 platforms)
   - ✅ Create GitHub Release
   - ✅ Publish to crates.io
   - ✅ Build and push Docker images

Expected duration: **25-30 minutes**

### Step 5: Verify Published Artifacts

#### GitHub Release

Check: `https://github.com/keystone-db/keystonedb/releases/tag/v0.2.0`

Should include:
- `kstone-<platform>.tar.gz` (6 archives)
- `kstone-server-<platform>.tar.gz` (6 archives)
- SHA256 checksums

#### crates.io

```bash
cargo search kstone-core
cargo search kstone-api
```

Check on [https://crates.io/crates/kstone-core](https://crates.io/crates/kstone-core)

#### Docker Hub

```bash
docker pull parkerdgabel/kstone:0.2.0
docker pull parkerdgabel/kstone-server:0.2.0
```

Check on [https://hub.docker.com/r/parkerdgabel/kstone](https://hub.docker.com/r/parkerdgabel/kstone)

### Step 6: Announce Release

Same as bindings release.

---

## Versioning Strategy

KeystoneDB follows [Semantic Versioning](https://semver.org/):

- **MAJOR** version: Incompatible API changes
- **MINOR** version: Backwards-compatible functionality
- **PATCH** version: Backwards-compatible bug fixes

### Version Independence

- **Core and Bindings**: Can be released independently
- **Bindings Compatibility**: Bindings v0.2.x should work with core v0.1.x (forward-compatible)
- **Server Compatibility**: Older clients should work with newer servers (protocol versioning)

### Version Numbering

- Core database: `v0.2.0`
- Language bindings: `bindings-v0.2.0`

### When to Bump Versions

**MAJOR (0.x.0 → 1.0.0)**:
- Breaking changes to API
- Protocol incompatibilities
- Major architectural changes

**MINOR (0.1.x → 0.2.0)**:
- New features
- New language bindings
- New gRPC methods

**PATCH (0.1.0 → 0.1.1)**:
- Bug fixes
- Documentation updates
- Performance improvements

---

## Troubleshooting

### Release Workflow Fails

**Problem**: GitHub Actions workflow fails during release

**Solutions**:

1. **Check secrets**: Ensure `PYPI_API_TOKEN`, `NPM_TOKEN`, `CARGO_REGISTRY_TOKEN` are set
2. **Check permissions**: Verify `GITHUB_TOKEN` has write access
3. **Re-run failed jobs**: Use "Re-run failed jobs" button in Actions tab
4. **Delete and recreate tag** (if nothing was published):
   ```bash
   git tag -d bindings-v0.2.0
   git push origin :refs/tags/bindings-v0.2.0
   git tag -a bindings-v0.2.0 -m "..."
   git push origin bindings-v0.2.0
   ```

### PyPI Upload Fails

**Problem**: `twine upload` fails with authentication error

**Solutions**:

1. Verify `PYPI_API_TOKEN` is set correctly in GitHub Secrets
2. Ensure token has upload permissions
3. Check if version already exists on PyPI (can't overwrite)

To test locally:

```bash
pip install twine
twine upload --repository testpypi dist/*.whl
```

### npm Publish Fails

**Problem**: `npm publish` fails with authentication error

**Solutions**:

1. Verify `NPM_TOKEN` is set correctly in GitHub Secrets
2. Ensure you have publish permissions for `@keystonedb` scope
3. Check if version already exists (can't overwrite)

To test locally:

```bash
cd bindings/javascript/client
npm login
npm publish --dry-run
npm publish --access public
```

### Version Already Exists

**Problem**: "Version 0.2.0 already exists" error

**Solution**: You cannot re-publish the same version. Options:

1. **Patch release**: Bump to 0.2.1 and try again
2. **Yank and republish** (PyPI only, not recommended):
   ```bash
   # PyPI allows yanking (hiding) versions
   twine upload --skip-existing dist/*.whl
   ```

### Docker Build Fails

**Problem**: Docker image build fails in release workflow

**Solutions**:

1. Test Docker build locally:
   ```bash
   docker build -f Dockerfile -t kstone:test .
   docker build -f Dockerfile.server -t kstone-server:test .
   ```

2. Check Dockerfile for syntax errors
3. Ensure all dependencies are available

### Artifacts Missing from Release

**Problem**: GitHub release created but some artifacts missing

**Solutions**:

1. Check individual job logs in Actions tab
2. Re-run failed jobs
3. Manually upload missing artifacts:
   ```bash
   gh release upload bindings-v0.2.0 path/to/artifact.tar.gz
   ```

---

## Checklist Template

Use this checklist for each bindings release:

```markdown
## Bindings Release v0.X.0

- [ ] All tests passing locally
- [ ] CHANGELOG updated (if applicable)
- [ ] Version bumped with script
- [ ] PR created and reviewed (for major releases)
- [ ] Tag created and pushed
- [ ] Release workflow completed successfully
- [ ] PyPI package verified
- [ ] npm package verified
- [ ] Go modules accessible
- [ ] GitHub release created with all artifacts
- [ ] Documentation updated
- [ ] Release announced
```

---

## Questions?

- **GitHub Issues**: [https://github.com/keystone-db/keystonedb/issues](https://github.com/keystone-db/keystonedb/issues)
- **Discussions**: [https://github.com/keystone-db/keystonedb/discussions](https://github.com/keystone-db/keystonedb/discussions)

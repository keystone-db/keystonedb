# KeystoneDB Release Process

This document describes the release process for KeystoneDB.

## Release Checklist

### Pre-Release

- [ ] All tests passing (`cargo test --workspace`)
- [ ] Documentation updated (README.md, CLAUDE.md, etc.)
- [ ] CHANGELOG.md updated with new features and fixes
- [ ] Version bumped in `Cargo.toml`
- [ ] All PRs merged and main branch is stable
- [ ] Local builds successful (`cargo build --release`)

### Release Process

#### 1. Automated Release (Recommended)

Use the release helper script:

```bash
./scripts/release.sh 0.1.0
```

This script will:
- Validate repository state
- Run tests
- Update version numbers
- Create git tag
- Push to GitHub (triggers automated builds)

#### 2. Manual Release

If you need to release manually:

```bash
# Update version in Cargo.toml
sed -i 's/version = "0.0.0"/version = "0.1.0"/' Cargo.toml

# Run tests
cargo test --workspace

# Commit version bump
git add Cargo.toml
git commit -m "Release version 0.1.0"

# Create and push tag
git tag -a v0.1.0 -m "Release 0.1.0"
git push origin main
git push origin v0.1.0
```

### Post-Release

After GitHub Actions completes the build:

#### 1. Verify GitHub Release

- [ ] Visit https://github.com/keystone-db/keystonedb/releases
- [ ] Verify all platform binaries are present:
  - Linux x86_64 (GNU)
  - Linux x86_64 (MUSL)
  - Linux ARM64
  - macOS x86_64
  - macOS ARM64
  - Windows x86_64
- [ ] Verify SHA256 checksums are present for all binaries
- [ ] Test download and extraction of at least one binary

#### 2. Update Homebrew Tap

```bash
# Download binaries and compute checksums
VERSION=0.1.0

# macOS ARM64
curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-aarch64-apple-darwin.tar.gz
MACOS_ARM64_SHA=$(shasum -a 256 kstone-aarch64-apple-darwin.tar.gz | cut -d' ' -f1)

# macOS x86_64
curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-x86_64-apple-darwin.tar.gz
MACOS_X64_SHA=$(shasum -a 256 kstone-x86_64-apple-darwin.tar.gz | cut -d' ' -f1)

# Linux ARM64
curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-aarch64-unknown-linux-gnu.tar.gz
LINUX_ARM64_SHA=$(shasum -a 256 kstone-aarch64-unknown-linux-gnu.tar.gz | cut -d' ' -f1)

# Linux x86_64
curl -LO https://github.com/keystone-db/keystonedb/releases/download/v${VERSION}/kstone-x86_64-unknown-linux-gnu.tar.gz
LINUX_X64_SHA=$(shasum -a 256 kstone-x86_64-unknown-linux-gnu.tar.gz | cut -d' ' -f1)

# Update formulas with checksums
cd homebrew-formula/Formula

# Edit kstone.rb - replace REPLACE_WITH_ACTUAL_SHA256_* with computed values
# Edit kstone-server.rb - replace REPLACE_WITH_ACTUAL_SHA256_* with computed values

# Commit and push to homebrew-keystonedb repo
git add kstone.rb kstone-server.rb
git commit -m "Update formulas for v${VERSION}"
git push
```

#### 3. Test Homebrew Installation

```bash
# Test on macOS
brew tap keystone-db/keystonedb
brew install kstone
kstone --version

brew install kstone-server
kstone-server --version

# Uninstall for cleanup
brew uninstall kstone kstone-server
brew untap keystone-db/keystonedb
```

#### 4. Docker Images

If Docker Hub credentials are configured in GitHub secrets:

- [ ] Verify images are pushed to Docker Hub
- [ ] Test CLI image: `docker run parkerdgabel/kstone:0.1.0 --version`
- [ ] Test server image: `docker run parkerdgabel/kstone-server:0.1.0 --version`

If not automated, build and push manually:

```bash
# Build CLI image
docker build -t parkerdgabel/kstone:0.1.0 -t parkerdgabel/kstone:latest .
docker push parkerdgabel/kstone:0.1.0
docker push parkerdgabel/kstone:latest

# Build server image
docker build -f Dockerfile.server -t parkerdgabel/kstone-server:0.1.0 -t parkerdgabel/kstone-server:latest .
docker push parkerdgabel/kstone-server:0.1.0
docker push parkerdgabel/kstone-server:latest
```

#### 5. Announce Release

- [ ] Create release announcement (blog post, social media, etc.)
- [ ] Update website with new version
- [ ] Notify community channels (Discord, Twitter, Reddit, etc.)

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
git checkout -b hotfix-0.1.1 v0.1.0

# Fix bug
git commit -am "Fix critical bug in X"

# Update version
sed -i 's/version = "0.1.0"/version = "0.1.1"/' Cargo.toml
git commit -am "Bump version to 0.1.1"

# Create tag
git tag -a v0.1.1 -m "Hotfix release 0.1.1"

# Push
git push origin hotfix-0.1.1
git push origin v0.1.1

# Merge back to main
git checkout main
git merge hotfix-0.1.1
git push origin main
```

## Rollback Process

If a release has critical issues:

1. Delete the Git tag:
   ```bash
   git tag -d v0.1.0
   git push origin :refs/tags/v0.1.0
   ```

2. Delete the GitHub release (via web interface)

3. Fix issues and create new release with incremented version

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

### Homebrew Formula Fails

- Test formula locally: `brew install --build-from-source ./Formula/kstone.rb`
- Check formula syntax: `brew audit --strict kstone`
- Verify URLs are accessible
- Ensure SHA256 checksums are correct

### Docker Build Fails

- Check Dockerfile syntax
- Ensure all required files are present (not excluded by .dockerignore)
- Test local build: `docker build -t test .`
- Check Docker Hub credentials if push fails

## Resources

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Homebrew Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- [Docker Build Documentation](https://docs.docker.com/engine/reference/commandline/build/)
- [Semantic Versioning](https://semver.org/)
- [Keep a Changelog](https://keepachangelog.com/)

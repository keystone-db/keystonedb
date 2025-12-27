#!/usr/bin/env bash
#
# KeystoneDB Release Helper Script
#
# This script automates the release process for KeystoneDB.
# It performs the following tasks:
# 1. Validates the repository state
# 2. Runs tests
# 3. Updates version numbers (Cargo.toml, Python, Node.js)
# 4. Updates CHANGELOG.md
# 5. Creates git tag
# 6. Pushes to trigger GitHub Actions
#
# Usage: ./scripts/release.sh <version>
# Example: ./scripts/release.sh 0.1.0

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Functions
error() {
    echo -e "${RED}ERROR: $1${NC}" >&2
    exit 1
}

info() {
    echo -e "${GREEN}INFO: $1${NC}"
}

warn() {
    echo -e "${YELLOW}WARN: $1${NC}"
}

step() {
    echo -e "${BLUE}==>${NC} $1"
}

confirm() {
    read -p "$1 (y/N): " -n 1 -r
    echo
    [[ $REPLY =~ ^[Yy]$ ]]
}

# Check arguments
if [ $# -ne 1 ]; then
    error "Usage: $0 <version>\nExample: $0 0.1.0"
fi

VERSION=$1

# Validate version format (semver)
if ! [[ $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9]+)?$ ]]; then
    error "Invalid version format: $VERSION\nExpected format: X.Y.Z or X.Y.Z-suffix"
fi

TAG="v${VERSION}"

echo ""
echo "=============================================="
echo "  KeystoneDB Release Process"
echo "  Version: ${VERSION}"
echo "=============================================="
echo ""

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ] || [ ! -d "kstone-core" ]; then
    error "Must run from repository root directory"
fi

# Check if git is clean
if ! git diff-index --quiet HEAD --; then
    error "Working directory is not clean. Commit or stash changes first."
fi

# Check if on main branch
CURRENT_BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$CURRENT_BRANCH" != "main" ]; then
    warn "Not on main branch (currently on: $CURRENT_BRANCH)"
    if ! confirm "Continue anyway?"; then
        exit 1
    fi
fi

# Check if tag already exists
if git rev-parse "$TAG" >/dev/null 2>&1; then
    error "Tag $TAG already exists"
fi

# ============================================================================
# Pre-release checks
# ============================================================================

step "Pre-release validation..."

# Check CHANGELOG has entry for this version
if [ -f "CHANGELOG.md" ]; then
    if ! grep -q "## \[${VERSION}\]" CHANGELOG.md; then
        warn "No CHANGELOG.md entry found for version ${VERSION}"
        if ! confirm "Continue without changelog entry?"; then
            echo ""
            echo "Please update CHANGELOG.md with:"
            echo ""
            echo "## [${VERSION}] - $(date +%Y-%m-%d)"
            echo ""
            echo "### Added"
            echo "- ..."
            echo ""
            exit 1
        fi
    else
        info "CHANGELOG.md entry found for ${VERSION}"
    fi
fi

# ============================================================================
# Update versions
# ============================================================================

step "Updating version numbers..."

# Update workspace version in Cargo.toml
info "Updating Cargo.toml workspace version to ${VERSION}..."
sed -i.bak "s/^version = \".*\"$/version = \"${VERSION}\"/" Cargo.toml
rm -f Cargo.toml.bak

# Update Python bindings version
if [ -f "bindings/python/pyproject.toml" ]; then
    info "Updating Python bindings version to ${VERSION}..."
    sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" bindings/python/pyproject.toml
    rm -f bindings/python/pyproject.toml.bak
fi

# Update Node.js bindings version
if [ -f "bindings/nodejs/package.json" ]; then
    info "Updating Node.js bindings version to ${VERSION}..."
    node -e "
        const fs = require('fs');
        const pkg = JSON.parse(fs.readFileSync('bindings/nodejs/package.json', 'utf8'));
        pkg.version = '${VERSION}';
        fs.writeFileSync('bindings/nodejs/package.json', JSON.stringify(pkg, null, 2) + '\n');
    "
fi

# Update Homebrew formulas (if they exist)
if [ -f "homebrew-formula/Formula/kstone.rb" ]; then
    info "Updating Homebrew formula versions..."
    sed -i.bak "s/version \".*\"$/version \"${VERSION}\"/" homebrew-formula/Formula/kstone.rb
    rm -f homebrew-formula/Formula/kstone.rb.bak
fi
if [ -f "homebrew-formula/Formula/kstone-server.rb" ]; then
    sed -i.bak "s/version \".*\"$/version \"${VERSION}\"/" homebrew-formula/Formula/kstone-server.rb
    rm -f homebrew-formula/Formula/kstone-server.rb.bak
fi

# ============================================================================
# Run tests
# ============================================================================

step "Running tests..."
if ! cargo test --workspace; then
    error "Tests failed. Fix issues before releasing."
fi

# ============================================================================
# Build release binaries
# ============================================================================

step "Building release binaries..."
if ! cargo build --release --bin kstone --bin kstone-server; then
    error "Release build failed"
fi

# ============================================================================
# Show changes
# ============================================================================

step "Changes to be committed:"
echo ""
git diff --stat
echo ""
git diff Cargo.toml

if [ -f "bindings/python/pyproject.toml" ]; then
    echo ""
    git diff bindings/python/pyproject.toml
fi

if [ -f "bindings/nodejs/package.json" ]; then
    echo ""
    git diff bindings/nodejs/package.json
fi

# ============================================================================
# Confirm release
# ============================================================================

echo ""
echo "=============================================="
info "Ready to release version ${VERSION}"
echo "=============================================="
echo ""
echo "This will:"
echo "  1. Commit version changes to:"
echo "     - Cargo.toml (workspace version)"
echo "     - bindings/python/pyproject.toml"
echo "     - bindings/nodejs/package.json"
echo "  2. Create annotated tag ${TAG}"
echo "  3. Push to GitHub (triggers release workflow)"
echo ""
echo "The GitHub Actions workflow will:"
echo "  - Build binaries for all platforms"
echo "  - Create GitHub release with artifacts"
echo "  - Publish to crates.io (Rust crates)"
echo "  - Publish to PyPI (Python wheels)"
echo "  - Publish to npm (Node.js package)"
echo "  - Build and push Docker images"
echo ""

if ! confirm "Continue with release?"; then
    info "Release cancelled. Reverting changes..."
    git checkout Cargo.toml
    git checkout bindings/python/pyproject.toml 2>/dev/null || true
    git checkout bindings/nodejs/package.json 2>/dev/null || true
    git checkout homebrew-formula/ 2>/dev/null || true
    exit 1
fi

# ============================================================================
# Commit and tag
# ============================================================================

step "Committing version bump..."
git add Cargo.toml
git add bindings/python/pyproject.toml 2>/dev/null || true
git add bindings/nodejs/package.json 2>/dev/null || true
git add homebrew-formula/ 2>/dev/null || true
git commit -m "Release version ${VERSION}

- Update workspace version to ${VERSION}
- Update Python bindings to ${VERSION}
- Update Node.js bindings to ${VERSION}"

step "Creating tag ${TAG}..."
git tag -a "$TAG" -m "Release ${VERSION}

KeystoneDB v${VERSION}

See CHANGELOG.md for details."

# ============================================================================
# Push to GitHub
# ============================================================================

step "Pushing to GitHub..."
git push origin "$CURRENT_BRANCH"
git push origin "$TAG"

# ============================================================================
# Success!
# ============================================================================

echo ""
echo "=============================================="
info "Release ${VERSION} initiated successfully!"
echo "=============================================="
echo ""
echo "GitHub Actions will now:"
echo "  1. Build binaries for Linux, macOS, and Windows"
echo "  2. Create GitHub release with all artifacts"
echo "  3. Publish Rust crates to crates.io"
echo "  4. Publish Python package to PyPI"
echo "  5. Publish Node.js package to npm"
echo "  6. Build and push Docker images"
echo ""
echo "Monitor progress at:"
echo "  https://github.com/keystone-db/keystonedb/actions"
echo ""
echo "Release will be available at:"
echo "  https://github.com/keystone-db/keystonedb/releases/tag/${TAG}"
echo ""
echo "Post-release checklist:"
echo "  [ ] Verify all CI jobs pass"
echo "  [ ] Check GitHub release has all platform binaries"
echo "  [ ] Verify crates.io: https://crates.io/crates/kstone-core"
echo "  [ ] Verify PyPI: https://pypi.org/project/keystonedb/"
echo "  [ ] Verify npm: https://www.npmjs.com/package/keystonedb"
echo "  [ ] Test Docker: docker pull parkerdgabel/kstone:${VERSION}"
echo "  [ ] Update Homebrew tap with SHA256 checksums"
echo "  [ ] Announce release on social media / Discord"
echo ""

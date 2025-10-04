#!/usr/bin/env bash
#
# KeystoneDB Release Helper Script
#
# This script automates the release process for KeystoneDB.
# It performs the following tasks:
# 1. Validates the repository state
# 2. Runs tests
# 3. Updates version numbers
# 4. Creates git tag
# 5. Pushes to trigger GitHub Actions
#
# Usage: ./scripts/release.sh <version>
# Example: ./scripts/release.sh 0.1.0

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
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

info "Starting release process for version ${VERSION}"

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

# Update version in Cargo.toml
info "Updating version in Cargo.toml files..."
sed -i.bak "s/^version = \".*\"$/version = \"${VERSION}\"/" Cargo.toml
rm Cargo.toml.bak

# Update Homebrew formulas
info "Updating version in Homebrew formulas..."
if [ -f "homebrew-formula/Formula/kstone.rb" ]; then
    sed -i.bak "s/version \".*\"$/version \"${VERSION}\"/" homebrew-formula/Formula/kstone.rb
    rm homebrew-formula/Formula/kstone.rb.bak
fi
if [ -f "homebrew-formula/Formula/kstone-server.rb" ]; then
    sed -i.bak "s/version \".*\"$/version \"${VERSION}\"/" homebrew-formula/Formula/kstone-server.rb
    rm homebrew-formula/Formula/kstone-server.rb.bak
fi

# Run tests
info "Running tests..."
if ! cargo test --workspace; then
    error "Tests failed. Fix issues before releasing."
fi

# Build release binaries to verify
info "Building release binaries..."
if ! cargo build --release --bin kstone --bin kstone-server; then
    error "Release build failed"
fi

# Show what will be committed
info "Changes to be committed:"
git diff Cargo.toml
if [ -f "homebrew-formula/Formula/kstone.rb" ]; then
    git diff homebrew-formula/Formula/kstone.rb
fi
if [ -f "homebrew-formula/Formula/kstone-server.rb" ]; then
    git diff homebrew-formula/Formula/kstone-server.rb
fi

# Confirm release
echo ""
info "Ready to release version ${VERSION}"
echo "This will:"
echo "  1. Commit version changes"
echo "  2. Create tag ${TAG}"
echo "  3. Push to GitHub (triggers release workflow)"
echo ""

if ! confirm "Continue with release?"; then
    info "Release cancelled"
    git checkout Cargo.toml homebrew-formula/ 2>/dev/null || true
    exit 1
fi

# Commit changes
info "Committing version bump..."
git add Cargo.toml homebrew-formula/ 2>/dev/null || true
git commit -m "Release version ${VERSION}"

# Create tag
info "Creating tag ${TAG}..."
git tag -a "$TAG" -m "Release ${VERSION}"

# Push
info "Pushing to GitHub..."
git push origin "$CURRENT_BRANCH"
git push origin "$TAG"

info "✓ Release ${VERSION} initiated!"
echo ""
echo "Next steps:"
echo "  1. GitHub Actions will build binaries for all platforms"
echo "  2. Release will be created at: https://github.com/keystone-db/keystonedb/releases/tag/${TAG}"
echo "  3. Update Homebrew formulas with SHA256 checksums from release"
echo "  4. Build and push Docker images (if not automated)"
echo ""
echo "Monitor progress at:"
echo "  https://github.com/keystone-db/keystonedb/actions"

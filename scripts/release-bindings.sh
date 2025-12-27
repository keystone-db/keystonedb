#!/usr/bin/env bash
#
# KeystoneDB Bindings Release Helper Script
#
# This script automates building and publishing language bindings.
# It can build and publish Python and Node.js bindings.
#
# Usage: ./scripts/release-bindings.sh [options] <version>
#
# Options:
#   --python-only    Only build/publish Python bindings
#   --nodejs-only    Only build/publish Node.js bindings
#   --build-only     Only build, don't publish
#   --dry-run        Show what would be done without executing
#
# Examples:
#   ./scripts/release-bindings.sh 0.1.0
#   ./scripts/release-bindings.sh --python-only 0.1.0
#   ./scripts/release-bindings.sh --build-only 0.1.0

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default options
PYTHON=true
NODEJS=true
BUILD_ONLY=false
DRY_RUN=false
VERSION=""

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

run() {
    if [ "$DRY_RUN" = true ]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} $*"
    else
        "$@"
    fi
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --python-only)
            PYTHON=true
            NODEJS=false
            shift
            ;;
        --nodejs-only)
            PYTHON=false
            NODEJS=true
            shift
            ;;
        --build-only)
            BUILD_ONLY=true
            shift
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [options] <version>"
            echo ""
            echo "Options:"
            echo "  --python-only    Only build/publish Python bindings"
            echo "  --nodejs-only    Only build/publish Node.js bindings"
            echo "  --build-only     Only build, don't publish"
            echo "  --dry-run        Show what would be done without executing"
            echo ""
            echo "Examples:"
            echo "  $0 0.1.0"
            echo "  $0 --python-only 0.1.0"
            exit 0
            ;;
        *)
            if [ -z "$VERSION" ]; then
                VERSION=$1
            else
                error "Unknown argument: $1"
            fi
            shift
            ;;
    esac
done

# Validate version
if [ -z "$VERSION" ]; then
    error "Version is required\nUsage: $0 [options] <version>"
fi

if ! [[ $VERSION =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9]+)?$ ]]; then
    error "Invalid version format: $VERSION\nExpected format: X.Y.Z or X.Y.Z-suffix"
fi

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ] || [ ! -d "bindings" ]; then
    error "Must run from repository root directory"
fi

info "Building language bindings for version ${VERSION}"
echo ""

# ============================================================================
# Python Bindings
# ============================================================================

if [ "$PYTHON" = true ]; then
    step "Building Python bindings..."

    if ! command -v maturin &> /dev/null; then
        warn "maturin not found, installing..."
        run pip install maturin
    fi

    cd bindings/python

    # Update version in pyproject.toml
    step "Updating Python package version to ${VERSION}..."
    if [ "$DRY_RUN" = false ]; then
        sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" pyproject.toml
        rm -f pyproject.toml.bak
    else
        echo "[DRY-RUN] Would update version in pyproject.toml"
    fi

    # Build wheel
    step "Building Python wheel..."
    run maturin build --release

    # Run tests
    step "Running Python tests..."
    if [ -d "tests" ] && [ -f "tests/test_keystonedb.py" ]; then
        run pip install -e . --quiet 2>/dev/null || true
        run python -m pytest tests/ -v || warn "Python tests failed or pytest not installed"
    else
        warn "No Python tests found, skipping"
    fi

    # Publish to PyPI
    if [ "$BUILD_ONLY" = false ]; then
        step "Publishing to PyPI..."
        if [ -n "${MATURIN_PYPI_TOKEN:-}" ]; then
            run maturin publish --skip-existing
        else
            warn "MATURIN_PYPI_TOKEN not set, skipping PyPI publish"
            warn "To publish manually: cd bindings/python && maturin publish"
        fi
    fi

    cd ../..
    info "Python bindings complete!"
    echo ""
fi

# ============================================================================
# Node.js Bindings
# ============================================================================

if [ "$NODEJS" = true ]; then
    step "Building Node.js bindings..."

    if ! command -v node &> /dev/null; then
        error "Node.js is required for building Node.js bindings"
    fi

    if ! command -v npm &> /dev/null; then
        error "npm is required for building Node.js bindings"
    fi

    cd bindings/nodejs

    # Update version in package.json
    step "Updating Node.js package version to ${VERSION}..."
    if [ "$DRY_RUN" = false ]; then
        # Use node to update package.json properly
        node -e "
            const fs = require('fs');
            const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
            pkg.version = '${VERSION}';
            fs.writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');
        "
    else
        echo "[DRY-RUN] Would update version in package.json"
    fi

    # Install dependencies
    step "Installing Node.js dependencies..."
    run npm install

    # Build native module
    step "Building native Node.js module..."
    run npm run build

    # Run tests
    step "Running Node.js tests..."
    if [ -f "test/test.js" ]; then
        run npm test || warn "Node.js tests failed"
    else
        warn "No Node.js tests found, skipping"
    fi

    # Publish to npm
    if [ "$BUILD_ONLY" = false ]; then
        step "Publishing to npm..."
        if [ -n "${NODE_AUTH_TOKEN:-}" ] || [ -n "${NPM_TOKEN:-}" ]; then
            run npm publish --access public
        else
            warn "NODE_AUTH_TOKEN/NPM_TOKEN not set, skipping npm publish"
            warn "To publish manually: cd bindings/nodejs && npm publish"
        fi
    fi

    cd ../..
    info "Node.js bindings complete!"
    echo ""
fi

# ============================================================================
# Summary
# ============================================================================

echo ""
echo "=============================================="
info "Bindings release for v${VERSION} complete!"
echo "=============================================="
echo ""

if [ "$BUILD_ONLY" = true ]; then
    echo "Build-only mode was enabled. To publish:"
    echo ""
    if [ "$PYTHON" = true ]; then
        echo "  Python:"
        echo "    cd bindings/python"
        echo "    MATURIN_PYPI_TOKEN=<token> maturin publish"
        echo ""
    fi
    if [ "$NODEJS" = true ]; then
        echo "  Node.js:"
        echo "    cd bindings/nodejs"
        echo "    npm login"
        echo "    npm publish --access public"
        echo ""
    fi
fi

echo "Artifacts:"
if [ "$PYTHON" = true ]; then
    echo "  - Python wheels: bindings/python/target/wheels/"
fi
if [ "$NODEJS" = true ]; then
    echo "  - Node.js module: bindings/nodejs/keystonedb.*.node"
fi

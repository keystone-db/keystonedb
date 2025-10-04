#!/usr/bin/env bash
#
# Publish KeystoneDB crates to crates.io
#
# This script publishes crates in the correct dependency order.
# Run after creating a release tag.
#
# Usage: ./scripts/publish-crates.sh [--dry-run]

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}INFO: $1${NC}"
}

warn() {
    echo -e "${YELLOW}WARN: $1${NC}"
}

error() {
    echo -e "${RED}ERROR: $1${NC}" >&2
    exit 1
}

# Check if dry-run mode
DRY_RUN=""
if [ "${1:-}" = "--dry-run" ]; then
    DRY_RUN="--dry-run"
    info "Running in DRY RUN mode - no actual publishing"
fi

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ] || [ ! -d "kstone-core" ]; then
    error "Must run from repository root directory"
fi

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    error "cargo not found. Please install Rust."
fi

# Verify we're logged in to crates.io (only if not dry-run)
if [ -z "$DRY_RUN" ]; then
    if ! cargo login --help &> /dev/null; then
        error "cargo login not available"
    fi
    info "Verifying crates.io authentication..."
    # This will fail if not logged in
    if ! cargo owner --list kstone-core 2>/dev/null | grep -q "users" 2>/dev/null; then
        warn "Not verified as owner of kstone-core. Continuing anyway..."
    fi
fi

# Function to publish a crate
publish_crate() {
    local crate_path=$1
    local crate_name=$(basename "$crate_path")

    info "Publishing $crate_name..."

    cd "$crate_path"

    # Verify the crate builds
    if ! cargo build --release; then
        error "$crate_name failed to build"
    fi

    # Publish
    if ! cargo publish $DRY_RUN; then
        error "Failed to publish $crate_name"
    fi

    cd - > /dev/null

    # Wait a bit for crates.io to process (only if not dry-run)
    if [ -z "$DRY_RUN" ]; then
        info "Waiting for crates.io to process $crate_name..."
        sleep 30
    fi
}

info "Starting crates.io publication process"
echo ""

# Publish in dependency order
# 1. Core library (no dependencies)
publish_crate "kstone-core"

# 2. Proto definitions (no internal dependencies)
publish_crate "kstone-proto"

# 3. API (depends on kstone-core)
publish_crate "kstone-api"

# 4. Client (depends on kstone-proto and kstone-core)
publish_crate "kstone-client"

info "✓ All crates published successfully!"
echo ""
echo "Published crates:"
echo "  - kstone-core"
echo "  - kstone-proto"
echo "  - kstone-api"
echo "  - kstone-client"
echo ""
echo "Not published (binaries/tests):"
echo "  - kstone-cli (binary)"
echo "  - kstone-server (binary)"
echo "  - kstone-tests (tests)"
echo ""
echo "Crates should appear at:"
echo "  https://crates.io/crates/kstone-core"
echo "  https://crates.io/crates/kstone-proto"
echo "  https://crates.io/crates/kstone-api"
echo "  https://crates.io/crates/kstone-client"

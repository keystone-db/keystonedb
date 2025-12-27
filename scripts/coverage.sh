#!/bin/bash
# Code coverage script for KeystoneDB
#
# Prerequisites:
#   cargo install cargo-tarpaulin
#   cargo install cargo-llvm-cov  # Alternative
#
# Usage:
#   ./scripts/coverage.sh          # Full HTML coverage report
#   ./scripts/coverage.sh quick    # Quick stdout summary
#   ./scripts/coverage.sh minimum  # CI check with fail threshold
#   ./scripts/coverage.sh llvm     # Use llvm-cov instead

set -e

MODE="${1:-default}"

case "$MODE" in
    quick)
        echo "Running quick coverage check..."
        cargo tarpaulin --config .tarpaulin.toml --profile quick
        ;;

    minimum)
        echo "Running minimum coverage check for CI..."
        cargo tarpaulin --config .tarpaulin.toml --profile minimum
        ;;

    llvm)
        echo "Running coverage with llvm-cov..."
        if ! command -v cargo-llvm-cov &> /dev/null; then
            echo "Installing cargo-llvm-cov..."
            cargo install cargo-llvm-cov
        fi
        cargo llvm-cov --workspace --exclude kstone-tests --exclude c-ffi --html
        echo "HTML report: target/llvm-cov/html/index.html"
        ;;

    default|full)
        echo "Running full coverage analysis..."
        cargo tarpaulin --config .tarpaulin.toml
        echo ""
        echo "HTML report: target/coverage/tarpaulin-report.html"
        echo "LCOV file: target/coverage/lcov.info"
        ;;

    ci)
        echo "Running CI coverage (JSON output)..."
        cargo tarpaulin --config .tarpaulin.toml --profile minimum --out Json
        ;;

    *)
        echo "Usage: $0 [quick|minimum|llvm|full|ci]"
        echo ""
        echo "Modes:"
        echo "  quick    - Fast stdout summary"
        echo "  minimum  - CI check with fail threshold"
        echo "  llvm     - Use llvm-cov instead of tarpaulin"
        echo "  full     - Full HTML and LCOV reports (default)"
        echo "  ci       - JSON output for CI integration"
        exit 1
        ;;
esac

#!/bin/bash
# Fuzz testing script for KeystoneDB
#
# Prerequisites:
#   cargo install cargo-fuzz
#   rustup default nightly  # cargo-fuzz requires nightly
#
# Usage:
#   ./scripts/fuzz.sh list              # List available fuzz targets
#   ./scripts/fuzz.sh <target>          # Run a fuzz target
#   ./scripts/fuzz.sh <target> <time>   # Run for specific duration (e.g., 60s, 5m, 1h)
#   ./scripts/fuzz.sh all               # Run all targets for 1 minute each
#   ./scripts/fuzz.sh corpus <target>   # Show corpus statistics

set -e

FUZZ_DIR="fuzz"

# Check if cargo-fuzz is installed
check_cargo_fuzz() {
    if ! command -v cargo-fuzz &> /dev/null; then
        echo "Error: cargo-fuzz not installed"
        echo "Install with: cargo install cargo-fuzz"
        echo "Note: Requires nightly Rust: rustup default nightly"
        exit 1
    fi
}

# List available fuzz targets
list_targets() {
    echo "Available fuzz targets:"
    echo "  - fuzz_put_get          : Database put/get operations"
    echo "  - fuzz_expression_parser: Condition expression parsing"
    echo "  - fuzz_key_encoding     : Key encode/decode roundtrip"
    echo "  - fuzz_value_serialization: Value type handling"
    echo "  - fuzz_partiql          : PartiQL parser"
}

# Run a fuzz target
run_target() {
    local target=$1
    local duration=${2:-60}  # Default 60 seconds

    check_cargo_fuzz

    echo "Running fuzz target: $target for $duration seconds..."
    cd "$FUZZ_DIR"
    cargo +nightly fuzz run "$target" -- -max_total_time="$duration"
}

# Run all targets
run_all() {
    local duration=${1:-60}

    check_cargo_fuzz

    targets=("fuzz_put_get" "fuzz_expression_parser" "fuzz_key_encoding" "fuzz_value_serialization" "fuzz_partiql")

    for target in "${targets[@]}"; do
        echo ""
        echo "=========================================="
        echo "Running: $target"
        echo "=========================================="
        cd "$FUZZ_DIR"
        cargo +nightly fuzz run "$target" -- -max_total_time="$duration" || true
        cd ..
    done

    echo ""
    echo "All fuzz targets completed!"
}

# Show corpus statistics
show_corpus() {
    local target=$1

    corpus_dir="$FUZZ_DIR/corpus/$target"
    if [ -d "$corpus_dir" ]; then
        count=$(find "$corpus_dir" -type f | wc -l)
        size=$(du -sh "$corpus_dir" 2>/dev/null | cut -f1)
        echo "Corpus for $target:"
        echo "  Files: $count"
        echo "  Size: $size"
    else
        echo "No corpus found for $target"
    fi
}

case "${1:-help}" in
    list)
        list_targets
        ;;

    all)
        run_all "${2:-60}"
        ;;

    corpus)
        if [ -z "$2" ]; then
            echo "Usage: $0 corpus <target>"
            exit 1
        fi
        show_corpus "$2"
        ;;

    help|--help|-h)
        echo "Usage: $0 <command> [args]"
        echo ""
        echo "Commands:"
        echo "  list              - List available fuzz targets"
        echo "  <target> [time]   - Run specific target (default 60s)"
        echo "  all [time]        - Run all targets (default 60s each)"
        echo "  corpus <target>   - Show corpus statistics"
        echo ""
        echo "Examples:"
        echo "  $0 list"
        echo "  $0 fuzz_put_get"
        echo "  $0 fuzz_put_get 300"
        echo "  $0 all 120"
        echo ""
        echo "Note: Requires nightly Rust and cargo-fuzz"
        ;;

    *)
        run_target "$1" "${2:-60}"
        ;;
esac

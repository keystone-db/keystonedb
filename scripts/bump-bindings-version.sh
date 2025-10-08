#!/bin/bash

# KeystoneDB Language Bindings Version Bump Script
#
# Updates version numbers across all language bindings consistently.
#
# Usage: ./scripts/bump-bindings-version.sh <new-version> [--commit] [--tag]
#
# Examples:
#   ./scripts/bump-bindings-version.sh 0.2.0
#   ./scripts/bump-bindings-version.sh 0.2.0 --commit
#   ./scripts/bump-bindings-version.sh 0.2.0 --commit --tag

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Parse arguments
NEW_VERSION="$1"
DO_COMMIT=false
DO_TAG=false

if [ -z "$NEW_VERSION" ]; then
    echo -e "${RED}Error: Version number required${NC}"
    echo "Usage: $0 <new-version> [--commit] [--tag]"
    echo "Example: $0 0.2.0"
    exit 1
fi

# Validate version format
if ! echo "$NEW_VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo -e "${RED}Error: Invalid version format. Use semantic versioning (e.g., 0.2.0)${NC}"
    exit 1
fi

shift
while [[ $# -gt 0 ]]; do
    case $1 in
        --commit)
            DO_COMMIT=true
            shift
            ;;
        --tag)
            DO_TAG=true
            shift
            ;;
        *)
            echo -e "${RED}Error: Unknown option $1${NC}"
            exit 1
            ;;
    esac
done

echo -e "${GREEN}KeystoneDB Bindings Version Bump${NC}"
echo "================================"
echo "New version: $NEW_VERSION"
echo ""

# Function to update file with sed (cross-platform)
update_file() {
    local file="$1"
    local pattern="$2"
    local replacement="$3"

    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS
        sed -i '' "$pattern" "$file"
    else
        # Linux
        sed -i "$pattern" "$file"
    fi
}

# Track files changed
CHANGED_FILES=()

# 1. Update Python pyproject.toml
echo -e "${YELLOW}Updating Python bindings...${NC}"
PYTHON_TOML="bindings/python/embedded/pyproject.toml"
if [ -f "$PYTHON_TOML" ]; then
    update_file "$PYTHON_TOML" "s/^version = \".*\"/version = \"$NEW_VERSION\"/"
    CHANGED_FILES+=("$PYTHON_TOML")
    echo "  ✓ Updated $PYTHON_TOML"
else
    echo "  ⚠ Warning: $PYTHON_TOML not found"
fi

# 2. Update JavaScript package.json
echo -e "${YELLOW}Updating JavaScript bindings...${NC}"
JS_PACKAGE="bindings/javascript/client/package.json"
if [ -f "$JS_PACKAGE" ]; then
    update_file "$JS_PACKAGE" "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/"
    CHANGED_FILES+=("$JS_PACKAGE")
    echo "  ✓ Updated $JS_PACKAGE"
else
    echo "  ⚠ Warning: $JS_PACKAGE not found"
fi

# 3. Update C FFI Cargo.toml (optional, for future versioning)
echo -e "${YELLOW}Updating C FFI library...${NC}"
C_FFI_TOML="c-ffi/Cargo.toml"
if [ -f "$C_FFI_TOML" ]; then
    # Note: This uses workspace version, so we'd need to update root Cargo.toml
    echo "  ℹ C FFI uses workspace version (no change needed)"
else
    echo "  ⚠ Warning: $C_FFI_TOML not found"
fi

# 4. Update BUILD_STATUS.md
echo -e "${YELLOW}Updating documentation...${NC}"
BUILD_STATUS="bindings/BUILD_STATUS.md"
if [ -f "$BUILD_STATUS" ]; then
    # Update version references in BUILD_STATUS.md
    update_file "$BUILD_STATUS" "s/keystonedb-[0-9]\+\.[0-9]\+\.[0-9]\+/keystonedb-$NEW_VERSION/g"
    CHANGED_FILES+=("$BUILD_STATUS")
    echo "  ✓ Updated $BUILD_STATUS"
fi

# Summary
echo ""
echo -e "${GREEN}Summary${NC}"
echo "======="
echo "Updated ${#CHANGED_FILES[@]} files:"
for file in "${CHANGED_FILES[@]}"; do
    echo "  - $file"
done

# Git operations
if [ "$DO_COMMIT" = true ]; then
    echo ""
    echo -e "${YELLOW}Creating git commit...${NC}"

    # Check if there are changes to commit
    if git diff --quiet "${CHANGED_FILES[@]}"; then
        echo -e "${RED}Error: No changes detected in tracked files${NC}"
        exit 1
    fi

    # Stage changed files
    git add "${CHANGED_FILES[@]}"

    # Commit
    COMMIT_MSG="chore: bump bindings version to $NEW_VERSION

Updated version numbers across all language bindings:
- Python: $NEW_VERSION
- JavaScript: $NEW_VERSION
- Documentation: BUILD_STATUS.md"

    git commit -m "$COMMIT_MSG"
    echo "  ✓ Created commit"

    # Create tag if requested
    if [ "$DO_TAG" = true ]; then
        echo ""
        echo -e "${YELLOW}Creating git tag...${NC}"
        TAG_NAME="bindings-v$NEW_VERSION"
        TAG_MSG="Language Bindings v$NEW_VERSION

This release includes:
- Python bindings v$NEW_VERSION
- JavaScript bindings v$NEW_VERSION
- Go bindings (via git tags)
- C FFI library"

        git tag -a "$TAG_NAME" -m "$TAG_MSG"
        echo "  ✓ Created tag: $TAG_NAME"

        echo ""
        echo -e "${GREEN}Next steps:${NC}"
        echo "  1. Review the commit: git show HEAD"
        echo "  2. Push changes: git push origin main"
        echo "  3. Push tag: git push origin $TAG_NAME"
        echo "  4. GitHub Actions will automatically build and publish bindings"
    else
        echo ""
        echo -e "${GREEN}Next steps:${NC}"
        echo "  1. Review the commit: git show HEAD"
        echo "  2. Push changes: git push origin main"
        echo "  3. Create a tag: $0 $NEW_VERSION --tag"
    fi
else
    echo ""
    echo -e "${GREEN}Next steps:${NC}"
    echo "  1. Review changes: git diff"
    echo "  2. Commit changes: $0 $NEW_VERSION --commit"
    echo "  3. Create tag and push: $0 $NEW_VERSION --commit --tag"
fi

echo ""
echo -e "${GREEN}✓ Version bump complete!${NC}"

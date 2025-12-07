#!/bin/bash
# Release helper script for Engraver
# Usage: ./release.sh <version>
# Example: ./release.sh 0.1.0

set -e

VERSION="${1:-}"

if [[ -z "$VERSION" ]]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.1.0"
    exit 1
fi

# Validate version format
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$ ]]; then
    echo "Error: Invalid version format. Use semver (e.g., 0.1.0 or 0.1.0-beta.1)"
    exit 1
fi

echo "╔════════════════════════════════════════╗"
echo "║  Engraver Release v${VERSION}              "
echo "╚════════════════════════════════════════╝"
echo

# Check for uncommitted changes
if ! git diff-index --quiet HEAD --; then
    echo "Error: Uncommitted changes detected. Please commit or stash them first."
    exit 1
fi

# Update version in Cargo.toml files
echo "Updating versions in Cargo.toml files..."
for cargo_file in crates/*/Cargo.toml Cargo.toml; do
    if [[ -f "$cargo_file" ]]; then
        sed -i.bak "s/^version = \".*\"/version = \"${VERSION}\"/" "$cargo_file"
        rm -f "${cargo_file}.bak"
        echo "  Updated: $cargo_file"
    fi
done

# Update workspace dependencies
echo "Updating workspace dependency versions..."
sed -i.bak "s/engraver-detect = { version = \"[^\"]*\"/engraver-detect = { version = \"${VERSION}\"/" crates/*/Cargo.toml 2>/dev/null || true
sed -i.bak "s/engraver-platform = { version = \"[^\"]*\"/engraver-platform = { version = \"${VERSION}\"/" crates/*/Cargo.toml 2>/dev/null || true
sed -i.bak "s/engraver-core = { version = \"[^\"]*\"/engraver-core = { version = \"${VERSION}\"/" crates/*/Cargo.toml 2>/dev/null || true
find . -name "*.bak" -delete

# Verify build
echo "Verifying build..."
cargo build --release -p engraver
cargo test --release

# Generate changelog entry template
echo
echo "Changelog entry template (add to CHANGELOG.md):"
echo "================================================"
cat << EOF
## [${VERSION}] - $(date +%Y-%m-%d)

### Added
- 

### Changed
- 

### Fixed
- 

EOF

# Commit version changes
echo
read -p "Commit version changes? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    git add -A
    git commit -m "chore: bump version to ${VERSION}"
    echo "Changes committed."
fi

# Create and push tag
echo
read -p "Create and push tag v${VERSION}? (y/n) " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    git tag -a "v${VERSION}" -m "Release v${VERSION}"
    git push origin main
    git push origin "v${VERSION}"
    echo "Tag v${VERSION} created and pushed."
    echo
    echo "GitHub Actions will now build and create the release."
    echo "Monitor at: https://github.com/mstephenholl/engraver/actions"
else
    echo
    echo "To manually create the release:"
    echo "  git tag -a v${VERSION} -m 'Release v${VERSION}'"
    echo "  git push origin main"
    echo "  git push origin v${VERSION}"
fi

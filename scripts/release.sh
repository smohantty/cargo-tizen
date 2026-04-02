#!/usr/bin/env bash
#
# Build a release binary and publish a GitHub release.
#
# Usage:
#   ./scripts/release.sh              # bump patch (0.1.0 -> 0.1.1)
#   ./scripts/release.sh minor        # bump minor (0.1.1 -> 0.2.0)
#   ./scripts/release.sh major        # bump major (0.2.0 -> 1.0.0)
#   ./scripts/release.sh 0.3.0        # set explicit version
#   ./scripts/release.sh --current    # release current version without bumping
#
# Requirements: cargo, gh (authenticated), git (clean working tree)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

die() { echo "error: $*" >&2; exit 1; }

current_version() {
    grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/'
}

bump_version() {
    local cur="$1" part="$2"
    local major minor patch
    IFS='.' read -r major minor patch <<< "$cur"

    case "$part" in
        major) echo "$((major + 1)).0.0" ;;
        minor) echo "${major}.$((minor + 1)).0" ;;
        patch) echo "${major}.${minor}.$((patch + 1))" ;;
        *)     die "unknown bump type: $part" ;;
    esac
}

set_version() {
    local new_version="$1"
    sed -i "s/^version = \".*\"/version = \"${new_version}\"/" Cargo.toml
    # Update Cargo.lock
    cargo check --quiet 2>/dev/null || true
}

# ---------------------------------------------------------------------------
# Preflight checks
# ---------------------------------------------------------------------------

command -v cargo >/dev/null || die "cargo not found"
command -v gh    >/dev/null || die "gh not found (install: https://cli.github.com)"
command -v git   >/dev/null || die "git not found"
gh auth status >/dev/null 2>&1 || die "gh not authenticated (run: gh auth login)"

if [ -n "$(git status --porcelain)" ]; then
    die "working tree is not clean — commit or stash changes first"
fi

# ---------------------------------------------------------------------------
# Resolve target version
# ---------------------------------------------------------------------------

OLD_VERSION="$(current_version)"
BUMP="${1:-patch}"

FORCE_CURRENT=false
if [ "$BUMP" = "--current" ]; then
    NEW_VERSION="$OLD_VERSION"
    FORCE_CURRENT=true
elif echo "$BUMP" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    NEW_VERSION="$BUMP"
elif echo "$BUMP" | grep -qE '^(major|minor|patch)$'; then
    NEW_VERSION="$(bump_version "$OLD_VERSION" "$BUMP")"
else
    die "invalid argument: $BUMP (expected major|minor|patch|X.Y.Z|--current)"
fi

TAG="v${NEW_VERSION}"

TAG_EXISTS=false
if git rev-parse "$TAG" >/dev/null 2>&1; then
    if [ "$FORCE_CURRENT" = true ]; then
        TAG_EXISTS=true
    else
        die "tag $TAG already exists (use --current to force-update)"
    fi
fi

echo "==> Version: ${OLD_VERSION} -> ${NEW_VERSION} (tag: ${TAG})"

# ---------------------------------------------------------------------------
# Bump version in Cargo.toml (if changed)
# ---------------------------------------------------------------------------

if [ "$NEW_VERSION" != "$OLD_VERSION" ]; then
    set_version "$NEW_VERSION"
    git add Cargo.toml Cargo.lock
    git commit -m "Release ${TAG}"
fi

# ---------------------------------------------------------------------------
# Build release binary
# ---------------------------------------------------------------------------

echo "==> Building release binary..."
cargo build --release
cargo test --quiet

BINARY="target/release/cargo-tizen"
[ -f "$BINARY" ] || die "release binary not found at $BINARY"

# ---------------------------------------------------------------------------
# Package the tarball
# ---------------------------------------------------------------------------

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)  ARCH_LABEL="x86_64"  ;;
    aarch64) ARCH_LABEL="aarch64" ;;
    armv7l)  ARCH_LABEL="armv7"   ;;
    *)       ARCH_LABEL="$ARCH"   ;;
esac

TARBALL_NAME="cargo-tizen-${NEW_VERSION}-linux-${ARCH_LABEL}.tar.gz"
TARBALL_DIR="$(mktemp -d)"
STAGE_DIR="${TARBALL_DIR}/cargo-tizen-${NEW_VERSION}"
mkdir -p "$STAGE_DIR"

cp "$BINARY" "$STAGE_DIR/cargo-tizen"
strip "$STAGE_DIR/cargo-tizen" 2>/dev/null || true
cp README.md "$STAGE_DIR/" 2>/dev/null || true

tar -czf "${TARBALL_DIR}/${TARBALL_NAME}" -C "$TARBALL_DIR" "cargo-tizen-${NEW_VERSION}"

TARBALL_PATH="${TARBALL_DIR}/${TARBALL_NAME}"
TARBALL_SIZE="$(du -h "$TARBALL_PATH" | cut -f1)"
echo "==> Packaged: ${TARBALL_NAME} (${TARBALL_SIZE})"

# ---------------------------------------------------------------------------
# Create tag and GitHub release
# ---------------------------------------------------------------------------

RELEASE_NOTES="$(cat <<EOF
## cargo-tizen ${NEW_VERSION}

### Install

\`\`\`bash
curl -fsSL https://raw.githubusercontent.com/smohantty/cargo-tizen/main/scripts/install.sh | bash
\`\`\`

Or download the binary manually and place it in \`~/.cargo/bin/\`.

### Binary

- \`${TARBALL_NAME}\` — Linux ${ARCH_LABEL}, statically linked where possible
EOF
)"

if [ "$TAG_EXISTS" = true ]; then
    echo "==> Moving tag ${TAG} to HEAD..."
    git tag -fa "$TAG" -m "Release ${TAG}"
    git push origin "$TAG" --force

    echo "==> Updating existing GitHub release..."
    # Delete old assets and upload new ones
    gh release upload "$TAG" "$TARBALL_PATH" --clobber
    gh release edit "$TAG" --title "$TAG" --notes "$RELEASE_NOTES"
else
    echo "==> Tagging ${TAG}..."
    git tag -a "$TAG" -m "Release ${TAG}"

    echo "==> Pushing tag..."
    git push origin HEAD "$TAG"

    echo "==> Creating GitHub release..."
    gh release create "$TAG" \
        "$TARBALL_PATH" \
        --title "$TAG" \
        --notes "$RELEASE_NOTES"
fi

# ---------------------------------------------------------------------------
# Cleanup
# ---------------------------------------------------------------------------

rm -rf "$TARBALL_DIR"

RELEASE_URL="$(gh release view "$TAG" --json url -q '.url')"
echo ""
echo "==> Released: ${RELEASE_URL}"

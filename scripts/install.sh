#!/usr/bin/env bash
#
# Install cargo-tizen from the latest GitHub release.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/smohantty/cargo-tizen/main/scripts/install.sh | bash
#   curl -fsSL ... | bash -s -- --version 0.2.0
#   curl -fsSL ... | bash -s -- --dir /usr/local/bin
#
# The binary is placed in ~/.cargo/bin/ by default (where cargo discovers subcommands).

set -euo pipefail

REPO="smohantty/cargo-tizen"
INSTALL_DIR="${HOME}/.cargo/bin"
VERSION=""

# ---------------------------------------------------------------------------
# Parse arguments
# ---------------------------------------------------------------------------

while [ $# -gt 0 ]; do
    case "$1" in
        --version) VERSION="$2"; shift 2 ;;
        --dir)     INSTALL_DIR="$2"; shift 2 ;;
        --help|-h)
            echo "Usage: install.sh [--version X.Y.Z] [--dir /path]"
            exit 0
            ;;
        *) echo "unknown option: $1" >&2; exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

die() { echo "error: $*" >&2; exit 1; }

detect_arch() {
    local arch
    arch="$(uname -m)"
    case "$arch" in
        x86_64)          echo "x86_64"  ;;
        aarch64|arm64)   echo "aarch64" ;;
        armv7l|armv7)    echo "armv7"   ;;
        *)               die "unsupported architecture: $arch" ;;
    esac
}

has_cmd() { command -v "$1" >/dev/null 2>&1; }

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------

[ "$(uname -s)" = "Linux" ] || die "this installer supports Linux only"
has_cmd curl || has_cmd wget || die "curl or wget required"

ARCH="$(detect_arch)"

# ---------------------------------------------------------------------------
# Resolve version and download URL
# ---------------------------------------------------------------------------

if [ -z "$VERSION" ]; then
    if has_cmd gh; then
        VERSION="$(gh release view --repo "$REPO" --json tagName -q '.tagName' 2>/dev/null | sed 's/^v//')" || true
    fi
    if [ -z "$VERSION" ]; then
        # Fallback: query the GitHub API
        API_URL="https://api.github.com/repos/${REPO}/releases/latest"
        if has_cmd curl; then
            VERSION="$(curl -fsSL "$API_URL" | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')" || true
        elif has_cmd wget; then
            VERSION="$(wget -qO- "$API_URL" | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')" || true
        fi
    fi
    [ -n "$VERSION" ] || die "failed to determine latest release version"
fi

TAG="v${VERSION}"
TARBALL="cargo-tizen-${VERSION}-linux-${ARCH}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${TARBALL}"

echo "Installing cargo-tizen ${VERSION} (linux-${ARCH})..."

# ---------------------------------------------------------------------------
# Download and extract
# ---------------------------------------------------------------------------

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

TARBALL_PATH="${TMP_DIR}/${TARBALL}"

if has_cmd curl; then
    curl -fSL --progress-bar -o "$TARBALL_PATH" "$DOWNLOAD_URL"
elif has_cmd wget; then
    wget -q --show-progress -O "$TARBALL_PATH" "$DOWNLOAD_URL"
fi

tar -xzf "$TARBALL_PATH" -C "$TMP_DIR"

# The tarball extracts to cargo-tizen-VERSION/cargo-tizen
EXTRACTED="${TMP_DIR}/cargo-tizen-${VERSION}/cargo-tizen"
[ -f "$EXTRACTED" ] || die "binary not found in tarball"

# ---------------------------------------------------------------------------
# Install
# ---------------------------------------------------------------------------

mkdir -p "$INSTALL_DIR"
mv "$EXTRACTED" "${INSTALL_DIR}/cargo-tizen"
chmod +x "${INSTALL_DIR}/cargo-tizen"

echo "Installed cargo-tizen to ${INSTALL_DIR}/cargo-tizen"

# ---------------------------------------------------------------------------
# Verify PATH
# ---------------------------------------------------------------------------

if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    echo ""
    echo "WARNING: ${INSTALL_DIR} is not in your PATH."
    echo "Add it with:"
    echo ""
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo ""
    echo "Or add that line to your ~/.bashrc or ~/.zshrc."
else
    echo ""
    echo "Verify with:"
    echo "  cargo tizen --help"
fi

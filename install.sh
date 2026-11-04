#!/usr/bin/env bash
# Install scrme (macOS): curl -fsSL https://raw.githubusercontent.com/cubetiqlabs/scrme/main/install.sh | bash
# Optional: VERSION=v0.2.0 to pin a release, INSTALL_DIR=~/Applications to change the target.
set -euo pipefail

REPO="cubetiqlabs/scrme"
APP="scrme.app"
DEST="${INSTALL_DIR:-/Applications}"

[ "$(uname -s)" = "Darwin" ] || { echo "scrme currently supports macOS only." >&2; exit 1; }
case "$(uname -m)" in
  arm64) ARCH="aarch64" ;;
  x86_64) ARCH="x64" ;;
  *) echo "Unsupported CPU: $(uname -m)" >&2; exit 1 ;;
esac

API="https://api.github.com/repos/$REPO/releases/${VERSION:+tags/$VERSION}"
[ -n "${VERSION:-}" ] || API="https://api.github.com/repos/$REPO/releases/latest"

echo "→ Finding the latest scrme release..."
URL=$(curl -fsSL "$API" | grep -o "https://[^\"]*_${ARCH}\.dmg" | head -n1 || true)
[ -n "$URL" ] || { echo "No .dmg found for $ARCH. See https://github.com/$REPO/releases" >&2; exit 1; }

TMP=$(mktemp -d)
MNT="$TMP/mnt"
cleanup() { hdiutil detach "$MNT" -quiet 2>/dev/null || true; rm -rf "$TMP"; }
trap cleanup EXIT

echo "→ Downloading $(basename "$URL")..."
curl -fL --progress-bar -o "$TMP/scrme.dmg" "$URL"

mkdir -p "$MNT"
hdiutil attach "$TMP/scrme.dmg" -nobrowse -quiet -mountpoint "$MNT"

echo "→ Installing to ${DEST}..."
mkdir -p "$DEST"
pkill -x scrme 2>/dev/null || true
rm -rf "${DEST:?}/$APP"
cp -R "$MNT/$APP" "$DEST/"
# Builds are not notarized yet; clear the download quarantine flag.
xattr -dr com.apple.quarantine "$DEST/$APP" 2>/dev/null || true

echo "✓ Installed. Launching scrme — look for it in the menu bar."
echo "  Allow Screen Recording when macOS asks (System Settings → Privacy & Security)."
open "$DEST/$APP"

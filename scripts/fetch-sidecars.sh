#!/usr/bin/env bash
# Fetch the pinned llama.cpp server build (MIT) for Apple Silicon, verify its SHA-256 and unpack it into
# src-tauri/binaries/llama/ (gitignored). The prebuilt llama-server loads its ggml/llama dylibs through
# @rpath next to itself, so the whole directory is kept together.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=versions.env
source scripts/versions.env

DEST="src-tauri/binaries/llama"
STAMP="$DEST/.build-$LLAMA_BUILD"
if [ -x "$DEST/llama-server" ] && [ -f "$STAMP" ]; then
  echo "llama-server $LLAMA_BUILD already present in $DEST"
  exit 0
fi

[ "$(uname -m)" = "arm64" ] || { echo "Only Apple Silicon (arm64) is supported" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
echo "downloading $LLAMA_ASSET ..."
curl -fsSL --retry 3 -o "$TMP/llama.tgz" "$LLAMA_URL"
echo "$LLAMA_SHA256  $TMP/llama.tgz" | shasum -a 256 -c - >/dev/null || { echo "SHA-256 mismatch for $LLAMA_ASSET" >&2; exit 1; }
tar -xzf "$TMP/llama.tgz" -C "$TMP"
SRC="$(dirname "$(find "$TMP" -name llama-server -type f | head -1)")"
[ -x "$SRC/llama-server" ] || { echo "llama-server not found in the archive" >&2; exit 1; }

rm -rf "$DEST"
mkdir -p "$DEST"
# Only what the server needs: the binary, its dylibs and the license.
cp "$SRC/llama-server" "$DEST/"
cp "$SRC"/*.dylib "$DEST/"
cp "$SRC/LICENSE" "$DEST/LICENSE-llama.cpp"
# Downloaded files carry the quarantine attribute; the binaries are verified above.
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true
touch "$STAMP"
"$DEST/llama-server" --version 2>&1 | head -1
echo "installed llama-server $LLAMA_BUILD into $DEST"

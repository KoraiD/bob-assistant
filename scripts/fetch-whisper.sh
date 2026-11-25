#!/usr/bin/env bash
# Fetch the pinned whisper.cpp xcframework (MIT), verify its SHA-256 and unpack it into
# native/bob-native/Frameworks/whisper.xcframework (gitignored). The Swift helper links it for on-device
# dictation; build-app.sh embeds the macOS framework in BOB.app/Contents/Frameworks.
set -euo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=versions.env
source scripts/versions.env
DEST="native/bob-native/Frameworks"
STAMP="$DEST/.whisper-$WHISPER_BUILD"
if [ -d "$DEST/whisper.xcframework" ] && [ -f "$STAMP" ]; then
  echo "whisper.xcframework $WHISPER_BUILD already present in $DEST"
  exit 0
fi
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
echo "downloading $WHISPER_ASSET ..."
curl -fsSL --retry 3 -o "$TMP/whisper.zip" "$WHISPER_URL"
echo "$WHISPER_SHA256  $TMP/whisper.zip" | shasum -a 256 -c - >/dev/null || { echo "SHA-256 mismatch for $WHISPER_ASSET" >&2; exit 1; }
unzip -q "$TMP/whisper.zip" -d "$TMP"
SRC="$(find "$TMP" -maxdepth 3 -name whisper.xcframework -type d | head -1)"
[ -d "$SRC/macos-arm64_x86_64/whisper.framework" ] || { echo "macOS slice not found in the archive" >&2; exit 1; }
rm -rf "$DEST/whisper.xcframework" "$DEST"/.whisper-*
mkdir -p "$DEST"
cp -R "$SRC" "$DEST/"
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true
touch "$STAMP"
echo "installed whisper.xcframework $WHISPER_BUILD into $DEST"

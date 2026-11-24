#!/usr/bin/env bash
# Build a signed BOB.app for local use, with the Swift helper inside it, so macOS shows "BOB" in
# Privacy & Security and the grants survive rebuilds.
#
#   export BOB_SIGN_IDENTITY="Developer ID Application: <Name> (<TEAMID)>"   # or the identity's SHA-1 hash
#   make app            -> src-tauri/target/debug/bundle/macos/BOB.app
#   open src-tauri/target/debug/bundle/macos/BOB.app
#
# Without BOB_SIGN_IDENTITY the app is ad-hoc signed: it works, but macOS treats every rebuild as a new
# app and asks for permissions again. The identity is read from the environment and never committed.
set -euo pipefail
cd "$(dirname "$0")/.."

IDENTITY="${BOB_SIGN_IDENTITY:--}"
[ "$IDENTITY" = "-" ] && echo "warning: BOB_SIGN_IDENTITY not set, using ad-hoc signing (permissions reset on every rebuild)" >&2

echo "== helper (release)"
(cd native/bob-native && swift build -c release >/dev/null)
HELPER="native/bob-native/.build/release/bob-native"

echo "== app bundle"
npx tauri build --debug --bundles app >/dev/null
APP="src-tauri/target/debug/bundle/macos/BOB.app"
[ -d "$APP" ] || { echo "bundle not found: $APP" >&2; exit 1; }

cp "$HELPER" "$APP/Contents/MacOS/bob-native"
# MCP server for AI agents (proposal 17): a small stdio program next to the app's executable.
echo "== bob-mcp"
cargo build --manifest-path src-tauri/Cargo.toml --bin bob-mcp >/dev/null 2>&1
cp src-tauri/target/debug/bob-mcp "$APP/Contents/MacOS/bob-mcp"

# whisper.cpp for on-device dictation (Task 8): embed the framework and let the helper find it.
WHISPER_FW="native/bob-native/Frameworks/whisper.xcframework/macos-arm64_x86_64/whisper.framework"
[ -d "$WHISPER_FW" ] || { echo "whisper.framework missing: run make whisper" >&2; exit 1; }
mkdir -p "$APP/Contents/Frameworks"
rm -rf "$APP/Contents/Frameworks/whisper.framework"
cp -R "$WHISPER_FW" "$APP/Contents/Frameworks/"
otool -l "$APP/Contents/MacOS/bob-native" | grep -q "@executable_path/../Frameworks" ||
  install_name_tool -add_rpath "@executable_path/../Frameworks" "$APP/Contents/MacOS/bob-native"

echo "== signing with: $([ "$IDENTITY" = "-" ] && echo ad-hoc || echo "configured identity")"
# Nested code first, then the bundle (which also re-signs the main executable).
codesign --force --options runtime --timestamp=none --sign "$IDENTITY" "$APP/Contents/Frameworks/whisper.framework"
codesign --force --options runtime --timestamp=none \
  --entitlements native/bob-native/BobNative.entitlements \
  --identifier dev.bob.assistant.native --sign "$IDENTITY" "$APP/Contents/MacOS/bob-native"
codesign --force --options runtime --timestamp=none \
  --identifier dev.bob.assistant.mcp --sign "$IDENTITY" "$APP/Contents/MacOS/bob-mcp"
codesign --force --options runtime --timestamp=none \
  --entitlements src-tauri/Entitlements.plist --sign "$IDENTITY" "$APP"
codesign --verify --strict --deep "$APP"

echo "== done: $APP"
echo "Open it with:  open \"$APP\""

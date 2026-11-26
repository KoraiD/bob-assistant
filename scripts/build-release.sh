#!/usr/bin/env bash
# Build a release of BOB: BOB.app with everything inside it, signed for distribution, in a DMG.
#
#   export BOB_SIGN_IDENTITY="Developer ID Application: <Name> (<TEAMID>)"   # or its SHA-1 hash
#   make release                       -> release/BOB-<version>-arm64.dmg (+ .sha256)
#
# Notarization is optional and off by default. To notarize, store the credentials once in your login
# keychain, then pass only the profile name (no password ever goes into a file, a variable or the log):
#
#   xcrun notarytool store-credentials bob-notary --apple-id <id> --team-id <TEAMID>   # asks for the password
#   BOB_NOTARY_PROFILE=bob-notary make release
#
# Without BOB_SIGN_IDENTITY the build stops, unless BOB_ALLOW_ADHOC=1 (local testing only: macOS will not
# open an ad-hoc signed download).
set -euo pipefail
cd "$(dirname "$0")/.."

[ "$(uname -m)" = "arm64" ] || { echo "Release builds need Apple Silicon (arm64)." >&2; exit 1; }
IDENTITY="${BOB_SIGN_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
  [ "${BOB_ALLOW_ADHOC:-}" = "1" ] || { echo "Set BOB_SIGN_IDENTITY (a Developer ID Application identity)." >&2; exit 1; }
  IDENTITY="-"
fi
VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
OUT="release"
APP_SRC="src-tauri/target/release/bundle/macos/BOB.app"
LLAMA="src-tauri/binaries/llama"
WHISPER_FW="native/bob-native/Frameworks/whisper.xcframework/macos-arm64_x86_64/whisper.framework"
# The model server's own libraries (from `otool -L`); the rest of the llama.cpp archive is not needed.
LLAMA_LIBS="libllama-server-impl.dylib libllama-common.1.dylib libmtmd.1.dylib libllama.1.dylib libggml.1.dylib
  libggml-cpu.1.dylib libggml-blas.1.dylib libggml-metal.1.dylib libggml-rpc.1.dylib libggml-base.1.dylib"

[ -x "$LLAMA/llama-server" ] || { echo "llama-server missing: run make sidecars" >&2; exit 1; }
[ -d "$WHISPER_FW" ] || { echo "whisper.framework missing: run make whisper" >&2; exit 1; }

# Source paths that compilers record (panic messages, debug info) must not carry the build machine's home
# folder (it names the user): map it to "~".
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~"
echo "== build (release)"
(cd native/bob-native && swift build -c release -Xswiftc -file-prefix-map -Xswiftc "$HOME=~" >/dev/null)
cargo build --release --manifest-path src-tauri/Cargo.toml --bin bob-mcp >/dev/null 2>&1
npx tauri build --bundles app >/dev/null
[ -d "$APP_SRC" ] || { echo "bundle not found: $APP_SRC" >&2; exit 1; }

echo "== assemble"
rm -rf "$OUT/stage"
mkdir -p "$OUT/stage"
APP="$OUT/stage/BOB.app"
cp -R "$APP_SRC" "$APP"
cp native/bob-native/.build/release/bob-native "$APP/Contents/MacOS/bob-native"
strip -S "$APP/Contents/MacOS/bob-native"  # the debug map lists object files by their build path
cp src-tauri/target/release/bob-mcp "$APP/Contents/MacOS/bob-mcp"
mkdir -p "$APP/Contents/Frameworks" "$APP/Contents/Resources/llama"
cp -R "$WHISPER_FW" "$APP/Contents/Frameworks/"
otool -l "$APP/Contents/MacOS/bob-native" | grep -q "@executable_path/../Frameworks" ||
  install_name_tool -add_rpath "@executable_path/../Frameworks" "$APP/Contents/MacOS/bob-native"
cp "$LLAMA/llama-server" "$APP/Contents/Resources/llama/"
for lib in $LLAMA_LIBS; do cp -L "$LLAMA/$lib" "$APP/Contents/Resources/llama/$lib"; done
cp "$LLAMA/LICENSE-llama.cpp" THIRD_PARTY_NOTICES.md LICENSE "$APP/Contents/Resources/"
xattr -cr "$APP"

echo "== sign ($([ "$IDENTITY" = "-" ] && echo "ad-hoc" || echo "configured identity"))"
TS="--timestamp"
[ "$IDENTITY" = "-" ] && TS="--timestamp=none"
sign() { codesign --force --options runtime $TS --sign "$IDENTITY" "$@"; }
# Inside out: libraries and nested programs first, the app last.
for f in "$APP"/Contents/Resources/llama/*.dylib; do sign "$f"; done
sign --identifier dev.bob.assistant.llama-server "$APP/Contents/Resources/llama/llama-server"
sign "$APP/Contents/Frameworks/whisper.framework"
sign --entitlements native/bob-native/BobNative.entitlements --identifier dev.bob.assistant.native \
  "$APP/Contents/MacOS/bob-native"
sign --identifier dev.bob.assistant.mcp "$APP/Contents/MacOS/bob-mcp"
sign --entitlements src-tauri/Entitlements.plist "$APP"
codesign --verify --strict --deep "$APP"

echo "== dmg"
DMG="$OUT/BOB-$VERSION-arm64.dmg"
rm -f "$DMG"
ln -s /Applications "$OUT/stage/Applications"
hdiutil create -quiet -volname "BOB $VERSION" -srcfolder "$OUT/stage" -fs HFS+ -format UDZO -ov "$DMG"
# Check before signing the DMG: no home folder path in anything that ships.
if grep -rlaF "$HOME/" "$APP" >/dev/null 2>&1; then
  echo "the app contains paths from your home folder:" >&2
  grep -rlaF "$HOME/" "$APP" >&2
  exit 1
fi
[ "$IDENTITY" = "-" ] || codesign --force $TS --sign "$IDENTITY" "$DMG"

if [ -n "${BOB_NOTARY_PROFILE:-}" ]; then
  echo "== notarize (profile: $BOB_NOTARY_PROFILE)"
  xcrun notarytool submit "$DMG" --keychain-profile "$BOB_NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
else
  echo "== not notarized (set BOB_NOTARY_PROFILE to notarize)"
fi

(cd "$OUT" && shasum -a 256 "$(basename "$DMG")" > "$(basename "$DMG").sha256")
rm -rf "$OUT/stage"
echo "== done: $DMG ($(du -h "$DMG" | cut -f1))"
cat "$DMG.sha256"

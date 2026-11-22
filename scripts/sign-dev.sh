#!/usr/bin/env bash
# Sign the debug helper with a STABLE identity so macOS keeps its Accessibility/Microphone grants
# across rebuilds (grants are tied to the code-signing identity; ad-hoc signatures change every build).
#
# The identity is read from the environment and never stored in the repository:
#   export BOB_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"   # or an "Apple Development" identity
#   make sign-dev
# List identities with:  security find-identity -v -p codesigning
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -z "${BOB_SIGN_IDENTITY:-}" ]; then
  echo "BOB_SIGN_IDENTITY is not set. Example:" >&2
  echo '  export BOB_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"' >&2
  echo "Available identities:" >&2
  security find-identity -v -p codesigning >&2 || true
  exit 1
fi

BIN="native/bob-native/.build/debug/bob-native"
[ -x "$BIN" ] || { echo "Helper not built. Run: make native" >&2; exit 1; }

# A fixed identifier keeps the designated requirement stable between builds.
codesign --force --sign "$BOB_SIGN_IDENTITY" --identifier dev.bob.assistant.native --timestamp=none "$BIN"
codesign --verify --verbose=2 "$BIN"
echo "signed $BIN"

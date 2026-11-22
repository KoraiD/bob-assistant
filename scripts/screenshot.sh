#!/usr/bin/env bash
# Visual check of the UI without the Tauri shell: serve with Vite and capture a PNG with headless
# Chrome after animations settle. Output: bench-results/local/ui-<width>x<height>.png (gitignored).
# Usage: scripts/screenshot.sh [width] [height] [daylight]
set -euo pipefail
cd "$(dirname "$0")/.."
W="${1:-980}"; H="${2:-680}"; THEME="${3:-}"
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
[ -x "$CHROME" ] || { echo "Google Chrome is required for screenshots" >&2; exit 1; }

OUT_DIR="bench-results/local"; mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/ui-${W}x${H}${THEME:+-$THEME}.png"
PORT=1421
npx vite --port "$PORT" --strictPort >/dev/null 2>&1 &
VITE=$!
trap 'kill $VITE 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do curl -fs "http://localhost:$PORT/" >/dev/null && break; sleep 0.2; done

URL="http://localhost:$PORT/${THEME:+?theme=$THEME}"
PROFILE="$(mktemp -d)"
# --virtual-time-budget lets the fade-in animations finish before capture.
"$CHROME" --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
  --window-size="$W,$H" --virtual-time-budget=3000 --user-data-dir="$PROFILE" \
  --screenshot="$OUT" "$URL" >/dev/null 2>&1
rm -rf "$PROFILE"
echo "wrote $OUT"

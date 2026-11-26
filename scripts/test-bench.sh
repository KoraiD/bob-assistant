#!/usr/bin/env bash
# Run the BOB Test Bench: a test copy of BOB (separate profile in /tmp/bob-test) suggests while the Test Bench
# types like a person into its own editor, and every step is checked.
#
#   scripts/test-bench.sh [--brain fake|builtin] [--minutes 5] [--seed N] [--profiles plain,smart]
#
# Before you start: quit your own BOB (the Test Bench refuses to run while it runs) and build the dev app
# (`make app`). The first run asks for Accessibility for "BOB Test Bench" (System Settings > Privacy & Security
# > Accessibility); it continues once the switch is on. During the run, keep your hands off the mouse and
# keyboard: touching either stops it. It types only into the Test Bench window.
# Results: bench-results/local/testbench/<time>-seed<N>/. Set BOB_SIGN_IDENTITY to sign with your certificate,
# so the Accessibility switch survives rebuilds (ad hoc signing otherwise).
#   --brain fake     a local fake brain that continues corpus sentences (default; quick and repeatable)
#   --brain builtin  the built-in model, as you use BOB (realistic suggestions and timing)
set -euo pipefail
cd "$(dirname "$0")/.."

brain=fake minutes=5 seed=$(( $(date +%s) % 100000 )) profiles=plain,smart
while [ $# -gt 0 ]; do
  case "$1" in
    --brain) brain=$2; shift 2 ;;
    --minutes) minutes=$2; shift 2 ;;
    --seed) seed=$2; shift 2 ;;
    --profiles) profiles=$2; shift 2 ;;
    *) sed -n '2,15p' "$0"; exit 64 ;;
  esac
done
case "$brain" in fake|builtin) ;; *) echo "--brain must be fake or builtin"; exit 64 ;; esac

app=src-tauri/target/debug/bundle/macos/BOB.app
profile=/tmp/bob-test
port=18080
real_lock="${TMPDIR%/}/bob-$(id -u)/bob.lock"
out="bench-results/local/testbench/$(date +%Y%m%d-%H%M%S)-seed$seed"

[ -d "$app" ] || { echo "Build the dev app first: make app"; exit 1; }
if pgrep -f "$app/Contents/MacOS/bob" >/dev/null; then
  echo "A test BOB from an earlier run is still running; stop it first: pkill -f '$app/Contents/MacOS/bob'"
  exit 1
fi

echo "== building the Test Bench"
(cd native/test-bench && swift build -c release -q)
bench=native/test-bench/.build/release/bob-test-bench
bundle="native/test-bench/.build/BOB Test Bench.app"
mkdir -p "$bundle/Contents/MacOS"
cp native/test-bench/Info.plist "$bundle/Contents/Info.plist"
cp "$bench" "$bundle/Contents/MacOS/bob-test-bench"
codesign --force --sign "${BOB_SIGN_IDENTITY:--}" "$bundle" 2>/dev/null
"$bench" --preflight --real-lock "$real_lock"
mkdir -p "$out"

echo "== preparing the test profile in $profile"
rm -rf "$profile"
mkdir -p "$profile/config"
chmod 700 "$profile" "$profile/config"
cfg=$profile/config
echo '{"enabled": true}' > "$cfg/ghost-settings.json"
echo '{"enabled": false}' > "$cfg/dictation-settings.json"
# Selection Actions on Ctrl+Opt+Cmd+J, away from the usual Cmd+Shift+J.
echo '{"hotkey": {"key_code": 38, "modifiers": ["control", "option", "cmd"]}}' > "$cfg/selection-settings.json"
echo '{"open_window_at_launch": false}' > "$cfg/app-settings.json"

fake_pid=""
cleanup() {
  pkill -TERM -f "$app/Contents/MacOS/bob" 2>/dev/null || true
  if [ -n "$fake_pid" ]; then
    curl -s "http://127.0.0.1:$port/stats" > "$out/fake-brain.json" 2>/dev/null || true
    kill "$fake_pid" 2>/dev/null || true
    wait "$fake_pid" 2>/dev/null || true
  fi
  sleep 2
  [ -f "$profile/logs/bob.log" ] && cp "$profile/logs/bob.log" "$out/test-bob.log"
  if [ -f "$out/test-bob.log" ]; then
    echo "== what the test BOB's Ghost Text did (from test-bob.log)"
    grep -oE "ghost: (quiet \([^)]*\)|request|first chip|stale chip dropped|[0-9]+ raw chars, nothing worth showing yet|the app's own completion owns Tab)" \
      "$out/test-bob.log" | sed -E 's/^ghost: [0-9]+ raw/ghost: N raw/' | sort | uniq -c | sort -rn | head -8 || true
    grep -E "WARN|ERROR" "$out/test-bob.log" | sed -E 's/^[0-9-]+ //' | cut -c1-160 | head -5 || true
  fi
  echo "== results in $out"
}
trap cleanup EXIT

if [ "$brain" = fake ]; then
  cat > "$cfg/brains.json" <<EOF
{"brains": [{"id": "test-bench-fake", "name": "Test Bench fake brain", "kind": "open_ai_compatible",
  "endpoint": "http://127.0.0.1:$port", "model": "fake", "capabilities": {"ghost": true, "chat": true}}],
 "roles": {"ghost": "test-bench-fake", "selection": "test-bench-fake", "dictation": "test-bench-fake"}}
EOF
  python3 scripts/fake-brain.py --port "$port" --corpus native/test-bench/corpus.txt &
  fake_pid=$!
fi

echo "== starting the test BOB ($brain brain)"
# Trace level: the Ghost engine says why it stays quiet (metadata only; the text is the bench's corpus).
open -n -g --env BOB_HOME="$profile" --env RUST_LOG=bob_lib=trace "$app"
for _ in $(seq 1 60); do
  [ -S "$profile/run/native.sock" ] && pgrep -f "$app/Contents/MacOS/bob-native" >/dev/null && break
  sleep 0.5
done
[ -S "$profile/run/native.sock" ] || { echo "the test BOB did not start (see $profile/logs/bob.log)"; exit 1; }
if [ "$brain" = builtin ]; then
  echo "== waiting for the built-in model"
  for _ in $(seq 1 150); do grep -q "built-in brain: ready" "$profile/logs/bob.log" 2>/dev/null && break; sleep 1; done
fi

echo "== Test Bench: click its window, then hands off for 3 seconds"
root=$(pwd)
open -W -n "$bundle" --args --out "$root/$out" --seed "$seed" --minutes "$minutes" --profiles "$profiles" \
  --corpus "$root/native/test-bench/corpus.txt" --real-lock "$real_lock"
cat "$out/result.txt" 2>/dev/null || echo "the Test Bench left no result; if it crashed, see ~/Library/Logs/DiagnosticReports/bob-test-bench-*.ips"
code=$(cat "$out/exit-code" 2>/dev/null || echo 1)
exit "$code"

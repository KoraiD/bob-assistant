#!/usr/bin/env bash
# Measure llama-server prompt-cache reuse for a growing prompt, the way typing extends it.
# Starts its own loopback server (random key in a 0600 file), prints llama.cpp's own timings, stops it.
# Usage: scripts/cache-probe.sh [model.gguf]
set -euo pipefail
cd "$(dirname "$0")/.."
MODEL="${1:-$HOME/Library/Application Support/dev.bob.assistant/models/qwen2.5-1.5b-instruct-q4_k_m.gguf}"
BIN=src-tauri/binaries/llama/llama-server
[ -x "$BIN" ] || { echo "run make sidecars first" >&2; exit 1; }
DIR="$(mktemp -d)"; chmod 700 "$DIR"
KEY="$(openssl rand -hex 32)"; printf '%s\n' "$KEY" > "$DIR/k"; chmod 600 "$DIR/k"
PORT=18777
"$BIN" --model "$MODEL" --host 127.0.0.1 --port $PORT --api-key-file "$DIR/k" --no-webui \
  --ctx-size 4096 --parallel 1 --n-gpu-layers 99 --flash-attn auto >/dev/null 2>&1 &
SERVER=$!
trap 'kill $SERVER 2>/dev/null || true; rm -rf "$DIR"' EXIT
for _ in $(seq 1 120); do
  [ "$(curl -s -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/health")" = 200 ] && break
  sleep 0.5
done
KEY="$KEY" PORT="$PORT" python3 - <<'EOF'
import json, os, time, urllib.request
key, port = os.environ["KEY"], os.environ["PORT"]
base = ("We are planning the quarterly review for the whole team next month. " * 12).strip()
def run(prompt):
    body = json.dumps({"prompt": prompt, "n_predict": 8, "cache_prompt": True, "id_slot": 0, "temperature": 0.2}).encode()
    req = urllib.request.Request(f"http://127.0.0.1:{port}/completion", data=body,
                                 headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"})
    t = time.time(); r = json.load(urllib.request.urlopen(req)); total = (time.time() - t) * 1000
    tm = r.get("timings", {})
    return tm.get("prompt_n"), tm.get("cache_n"), round(tm.get("prompt_ms", 0), 1), round(total, 1)
print(f"{'request':<12} {'evaluated':>10} {'reused':>8} {'prompt_ms':>10} {'total_ms':>9}")
def show(name, r): print(f"{name:<12} {r[0]!s:>10} {r[1]!s:>8} {r[2]:>10} {r[3]:>9}")
show("cold", run(base + " Please"))
for i, s in enumerate([" Please s", " Please se", " Please sen", " Please send"]):
    show(f"keystroke {i+1}", run(base + s))
show("unrelated", run("Completely different text about cooking pasta at home tonight with"))
EOF

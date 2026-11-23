#!/usr/bin/env bash
# Task 5 sweep: settings on the default model, plus the 4B model as a large-model reference.
# Results: bench-results/local/ghost-bench-*.json; curated table in docs/PERFORMANCE.md.
set -euo pipefail
cd "$(dirname "$0")/.."
B=src-tauri/target/release/ghost-bench
$B --model qwen2.5-1.5b-instruct-q4km --quiet-ms 40,110 --temperature 0.2
$B --model qwen2.5-1.5b-instruct-q4km --quiet-ms 70 --temperature 0.0
$B --model qwen3-4b-instruct-2507-q4km --quiet-ms 70 --temperature 0.2

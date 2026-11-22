# Performance

Budgets are **targets** until a measurement below says otherwise. Reference floor: Apple M3, 16 GB.

## Budgets
| Path | Target |
|---|---|
| Ghost Text: last keystroke to first chip paint (warm local model) | p50 <= 150 ms, p95 <= 300 ms |
| Ghost Text: typing-through chip update | < 16 ms |
| Selection Actions: first streamed token (local) | <= 800 ms |
| Dictation: release to inserted text (short utterance) | <= 1.5 s |
| Idle CPU with models loaded | < 2% |

## Method (required for every optimization or bug fix)
1. Reproduce and measure a baseline with the benchmark harness (replayed typing traces, p50/p95).
2. Research the cause. Cite sources (llama.cpp docs/issues, Apple docs) in this file and the commit message.
3. Change exactly one thing.
4. Re-measure with the same traces and hardware; record before/after here.
5. Keep the change only if it helps or removes complexity at equal speed.

## Planned techniques (from the llama.cpp server README)
- `cache_prompt` (default on) reuses the KV cache for the shared prompt prefix: only the new suffix is evaluated.
- `id_slot` pins requests to one slot so the cached prefix is not evicted.
- `n_predict` small, `t_max_predict_ms` cap, stop at newline for short continuations.
- `n_predict: 0` evaluates the prompt into the cache: use it for warm-up.
- Closing the HTTP connection cancels generation.
Source: https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md

## Measurements
_None yet. Task 5 adds the harness and the first results table (model, quant, hardware, p50/p95, settings)._

| Date | Change | Hardware | Model | Metric | Before | After | Notes |
|---|---|---|---|---|---|---|---|

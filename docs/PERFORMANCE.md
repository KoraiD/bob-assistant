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
First real numbers come from the ignored end-to-end test (`real_llama_server_streams_a_ghost_completion`), not from
the replay harness (Task 5). Two prompts only, so these are smoke numbers, not p50/p95.

| Date | Change | Hardware | Model | Metric | Result | Notes |
|---|---|---|---|---|---|---|
| 2026-11-22 | baseline | Apple M4 Max, macOS 27.2 (dev machine, faster than the M3 floor) | Qwen2.5-1.5B-Instruct Q4_K_M, llama-server b12633, `-ngl 99`, flash-attn auto, ctx 4096, 1 slot | request sent to first token, warm server | 94 ms and 143 ms | via `/completion`, `cache_prompt`, `id_slot 0`, `n_predict 12`. Not yet measured on an M3 |

### Quality notes from the same run
- "The quick brown fox jumps over the" -> " lazy dog." (good).
- "Thanks for your email. I will get back to you" -> " as soon as possible. A. 正确 B." The instruct model drifts into
  exam-style text after the sentence ends. Chip polishing must stop at the sentence boundary and drop script switches
  (Task 4), and Task 5 compares a base model.

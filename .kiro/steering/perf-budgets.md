---
inclusion: always
---

# Performance budgets (targets until proven by the benchmark harness)

Reference hardware: Apple M3 with 16 GB (the floor). Numbers are targets, not facts, until
`docs/PERFORMANCE.md` records measurements.

| Path | Budget |
|---|---|
| Ghost Text: last keystroke to first chip paint (warm local model) | p50 <= 150 ms, p95 <= 300 ms |
| Ghost Text: chip update from typing-through (no model call) | < 16 ms |
| Quiet gate after last keystroke | 60 to 80 ms |
| Selection Actions: first streamed token | <= 800 ms (local), network-bound for cloud |
| Dictation: release to text inserted (short utterance) | <= 1.5 s |
| Idle CPU of BOB with models loaded | < 2% |

## Design rules that protect the budget
- Cancel stale requests by dropping the HTTP connection on every new keystroke.
- Typing-through: if the typed character equals the chip's next character, shrink the chip locally.
- Stable prompt prefix: truncate old context in coarse steps so llama.cpp `cache_prompt` keeps hitting.
- `n_predict` about 10 to 16, stop at newline, `t_max_predict_ms` cap, low temperature/top_k.
- Pin one server slot (`id_slot`), warm up at launch with `n_predict: 0`.
- Render the chip natively (Swift NSPanel); no webview round trip on the hot path.
- Never block the event-tap callback: check an atomic flag and return.

## Process
Before any optimization: reproduce, measure baseline, research the cause, change one thing,
re-measure, and record before/after numbers plus sources in `docs/PERFORMANCE.md`.

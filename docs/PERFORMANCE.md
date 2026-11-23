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

## Ghost Text benchmark (Task 5)

### How it works
`ghost-bench` (`src-tauri/src/bin/ghost-bench.rs`, run with `make bench`) replays 12 original sentences
(`src-tauri/src/bench/corpus.rs`) through the **real Ghost engine** and a **real llama-server**, in real time. Typing is a
seeded synthetic model (~50 WPM key intervals, longer pauses after words and punctuation, occasional thinking pauses),
so no real keystrokes are needed and runs are reproducible. Every chip is scored against what is typed next:

| Metric | Meaning |
|---|---|
| p50 / p95 | last keystroke to first chip from the model (typing-through updates excluded; includes the quiet gate) |
| chip in pauses | share of pauses of 300 ms or more with a chip on screen |
| right word | of the chips on screen during a pause, the share whose first word is exactly the next word typed |
| saved | characters a perfect Tab user would not type: leading **whole** words of a chip that match, counted once |

The pipeline itself is tested in CI with an "oracle" brain that always continues the sentence correctly
(`bench::runner` test) and with property tests on the scoring. A first version of the savings metric counted
partial-word matches and the leading space, which reported an impossible 49% savings with 8% word accuracy; it now
follows Tab semantics (only whole matching words count).

### Results (Apple M4 Max, macOS 27.2, llama-server b12633, 12 traces, 2026-11-23)
| Model | Quiet gate | Temp | p50 | p95 | Chip in pauses | Right word | Saved | Server RAM |
|---|---|---|---|---|---|---|---|---|
| **Qwen2.5-1.5B-Instruct Q4_K_M (default)** | **70 ms** | **0.2** | **89 ms** | **99 ms** | **99%** | **35%** | **28%** | **1383 MB** |
| Qwen3-1.7B-Base Q4_K_M | 70 ms | 0.2 | 96 ms | 104 ms | 97% | 31% | 24% | 1972 MB |
| Qwen3-4B-Instruct-2507 Q4_K_M | 70 ms | 0.2 | 96 ms | 108 ms | 90% | 34% | 27% | 3142 MB |
| Qwen2.5-1.5B-Instruct | 40 ms | 0.2 | 58 ms | 67 ms | 93% | 33% | 27% | 1386 MB |
| Qwen2.5-1.5B-Instruct | 110 ms | 0.2 | 134 ms | 143 ms | 95% | 32% | 26% | 1396 MB |
| Qwen2.5-1.5B-Instruct | 70 ms | 0.0 | 96 ms | 103 ms | 92% | 34% | 28% | 1393 MB |

**Run-to-run noise:** the two 70 ms runs of the default model differ by 7 points in pause coverage, so differences of a
few points are not significant at 12 traces.

### Decisions
- **Default model stays Qwen2.5-1.5B-Instruct.** It is best or tied on every quality metric, and uses the least memory.
  The base model was the hypothesis for better continuations; on this corpus it is slightly worse and 590 MB larger.
  The 4B model adds 1.8 GB of memory for no Ghost gain (it stays in the catalog for Selection Actions).
- **Quiet gate stays 70 ms.** 40 ms is 31 ms faster to the first chip but not more accurate and fires more model requests
  mid-word (more energy on battery); 110 ms is slower and no better. 70 ms is well inside the 150/300 ms budget.
- **Temperature stays 0.2.** Greedy decoding (0.0) was not measurably better.
- All three models fit a 16 GB Mac comfortably (model server 1.4 to 3.1 GB). **Not measured on an M3 yet:** the M4 Max
  hides compute cost, so the 4B model will be relatively slower there.

### Prompt cache (prefix-stable window, GT-16)
`scripts/cache-probe.sh` sends a growing prompt the way typing does and prints llama.cpp's own timings:

| Request | Tokens evaluated | Tokens reused | Prompt time | Total |
|---|---|---|---|---|
| cold | 157 | 0 | 300 ms | 459 ms |
| each next keystroke | 1 | 157 | 4 ms | 33 ms |
| unrelated prompt | 11 | 0 | 10 ms | 41 ms |

Between keystrokes only the new token is evaluated, so the prompt cost drops from 300 ms to 4 ms. This is why the
window moves in 256-character steps instead of sliding per keystroke.

## Measurements
First real numbers come from the ignored end-to-end test (`real_llama_server_streams_a_ghost_completion`), not from
the replay harness (Task 5). Two prompts only, so these are smoke numbers, not p50/p95.

| Date | Change | Hardware | Model | Metric | Result | Notes |
|---|---|---|---|---|---|---|
| 2026-11-22 | baseline | Apple M4 Max, macOS 27.2 (dev machine, faster than the M3 floor) | Qwen2.5-1.5B-Instruct Q4_K_M, llama-server b12633, `-ngl 99`, flash-attn auto, ctx 4096, 1 slot | request sent to first token, warm server | 94 ms and 143 ms | via `/completion`, `cache_prompt`, `id_slot 0`, `n_predict 12`. Not yet measured on an M3 |

| 2026-11-23 | Ghost engine v1 (quiet gate 70 ms, polish, early stop) | Apple M4 Max | same | last keystroke to first chip, full engine minus the native helper (10 prompts) | p50 104 ms, p95 114 ms, 10/10 chips | `real_ghost_pipeline_quality_and_latency` (ignored test). Includes the quiet gate. Typing-through updates need no model call |

### Ghost Text quality (polished chips, same run)
| Typed | Chip |
|---|---|
| Thanks for your email. I will get back to you | " as soon as I can." |
| Wait a minute I think I k | "ow what you're saying." |
| The quick brown fox jumps over the | " lazy dog." |
| I'm running a bit late, I should be there in | " 10 minutes." |
| Happy birthday! I hope you have a | " wonderful day." |
| Sorry for the late reply, I was | " out of the country." |
| Could you please send me the | " list of all the countries that" (plausible but odd: a base model may do better, Task 5) |
| Dear hiring manager, I am writing to apply for the | " position of Marketing Manager at your" (invents a detail; Task 6 personalization) |

### Quality notes from the first run (raw model output)
- "The quick brown fox jumps over the" -> " lazy dog." (good).
- "Thanks for your email. I will get back to you" -> " as soon as possible. A. 正确 B." The instruct model drifts into
  exam-style text after the sentence ends. Chip polishing must stop at the sentence boundary and drop script switches
  (Task 4), and Task 5 compares a base model.

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

## Ghost Text context layers (personal-context spec, PC-8)
`ghost-bench --context none,profile,memory,window,profile+memory+window` runs the same corpus with each layer of
the Ghost preamble switched on. The fixtures are in `bench::corpus`:
- a five-line sample profile (name, role, writing style, sign-off);
- 12 seeded memory phrases, 6 of which recur in the corpus and 6 do not;
- one window title per trace kind ("Mail: Re: Project update", "Messages: Anna", ...).

There were two runs with different typing seeds (2026 and 7). Hardware and versions match the Task 5 table; the
model is the default Qwen2.5-1.5B-Instruct, with a 70 ms gate and temperature 0.2.

| Layers | p50 (seed 2026 / 7) | p95 | Chip in pauses | Right word | Saved | Mean right word | Mean saved |
|---|---|---|---|---|---|---|---|
| none (default) | 88 / 88 ms | 95 / 96 ms | 98% / 95% | 32% / 32% | 26% / 27% | 32% | 26.5% |
| profile | 88 / 89 ms | 95 / 97 ms | 98% / 96% | 37% / 32% | 30% / 28% | 34.5% | 29% |
| memory | 92 / 91 ms | 100 / 100 ms | 93% / 97% | 38% / 43% | 31% / 36% | 40.5% | 33.5% |
| window | 94 / 96 ms | 102 / 105 ms | 92% / 97% | 38% / 38% | 31% / 31% | 38% | 31% |
| profile + memory + window | 89 / 96 ms | 97 / 104 ms | 95% / 95% | 43% / 38% | 36% / 35% | 40.5% | 35.5% |

What this shows (12 traces per run; Task 5 found about 7 points of run-to-run noise):
- **Speed:** every layer stays far inside the budget (p50 ≤ 150 ms, p95 ≤ 300 ms).
  - Memory and profile cost 0 to 4 ms at p50, because the notes are a stable prefix that the prompt cache keeps.
  - The window title costs about 7 ms, because it changes with every trace and is re-evaluated once.
  - Server memory did not change.
- **Accuracy:**
  - **Memory** is the only layer that beat the no-context baseline by more than the noise in both runs (+6 and +11
    points right word, +5 and +9 points saved).
  - The window title helps a little (+6 points in both runs).
  - The profile alone is within noise.
- **Reliability:** pause coverage stayed between 92% and 98% for every layer (baseline 95% to 98%). The dips of up to
  6 points are within noise. Chip texts were not inspected one by one for leaked note text; the right-word rate did
  not drop for any layer.
- **Caveat:** half of the seeded memory phrases appear in the corpus, so the memory gain is what a user whose
  phrases recur can expect, not a general gain. A corpus from real accepted phrases would be the next step.

**Decision (owner, 2026-11-23):** every layer stays off by default. Personalize offers "Use my remembered
phrases" on its own, with "Also use the window title" as an optional addition that needs memory. Profile and
screen text are not offered for Ghost Text (the profile showed no gain; screen text was not benchmarked). They
remain available to Selection Actions and Dictation, and the benchmark can still measure them.

## Voice Dictation (Task 8, VD-10)
`bob-native --transcribe <audio> --model <ggml.bin> [--lang xx] [--audio-ctx N]` runs the helper's own
`Transcriber` (whisper.cpp b5907 xcframework, Metal, greedy decoding, no timestamps) on a file and prints timings.
Fixtures are synthetic speech from macOS `say`, generated on demand and never committed. They are a 3.2 s English
sentence, a 12.3 s English paragraph, and a 3.7 s Hungarian sentence (voice Tünde). Measured on an Apple M4 Max,
2026-11-23; times are warm runs (runs 2 and 3), the first inference after loading was 504 to 591 ms (Metal warm-up).

| Model | Encoder window | 3.2 s English | 12.3 s English | 3.7 s Hungarian | Peak memory | Errors |
|---|---|---|---|---|---|---|
| **large-v3-turbo Q5_0 (default)** | **full (30 s)** | **258 ms** | **307 ms** | **266 ms** | **0.9 GB** | **none** |
| large-v3-turbo Q5_0 | shortened to the clip | 54 ms | 159 ms | 983 ms | 0.9 GB | "the larger sample"; Hungarian sentence repeated (hallucination) |
| base | full (30 s) | 32 ms | 77 ms | 43 ms | 0.45 GB | "the larger sample"; Hungarian unusable ("Szió annak, hónak...") |
| base | shortened | 18 ms | 77 ms | 45 ms | 0.45 GB | same |

Decisions:
- **Default model: large-v3-turbo Q5_0.** It was the only model with no errors and works in many languages.
  Transcription takes a quarter to a third of a second here, which leaves most of the 1.5 s budget (VD-10) for slower
  Macs. Base remains as the light option.
- **Full encoder window.** Shortening it is faster but less accurate and, for Hungarian, both slower and wrong.
- **Warm-up after loading.** One second of silence is transcribed right after the model loads, so the user's first
  dictation is not the slow one.
- The model stays loaded while dictation is on (helper footprint about 0.75 to 0.8 GB). Together with the built-in
  brain (1.4 GB) that is about 2.3 GB, which fits a 16 GB Mac.
- **Not measured yet:** release-to-insert in real apps (the Dictation card shows it after each dictation) and any
  M3 numbers.

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

## Ghost Text settings matrix and stale updates
- **Matrix** (`ghost::matrix_tests`, CI): 16 layer combinations (memory, window, language hint, clipboard) x 2
  language settings x 3 quote styles x 2 sharing permissions = 192 cases through the real engine and a local test
  server. Every case checks that the prompt carries exactly the allowed notes, never the copied secret, and that the
  chip uses the chosen quote style. Engine overhead with a 5 ms gate and an instant server: **p50 8 ms, p95 9 ms**.
  A second matrix covers on/off x pause x quiet app x secure field x selection (32 cases): the model is asked only
  when every gate is open. Randomized typing sessions (6 seeds x 25 steps) check that key capture follows the chip and
  that BOB inserts text only while a chip is visible.
- **Bug found by the matrix:** a chip such as `it's "fine" today` stopped at the closing quote. `chip::is_final` now
  ends a chip at a quote only after sentence punctuation.
- **Chrome stale updates** (owner's screenshot): Chrome reports the new caret before the new text after typing or
  an accept, so the old chip was repainted at the new caret ("Investigate the | te the possibility of using"). Fix,
  in one change per layer: the helper re-reads the field 40, 120 and 300 ms after each change and sends only real
  changes (about 1 ms per read); the engine ignores text that is an older prefix of what it just inserted (500 ms
  window), keeps the chip in place when only the caret moved sideways, and treats no-break spaces as spaces. Covered
  by three engine tests. `scripts/ax-lag-probe.swift` measures the lag in a page it opens itself (needs the probe page
  focused; owner check pending).

## Writing languages and style (`style-bench`)
Default model (Qwen2.5-1.5B-Instruct Q4_K_M), Apple M4 Max, 2026-11-24. Three runs before the fixes and one after;
small samples, so the direction matters more than single counts.

**A. Ghost Text language** on 13 short openings (8 Hungarian, 5 English), where small models drift:

| Condition | Raw output in the expected language | Chips shown | Chips in a wrong language |
|---|---|---|---|
| No preference | 9 to 11 of 13 | 12 | 1 (French after "Bocs, de") |
| Preferred Hungarian + English (filter) | 10 to 12 | 11 to 12 | 1, then **0** after the common-word check |
| Filter + "I write in Hungarian and English" hint | 12 to 13 | 13 | **0** |

Raw drift seen: Vietnamese after "Hát." in every run (the owner's screenshot), French after "Bocs, de". Letters
alone cannot catch French ("é" is Hungarian too), so with preferences a chip made of another language's common words
and none of the user's is dropped. The hint costs about 2 ms (p50 50 vs 48 ms). Known limit: Hungarian shares "ü"
with German, so a German word can pass for a Hungarian and English writer.

**B. Rephrase with writing style** (6 English sentences each):

| Condition | Mean words | Contractions | Exclamations | US / UK spellings |
|---|---|---|---|---|
| Natural | 15.2 | 3 to 5 | 0 | 4 / 0 |
| Friendly (old wording) | 14.8 | 4 | 0 | |
| Friendly (new wording) | 14.2 | 5 | 2 | |
| Professional (old / new) | 16 / 14.7 | 4 / **0** | 0 | |
| Concise (old / new) | 14.3 / **9.7 to 10.3** | | | |
| Style notes "No emoji. At most 12 words." | 10.5 | | | 4 of 6 within 12 words |
| Spelling UK, instruction only | 15.2 | | | 4 / **0** UK |
| Spelling UK, instruction + BOB's respelling | 14.3 | | | 0 / **5** |

The model ignored the spelling instruction completely, so BOB now respells unambiguous US/UK pairs in everything it
types (`typography::respell`, whole words only, case kept). Old tone wording barely changed the output; the new
wording works for professional (no contractions) and concise (about a third shorter).

**C. Language of rewrites** (Fix grammar and Rephrase on 4 Hungarian texts, 8 outputs):

| Condition | Before the check | After the check and retry | After the Translate fallback |
|---|---|---|---|
| No preference | 6 to 8 Hungarian | 8 Hungarian | 8 |
| Preferred Hungarian + English | 6 to 8 Hungarian | 8 Hungarian | 8 |
| Per-app mode "answer in English" | 2 to 4 English | 4 English | **7 English** |

Rewrites are now checked with `languages::detect_strong`: in the wrong language BOB asks once more with a firmer
rule, and if that is still wrong it translates the first answer (a small model handles one task at a time better).
Unit test: `selection::engine::tests::an_answer_in_the_wrong_language_is_asked_again`.

## Screen text for Ghost Text (2026-11-25)
`ghost-bench --context none,screen,memory,memory+screen` with per-trace screen text: the message being answered or
the page being worked on, plus menu and sidebar noise of the kind text recognition picks up (`bench::corpus::screen_text`).

| Layers | p50 | p95 | Right word | Keystrokes saved |
|---|---|---|---|---|
| none (default) | 88 ms | 98 ms | 34% | 26% |
| screen | 89 ms | 104 ms | 53% | 50% |
| memory | 88 ms | 97 ms | 46% | 34% |
| memory + screen | 91 ms | 106 ms | **58%** | **54%** |

The screen text in the corpus is relevant to each sentence, so this is an upper bound; unrelated screen text helps
less. Before this change, "Active window" and "All screens" fed only Selection Actions and dictation cleanup, which is
why Ghost Text showed no difference. Screen text is now an opt-in Ghost layer that needs a capture scope. In the
real app a capture of the active window took 105 to 146 ms, off the typing path.

## Suggestions inside a sentence (2026-11-25)
`style-bench --mid`, default model, cursor inside 8 sentences:
- Plain continuation ignores the words after the cursor: "Could we move the | to Thursday afternoon?" gave
  "4th and 5th elements of a list to the".
- Fill-in-the-middle tokens (`<|fim_prefix|>`...) returned nothing at all: the instruct model was not trained on them.
- A two-example "fill the gap" prompt (`ghost::prompt::gap`) works: "meeting", "increase", "mistake", "latest
  report", "beach", "have" for the English cases, 13 to 66 ms. The Hungarian case answered in English ("your"), which
  the language filter drops.

## End-to-end check in the real app (2026-11-25)
`make e2e` drives TextEdit with real key events while BOB runs, and reads the result through Accessibility.
12 of 12 checks passed (14 of 14 since Explain was added). Two of the bugs it found could not show up in unit tests:
- No suggestion on the first line of a TextEdit document: TextEdit reports the empty range at the end of the first
  line 13 points higher than the character before it, so the caret was rejected as outside the field. A caret box
  above the previous glyph is now treated as this quirk (a soft wrap only moves the caret down).
- Rewrites were not undoable: writing the selection through Accessibility does not reach TextEdit's undo stack.
  BOB now pastes (with the clipboard restored and marked transient), which every app undoes in one step.

## Typing never waits for BOB (2026-11-25)

**Report:** typing sometimes froze for a few seconds or lost characters while BOB ran.

**Cause, reproduced:** an active `CGEventTap` holds every keystroke in the system until its callback returns. The tap
lived on the helper's main run loop, which also runs Accessibility reads, the chip and the menus. When the main thread
was busy, every key waited for it; past about a second macOS disabled the tap (`tapDisabledByTimeout`) and keys were
lost. Accessibility calls wait up to about 6 s by default for an app that is busy or not responding
([`AXUIElementSetMessagingTimeout`](https://developer.apple.com/documentation/applicationservices/1462085-axuielementsetmessagingtimeout)),
and the text was read twice per snapshot, so long documents in slow apps could stall the main thread for that long.

**Method:** a stress tool types 84 characters with real key events into a TextEdit document (5 KB and 200 KB) or a
Chrome `contenteditable` page (10 KB and 80 KB) and checks that all of them arrive. A test-only switch,
`BOB_TEST_MAIN_STALL_MS`, blocks the helper's main thread for that many ms every 2.5 s to stand in for a slow app
(since code review SEC-02 only in a helper built with `-Xswiftc -DBOB_TEST_HOOKS`, at most 10,000 ms).
The helper now logs `slow <kind>: N ms` for main-thread work over 30 ms and how long a key waited for the tap.

| Main-thread stall | Before: keys lost | Before: key waited | After: keys lost | After: key waited |
|---|---|---|---|---|
| none | 0 of 84 | under 30 ms | 0 of 84 | under 30 ms |
| 600 ms | 0 of 84 | 592 ms | 0 of 84 | under 30 ms |
| 1500 ms | 42 of 84, tap disabled | 1471 ms | 0 of 84 | under 30 ms |
| 3000 ms | not run | | 0 of 84 in 2 of 3 runs | under 30 ms |

One 3000 ms run lost 26 characters with no tap timeout logged; two reruns with trace logging were clean, so it
remains unexplained (a 3 s stall every 2.5 s is far beyond any real read). Normal BOB afterwards: TextEdit 5 KB and
200 KB and Chrome 10 KB and 80 KB, 84 of 84 every run. Chrome reads still take 33 to 56 ms on the main thread,
which now delays only the chip, never a key.

**Changes:** the key tap runs on its own thread (`dev.bob.keytap`, user-interactive) and still only reads atomic
flags; Accessibility calls time out after 0.4 s; a snapshot reads the text once.

## Model check before loading (code review SEC-13, 2026-11-27)
BOB now hashes a model file (SHA-256) once per launch before llama-server or the helper's whisper loads it; a file
it downloaded itself counts as checked. The check runs before the model server starts, so it adds to the time
until the first suggestion after launch (not to any keystroke). **Method:** the ignored test
`brain::models::tests::real_model_check_time` hashes an installed catalog file three times (read-only; the file is
already in the page cache after the first run, as it is when llama-server reads it right after). Apple M4 Max.
| Build | sha2 | Qwen2.5 1.5B (1.12 GB) | Whisper large-v3 turbo (574 MB) |
|---|---|---|---|
| release | 0.10.9 (software on aarch64 without the `asm` feature) | 1,900 to 1,960 ms | 989 to 1,008 ms |
| release | 0.11.1 (Arm SHA-256 instructions, picked at runtime) | 445 to 484 ms | 229 to 254 ms |
| debug | 0.11.1, unoptimized | 11,484 to 11,745 ms | not run |
| debug | 0.11.1 with `[profile.dev.package.sha2] opt-level = 3` | 440 to 474 ms | not run |
For comparison, `openssl dgst -sha256` (OpenSSL 4.1) takes 0.46 s for the 1.12 GB file. **Research:** sha2 0.10
compiles its aarch64 backend only with the `asm` feature (a C build through `sha2-asm`); sha2 0.11 selects the
Arm `sha2` instructions at runtime through `cpufeatures` (read in the crates' `src/sha256.rs`). sha2 0.11.1 was
already in the build through Tauri, so the change adds no new crate.

## Saving accepted phrases off the typing path (code review P-01, 2026-11-27)
Each accepted phrase saved `ghost-memory.json` on the Ghost engine task, so the next text event waited for the
write. `write_private_file` syncs with `F_FULLFSYNC` on macOS. **Method:** the ignored test
`ghost::engine::tests::accept_save_cost` (release build, Apple M4 Max) records 60 phrases into a memory of 200
and times what the engine task spends per accept.
| Path | Memory file | Engine task per accept, p50 | max |
|---|---|---|---|
| Before: synchronous save | 14.5 KB | 5.587 ms | 15.460 ms |
| After: background save, phrases within 2 s written together | 14.5 KB | 0.0001 ms | 0.037 ms |
Settings and memory are now copied under their lock and written after it is released, so `is_quiet` (taken on
every text event) never waits for a write either. Unsaved phrases are written at exit.

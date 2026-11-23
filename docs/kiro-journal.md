# Kiro journal

Running log feeding `docs/KIRO.md` and the Devpost write-up. One entry per task or notable event:
what worked, what did not, IDE vs CLI observations, spec vs vibe-coding comparisons.

## 2026-11-22: Task 1, foundation
- **Planning before code:** researched the hackathon page and the Kiro docs (specs, hooks, steering, PBT, powers,
  custom agents, MCP) first, then wrote the seven specs with EARS requirement IDs that tests reference.
- **Steering split:** always-on files hold non-negotiables (performance budgets, privacy). File-matched files hold
  domain detail (haunted theme for UI files, macOS notes for Swift files), which keeps context small.
- **Hooks:** created through the hook tool (v1 JSON). Save hooks give immediate feedback on Rust, Swift and TS.
  A guard hook protects the Kiroween rule that `.kiro` must never be ignored.
- **Friction:** the agent could not write `.kiro/settings/mcp.json` (permission rule on the settings folder). The
  owner has to create it. Also, writing JSON files with a remote `$schema` URL was refused in supervised mode, so
  the Tauri configs omit `$schema` (it is optional).
- **Tooling surprise:** Vitest 2 pulled a nested Vite and broke `vite.config.ts` typing; fixed by moving to Vitest 3.
- **Tests at this point:** Rust 4 (incl. 2 property tests), Swift 2, Vitest 3.

## 2026-11-22: Task 2, native bridge
- **Spec first paid off:** the EARS IDs (NB-1..NB-9) made the test list obvious; each test names the requirement it covers.
- **Property-based testing caught a real bug on the first run.** The chunking property
  (`decoding_is_independent_of_chunking`) failed on a caret rectangle: `serde_json` parses ~18% of coordinate-range
  floats one ULP off by default. Shrinking reduced it to a single `TextState` with a caret. Fix: the `float_roundtrip`
  feature. An example-based test with hand-picked round numbers would have passed. This is the strongest PBT story so far.
- **Shared fixtures** (`Tests/Fixtures/*.jsonl`) keep the Rust and Swift protocol implementations from drifting: both suites
  decode and re-encode the same files. Swift Testing has no PBT library, so the chunking property is a seeded randomized test.
- **Cross-language end-to-end test:** the Rust integration test launches the real Swift binary and checks hello, permissions,
  commands, quit and exit-on-EOF.
- **Friction:** `.kiro/settings/mcp.json` is still blocked for the agent by a permission rule (retried at the owner's request;
  same result). The owner has to create it. Also hit a macOS 15-only API (`String(validating:as:)`) with a macOS 14 target.
- **Tests now:** Rust 29 unit/property + 2 end-to-end, Swift 31, Vitest 9.

## 2026-11-22: Task 2 spike, first real-hardware results
- **Permission model surprise:** the owner granted Accessibility to `bob-native` and it had no effect. A command-line tool runs
  with the trust of the terminal app hosting it. The helper now names that app (from `TERM_PROGRAM`) and opens the pane.
  Recorded in the `macos-native` steering so later sessions do not repeat the mistake.
- **Probe data drove the next change:** iTerm returned caret bounds, the Kiro (Electron) chat input returned text but no caret.
  Research (Chromium source, Electron issue #34755, SDK headers) pointed to the text-marker API and to a coordinate-flip risk.
- **Randomized test found a design gap before any user did:** the first version of the orientation check assumed only one
  orientation could fit the field. The seeded randomized test generated tall fields near the screen middle where both fit.
  The fix makes ambiguity explicit and resolves it from earlier evidence per app.
- **Visual review without new permissions:** headless Chrome screenshots of the Vite build (`make screenshot`). The first
  capture looked faded only because it was taken mid fade-in; re-capturing after animations settled showed the real render.
- **Tests now:** Rust 29 + 2 end-to-end, Swift 42, Vitest 12.

## 2026-11-22: Task 3, Brain layer
- **Spec to code:** BR-1..BR-11 mapped one-to-one to modules and tests; the correctness properties P1 (SSE chunking),
  P2 (cancellation), P3 (only verified downloads installed) and P4 (backoff) became property or integration tests.
- **Research before choosing:** model licenses checked through the Hugging Face API (Qwen2.5-3B is not Apache-2.0, so it
  was dropped), GGUF SHA-256 from the HF tree API, llama-server flags from `--help` of the pinned build. A unit test now
  keeps every catalog entry Apache-2.0.
- **Real-hardware check found three bugs that unit tests could not:** (1) a scripted edit to `lib.rs` silently failed
  after `cargo fmt` reformatted the code, so the commands were never registered; a new test compares every
  `invoke("...")` in the UI with the handler list. (2) No logger was installed, so warnings vanished. (3) SIGTERM left
  `llama-server` running with ~1 GB of RAM. All three are fixed and verified on the running app.
- **Guard hook paid off:** the Kiroween secret scan flagged a fake `sk-...` test key; the test now builds it at runtime.
- **Tests now:** Rust 88 + 2 end-to-end (+2 ignored: real Keychain, real model), Swift 48, Vitest 22.

## 2026-11-23: owner feedback on the Brains tab
- **Duplicated words in "Try it"** turned out to be a UI bug, not the model: an async event listener leaked under React
  StrictMode's double mount, so every token was appended twice. Fixed with a `useTauriEvent` hook and a test that mounts
  under StrictMode with an async fake `listen()`. All three listeners in the panel had the same pattern.
- **"Wrong text"** is model drift after the first sentence (instruct model). Raw output is shown in "Try it"; Ghost Text
  polishing (Task 4) addresses it.
- **Where is the app?** There was none: `make dev` runs a bare binary, so permissions went to the terminal. Added
  `make app`: a Developer-ID-signed, hardened-runtime BOB.app with the helper inside, microphone entitlement and usage
  strings (the helper embeds its Info.plist via `-sectcreate`). Verified signature, entitlements, and a clean launch.

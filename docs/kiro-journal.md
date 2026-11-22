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

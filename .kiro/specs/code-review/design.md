# Code review: design

## Method
- **Inventory:** 226 files in eight groups (Rust IPC and shell, brain layer, Ghost Text, Selection Actions and
  Dictation, context and commands, Swift helper, UI, docs and CI). Each group is read line by line with the six lenses
  of CR-1. Large panels in the UI and dev-only benchmark tools were read for structure, state and error paths.
- **Tools (CR-2):** results are summarized at the top of the security section of the findings. `cargo deny` passes
  with the proposed `deny.toml` (to be committed in the CI task); `npm audit` reports 0.
- **Baseline:** Rust 296 tests (line coverage 79.8% at the start), Swift 102 tests (39.4%), UI 130 tests (86%).

## Test harness
- **Simulator** (`src-tauri/src/sim/`, test builds only): the real engines on tokio's virtual clock; a fake app with
  profiles (plain text, smart quotes, Electron, Chrome) that report text and caret with realistic delays; a scripted
  brain; a human-like typist (typos, pauses, Tab, Shift+Tab, type-through, Esc). Seeded sessions replay exactly.
  Regression scenarios for confirmed bugs are `#[ignore]`d until fixed. Soak: defects per 1,000 keys per profile.
- **Test Bench** (`native/test-bench/`, `scripts/test-bench.sh`): a BOB-owned editor, a test BOB in a separate
  profile, and a fake OpenAI-compatible brain or the built-in model. Editor profiles: plain, smart quotes, and late
  (Accessibility reports the text 60 to 250 ms behind, like Chrome and Electron). Every step is checked: text,
  Tab and Shift+Tab, the chip following the text, Esc.
- **Separate profile** (`BOB_HOME`): see CR-5. It must be compiled out of release builds (finding SEC-12).

## Findings
#[[file:findings.md]]

## Fix batches
Ordered by risk to users and by dependency. Each batch is one task in `tasks.md`; the owner approves batches, and may
move, split or cut findings.

| Batch | Theme | Findings | Why first |
|---|---|---|---|
| A | Security and privacy of what BOB starts and sends | SEC-12 (with 12b, 12c), SEC-13, VD-02, SEC-07 (07b), SEC-01, SEC-03, SEC-09, LOG-03, SEC-17, SEC-02 | other programs could use BOB's permissions; credentials can reach cloud brains; typing into password fields can fail open |
| B | Ghost Text correctness (the owner's flaws) | GT-01 to GT-12, GT-16, GT-17, GT-18, P-01 | the twelve regression scenarios turn green; soak rate drops |
| C | Selection Actions, Dictation and the shell | VD-01, VD-03, SA-01, PAUSE-01, SEC-05 (05b), BR-02, BR-03, Q-13 | voice edit is broken; cut answers replace text; pause can stick |
| D | Hardening that needs design | SEC-06, SEC-16, SEC-15, SEC-04, SEC-10, SEC-11, SEC-14, SEC-08 | each needs a decision or a benchmark before code |
| E | Logging, tests and performance | LOG-01, LOG-02, LOG-04, TEST-01 to TEST-07, P-02, P-03 | makes later bugs diagnosable and the helper testable |
| F | Code quality and UI | Q-01 to Q-15 (open ones), UI-01, A11Y-01, Q-14, Q-15, SUP-01 | small, low risk; UI-01 goes with shortcut remapping |
| CI | Gates | CI-01 to CI-04, Q-07 (SwiftLint config), CI-03 (ESLint) | Task 18 |
| Docs | Consolidation | DOC-02, DOC-03, SUP-02, SUP-03, L-01 | Task 19 |

## Done criteria per batch
- Every finding in the batch is fixed or explicitly moved, with its regression test (CR-7, CR-10).
- `cargo fmt`, `clippy -D warnings`, `cargo test`, `swift test` (helper and Test Bench), `vitest`, `tsc` and
  `scripts/check-kiro.sh` pass; CI is green.
- Batch B also: all simulator scenarios pass without `#[ignore]`, and the soak rate per profile is recorded before and
  after in `docs/PERFORMANCE.md`; a Test Bench run with the late editor shows no regression.

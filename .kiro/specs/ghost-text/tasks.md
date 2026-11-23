# Ghost Text: tasks

Plan tasks 4 to 6. Tick a box in the same commit as the code.

- [x] 1. Pure core (Rust): `redact`, `chip` (polish, type_through, accept_word), `prompt` window, `memory` lexicon
  - [x] 1.1 Unit tests for each module (_GT-15, GT-16, GT-17, GT-18_)
  - [x] 1.2 Property tests P1 to P5 (P6, memory lexicon, moves to Task 6 with the lexicon itself)
- [x] 2. Engine: debounce, cancel on keystroke, streaming from Brain, state machine (_GT-1, GT-2, GT-5, GT-6_)
  - [x] 2.1 Mock-brain integration tests (cancellation, streaming, backoff)
- [x] 3. Native: chip NSPanel, conditional event tap, insertion (_GT-5, GT-10 to GT-14_)
  - [x] 3.1 Swift tests for chip geometry and key-swallow decision logic
- [ ] 4. Wire native <-> engine end to end in TextEdit and Notes (_GT-1 to GT-12_): wired and tested up to the helper; **typing in real apps needs the owner's Accessibility grant for BOB.app**
- [x] 5. Quiet zones: secure fields, no caret, active selection (_GT-3_, GT-4); per-app blocklist moves to Task 6
- [ ] 6. Robustness: Chromium/Electron AX enablement, missing caret bounds fallback (_GT-4, GT-9_)
- [ ] 7. Benchmark harness and tuning; record results in `docs/PERFORMANCE.md` (_GT-19, GT-20_)
- [ ] 8. Docs: ARCHITECTURE, PRIVACY, COMPATIBILITY updated; journal entry

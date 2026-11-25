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

## 2026-11-23: Task 4, Ghost Text engine
- **Spec IDs drove the tests again:** GT-1..GT-18 map to engine tests (quiet gate, cancellation, typing-through, Tab,
  Shift+Tab, Esc, quiet zones, redaction, helper restart) and to properties P1 to P5.
- **Property tests found two real bugs in chip polishing before any user saw them:** a quiz check that lowercased text
  before testing for an uppercase letter, and a repeat check skipped when the caret looked mid-word. 2,560 cases per
  property afterwards.
- **A real-model test found a bug the unit tests missed:** every chip froze at its first word, because the overlay was
  only updated when a chip first appeared. The unit test had asserted only the first chip; it now asserts the final one.
- **Tooling lesson (third time):** scripted text replacement breaks after `cargo fmt` reformats code. Edits to
  formatted files now go through exact-match replacement or a full rewrite, and builds run strictly after edits.
- **Tests now:** Rust 122 + 2 end-to-end (+3 ignored real-world tests), Swift 54, Vitest 25.

## 2026-11-23: Cmd+Q investigation and Task 5, benchmark
- **Cmd+Q:** Dock > Quit worked, Cmd+Q did not. Reading the Tauri, muda and wry sources showed the default menu quits
  with `terminate:` (same as Dock), so the shortcut was lost before the menu; wry documents WKWebView consuming Cmd
  key equivalents. BOB now has its own menu and an in-page Cmd+Q fallback. Owner verification pending.
- **Benchmark caught its own bug:** the first run reported 49% keystroke savings with 8% word accuracy, which cannot
  both be true. Per-trace data showed the savings metric credited partial words and the leading space. Fixed to Tab
  semantics; a property test then flagged a wrong property of mine (accepting nothing is always valid).
- **Evidence over intuition:** the base model was expected to beat the instruct model for continuation. It did not
  (31% vs 35% right word, +590 MB). Defaults kept, with the numbers and the run-to-run noise documented.
- **Prompt cache verified with llama.cpp's own timings:** 1 token evaluated per keystroke instead of 157.
- **Kiro hook:** a Stop hook now runs the oracle benchmark smoke test (about 3 s) at the end of every agent turn.
- **Process slip, twice:** I started the benchmark in the same batch as its rebuild and measured the old binary. Fixed
  by stopping and rerunning; noted so long-running measurements always start after their build completes.
## 2026-11-23: Task 6, quiet apps and phrase memory
- **Spec gap closed:** the ghost-text spec left the per-app blocklist and the GT-17 memory lexicon for later. Both now
  exist with their own property tests: settings keep one entry per app with the last edit winning, and memory (P6)
  stays bounded, unique and most recent first. Secret-looking phrases are rejected through the same redaction code
  the prompts use.
- **Quiet by default where Tab already means something:** terminals (shell completion) and password managers. BOB's
  own window is quiet too, so the Try-it box does not haunt itself.
- **Privacy wording follows the code:** PRIVACY.md says memory is stored locally and is not sent to any brain yet,
  because the personalization context stack (profile file, visible context, memory in prompts) waits on the owner's
  decisions.
- **Lint catch:** clippy's `field_reassign_with_default` flagged a test that built settings and then mutated a field;
  rewritten with struct-update syntax.
- **Tests now:** Rust 143 + 2 end-to-end (+3 ignored real-world tests), Swift 54, Vitest 32.
## 2026-11-23: Cmd+Q root cause, personal context, Task 7 Selection Actions
- **Cmd+Q, measured instead of guessed:**
  - The menu item was correct (read back through Accessibility) and clicking it quit BOB.
  - An AppKit key monitor saw no Cmd keys at all.
  - `CGGetEventTapList` showed a third-party utility, Vorssaint, with an active key filter, and its preferences had
    Quit Protection on. The fix is a setting on the owner's Mac, not code. The investigation stays in
    COMPATIBILITY.md so nobody repeats it.
- **Owner decisions became a spec first** (`.kiro/specs/personal-context`, PC-1 to PC-10):
  - Accessibility-only context by default; screen text opt-in.
  - The profile in a fixed `profile.md` edited in the UI.
  - A per-brain sharing switch, off for cloud brains.
  - Ghost Text layers off until measured.
  - The steering rule "no screen capture in v1" was rewritten to "opt-in only" in the same change, so the agent
    instructions match the decision.
- **Protocol v3 without churn:** the new surroundings travel in new `context_state` and `screen_text` events instead of
  new fields on `text_state`. No existing test had to change. The shared fixtures gained one line per new message.
- **Benchmark before defaults:** `ghost-bench --context` measures each Ghost layer on the real model. Results and the
  noise caveat are in PERFORMANCE.md. The owner decides the defaults from those numbers.
- **Selection Actions design choice found by thinking through data loss:** the spec said "redact before cloud". For a
  rewrite that would put "[redacted]" into the user's document, so text with secrets is refused for cloud brains instead.
  The design doc records the change.
- **A test expectation was wrong, not the code:** `NSPasteboard.string(forType:)` joins all items with newlines. A
  probe program confirmed it before the test was changed.
- **Tests now:** Rust 175 + 2 end-to-end (+3 ignored real-world tests), Swift 76, Vitest 48.
## 2026-11-23: Ghost memory decision, Task 8 Voice Dictation
- **Owner decision encoded where it cannot drift:** `ContextSettings::sanitized` reduces any saved Ghost layers to
  "memory, optionally plus the window title". The UI only offers that, and a test covers it (PC-8a in the spec).
- **The spike question answered with release metadata, not assumptions:** listing whisper.cpp's release assets showed no
  macOS server binary, but an official xcframework. Linking it into the helper keeps audio inside the process that
  records it.
- **Measured before choosing:** a small `--transcribe` mode in the helper timed two models on `say` fixtures (English and
  Hungarian). The shortened encoder window looked like a free 2 to 5 times speedup, but the Hungarian fixture exposed a
  hallucination, so it stays off.
- **A smoke test found a crash the unit tests could not:** quitting BOB with a model loaded produced a crash report.
  The log showed a ggml Metal assertion in a static destructor at `exit`. The helper now frees the model before every
  exit path, verified by counting crash reports before and after.
- **State machine first:** the timing rules (250 ms minimum, 5-minute cap, cleanup fallback) are a pure Rust machine with
  two property tests (P1, P2). The async engine around it stays thin.
- **Tests now:** Rust 191 + 2 end-to-end (+3 ignored real-world tests), Swift 84 (incl. a golden transcription when a model is installed), Vitest 53.
## 2026-11-23: Task 9, haunted UI
- **The spec's properties paid off immediately:** the contrast test (P1) failed 7 of 30 pairs on its first run, all in
  the daylight theme, which had never overridden the status colors. A short script found the closest darker shade of
  each hue that passes.
- **Orb state without touching the engines:** every engine already tells the helper what to show (chip, orb, menu
  status). Wrapping the helper sink in an observer gives one place that knows BOB's state. It took three unit tests
  and changed no engine.
- **axe-core in unit tests** caught an `aria-label` on a plain `span` (the wizard's step counter). That is invisible
  to a screen reader, so it became visually hidden text.
- **Screenshots as review:** the screenshot script now takes `?theme=` and `?panel=`, so I could see a real layout
  problem (panels started lower than the Overview heading) and stale copy ("never records your screen", untrue since
  opt-in screen context). The first fix (a Tailwind arbitrary variant) lost to the utility layer, so a plain
  unlayered rule replaced it.
- **Tests now:** Rust 194 + 2 end-to-end (+3 ignored), Swift 84, Vitest 104 (incl. 35 token/palette checks and axe audits of every panel).
## 2026-11-24: Task 10, polish, hardening, research
- **Measuring found a bug no test had:** the idle CPU measurement showed two BOBs at once: a leftover instance, and
  a second one started next to it, each with a helper and a 1.4 GB model server. A per-user lock
  (`File::try_lock`, which the OS releases if BOB crashes) now stops the second copy before it touches the socket.
- **Idle cost, measured:** about 0.3% CPU for core, helper and model server together (budget 2%). Memory is
  dominated by the two models (1.3 GB and 0.8 GB).
- **Security review findings, fixed with tests:** API keys could travel over plain http to a remote server, and the
  "no proxy" setting meant for local servers also applied to cloud brains. `cargo audit` (new) found 9 advisories,
  including four in rustls, fixed by compatible updates. `npm audit` found critical advisories in dev-only Vitest 3,
  fixed by moving to Vitest 5 with no test changes. Both audits now run in CI.
- **Usability:** a Settings panel (⌘,), pause, menu-bar toggles, ⌘1 to ⌘7, brain editing, confirmed removal,
  single-phrase forgetting, an autosaving profile, and "start quietly in the menu bar". The last two fixed real
  problems: switching panels lost unsaved profile edits, and hiding the Dock icon reopened the window at every login.
- **Research before Task 11:** reviews and feature-request boards of Cotypist, Superwhisper, VoiceInk, Wispr Flow
  and rewrite tools, summarized with links in docs/RESEARCH.md. The owner chooses from the proposals.
- **A deadlock the smoke test caught, not the unit tests:** BOB ignored SIGTERM after the Settings work. A stack
  sample of the frozen process pointed straight at `general_get`, waiting on a mutex it already held. Two `lock()`
  calls in one struct literal keep the first guard alive until the end of the statement. The fix locks once, a
  regression test runs the view on a thread with a 2 s timeout, and a scan found no other statement locking the same
  mutex twice.
## 2026-11-24: Task 11, features from the research
- **Owner picked eight proposals** from docs/RESEARCH.md plus four requests (quiet-apps and phrases editors, open at
  login, statistics, a mouse-reactive ghost). Everything landed in one pass with tests first where the logic is pure.
- **Voice edit reused two engines:** the helper notices a selection when recording starts and sends a separate
  event; the dictation engine hands the transcript to the Selection engine as an instruction. No new brain code, and
  the selection-unchanged guard for replacement still applies.
- **Tap to toggle changed the state machine, not the engine:** one new `Latched` state. The P1 property test now runs
  with the toggle on and off, which is what made the change safe.
- **Open at login, verified instead of assumed:** the startup log showed `SMAppService` reports "not found" for this
  build, which is why the switch had no effect. A LaunchAgent fallback fixed it; an ignored test writes and removes
  the real plist, run once on this Mac.
- **Trust boundary found while building the quiet-apps editor:** typing a bundle id by hand meant arbitrary strings
  could reach `ghost-settings.json`. The core now validates them, with a test.
- **Tests now:** Rust 222 + 2 end-to-end (+4 ignored), Swift 84, Vitest 122 (axe audits of all eight panels).

## 2026-11-24: Task 12a, five proposals, hardening, logging, UI, languages
- **Two bugs from screenshots, fixed at their source:** in Chrome the chip repeated typed text because Chrome sends
  the new caret before the new text; the helper now re-reads the field shortly after each change, and the engine
  ignores stale text after an accept. A Vietnamese suggestion after the Hungarian "Hát." passed the script check
  (both Latin); a language filter from letters and common words now takes such chips back.
- **Measuring changed the design twice.** `style-bench` showed the local model ignores US/UK spelling entirely, so
  BOB now respells the output itself; the old tone wording barely changed anything, so the presets were rewritten
  and re-measured. The per-app "answer in English" rule held 3 of 8 times; a language check with one retry and a
  Translate fallback brought it to 7 of 8.
- **The settings matrix found a real bug:** 192 combinations through the real engine showed a chip like
  `it's "fine" today` stopping at the closing quote. The randomized sessions confirm key capture always follows the
  chip.
- **The full test run caught a race that single runs missed:** a new handshake deadline made `set_read_timeout`
  fail with EINVAL on macOS when the peer had already hung up. The fix ignores that error, with a regression test
  that connects and hangs up 50 times.
- **MCP both ways:** Kiro used `fetch` while building BOB, and now BOB is an MCP server itself (`bob-mcp`), tested
  end to end against the running app and the local model, plus a Skill file for other tools.
- **Tests now:** Rust 253 + 2 end-to-end (+4 ignored), Swift 87, Vitest 127.

## 2026-11-25: Task 12, release, and the end-to-end run
- **Running the real app found what the tests could not.** A new `make e2e` drives TextEdit with real key events.
  On its first run nothing worked: TextEdit reports the caret one line too high on the first line, so BOB hid every
  suggestion there. Two typo and clipboard events from the helper were never forwarded to the engine (the unit tests
  call the engine directly). The undo check failed, first because the script sent Cmd+Y (the owner uses a Hungarian
  layout), then for real: the Accessibility write was not undoable. All fixed; 12 of 12 checks pass.
- **Owner reports, reproduced before fixing:** the frozen chip after a window switch came from my own Chrome fix
  (keep the chip while only the caret moves), which had no time limit. Slack's `@mention` list lost Tab to BOB; now BOB
  steps aside while the word starts with `@`, `#` or `:`.
- **Measured before building:** "Active window" seemed to do nothing for Ghost Text because Ghost never used screen
  text; the benchmark showed it could (34% to 58% right word with memory), so it became an opt-in layer. For
  suggestions inside a sentence, fill-in-the-middle tokens returned nothing on the default model; a two-example gap
  prompt works.
- **Release:** `make release` builds a signed DMG with the model server inside. A stripped proc-macro library broke
  `tauri build`; the fix exempts build-time code from stripping. Benchmark tools no longer end up in the bundle.
- **Public-repo audit:** no keys, signing hashes, team IDs or home paths in any revision. The sample name in tests
  was replaced. The signing certificate's company name is visible in signed builds; the owner fully owns that
  company and chose to sign the public release with it.
- **Tests now:** Rust 259 + 2 end-to-end (+4 ignored), Swift 92, Vitest 128, and the 12 checks of `make e2e`.
## 2026-11-25: Typing froze or lost characters
- **Owner report, reproduced first:** a test switch blocks the helper's main thread on purpose. At 1.5 s macOS
  disabled BOB's key tap and half of the typed characters were lost, because the tap shared the main thread with
  Accessibility reads, and those can wait about 6 s for a busy app.
- **Fix, then re-measured:** the key tap got its own thread, Accessibility calls time out after 0.4 s, and the text
  is read once per snapshot. The same stalls are now lossless (table in `docs/PERFORMANCE.md`).

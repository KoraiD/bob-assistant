# Changelog

All notable changes to BOB. Format follows [Keep a Changelog](https://keepachangelog.com/); versions follow SemVer.

## [Unreleased]

### Added (Task 10, polish and hardening)
- Settings panel (⌘,): open at login (macOS Login Items), Dock icon on/off, open the window at launch, pause, theme, keyboard shortcuts.
- Pause BOB for an hour, until tomorrow morning or until resumed, from the menu bar or Settings; a sidebar banner resumes it.
- Menu bar: Ghost Text and Voice Dictation toggles, pause and resume, Settings.
- ⌘1 to ⌘7 switch panels; the window remembers its size and position.
- Edit a brain (name, endpoint, model, key) and confirm before removing one; forget a single remembered phrase.
- The profile autosaves (800 ms after typing stops, and when leaving the panel).
- `make audit` and a CI job for RustSec and npm advisories; docs/SECURITY.md.

### Fixed
- The window froze when the Settings view loaded (a mutex locked twice in one statement); found with a stack sample, fixed and covered by a test.
- Two BOB instances could run at once (two helpers, two model servers, a contested socket); now one per user.
- API keys could be sent over plain http to a server outside this Mac; now refused, with a clear message in the form.
- Cloud brains ignored the system proxy; only local brains bypass it now.
- Dependency advisories: rustls, rustls-webpki, quinn-proto, quinn-udp, cookie updated; Vitest 3 to 5.

### Added (Task 9, haunted UI)
- Sidebar navigation with a panel per feature (Overview, Ghost Text, Selection Actions, Voice Dictation, Brains, Personalize, About).
- A live orb in the sidebar (floating, listening, thinking, haunting, needs a hand), driven by the core; the tray tooltip follows it.
- First-run welcome tour: permissions, BOB's own brain, the speech model and a cheat sheet; reopen it from About.
- Themes: Midnight (default), Daylight, Match macOS.
- Accessibility: WCAG AA contrast proven from the tokens in both themes, axe-core audit of every panel, visible focus ring.

### Fixed
- Daylight theme: accent, pumpkin and status colors failed WCAG AA on white; darkened to pass.

### Added (Task 8, Voice Dictation)
- Hold Option+Space (configurable), speak, release: the text is typed at the cursor. Speech is recognized on the Mac by whisper.cpp inside BOB's helper; audio is never saved or sent.
- Speech models in Overview > Voice Dictation: Whisper large-v3 turbo Q5_0 (default, about 0.3 s for a short phrase) and Whisper base (light); language choice or automatic detection.
- Optional cleanup presets on the Dictation brain (clean up fillers, translate to English, meeting notes) with a fallback to the raw transcript.
- A native ghost orb near the cursor: listening (breathes with your voice), thinking, done, or a short message.
- Taps under 250 ms are ignored, silence is detected, recordings stop at 5 minutes, and password fields are never typed into.

### Changed
- Ghost Text context: remembered phrases (off by default) can be switched on alone, with the window title as an optional addition. Profile and screen text are no longer offered for Ghost Text.

### Fixed
- The helper no longer crashes on quit once a speech model is loaded (ggml Metal assertion at exit).

### Added (Task 7, Selection Actions)
- Select text in any app, press Cmd+Shift+J (configurable), pick an action with the arrow keys, and the result replaces the selection: Fix grammar, Rephrase, Make shorter, Make friendlier, Make professional, Summarize, Translate, and up to 12 of your own actions.
- Native: Carbon hotkey, selection via Accessibility or Cmd+C (clipboard restored), a non-activating menu panel with streamed preview, and replacement only if the selection is unchanged (Accessibility write, paste fallback).
- Cloud brains never receive text that looks like it contains a key or password; the action is refused instead of redacted.

### Added (personal context)
- Personalize tab: `profile.md` editor (Markdown toolbar, preview with GitHub-flavored Markdown and emoji), tone and language, and a preview of exactly what BOB adds to prompts.
- Visible context: Accessibility only by default (window title, text after the cursor). Opt-in "Active window" or "All screens" reads on-screen text with ScreenCaptureKit and on-device Vision OCR (Screen Recording asked only then; BOB and quiet apps never captured).
- Per-brain "share personal context" switch: on by default for brains on this Mac, off for cloud brains.
- Ghost Text context layers (profile, memory, window, screen), all off by default; `ghost-bench --context` measures each.
- Protocol v3.

### Fixed
- Cmd+Q: traced to a third-party quit-protection utility (Vorssaint) filtering Cmd+Q before it reaches BOB; documented in COMPATIBILITY. BOB also handles Cmd+Q in an AppKit key monitor.

### Added (Task 6)
- Quiet apps: Ghost Text stays silent in terminals, password managers and BOB by default; the Ghost card lists quiet and recently used apps with one switch each. Saved in `ghost-settings.json`, together with Ghost on/off.
- Accepted-phrase memory: phrases accepted with Tab / Shift+Tab are kept locally in `ghost-memory.json` (bounded, secret-free); the Ghost card shows the count, lists them on demand and can forget all. Not yet used in prompts.

### Added
- Ghost Text benchmark: `make bench` / `ghost-bench` replays seeded typing traces through the real engine and model; scores latency, pause coverage, right-word rate and Tab-accurate keystroke savings; oracle test in CI; `scripts/cache-probe.sh` for prompt-cache reuse; Stop hook with a benchmark smoke test.

### Fixed
- Cmd+Q now quits BOB (own app menu named BOB, plus an in-page fallback).

### Added (Task 4)
- Ghost Text v1: polished inline suggestions (sentence boundary, 6 words, script-switch and repeat filters), 70 ms quiet gate, cancellation on every keystroke, typing-through, Tab / Shift+Tab / Esc, quiet zones, redaction, latency stats; Ghost card with on/off and p50/p95; Try it shows the polished chip.
- Protocol v2: `key`, `insert_text`, `capture_keys`; conditional key tap and Unicode text insertion in the helper.
- `make app`: signed BOB.app (Developer ID from `BOB_SIGN_IDENTITY`, hardened runtime, microphone entitlement) with the helper bundled.

### Fixed
- "Try it" showed every token twice (event listener leaked under React StrictMode).

### Added (earlier)
- Brain layer: provider registry with roles, OpenAI-compatible and llama.cpp dialects, streaming with cancellation, connection test, Keychain secrets, verified resumable model downloads, supervised llama-server sidecar (loopback, key file, crash cleanup).
- Brain panel: model catalog with download progress, brain list with local/cloud labels, role routing, "Try it" with first-token latency.
- `make sidecars` (pinned, SHA-256 verified llama.cpp b12633); stderr logging; clean shutdown on SIGTERM/SIGINT.
- Native bridge: JSON-lines protocol (Rust + Swift, shared fixtures), Unix-socket server with version handshake, helper supervisor with backoff.
- Swift helper: Accessibility focus/text/caret observer, native chip panel, permission checks, `--probe` and `--chip-demo` modes.
- Permissions card in the UI and permission status in the tray menu.
- `make native|probe|chip-demo|sign-dev`; `scripts/sign-dev.sh` (identity from the environment, never committed).
- Project foundation: Tauri 2 shell with tray icon, hide-on-close, `app_info` IPC command.
- Haunted design tokens (dark default, optional daylight), ghost mark, capability cards.
- Swift helper package `bob-native` (library + executable + tests).
- Kiro workspace: steering, hooks, custom agents, seven specs with EARS requirements.
- Guards and CI: `scripts/check-kiro.sh` (Kiroween rules), GitHub Actions workflow.
- Docs: architecture, privacy, performance method, compatibility matrix, design system, submission checklist.

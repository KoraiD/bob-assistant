# Changelog

All notable changes to BOB. Format follows [Keep a Changelog](https://keepachangelog.com/); versions follow SemVer.

## [Unreleased]

### Added
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

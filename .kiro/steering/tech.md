---
inclusion: always
---

# Technology stack and conventions

## Stack
- **Rust core (Tauri 2)** in `src-tauri/`: engines (ghost, selection, dictation), Brain router,
  settings, Keychain, memory, metrics. Library crate `bob_lib`; `main.rs` is a thin shell.
- **Swift helper** in `native/bob-native/` (SwiftPM, Swift 6, macOS 14+): Accessibility, CGEventTap,
  native overlay panels (chip, action menu), mic capture, text insertion. Logic lives in the
  `BobNativeCore` library (testable); the executable is thin. Talks to Rust over a Unix socket (JSON lines).
- **React 18 + TypeScript + Tailwind v4 + Vite 6** in `src/`: settings UI and orb. Tests with Vitest.
- **Sidecars** (`externalBin`, `-aarch64-apple-darwin` suffix): `llama-server`, `whisper-server`.
  Bind 127.0.0.1 only, API key required, no web UI.

## Commands (Makefile)
`make setup | dev | build | test | lint | fmt | check-kiro | check`

## Conventions
- Rust: `cargo fmt`, clippy with `-D warnings`. Prefer small pure modules with unit + `proptest` tests.
  No `unwrap()` on I/O or user data outside tests. Errors via `thiserror`-style enums once added.
- Swift: strict concurrency, no force unwraps on AX values, keep AppKit/AX calls behind small protocols
  so logic can be tested without the OS.
- TypeScript: `strict`, no `any`. UI state is derived from Rust events; no business logic in React.
- Conventional commits (`feat:`, `fix:`, `docs:`, `test:`, `perf:`, `chore:`). Small, reviewable commits.
  Update docs, specs and `tasks.md` in the same commit as the code change.
- Bug fixes and optimizations: reproduce, measure baseline, research (cite sources in docs/commit),
  change one thing, re-measure, record numbers in `docs/PERFORMANCE.md`.
- Dependencies: prefer well-maintained, permissively licensed crates/packages; record new ones in
  `docs/ARCHITECTURE.md` if they affect the design.

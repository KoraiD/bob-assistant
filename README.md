# BOB: Bring Your Own Brain

<p align="center"><img src="assets/bob-icon.png" alt="BOB, the friendly ghost" width="140" /></p>

<p align="center"><em>A friendly ghost that haunts every app you type in, powered by any model you choose.</em></p>

BOB is a privacy-first macOS assistant. It adds three capabilities to every app, using a local model
(llama.cpp on Apple Silicon, Ollama, LM Studio) or any OpenAI-compatible cloud model:

| Capability | What it does | Status |
|---|---|---|
| **Ghost Text** | Inline next-word predictions at your caret. `Tab` accepts a word, `Shift+Tab` accepts all, `Esc` dismisses. | Planned (Task 4) |
| **Voice Dictation** | Hold a key, speak, release. Clean text lands at the caret. Fully offline with local Whisper. | Planned (Task 8) |
| **Selection Actions** | Highlight text, press a hotkey, rephrase / fix / summarize / translate in place. | Planned (Task 7) |

> Built for **Kiroween 2026** (category: Frankenstein) with [Kiro](https://kiro.dev). The `/.kiro` directory
> contains the specs, steering, hooks and custom agents used to build it. See [docs/KIRO.md](docs/KIRO.md).

## Status

Tasks 1 to 3 are in place: app shell and tray, haunted theme, the Rust <-> Swift native bridge (protocol, handshake,
supervision), the Accessibility caret observer, the permissions UI and the Brain layer (any local or cloud model).
Ghost Text itself is next. Progress is tracked in [docs/SUBMISSION.md](docs/SUBMISSION.md) and in each spec's `tasks.md`.

## Requirements

- macOS 14+ on Apple Silicon (tested target: M3 with 16 GB or better)
- Node.js 20+, Rust 1.80+ (stable), Xcode command line tools with Swift 6

## Quickstart (from source)

```bash
git clone https://github.com/KoraiD/bob-assistant.git
cd bob-assistant
make setup        # installs JS dependencies
make dev          # runs the app (Tauri + Vite)
make test         # Rust, Swift and UI tests
make check        # everything CI runs
```

`make help` lists all tasks.

### Local model (BOB's own brain)
```bash
make sidecars     # downloads the pinned llama.cpp server (SHA-256 verified) into src-tauri/binaries/llama
make dev          # then open Brains > "BOB's own brain" > Download, then Use
```
The default model is Qwen2.5-1.5B-Instruct (Q4_K_M, 1.1 GB, Apache-2.0). You can also add Ollama, LM Studio or any
OpenAI-compatible endpoint in the Brains tab. Models and binaries are never committed.

### Run BOB as a real app (needed for permissions)
`make dev` runs a bare binary from your terminal, so macOS attributes its permissions to the terminal app.
To give BOB its own Accessibility and Microphone permissions, build the signed bundle:
```bash
export BOB_SIGN_IDENTITY="Developer ID Application: <Name> (<TEAMID>)"   # optional; keeps grants across rebuilds
make app
open src-tauri/target/debug/bundle/macos/BOB.app
```
Then Overview > Grant permissions, and switch BOB on in System Settings > Privacy & Security.

### Permissions (added as features land)
BOB asks for **Accessibility** (read the caret, insert text) and **Microphone** (dictation). It never asks for
Screen Recording. See [docs/PRIVACY.md](docs/PRIVACY.md).

## Architecture in one picture

```
React/TS UI  <--IPC-->  Rust core (Tauri 2)  <--Unix socket-->  Swift helper (AX, event tap, overlays, mic)
                           |   brain router
                           +-- llama-server sidecar (local)   +-- Ollama / LM Studio / cloud (OpenAI-compatible)
                           +-- whisper sidecar (local)
```

Details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Performance targets and measurements:
[docs/PERFORMANCE.md](docs/PERFORMANCE.md).

## Repository layout

See `.kiro/steering/structure.md`.

## License

[MIT](LICENSE). Third-party components and their licenses are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

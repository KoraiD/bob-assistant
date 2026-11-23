---
inclusion: always
---

# BOB: product overview

**BOB (Bring Your Own Brain)** is a privacy-first macOS assistant that brings AI into every app
you type in, powered by any local or cloud model the user chooses. BOB is a friendly ghost.

Submitted to **Kiroween 2026** (Devpost), category **Frankenstein**: a Rust core, a Swift native helper,
llama.cpp and whisper.cpp sidecars, macOS Accessibility APIs, a React/TS haunted UI and any LLM,
stitched into one app. Secondary strengths: haunted UI (Costume Contest flavor), Best Demo, Student bonus.

## Capabilities, in priority order
1. **Ghost Text (priority 1):** inline next-words prediction at the caret in any app. Tab accepts a word,
   Shift+Tab accepts all, Esc dismisses.
2. **Voice Dictation (required):** hold a hotkey, speak, release; clean text lands at the caret.
3. **Selection Actions (required):** highlight text, press a hotkey, rephrase / fix / summarize /
   translate / custom prompt, replaced in place.
4. Optional, cut first: MCP chat mode, extra brain providers.

## Product principles
- **Bring Your Own Brain:** every capability works with a local sidecar model, Ollama, LM Studio or any
  OpenAI-compatible cloud endpoint. Capability flags decide what each brain can do.
- **Local-first and private:** no telemetry, no accounts. Screen reading is opt-in, on-device, and its text
  goes only to brains the user allows.
- **Demoable first:** a working end-to-end path beats breadth. Cut stretch work before core work.
- **Scope guard:** macOS (Apple Silicon, macOS 14+) only. Must run well on an M3 with 16 GB.

## Kiroween rules this project must keep satisfying
- New project built during Oct 30 to Dec 4, 2026. No code or history from earlier attempts.
- Public GitHub repo, MIT license detectable, `/.kiro` committed and never in any `.gitignore`.
- Demo video under 3 minutes, shown on the build machine; no copyrighted music or third-party trademarks.
- Write-up of how Kiro was used (see `docs/KIRO.md`).
- Deadline: **Dec 4, 2026, 2:00 PM PT**. Full checklist: `docs/SUBMISSION.md`.

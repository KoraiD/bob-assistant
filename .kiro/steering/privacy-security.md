---
inclusion: always
---

# Privacy and security rules

- **Local-first, no telemetry, no accounts.** Nothing leaves the machine except prompts sent to the brain
  the user configured. Cloud brains are opt-in and clearly labeled in the UI.
- **Screen capture is opt-in only** (owner decision, 2026-11-23; spec `personal-context`). Default context is
  Accessibility only. Request Screen Recording only after the user picks "Active window" or "All screens"; OCR on
  the device; never store or send images; never capture secure fields, quiet apps or BOB itself.
- **Personal context goes only to allowed brains** (per-brain switch, off by default for cloud brains).
- **Secrets:** API keys live only in the macOS Keychain. Never in the repo, logs, settings JSON or prompts.
- **Redaction:** before any prompt leaves the machine, drop or redact credential-like tokens
  (`sk-`, `AKIA`, `ghp_`, JWT `eyJ`, PEM blocks, long opaque/hex tokens). Property-test this.
- **Quiet zones:** never read or suggest in secure text fields (`AXSecureTextField`) or in apps on the blocklist.
- **Sidecars:** bind to `127.0.0.1`, require a random per-launch API key, disable the web UI, never expose
  tools or MCP proxies on them.
- **Tauri:** least-privilege capabilities, strict CSP, no remote content, shell permissions scoped to exact sidecars.
- **Clipboard:** when a paste fallback is used, restore the previous clipboard contents.
- **Logs:** never log typed text, selections, transcripts or keys. Log metadata (sizes, timings) only.
- **Permissions:** only Accessibility, Microphone and (if needed) Input Monitoring, each requested with an
  explanation and a graceful degraded mode when denied.
- Document everything user-visible in `docs/PRIVACY.md` and keep it current.

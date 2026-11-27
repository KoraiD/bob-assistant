# Security

What BOB protects, how, and how it is checked. Privacy (what data goes where) is in [PRIVACY.md](PRIVACY.md).

## Threat model in short
- **Assets:** what you type, select and say; your profile; API keys for cloud brains.
- **Trust boundary 1, other local processes:** the core and the helper talk over a Unix socket in a per-user 0700
  directory (socket 0600). The built-in model server listens on 127.0.0.1 only, needs a random per-launch key (read
  from a 0600 file, never on the command line), and has its web UI off.
- **Trust boundary 1b, the launch environment:** another process of the same user can start BOB with any
  environment variables. Release builds ignore them for every choice of program or folder (see the table).
  BOB does not check its helper's and model server's signatures before starting them: they sit inside the signed,
  notarized bundle, which macOS (App Management) protects from other apps.
- **Trust boundary 2, the network:** only the brains you configure and Hugging Face downloads (pinned URLs,
  SHA-256 verified before install).
- **Trust boundary 3, the web view:** strict CSP (`default-src 'self'`, no remote scripts, no `eval`). IPC is limited
  to BOB's own commands; every input is bounded or validated in Rust (endpoints, file sizes, labels, languages,
  hotkeys, model ids).

## Rules the code enforces (with tests)
| Rule | Where |
|---|---|
| API keys only in the macOS Keychain, never in files, logs or `Debug` output | `brain/secrets.rs`, `brain/service.rs`, `brain/client.rs` tests |
| API keys are never sent over plain http to a server outside this Mac (and the form says why) | `HttpBrain::new`, `validateDraft` |
| Loopback brains bypass system proxies; cloud brains may use them | `HttpBrain::new` |
| Credential-like tokens are redacted from prompts; text that contains them is never sent to a cloud brain by Selection Actions | `ghost/redact.rs`, `selection/engine.rs` |
| Secure (password) fields are never read, typed into or captured | helper `AXReader`, `SelectionIO`, insert guard; engines |
| One BOB per user (an exclusive lock), so a second copy cannot take over the helper socket | `native_ipc::acquire_instance_lock` |
| Screenshots for screen context stay in memory for the OCR call only | `ScreenContext.swift` |
| Audio stays in the helper's memory, never on disk | `AudioRecorder.swift`, `Transcriber.swift` |
| Downloads: pinned URL, size cap, SHA-256 before install, resumable | `brain/models.rs` (property test) |
| Third-party binaries (llama-server, whisper.xcframework): pinned version and SHA-256 | `scripts/versions.env`, fetch scripts |
| Files with personal data are created 0600, written to a temporary file, synced, then renamed; the settings folder is 0700 | `fsutil.rs` (test) |
| A brain server cannot exhaust memory: 8 MB per streamed answer, 64 KB per error body, 4 MB per SSE event | `brain/client.rs`, `brain/sse.rs` (tests) |
| Local sockets accept only processes of the same user (`getpeereid`), and the helper must say hello within 5 s | `native_ipc.rs`, `mcp.rs` (tests) |
| AI agents (MCP) are off by default; when on: same rules as the hotkeys, 60 calls per minute, 256 KB per request, every call audited | `mcp.rs` (tests) |
| Logs and the activity trail never contain text; every log line and audit value is redacted as a safety net | `logging.rs`, `audit.rs` (tests) |
| One panicked thread cannot take the rest of BOB down through poisoned locks; panics are logged and audited | all `Mutex` use, `install_panic_hook` |
| Clipboard context skips concealed and transient pasteboard items and quiet apps | `ClipboardWatcher.swift` (tests) |
| Release builds: Developer ID signature with hardened runtime and secure timestamp; credentials stay in the keychain, never in files or CI | `scripts/build-release.sh`, docs/RELEASE.md |
| Typo fixes delete only the exact word they were made for, and never in a password field | `HelperRuntime.swift` (`replace_before_caret`) |
| Release builds start only their own bundled programs and use only their own folders: the development overrides (`BOB_HOME`, `BOB_NATIVE_PATH`, `BOB_LLAMA_SERVER`, `BOB_CONTROL_SOCKET`) are read in debug builds only, and at launch BOB removes them and takes `HOME`, `TMPDIR` and `PATH` from the system, not from whoever started it (`open --env`, a login item). System tools run by absolute path | `paths::dev_env`, `paths::harden_environment` (tests), `supervisor::locate_helper`, `brain/sidecar.rs` |

## Dependency advisories
`make audit` runs `cargo audit` (RustSec) and `npm audit`; CI runs both on every push.

Audit of 2026-11-24:
- Rust: 9 advisories fixed by semver-compatible updates. They were rustls 0.23.43 to 0.23.46 and rustls-webpki to
  0.103.17 (TLS for cloud brains and downloads), plus quinn-proto, quinn-udp and cookie. Three informational warnings
  remain, none exploitable here: `chrono` and `proc-macro-error` are unmaintained transitive dependencies, and `glib`
  belongs to Tauri's Linux backend, which is not compiled on macOS.
- npm: production dependencies had none. Dev-only Vitest 3 had two critical and one moderate advisory (tests never
  ship in the app); upgraded to Vitest 5. Now 0.

## Reporting
Open a private security advisory on the GitHub repository.

# Privacy

BOB is local-first. This document lists exactly what BOB reads, stores and sends. It is updated with every
feature that changes the answer.

## Summary
- No accounts, no telemetry, no analytics, no crash reporting.
- No screen capture and no OCR. BOB never asks for the Screen Recording permission.
- Text is sent only to the brain you configured. Local brains stay on `127.0.0.1`; cloud brains are labeled "cloud".
- API keys are stored in the macOS Keychain only.

## Permissions (when each feature lands)
| Permission | Used for | If denied |
|---|---|---|
| Accessibility | read text near the caret, caret position, insert text, capture Tab/Esc only while a suggestion is visible | Ghost Text and Selection Actions are disabled with an explanation |
| Microphone | Voice Dictation | Dictation is disabled with an explanation |
| Input Monitoring | only if the spike shows it is required for modifier-only hotkeys | documented in `docs/COMPATIBILITY.md` |

## Data handling
| Data | Where it goes | Stored? |
|---|---|---|
| Text before the caret (Ghost Text) | the active Ghost brain, after redaction | not stored |
| Accepted phrases | local `ghost-memory.json` (at most 200 phrases of 64 characters, most recent first; secret-looking text is never kept); not sent to any brain yet | yes, locally; view and "Forget all" in the Ghost card |
| Quiet apps, Ghost on/off | local `ghost-settings.json` | yes, locally |
| Recently used apps (bundle id and name, last 12) | the quiet-apps list in the Ghost card | in memory only, gone when BOB quits |
| Selected text (Selection Actions) | the selection brain, after redaction | not stored |
| Audio (Dictation) | in memory, to the local transcriber | never written to disk |
| Transcripts, typed text, selections | never logged | no |
| API keys | macOS Keychain | yes |

## Brains
- Every brain shows "on this Mac", "local" (loopback endpoint) or "cloud: text leaves your Mac". Unparseable endpoints
  count as cloud. Cloud brains are never used for Ghost Text unless a connection test proves they support it and you
  pick them.
- The built-in model server listens on `127.0.0.1` only, needs a random per-launch key (stored in a 0600 file in a
  0700 per-user directory, never on the command line) and has its web UI disabled. It is stopped when BOB quits; a
  leftover from a crash is stopped at the next launch.
- Model files are downloaded from Hugging Face only when you press Download, and are verified with SHA-256.

## Redaction
Before a prompt leaves the machine, credential-like tokens are dropped or replaced (`sk-`, `AKIA`, `ghp_`, JWTs,
PEM blocks, long opaque or hex tokens). Behavior is property-tested.

## Quiet zones
BOB does not read or suggest in secure text fields, when no caret position is known, or while text is selected.
Prompts are redacted before they leave the engine, even for the local brain.

Quiet apps get no suggestions and no text from them reaches any brain. The defaults are terminals (Terminal, iTerm, Warp,
Ghostty, kitty, Alacritty, WezTerm), password managers (1Password, Bitwarden, Passwords, Keychain Access) and BOB
itself; the Ghost card lists them with the apps you used recently, and
one switch per app adds or removes it. Changes apply at once, including to a suggestion already on screen.

## Keyboard
While (and only while) a suggestion is visible, BOB swallows Tab, Shift+Tab and Esc. Shortcuts with Cmd, Ctrl, Option
or Fn always pass through. Text BOB types is marked so BOB never captures its own keystrokes. Key presses are not
logged or stored.

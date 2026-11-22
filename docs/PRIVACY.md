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
| Accepted phrases | local `ghost-memory.json` (bounded, most recent first) | yes, locally, deletable in Settings |
| Selected text (Selection Actions) | the selection brain, after redaction | not stored |
| Audio (Dictation) | in memory, to the local transcriber | never written to disk |
| Transcripts, typed text, selections | never logged | no |
| API keys | macOS Keychain | yes |

## Redaction
Before a prompt leaves the machine, credential-like tokens are dropped or replaced (`sk-`, `AKIA`, `ghp_`, JWTs,
PEM blocks, long opaque or hex tokens). Behavior is property-tested.

## Quiet zones
BOB does not read or suggest in secure text fields or in apps on the blocklist.

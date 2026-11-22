# Native bridge: requirements

The Swift helper (`bob-native`) and the Rust core communicate over a Unix socket using JSON lines.
EARS IDs `NB-n`. This spec is an addition to the original six (added because Task 2 is a distinct deliverable).

## Requirements
- **NB-1** THE System SHALL connect the Swift helper and the Rust core over a Unix domain socket in a per-user, mode-0600 location.
- **NB-2** WHEN a connection is established, both sides SHALL exchange a `hello` message carrying the protocol version, and SHALL refuse to proceed on a mismatch.
- **NB-3** THE System SHALL encode every message as one JSON object per line with a `type` field; unknown types SHALL be ignored and logged, never crash the peer.
- **NB-4** THE System SHALL decode messages correctly regardless of how the byte stream is chunked.
- **NB-5** WHEN the helper exits, THE System SHALL restart it with backoff; WHEN the core exits, THE helper SHALL exit.
- **NB-6** THE System SHALL report Accessibility and Microphone permission status to the UI/tray and SHALL guide the user to the right System Settings pane.
- **NB-7** THE System SHALL emit focus and text-state events (app bundle id, secure flag, text before caret, selection, caret rect) without polling the whole AX tree.
- **NB-8** THE System SHALL NOT include typed text in logs.
- **NB-9** THE System SHALL document per-app compatibility in `docs/COMPATIBILITY.md`.

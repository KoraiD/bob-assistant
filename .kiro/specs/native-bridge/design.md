# Native bridge: design

## Transport
Socket at `$TMPDIR/bob-<uid>/native.sock` (directory 0700, socket 0600). Rust listens; Swift connects.
Framing: newline-delimited JSON (UTF-8). A max line length (1 MiB) guards against runaway input.

## Messages (v0, to be finalized in Task 2)
Swift -> Rust: `hello`, `permission_status`, `focus_changed`, `text_state`, `key` (only while capture is active), `hotkey`, `error`.
Rust -> Swift: `hello_ack`, `show_chip`, `update_chip`, `hide_chip`, `insert_text`, `capture_keys {on, set}`, `show_menu`, `quit`.
Both sides define the schema in code (serde enums / Codable enums) with a shared fixture file under
`native/bob-native/Tests/Fixtures/` used by Rust and Swift round-trip tests.

## Properties
- **P1 (NB-4):** for any sequence of messages and any chunking of their encoded bytes, the decoded sequence is identical.
- **P2 (NB-3):** random JSON objects with unknown `type` never cause a decode panic.
- **P3 (NB-2):** hello with a different protocol version is rejected on both sides.

## Spike deliverables (Task 2)
Caret rect accuracy, Chromium/Electron behavior, TCC attribution of the nested helper, whisper-server availability.
Results go to `docs/COMPATIBILITY.md`.

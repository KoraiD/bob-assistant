# Voice Dictation: design

## Flow
Native hotkey (key down/up via the event tap, modifier-only capable) -> `AVAudioEngine` tap, convert to 16 kHz mono
Float32/Int16 -> buffer in memory -> on release, send to Rust -> `whisper` sidecar HTTP (`base.en` default, `small.en` option)
-> optional brain cleanup -> `insert_text` in native.

## Open question for the Task 2 spike
Does whisper.cpp ship a resident server binary we can bundle? If not: `whisper-rs` (in-process, Metal) or a small
custom server wrapper. Decision recorded in `docs/ARCHITECTURE.md`.

## State machine
`Idle -> Recording -> (Discarded | Transcribing -> (Cleaning ->) Inserting -> Idle)`. Pure Rust, table-driven,
property-tested against arbitrary press/release/timeout event sequences.

## Correctness properties
- **P1 (VD-4, VD-11):** for any event sequence, the machine ends in Idle, never inserts for holds < 250 ms, and never records beyond the cap.
- **P2 (VD-6):** cleanup failure always yields the raw transcript.
- **P3 (VD-8):** no code path writes the audio buffer to a file (enforced by a unit test over the module's public API and a grep check in CI).

## Testing
Golden test with a short recorded WAV fixture (synthetic speech generated with macOS `say`, license-free).

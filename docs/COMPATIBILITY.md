# Compatibility matrix

Filled in by the Task 2 spike and extended by Tasks 4 to 8. Each cell records what was **observed on real hardware**,
not what we expect.

Test machine: _to be recorded_ (chip, RAM, macOS version).

| App | Caret rect | Text read | Chip shown | Tab capture | Insert (Unicode keys) | Notes |
|---|---|---|---|---|---|---|
| TextEdit | - | - | - | - | - | |
| Notes | - | - | - | - | - | |
| Mail | - | - | - | - | - | |
| Safari (textarea) | - | - | - | - | - | |
| Chrome (textarea) | - | - | - | - | - | |
| VS Code | - | - | - | - | - | |
| Slack | - | - | - | - | - | |

## Open spike questions
1. Does TCC attribute Accessibility/Microphone grants correctly to a helper nested in the app bundle?
2. Does whisper.cpp ship a resident server we can bundle, or do we use `whisper-rs`?
3. Is Input Monitoring needed in addition to Accessibility for an active event tap and modifier-only hotkeys?
4. Which apps return usable `kAXBoundsForRangeParameterizedAttribute` values?

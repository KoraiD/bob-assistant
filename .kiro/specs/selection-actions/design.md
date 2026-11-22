# Selection Actions: design

## Flow
Hotkey (native) -> `read_selection` -> Rust `selection` engine builds an instruct prompt (`prompts` module: one template per action)
-> selection brain streams -> accumulate -> native `replace_selection` with verification that the selection is unchanged.

## Components
- Native: hotkey listener, selection reader (AX first, Cmd+C fallback), menu panel (list, keyboard navigation), replace.
- Rust: `selection::actions` (action definitions and prompt templates), `selection::engine` (state machine
  `Idle -> Menu -> Running -> Replacing -> Done/Failed`), output post-processing (strip quotes, preambles).
- Local default: Qwen2.5-3B-Instruct (candidate; benchmark in Task 5), or any chat-capable brain.

## Correctness properties
- **P1 (SA-2):** clipboard save/restore is the identity on the pasteboard's string content (mock pasteboard).
- **P2 (SA-7):** replacement is never issued when the pre-replace selection text differs from the captured selection.
- **P3 (SA-4):** the key-capture predicate swallows exactly {Up, Down, Enter, Esc} while the menu is visible and nothing otherwise.
- **P4 (SA-9):** prompts sent to cloud brains contain no credential-like tokens.
- **P5:** output cleaner is idempotent and never returns text longer than the model output.

## Risks
Apps that ignore AX replacement (Electron): paste fallback. Large selections: cap and warn.
